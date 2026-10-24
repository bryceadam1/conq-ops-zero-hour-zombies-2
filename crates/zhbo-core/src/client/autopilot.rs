//! The in-game test harness's scripted input (refactor plan 4.3): what the autopilot does frame by
//! frame, behind the `autopilot` feature (enabled by default; release builds can strip it). The
//! C++ bridge's autopilot (`ZhboAutopilot.cpp`) owns the scene setup — the General, its enemies
//! and equipment; the time-window scripts here own the *input*, so the two sides are documented
//! together in docs/TESTING.md.

use crate::input::{Key, KeySet};
use super::Bindings;

/// One classpick script action: a key (held for a frame) or the re-open one-shot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClasspickAction {
    Press(Key),
    Reopen,
}

/// The number key for the profile's first custom class, from the live pick list (never a
/// hard-coded index: the match list is the set's custom slots, then the default classes,
/// `changeclass.lua`). The standard and equipment runs seed a custom in the profile; with none,
/// fall back to the first default class (an untouched slot is not pickable in a match), so a run
/// still reaches first person.
fn custom_class_key() -> Key {
    let rows = crate::classui::pick_rows(false);
    rows.iter()
        .position(|r| matches!(r, crate::classui::PickRow::Custom(0)))
        .or_else(|| rows.iter().position(|r| matches!(r, crate::classui::PickRow::Preset(_))))
        .and_then(crate::classui::pick_key)
        .unwrap_or(Key::Key1)
}

/// Scripted input for in-game verification (`ZHBO_AUTOPILOT=1`): walks, turns, aims and fires,
/// changes stance, jumps, glances at the map, swaps to the pistol and fires it aimed, swaps back, knifes,
/// cooks and throws a frag, throws a concussion, fires the rifle's grenade launcher, then opens the
/// field computer. Times are seconds in first person.
///
/// A General spawns unarmed with the class pick up, so every script that needs a gun presses a
/// class key first: the standard run picks the first custom class the profile holds — the autotest
/// seeds one, the AN-94 with its under-barrel grenade launcher, because BO2's premades carry no
/// attachments and the script's alt-mode step has to toggle something — and falls back to the first
/// default class in the match list's order (SMG) when the profile has none. The build run picks the
/// first preset, and the
/// equipment run picks the C4/tactical class the autotest seeds in the profile.
///
/// `ZHBO_AUTOPILOT=equipment` runs [`Autopilot::equipment_input`] instead, for a loadout with C4 and
/// a Trophy System or Black Hat; `ZHBO_AUTOPILOT=build` runs [`Autopilot::build_input`].
/// `ZHBO_AUTOPILOT=classpick` runs [`Autopilot::classpick_input`]: the profile is seeded with a
/// custom class before launch; in first person it picks that class from the spawn screen by its row
/// key, leaves via the field computer, and on the next take-control forces the screen open and
/// Escapes it — exercising the pick-only in-match flow end to end.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Autopilot {
    /// Seconds in first person (logic ticks, set by the client from the sim's frame counter).
    pub t: f32,
    /// The logic frame first person was entered at; `t` counts from there, immune to render hitches.
    pub base_frame: u32,
    pub equipment: bool,
    pub build: bool,
    /// The create-a-class run: pick the seeded custom, then Escape a second offer.
    pub classpick: bool,
    /// The scope-overlay run: the AS50 in hand, hold ADS past the hand-off and report the log line.
    pub scope: bool,
    /// The classpick script's next action and whether the previous key still needs a release frame
    /// (a render hitch may jump `t` past several windows; one action per frame keeps every edge).
    pub(crate) classpick_step: usize,
    pub(crate) classpick_release: bool,
    /// After the field-computer action the script waits for the next first-person session (the
    /// clock resets on take-control), so the Escape phase cannot fire while still leaving.
    pub(crate) classpick_wait_reset: bool,
    /// The previous `t`, to notice the reset between first-person sessions.
    pub(crate) classpick_last_t: f32,
}

/// What the autopilot does in one frame: keys held, mouse movement (counts: x, y) and wheel notches.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scripted {
    pub keys: KeySet,
    pub dx: f32,
    pub dy: f32,
    pub wheel: f32,
    /// One-shot for the host: force the class pick screen open (the classpick run's re-pick).
    pub pick_force: i32,
    /// The class row key to press this frame *if the spawn pick is actually up*. The client gates
    /// it (`class_pick_wanted`) so a script that runs again after take-control on an already
    /// classed General (the RTS run's first-person phase) does not turn the pick key into a weapon
    /// swap.
    pub class_pick: Option<Key>,
}

