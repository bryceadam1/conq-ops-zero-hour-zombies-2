//! The spectating client: a machine that only watches (a Zero Hour observer seat, a replay's
//! playback, a defeated player who stays to watch; `ClientIn::spectating`). It never sends sim
//! input — no `FpsInput`, no class pick, no selection message — and never captures the mouse.
//! When it views a first-person General it follows him the way BO2's dead spectate does
//! (`hud/deadspectate.lua`): the camera is his eyes at the sim's authoritative aim (the input
//! angles every peer replayed, plus the sim's view kick), smoothed between logic frames like the
//! eye; the gun is his viewmodel; the HUD is his, reduced to what a BO2 spectator sees
//! (`hud::spectate`).
//!
//! Who is followed is the core's choice, so it works whether or not the engine's "observe a
//! player" option is on:
//! - the seat the engine observes (`ClientIn::local_player`, Zero Hour's observer list) is adopted
//!   whenever it changes, if it has a General;
//! - BO2's switch-player bar (`DualButtonPrompt("shoulderl", MPUI_SPECTATING, "shoulderr", …,
//!   "mouse1", "mouse2")`): the attack button follows the previous first-person General, the aim
//!   button the next;
//! - BO2's spectate-mode button (`ButtonPrompt("alt2", …, "mouse3")`, third ↔ first person) is the
//!   overhead view here: the middle button or the field-computer key leaves for the overhead view
//!   and comes back;
//! - when the followed General dies or leaves first person the view cuts to the next one (BO2's
//!   spectator moves on), or flies out overhead when nobody is left in first person, and follows
//!   the next General who enters.
use super::{Client, ClientIn, ViewMode};
use crate::input::{mouse_buttons, Key, KeySet};
use crate::sim::{PlayerSim, Sim};

/// The follow state of a spectating client.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spectator {
    /// The seat followed in first person this frame (`None`: overhead).
    pub target: Option<i32>,
    /// The seat last followed (or picked): switching and resuming start from it.
    pub last: Option<i32>,
    /// The spectator chose the overhead view (the spectate-mode button).
    pub overhead: bool,
    /// The seat the engine observed last frame, so a change is adopted once.
    observed: Option<i32>,
    /// Last frame's keys and mouse buttons, for press edges.
    prev_keys: KeySet,
    prev_buttons: u32,
    /// What the log last said (the followed seat) and on which logic frame, for [`Client::spectator_log`].
    reported: Option<Option<i32>>,
    reported_frame: u32,
}

/// Whether a seat's General can be followed: in first person and spawned (an unclassed General
/// sits behind his class screen; BO2 never spectates an unspawned player).
pub fn followable(p: &PlayerSim) -> bool {
    p.is_first_person() && !p.awaiting_class()
}

/// The followable seats, in seat order.
pub fn candidates(sim: &Sim) -> Vec<i32> {
    sim.players.iter().enumerate().filter_map(|(i, p)| p.as_ref().filter(|p| followable(p)).map(|_| i as i32)).collect()
}

/// The next (`step` > 0) or previous candidate after `from`, wrapping; `from` need not be a
/// candidate any more (a General who just died). With no `from`, the first or the last.
pub fn cycle(cands: &[i32], from: Option<i32>, step: i32) -> Option<i32> {
    let (first, last) = (cands.first().copied()?, cands.last().copied()?);
    let Some(from) = from else { return Some(if step < 0 { last } else { first }) };
    if step < 0 {
        Some(cands.iter().rev().copied().find(|&c| c < from).unwrap_or(last))
    } else {
        Some(cands.iter().copied().find(|&c| c > from).unwrap_or(first))
    }
}

impl Spectator {
    /// This frame's follow choice from the spectator's buttons and the sim. `following`: the view
    /// is in (or flying into) first person.
    pub fn update(&mut self, sim: &Sim, inp: &ClientIn, field_computer: Key, following: bool) {
        let allowed = inp.input_allowed != 0;
        let pressed = |k: Key| allowed && inp.keys.is_down(k) && !self.prev_keys.is_down(k);
        let (prev_player, next_player) = (pressed(Key::MouseLeft), pressed(Key::MouseRight));
        let toggle = pressed(field_computer) || (allowed && inp.mouse_buttons & mouse_buttons::MIDDLE != 0 && self.prev_buttons & mouse_buttons::MIDDLE == 0);
        self.prev_keys = inp.keys;
        self.prev_buttons = inp.mouse_buttons;

        // The engine's observed seat (Zero Hour's observer list): a pick there is followed when he
        // is in first person, and watched overhead otherwise. The observer's own seat (a replay's
        // observer, an observer slot, a defeated player) has no General and changes nothing.
        if self.observed != Some(inp.local_player) {
            self.observed = Some(inp.local_player);
            if let Some(p) = sim.player(inp.local_player) {
                self.last = Some(inp.local_player);
                self.overhead = !followable(p);
            }
        }
        if toggle {
            self.overhead = following;
        }
        let cands = candidates(sim);
        self.target = if self.overhead {
            None
        } else {
            let step = next_player as i32 - prev_player as i32;
            let keep = self.last.filter(|t| cands.contains(t));
            if step != 0 && following {
                cycle(&cands, self.last, step)
            } else {
                // The followed General is gone (dead, overhead): the next one, as BO2 moves on.
                keep.or_else(|| cycle(&cands, self.last, 1))
            }
        };
        if self.target.is_some() {
            self.last = self.target;
        }
    }
}

