//! The local player's prediction: what his own General will do over the inputs still in flight.
//!
//! Zero Hour runs every command its run-ahead after it was sent (4 frames on a LAN, 6-10 online,
//! 30 while a match starts; 1 in a skirmish, whose command list is read on the next logic frame),
//! so the sim's record of the local player lags his hands by that much. BO2's client predicts its
//! own player (`CG_PredictPlayerState` runs `Pmove` over the unacknowledged user commands) and
//! draws the camera, the gun, the zoom and every fire cue from the prediction; this does the same
//! with the sim's own step ([`crate::sim::predict`]): each logic frame, from the sim's record, it
//! steps a copy of the sim over the inputs the record has not taken in yet (each carries the frame
//! it was sent at, and the record says the latest it holds, [`PlayerSim::seen_frame`]).
//!
//! * The camera, the view kick, the zoom, the viewmodel, the HUD and the cues read the predicted
//!   record ([`Predictor::view`]); the eye is drawn between the last two predicted frames at the
//!   render clock ([`Predictor::eye`]), so it moves at the render rate.
//! * The weapon's events (a shot, its impacts and hit marker, a reload, a swap, a throw…) are
//!   presented once, when an input is first predicted; when the sim's record later takes that input
//!   in, its own events of those kinds are matched against them and only the ones the prediction
//!   missed are presented then ([`Predictor::reconcile`]). A predicted hit marker cannot know of a
//!   kill (the prediction hurts nobody): the sim's confirms it.
//! * When the sim's record arrives and the re-prediction puts the eye somewhere else than the last
//!   prediction did (another player's machine moved something it collides with, the run-ahead
//!   changed), the difference is carried as an error and decayed away over [`ERROR_DECAY`]
//!   seconds; past [`SNAP_DISTANCE`] (a teleport) the view snaps.
//!
//! Presentation only: nothing here reaches the sim but the inputs the client sends anyway.

use std::collections::VecDeque;

use crate::host::LogicHost;
use crate::input::FpsInput;
use crate::math::Vec3;
use crate::sim::{EventKind, PlayerSim, ShotTracer, Sim};
use crate::units::LOGIC_DT;

/// A reconciliation moving the eye further than this (Zero Hour units, about 1.2 m) snaps the
/// view instead of smoothing it.
pub const SNAP_DISTANCE: f32 = 48.0;
/// How quickly a reconciliation's eye error decays, seconds (its time constant).
pub const ERROR_DECAY: f32 = 0.1;
/// Zero Hour's longest run-ahead (`MAX_FRAMES_AHEAD`): no more inputs than that can be in flight.
pub const MAX_PENDING: usize = 128;

/// The events the prediction presents ahead of the sim: the player's own weapon and offhand, as
/// BO2's client plays them from its predicted player state.
pub fn predictable(kind: &EventKind) -> bool {
    use EventKind::*;
    matches!(
        kind,
        Shot { .. }
            | Recoil { .. }
            | Impact { .. }
            | ShieldHit { .. }
            | HitMarker { .. }
            | ReloadStarted
            | Swapped
            | AltToggled
            | Melee
            | MeleeHit
            | OffhandReady { .. }
            | Thrown { .. }
            | Launched
            | CantPlant { .. }
            | Hammer { .. }
            | WeaponPickedUp { .. }
    )
}

/// What a prediction is checked by when the sim takes its input in: everything the view shows of
/// the player's own state.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Snap {
    pos: Vec3,
    vel: Vec3,
    eye_height: f32,
    aim: (f32, f32),
    kick: (f32, f32),
    ads: f32,
    active: usize,
    ammo: u16,
}

impl Snap {
    fn of(p: &PlayerSim) -> Snap {
        let a = &p.arsenal;
        Snap {
            pos: p.body.pos,
            vel: p.body.vel,
            eye_height: p.body.eye_height,
            aim: (p.yaw, p.pitch),
            kick: p.aim_offset(),
            ads: a.ads,
            active: a.active,
            ammo: a.current().ammo,
        }
    }
}