impl Autopilot {
    /// What it does at time `t`.
    pub fn input_at(&mut self, b: &Bindings) -> Scripted {
        if self.scope {
            return self.scope_input(b);
        }
        if self.equipment {
            let pick = (0.3..0.4).contains(&self.t).then(custom_class_key);
            return Scripted { keys: self.equipment_input(b), class_pick: pick, ..Default::default() };
        }
        if self.build {
            return self.build_input(b);
        }
        if self.classpick {
            return self.classpick_input(b);
        }
        let t = self.t;
        let mut keys = KeySet::default();
        let mut dx = 0.0;
        // The General spawns unarmed and the spawn pick is up: pick the seeded custom before the
        // combat script, all of which assumes a gun (the autotest's seed is the AN-94 with its
        // grenade launcher; no premade has one, so the alt-mode step needs it). Without a custom
        // the first preset is the pick, and a run that reaches first person unseeded still works.
        // The client only sends it while the screen is actually open, so a re-entry on a classed
        // General never turns it into a swap.
        let pick = (0.3..0.4).contains(&t).then(custom_class_key);
        if (1.0..3.0).contains(&t) {
            keys.set_down(b.forward, true);
        }
        if (2.0..2.1).contains(&t) || (2.8..2.9).contains(&t) {
            keys.set_down(b.crouch, true);
        }
        if (3.0..3.5).contains(&t) {
            dx = 12.0;
        }
        if (3.5..4.0).contains(&t) {
            dx = -12.0;
        }
        if (3.6..3.7).contains(&t) {
            keys.set_down(b.jump, true);
        }
        if (4.0..4.4).contains(&t) {
            keys.set_down(b.map, true);
        }
        // Aim the rifle, then (after the swap) the pistol.
        if (4.5..7.5).contains(&t) || (7.6..9.9).contains(&t) {
            keys.set_down(b.ads, true);
        }
        if (5.0..7.0).contains(&t) {
            keys.set_down(b.fire, true);
        }
        if (7.2..7.3).contains(&t) || (10.0..10.1).contains(&t) {
            keys.set_down(b.swap, true);
        }
        // After the pistol's first raise (BO2's longer first draw).
        if (9.3..9.4).contains(&t) || (9.6..9.7).contains(&t) {
            keys.set_down(b.fire, true);
        }
        if (10.7..10.8).contains(&t) {
            keys.set_down(b.melee, true);
        }
        if (11.4..11.8).contains(&t) {
            keys.set_down(b.lethal, true);
        }
        // After the frag has left the hand and the rifle is coming back up.
        if (12.7..12.8).contains(&t) {
            keys.set_down(b.tactical, true);
        }
        // The rifle's grenade launcher: switch to it, fire, switch back.
        if (13.9..14.0).contains(&t) || (15.4..15.5).contains(&t) {
            keys.set_down(b.alt_mode, true);
        }
        if (14.8..14.9).contains(&t) {
            keys.set_down(b.fire, true);
        }
        if (16.1..16.2).contains(&t) {
            keys.set_down(b.field_computer, true);
        }
        Scripted { keys, dx, class_pick: pick, ..Default::default() }
    }

    /// Building in first person (facing open ground): the hammer up, look down (the structure goes
    /// as near as it can), turn it a notch, place it (the hammer goes away), step up to it, and
    /// hold the use key until it is built. The test then damages it: hold the use key again to
    /// repair it, and open the field computer to go back to the overhead view.
    pub fn build_input(&self, b: &Bindings) -> Scripted {
        let t = self.t;
        let mut keys = KeySet::default();
        // The spawn select is up on the unarmed spawn: pick the first default class so the
        // hammer's own input is not eaten by the screen (the hammer itself works unarmed).
        let pick = (0.3..0.4).contains(&t).then(custom_class_key);
        let dy = if (3.0..3.4).contains(&t) { 120.0 } else { 0.0 };
        let wheel = if (3.6..3.65).contains(&t) { 1.0 } else { 0.0 };
        if (2.6..2.7).contains(&t) {
            keys.set_down(b.hammer, true);
        }
        if (4.0..4.1).contains(&t) {
            keys.set_down(b.fire, true);
        }
        if (4.6..5.2).contains(&t) {
            keys.set_down(b.forward, true);
        }
        if (5.6..32.0).contains(&t) || (34.0..56.0).contains(&t) || (58.0..58.1).contains(&t) {
            keys.set_down(b.field_computer, true);
        }
        Scripted { keys, dy, wheel, class_pick: pick, ..Default::default() }
    }

