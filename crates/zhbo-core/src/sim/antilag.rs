//! BO2's antilag for the lockstep: a shot is resolved against the players as its shooter saw them.
//!
//! A first-person input reaches the sim Zero Hour's run-ahead after it was sent (4 frames on a LAN,
//! 6-10 online, 30 at a match's start), while the shooter aimed at the world his machine showed
//! then. BO2's server rewinds the other players to the shooter's view before it traces a bullet
//! (t6mp `FUN_0066b6e0`: the target time is clamped to at most 400 ms back, and the client
//! entities' positions and angles are set from the cached snapshots around it, `FUN_006efd30`,
//! then relinked); this module does the same with the frame each input carries
//! ([`super::PlayerSim::seen_frame`]): every logic frame the sim keeps each first-person General's
//! position, yaw and body state (the pose his hit boxes take) and the Assault Shields as they stood
//! (`History`), and [`super::Sim::fire_round`] traces against the frame the shooter saw, at most
//! `AntilagMaxRewind` back (the tuning pack). The lockstep needs no snapshot interpolation: the
//! shooter saw exactly one logic frame.
//!
//! Only the sim's own bodies are rewound, as BO2 rewinds only players: the Generals in first person.
//! Zero Hour's units (and a General under RTS control) are the engine's and stand where they are
//! now. Deterministic: the history is a function of the stepped state, the seen frame travels in the
//! replicated input message, and the cap comes from the tuning pack every player holds.
/*
** Conquer Ops - Zero Hour gameplay core: lag-compensated hitscan (BO2's antilag rewind).
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::*;
use std::collections::VecDeque;

/// One first-person General as a logic frame left him: what a bullet traced against him then.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub general: u32,
    pub pos: Vec3,
    pub yaw: f32,
    /// His body state (`nativebody::state_of`): the pose of his hit boxes.
    pub state: u16,
}

/// The rewindable world after one logic frame.
#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub frame: u32,
    pub bodies: Vec<Pose>,
    /// The Assault Shields carried then (held or stowed), and the deployed ones.
    pub shields: Vec<crate::shield::Bearer>,
}

impl Frame {
    pub fn pose(&self, general: u32) -> Option<&Pose> {
        self.bodies.iter().find(|b| b.general == general)
    }
}

/// The last frames, oldest first.
#[derive(Clone, Debug, Default)]
pub struct History {
    frames: VecDeque<Frame>,
}

impl History {
    /// Keep `frame`, dropping what is older than `keep` frames.
    pub fn record(&mut self, frame: Frame, keep: usize) {
        self.frames.retain(|f| f.frame < frame.frame);
        self.frames.push_back(frame);
        while self.frames.len() > keep.max(1) {
            self.frames.pop_front();
        }
    }

    pub fn at(&self, frame: u32) -> Option<&Frame> {
        self.frames.iter().find(|f| f.frame == frame)
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

/// `AntilagMaxRewind` (milliseconds) in whole logic frames, rounded down (400 ms: 12 frames).
pub fn max_rewind_frames(ms: f32) -> u32 {
    let frames = ms.max(0.0) / (LOGIC_DT * 1000.0);
    // A hair of slack so 400 ms at 30 Hz is 12 frames, not 11.999.
    (frames + 1e-3) as u32
}

impl Sim {
    /// Remember this frame's first-person Generals and shields for later shots (called once per
    /// step, after every system ran).
    pub(super) fn record_history(&mut self) {
        let run_speed = self.tuning.run_speed(false);
        let bodies = self
            .players
            .iter()
            .flatten()
            .filter(|p| p.is_first_person())
            .map(|p| Pose { general: p.general, pos: p.body.pos, yaw: p.yaw, state: p.body_state(false, run_speed) })
            .collect();
        let frame = Frame { frame: self.frame, bodies, shields: self.shield_bearers() };
        self.history.record(frame, self.antilag_frames as usize + 1);
    }

    /// The frame `p`'s shot this step is resolved against: the one his input says he saw, at most
    /// the tuning's rewind back; `None` when that is now (or he sent none: an input without a seen
    /// frame, the old message, or the sim's own tests).
    pub(super) fn rewind_frame(&self, p: &PlayerSim) -> Option<&Frame> {
        let seen = p.seen_frame;
        if seen == 0 || seen >= self.frame {
            return None;
        }
        let oldest = self.frame.saturating_sub(self.antilag_frames);
        self.history.at(seen.max(oldest))
    }
}

/// The targets as the shooter saw them: each first-person General in `then` moved to where he
/// stood, facing as he faced, in the body state he was in; everyone else as now. `states` is the
/// sim's body states now, rewritten for the rewound.
pub fn rewind(objs: &mut [ObjInfo], states: &mut [(u32, u16)], then: &Frame) {
    for o in objs.iter_mut() {
        if let Some(b) = then.pose(o.id) {
            o.pos = b.pos;
            o.yaw = b.yaw;
        }
    }
    for (g, s) in states.iter_mut() {
        if let Some(b) = then.pose(*g) {
            *s = b.state;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::tests::{general, spawn, FakeHost};

    #[test]
    fn four_hundred_milliseconds_is_twelve_frames() {
        assert_eq!(max_rewind_frames(400.0), 12);
        assert_eq!(max_rewind_frames(0.0), 0);
        assert_eq!(max_rewind_frames(-5.0), 0);
        assert_eq!(max_rewind_frames(crate::tuning::Tuning::default().antilag_max_rewind), 12);
    }

    #[test]
    fn the_history_keeps_the_last_frames() {
        let mut h = History::default();
        for f in 1..=20 {
            h.record(Frame { frame: f, ..Default::default() }, 13);
        }
        assert_eq!(h.len(), 13);
        assert!(h.at(7).is_none());
        assert!(h.at(8).is_some() && h.at(20).is_some());
        // A frame stepped again (a replayed frame number) replaces what came after it.
        h.record(Frame { frame: 15, ..Default::default() }, 13);
        assert!(h.at(16).is_none() && h.at(15).is_some());
    }

    /// A General runs across; the frame he was seen at is the one the shot rewinds to, bounded by
    /// the tuning's cap.
    #[test]
    fn a_shot_is_resolved_at_the_seen_frame_within_the_cap() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(general(2, 1));
        let mut sim = Sim::new();
        spawn(&mut sim, &host, 0, 1);
        spawn(&mut sim, &host, 1, 2);
        let run = FpsInput { move_packed: crate::input::pack_move(0.0, 1.0), ..Default::default() };
        for f in 1..=30 {
            sim.input(1, 2, run);
            sim.step(&host, f);
        }
        let at = |f: u32| sim.history.at(f).and_then(|h| h.pose(2)).map(|b| b.pos);
        assert!(at(25).is_some() && at(25) != at(29), "he moved, frame by frame");
        let mut p = sim.player(0).unwrap().clone();
        p.seen_frame = 25;
        assert_eq!(sim.rewind_frame(&p).map(|h| h.frame), Some(25));
        p.seen_frame = 2;
        assert_eq!(sim.rewind_frame(&p).map(|h| h.frame), Some(30 - 12), "at most 400 ms back");
        p.seen_frame = 30;
        assert!(sim.rewind_frame(&p).is_none(), "seen now: no rewind");
        p.seen_frame = 0;
        assert!(sim.rewind_frame(&p).is_none(), "no seen frame: no rewind");
    }
}