/// One input sent and not yet in the sim's record.
#[derive(Clone, Debug)]
struct Sent {
    seen: u32,
    input: FpsInput,
    /// The predictable events its first prediction presented (the sim's own are matched to them).
    presented: Option<Vec<EventKind>>,
    /// Its first prediction's result (the misprediction count compares the sim's to it).
    first: Option<Snap>,
    /// The eye after it in the latest prediction.
    eye: Vec3,
}

/// How well the prediction has done (the latency harness and the log report these).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    /// Inputs the sim's record took in.
    pub acked: u32,
    /// Of those, the ones whose first prediction left a different record from the sim's.
    pub mispredicted: u32,
    /// Re-predictions that moved the eye (smoothed), and those that snapped it.
    pub corrections: u32,
    pub snaps: u32,
    /// The sim's own events of the predicted kinds that no prediction had presented (shown late).
    pub late: u32,
    /// Predicted events the sim did not produce.
    pub unconfirmed: u32,
    /// The most inputs in flight at once.
    pub max_pending: u32,
}

/// What a frame's prediction brought that had not been presented yet.
#[derive(Debug, Default)]
pub struct Advance {
    /// The predictable events of the inputs predicted for the first time.
    pub events: Vec<EventKind>,
    /// Their tracer bullets (the own gun's).
    pub tracers: Vec<ShotTracer>,
}

/// The sim's own events for the local player after the prediction presented its share.
#[derive(Debug, Default, PartialEq)]
pub struct Reconciled {
    /// To present now: every unpredictable event, and the predictable ones no prediction showed.
    pub present: Vec<EventKind>,
    /// Predicted hit markers the sim says killed (`headshot` each): the kill's feedback, without
    /// another hit sound.
    pub kills: Vec<bool>,
}

/// The local player's prediction (see the module doc).
#[derive(Clone, Debug, Default)]
pub struct Predictor {
    pending: VecDeque<Sent>,
    /// The predicted record after the last input in flight, and after the one before it (the
    /// sim's own record when only one is in flight).
    now: Option<PlayerSim>,
    prev: Option<PlayerSim>,
    /// The sim frame the prediction was made from.
    base_frame: Option<u32>,
    /// Seconds since the latest input went out (the render clock inside the logic frame).
    phase: f32,
    /// The last prediction's tail: the newest input's seen frame and the eye after it.
    tail: Option<(u32, Vec3)>,
    /// The eye error a reconciliation left, decaying.
    pub correction: Vec3,
    pub stats: Stats,
}

impl Predictor {
    /// Out of first person: nothing in flight, nothing predicted (the stats stay).
    pub fn reset(&mut self) {
        *self = Predictor { stats: self.stats, ..Default::default() };
    }

    pub fn active(&self) -> bool {
        self.now.is_some()
    }

    pub fn in_flight(&self) -> usize {
        self.pending.len()
    }

    /// The record the view is drawn from: the prediction's, when there is one.
    pub fn view(&self) -> Option<&PlayerSim> {
        self.now.as_ref()
    }

    /// The record one logic frame before [`Predictor::view`].
    pub fn view_prev(&self) -> Option<&PlayerSim> {
        self.prev.as_ref()
    }

    /// How far the render clock is between [`Predictor::view_prev`] and [`Predictor::view`] (0..1).
    pub fn alpha(&self) -> f32 {
        (self.phase / LOGIC_DT).clamp(0.0, 1.0)
    }

    /// The eye this rendered frame: between the last two predicted frames at the render clock, plus
    /// what is left of a reconciliation's error.
    pub fn eye(&self) -> Option<Vec3> {
        let now = self.now.as_ref()?.body.eye();
        let prev = self.prev.as_ref().map_or(now, |p| p.body.eye());
        Some(prev.lerp(now, self.alpha()) + self.correction)
    }