    /// The create-a-class run (`ZHBO_AUTOPILOT=classpick`). In a match the screen is pick-only:
    /// the seeded "AB" custom (written to the profile before launch) leads the list
    /// (`changeclass.lua`: the set's custom slots, then the defaults), so the script presses its
    /// number key and the sim assumes it; no editor and no profile write.
    /// It then opens the field computer to return overhead, and on the *next* take-control forces
    /// the screen open (the command-bar path) and Escapes it, proving first-person play continues.
    ///
    /// The schedule is a list of actions rather than time windows: a render hitch can jump the
    /// logic clock past a window (the macOS runs do), so each due action is emitted on a frame of
    /// its own and followed by a release frame, which preserves the fresh-press edge. Actions fire
    /// no earlier than their times. The clock resets with each take-control, so after the
    /// field-computer action the script waits for the reset before its second phase.
    pub fn classpick_input(&mut self, b: &Bindings) -> Scripted {
        use ClasspickAction::*;
        let custom_key = custom_class_key();
        // Phase one (this first-person session): pick the seeded class, then leave overhead.
        // Phase two (the next take-control, t reset): force the screen open and Escape it.
        let schedule: [(f32, ClasspickAction); 4] = [
            (0.3, Press(custom_key)),
            (1.6, Press(b.field_computer)),
            (0.3, Reopen),
            (0.9, Press(Key::Escape)),
        ];
        // A held key gets one frame on its own, then one released frame, so the next action's
        // fresh-press edge is never merged with it.
        if self.classpick_release {
            self.classpick_release = false;
            return Scripted::default();
        }
        // New first-person session? The client's `t` restarts from the take-control frame.
        if self.t + 0.05 < self.classpick_last_t {
            self.classpick_wait_reset = false;
        }
        self.classpick_last_t = self.t;
        if self.classpick_wait_reset {
            return Scripted::default();
        }
        let mut scripted = Scripted::default();
        if let Some((when, action)) = schedule.get(self.classpick_step).copied() {
            if self.t >= when {
                match action {
                    Press(k) => scripted.keys.set_down(k, true),
                    Reopen => scripted.pick_force = 1,
                }
                // The field computer hands the General back: the next phase is a new session.
                if action == Press(b.field_computer) {
                    self.classpick_wait_reset = true;
                }
                self.classpick_step += 1;
                self.classpick_release = true;
            }
        }
        // After Escape, walk forward for a couple of seconds: the autotest checks the position
        // changes, proving first-person play resumed (a still-open screen would eat the input).
        if self.classpick_step >= schedule.len() && (1.2..3.0).contains(&self.t) {
            scripted.keys.set_down(b.forward, true);
        }
        scripted
    }

    /// The scope-overlay run (`ZHBO_AUTOPILOT=scope`): pick the seeded AS50 class and hold ADS from
    /// 0.5 s so the AS50's 0.40 s transition reaches full ADS; the native draw then logs the physical
    /// scope's suppression and the screen overlay's hand-off. The autotest waits for that line.
    pub fn scope_input(&self, b: &Bindings) -> Scripted {
        let mut keys = KeySet::default();
        let pick = (0.3..0.4).contains(&self.t).then(custom_class_key);
        if (0.5..3.0).contains(&self.t) {
            keys.set_down(b.ads, true);
        }
        Scripted { keys, class_pick: pick, ..Default::default() }
    }

    /// The equipment run: throw the lethal (C4), hold the tactical up for four seconds (the Black
    /// Hat on the drone the test puts in front; a Trophy System goes down when it is let go),
    /// double-tap reload to set the C4 off, fire a burst and reload, wait while enemy rockets come
    /// in, then open the field computer.
    pub fn equipment_input(&self, b: &Bindings) -> KeySet {
        let t = self.t;
        let mut keys = KeySet::default();
        if (0.5..1.0).contains(&t) {
            keys.set_down(b.lethal, true);
        }
        if (2.0..6.0).contains(&t) {
            keys.set_down(b.tactical, true);
        }
        if (7.0..7.08).contains(&t) || (7.2..7.28).contains(&t) {
            keys.set_down(b.reload, true);
        }
        // A burst and a reload (the reload animation's magazines).
        if (9.0..10.0).contains(&t) {
            keys.set_down(b.fire, true);
        }
        if (10.3..10.4).contains(&t) {
            keys.set_down(b.reload, true);
        }
        if (20.0..20.1).contains(&t) {
            keys.set_down(b.field_computer, true);
        }
        keys
    }
}