impl Client {
    /// Become (or stop being) a spectating client. Becoming one drops everything the local
    /// player's own client was doing for him: the class screen, latched presses, the input clock.
    pub(super) fn set_spectating(&mut self, spectating: bool) {
        match (spectating, self.spectator.is_some()) {
            (true, false) => {
                self.spectator = Some(Spectator::default());
                self.class_pick_forced = false;
                self.class_pick_declined = false;
                self.hud.class_pick_seen = false;
                self.latch = 0;
                self.last_sent_frame = None;
                // Nothing of the own player's is in flight any more: the prediction's inputs and
                // its presented view go (the view shown is the followed General's, as the sim has
                // him).
                self.predict.reset();
            }
            (false, true) => {
                self.spectator = None;
                // Back to playing (a replay ends, a seat comes back): the own client starts from
                // the overhead view, and enters first person again from the sim's record.
                if !matches!(self.view, ViewMode::Rts) {
                    self.view = ViewMode::Rts;
                }
            }
            _ => {}
        }
    }

    /// The spectator's follow choice and the view moves it asks for, before the frame's view runs:
    /// fly out when nobody is followed, cut to another General while following one. Returns the
    /// seat this frame presents (-1: none, the overhead view). On the frame the local player
    /// becomes a spectator (`became`) the view is still his own (his General's death plays out:
    /// the banner and the fly-out), so the choice is made but nothing moves and nobody is
    /// presented yet.
    pub(super) fn spectate(&mut self, sim: &Sim, inp: &ClientIn, became: bool) -> i32 {
        let following = matches!(self.view, ViewMode::Fps | ViewMode::Entering { .. });
        let field_computer = self.cfg.bindings.field_computer;
        let Some(s) = self.spectator.as_mut() else { return inp.local_player };
        s.update(sim, inp, field_computer, following);
        if became {
            return -1;
        }
        let target = s.target.and_then(|t| sim.player(t).map(|p| (t, p)));
        match (following, target) {
            (true, None) => {
                // Overhead: from where the eyes are now, centred on him (or where he fell).
                let at = self.eye_curr - crate::math::Vec3::new(0.0, 0.0, 10.0);
                self.begin_leaving(inp, at);
            }
            (true, Some((_, p))) if p.general != self.general => self.cut_to(p),
            _ => {}
        }
        match self.view {
            // Still flying out: nobody is presented until the view is overhead again.
            ViewMode::Leaving { .. } => -1,
            _ => target.map_or(-1, |(t, _)| t),
        }
    }

    /// A cut to another General while following (BO2's spectator switches without a move): his
    /// eyes, his aim and a fresh viewmodel at once, no interpolation from the last one.
    fn cut_to(&mut self, p: &PlayerSim) {
        self.general = p.general;
        self.vm = Default::default();
        self.vm_motion = Default::default();
        self.vm_kick = None;
        self.cues.reset();
        self.snap_follow(p);
    }

    /// Start the interpolation over at `p`'s state (no blend from someone else's).
    pub(super) fn snap_follow(&mut self, p: &PlayerSim) {
        let eye = p.body.eye();
        self.eye_prev = eye;
        self.eye_curr = eye;
        self.aim_prev = (p.input.yaw, p.input.pitch);
        self.aim_curr = self.aim_prev;
        let (yaw, pitch) = p.aim_offset();
        self.kick_prev = (yaw, pitch, p.view_kick.roll());
        self.kick_curr = self.kick_prev;
        self.yaw = p.input.yaw;
        self.pitch = p.input.pitch;
    }

    /// The followed General's view angles this frame: the input angles the sim stepped on (what
    /// every peer replayed), interpolated across the logic frame like the eye.
    pub(super) fn follow_aim(&mut self, alpha: f32) {
        self.yaw = crate::math::lerp_angle(self.aim_prev.0, self.aim_curr.0, alpha);
        self.pitch = self.aim_prev.1 + (self.aim_curr.1 - self.aim_prev.1) * alpha;
    }

    /// A line for the game log when the spectator's view changes hands, and once a second while it
    /// follows: the view's angles beside the sim's, and the previous logic frame's yaw the view
    /// interpolates from (the in-game replay check reads them: above 30 Hz the view sits between the
    /// two, so it may trail the sim's aim by up to one frame's turn).
    pub fn spectator_log(&mut self, sim: &Sim, logic_frame: u32) -> Option<String> {
        let following = self.is_following();
        let (yaw, pitch, general, prev_yaw) = (self.yaw, self.pitch, self.general, self.aim_prev.0);
        let s = self.spectator.as_mut()?;
        let target = s.target.filter(|_| following);
        let changed = s.reported != Some(target);
        let due = following && logic_frame >= s.reported_frame + 30;
        if !changed && !due {
            return None;
        }
        s.reported = Some(target);
        s.reported_frame = logic_frame;
        Some(match target.and_then(|t| sim.player(t).map(|p| (t, p))) {
            Some((t, p)) => format!(
                "spectator: {} seat {t}'s General {general}: view yaw {yaw:.3} pitch {pitch:.3}, sim aim yaw {:.3} pitch {:.3}, from yaw {prev_yaw:.3}",
                if changed { "following" } else { "still following" },
                p.input.yaw,
                p.input.pitch
            ),
            None => "spectator: overhead".to_string(),
        })
    }

    /// The spectator is following a General in first person this frame.
    pub fn is_following(&self) -> bool {
        self.spectator.as_ref().is_some_and(|s| s.target.is_some()) && matches!(self.view, ViewMode::Fps)
    }
}

#[cfg(test)]
#[path = "spectate_tests.rs"]
mod tests;