    /// The sim's record took in every input up to its [`PlayerSim::seen_frame`]: they leave the
    /// flight, and the predictable events their predictions presented are returned (owed: the sim's
    /// own of those kinds are matched to them, [`Predictor::reconcile`]).
    pub fn ack(&mut self, auth: &PlayerSim) -> Vec<EventKind> {
        let mut owed = Vec::new();
        let mut last = None;
        while self.pending.front().is_some_and(|s| s.seen <= auth.seen_frame) {
            let s = self.pending.pop_front().expect("checked");
            self.stats.acked += 1;
            owed.extend(s.presented.iter().flatten().copied());
            last = Some(s);
        }
        // The record holds the newest of them: its first prediction should have been it.
        if let Some(Sent { seen, first: Some(first), .. }) = last {
            if seen == auth.seen_frame && first != Snap::of(auth) {
                self.stats.mispredicted += 1;
            }
        }
        owed
    }

    /// Match the sim's own events for the player (`auth`, this frame's) against what the acked
    /// inputs' predictions presented (`owed`, from [`Predictor::ack`]).
    pub fn reconcile(&mut self, auth: Vec<EventKind>, mut owed: Vec<EventKind>) -> Reconciled {
        let mut out = Reconciled::default();
        for kind in auth {
            if !predictable(&kind) {
                out.present.push(kind);
                continue;
            }
            let same = std::mem::discriminant(&kind);
            match owed.iter().position(|o| std::mem::discriminant(o) == same) {
                Some(i) => {
                    owed.remove(i);
                    if let EventKind::HitMarker { headshot, kill: true } = kind {
                        out.kills.push(headshot);
                    }
                }
                None => {
                    self.stats.late += 1;
                    out.present.push(kind);
                }
            }
        }
        self.stats.unconfirmed += owed.len() as u32;
        out
    }

    /// A rendered frame of first person: `sent` is the input that went out this frame (its seen
    /// frame and itself), `dt` the frame's time. Re-predicts from the sim's record whenever a logic
    /// frame passed or an input went out; returns what the inputs predicted for the first time
    /// brought.
    pub fn advance(&mut self, sim: &Sim, host: &dyn LogicHost, idx: usize, sent: Option<(u32, FpsInput)>, dt: f32) -> Advance {
        let mut out = Advance::default();
        let Some(auth) = sim.players.get(idx).and_then(|p| p.as_ref()) else {
            self.reset();
            return out;
        };
        // The render clock inside the logic frame: an input goes out as a logic frame starts, and
        // what is left over of the frame's time carries into the next (dropping it would draw a
        // frame-rate that does not divide 30 Hz unevenly).
        self.phase += dt.max(0.0);
        if let Some((seen, input)) = sent {
            self.phase -= LOGIC_DT;
            self.pending.push_back(Sent { seen, input, presented: None, first: None, eye: Vec3::ZERO });
            while self.pending.len() > MAX_PENDING {
                self.pending.pop_front();
            }
        }
        self.phase = self.phase.clamp(0.0, LOGIC_DT);
        self.stats.max_pending = self.stats.max_pending.max(self.pending.len() as u32);
        // A reconciliation's error decays (`1 / (1 + dt / tau)` per frame: no transcendental, and
        // the same over any frame rate to first order).
        self.correction = self.correction * (1.0 / (1.0 + dt.max(0.0) / ERROR_DECAY));
        if self.correction.length() < 1e-3 {
            self.correction = Vec3::ZERO;
        }
        if sent.is_none() && self.base_frame == Some(sim.frame) && self.now.is_some() {
            return out;
        }
        self.base_frame = Some(sim.frame);
        let tail = self.tail.take();

        let mut scratch = sim.scratch();
        let mut prev = auth.clone();
        let n = self.pending.len();
        for (i, s) in self.pending.iter_mut().enumerate() {
            if i + 1 == n {
                if let Some(p) = scratch.player(idx as i32) {
                    prev = p.clone();
                }
            }
            if !scratch.predict_step(host, idx, s.input, s.seen) {
                break;
            }
            let events: Vec<EventKind> = scratch.events.drain(..).filter(|e| e.player == idx as i32).map(|e| e.kind).filter(predictable).collect();
            let tracers: Vec<ShotTracer> = std::mem::take(&mut scratch.tracers);
            let p = scratch.player(idx as i32).expect("kept");
            s.eye = p.body.eye();
            if s.presented.is_none() {
                s.first = Some(Snap::of(p));
                out.events.extend(events.iter().copied());
                out.tracers.extend(tracers);
                s.presented = Some(events);
            }
        }
        let now = scratch.player(idx as i32).cloned().unwrap_or_else(|| auth.clone());
        if n == 0 {
            prev = now.clone();
        }

        // Reconcile the eye: where this prediction has the last one's newest input against where
        // that one had it.
        if let Some((seen, old)) = tail {
            let new = self.pending.iter().find(|s| s.seen == seen && s.first.is_some()).map(|s| s.eye).or_else(|| (auth.seen_frame == seen).then(|| auth.body.eye()));
            if let Some(new) = new {
                let err = old - new;
                if err.length() > SNAP_DISTANCE {
                    self.correction = Vec3::ZERO;
                    self.stats.snaps += 1;
                } else if err.length() > 1e-4 {
                    self.correction += err;
                    self.stats.corrections += 1;
                }
            }
        }
        self.tail = self.pending.back().map(|s| (s.seen, s.eye));
        self.prev = Some(prev);
        self.now = Some(now);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::impacts::Surface;

    fn shot() -> EventKind {
        EventKind::Shot { origin: Vec3::ZERO, end: Vec3::new(1.0, 0.0, 0.0), left: false }
    }

    /// The sim's own events of the predicted kinds are matched one for one against what the
    /// predictions presented; the rest are presented, and a predicted marker the sim says killed
    /// confirms the kill.
    #[test]
    fn the_sims_events_are_matched_against_the_predicted_ones() {
        let mut p = Predictor::default();
        let owed = vec![shot(), EventKind::HitMarker { headshot: false, kill: false }, EventKind::ReloadStarted];
        let impact = EventKind::Impact { at: Vec3::ZERO, dir: Vec3::ZERO, surface: Surface::Dirt, head: false, fatal: false, impact_type: 0, exit: false };
        let auth = vec![shot(), shot(), EventKind::HitMarker { headshot: true, kill: true }, EventKind::Damaged { amount: 5.0, dir: crate::math::Vec3::ZERO }, impact];
        let r = p.reconcile(auth, owed);
        // The second shot and the impact were not predicted (late); the damage is never predicted.
        assert_eq!(r.present, vec![shot(), EventKind::Damaged { amount: 5.0, dir: crate::math::Vec3::ZERO }, impact]);
        assert_eq!(r.kills, vec![true], "the predicted marker's kill, confirmed");
        assert_eq!((p.stats.late, p.stats.unconfirmed), (2, 1), "the reload was predicted and never came");
    }

    /// The flight leaves on the record's seen frame, and the newest acked input's first prediction
    /// is checked against the record.
    #[test]
    fn inputs_leave_the_flight_as_the_record_takes_them_in() {
        let mut p = Predictor::default();
        let rec = PlayerSim::spawn_for_tests();
        for seen in 10..15 {
            let first = Snap::of(&rec);
            p.pending.push_back(Sent { seen, input: FpsInput::default(), presented: Some(vec![shot()]), first: Some(first), eye: Vec3::ZERO });
        }
        let mut auth = rec.clone();
        auth.seen_frame = 12;
        let owed = p.ack(&auth);
        assert_eq!(owed.len(), 3, "three inputs' predicted shots are owed");
        assert_eq!(p.in_flight(), 2);
        assert_eq!((p.stats.acked, p.stats.mispredicted), (3, 0));
        auth.seen_frame = 13;
        auth.body.pos = Vec3::new(5.0, 0.0, 0.0);
        p.ack(&auth);
        assert_eq!(p.stats.mispredicted, 1, "the record is not where the prediction had him");
    }
}
