//! The in-game test harness's scripted input (refactor plan 4.3): what the autopilot does frame by
//! frame, behind the `autopilot` feature (enabled by default; release builds can strip it). The
//! C++ bridge's autopilot (`ZhboAutopilot.cpp`) owns the scene setup — the General, its enemies
//! and equipment; the time-window scripts here own the *input*, so the two sides are documented
//! together in docs/TESTING.md.

use crate::input::{Key, KeySet};
use super::Bindings;

/// How long a create-a-class pulse holds a key: long enough for one logic tick (30 Hz) to see the
/// press and the next to see the release, short enough that the next pulse is a fresh edge.
const PULSE: f32 = 0.1;

/// Scripted input for in-game verification (`ZHBO_AUTOPILOT=1`): walks, turns, aims and fires,
/// changes stance, jumps, glances at the map, swaps to the pistol and fires it aimed, swaps back, knifes,
/// cooks and throws a frag, throws a concussion, fires the rifle's grenade launcher, then opens the
/// field computer. Times are seconds in first person.
///
/// `ZHBO_AUTOPILOT=equipment` runs [`Autopilot::equipment_input`] instead, for a loadout with C4 and
/// a Trophy System or Black Hat; `ZHBO_AUTOPILOT=build` runs [`Autopilot::build_input`].
/// `ZHBO_AUTOPILOT=classpick` runs [`Autopilot::classpick_input`]: it opens the create-a-class editor
/// from the pick screen (key 6), names the class, changes the secondary, and saves — so an in-game
/// run exercises the editor and the profile write end to end.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Autopilot {
    /// Seconds in first person (logic ticks, set by the client from the sim's frame counter).
    pub t: f32,
    /// The logic frame first person was entered at; `t` counts from there, immune to render hitches.
    pub base_frame: u32,
    pub equipment: bool,
    pub build: bool,
    /// The create-a-class run: drive the editor from the pick screen and save a named class.
    pub classpick: bool,
}

/// What the autopilot does in one frame: keys held, mouse movement (counts: x, y) and wheel notches.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scripted {
    pub keys: KeySet,
    pub dx: f32,
    pub dy: f32,
    pub wheel: f32,
}

impl Autopilot {
    /// What it does at time `t`.
    pub fn input_at(&self, b: &Bindings) -> Scripted {
        if self.equipment {
            return Scripted { keys: self.equipment_input(b), ..Default::default() };
        }
        if self.build {
            return self.build_input(b);
        }
        if self.classpick {
            return self.classpick_input();
        }
        let t = self.t;
        let mut keys = KeySet::default();
        let mut dx = 0.0;
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
        Scripted { keys, dx, ..Default::default() }
    }

    /// Building in first person (facing open ground): the hammer up, look down (the structure goes
    /// as near as it can), turn it a notch, place it (the hammer goes away), step up to it, and
    /// hold the use key until it is built. The test then damages it: hold the use key again to
    /// repair it, and open the field computer to go back to the overhead view.
    pub fn build_input(&self, b: &Bindings) -> Scripted {
        let t = self.t;
        let mut keys = KeySet::default();
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
        Scripted { keys, dy, wheel, ..Default::default() }
    }

    /// The create-a-class run (`ZHBO_AUTOPILOT=classpick`): on the pick screen, `6` opens the editor;
    /// then the script names the class, changes the secondary weapon, and walks the row to Save and
    /// confirms — so an in-game run captures the editor's drawing and the profile write end to end.
    ///
    /// The editor is edge-driven (a key acts on its *fresh press*), so the pulses below hold each key
    /// for a short window and release it, giving exactly one edge per step. The row walk is long
    /// (twelve rows), so `pulse` fires `Down` on a fixed period from `t0`.
    pub fn classpick_input(&self) -> Scripted {
        let t = self.t;
        let mut keys = KeySet::default();
        // A key held in `[start, start + width)`, giving one fresh-press edge when the window opens.
        let pulse = |keys: &mut KeySet, key: Key, start: f32| {
            if (start..start + PULSE).contains(&t) {
                keys.set_down(key, true);
            }
        };
        // 1. Open the editor from the pick screen (`6`).
        pulse(&mut keys, Key::Key6, 0.2);
        // 2. Walk up to the Name row and name the class "AB" (two separate key edges, so the second
        //    A is a fresh press), then leave the field with Enter.
        pulse(&mut keys, Key::Up, 0.6);
        pulse(&mut keys, Key::Enter, 1.0);
        pulse(&mut keys, Key::A, 1.4);
        pulse(&mut keys, Key::B, 1.8);
        pulse(&mut keys, Key::Enter, 2.2);
        // 3. Walk down to the Secondary row (Name -> Primary -> PrimaryAtts -> Secondary: three downs).
        for i in 0..3 {
            pulse(&mut keys, Key::Down, 2.6 + i as f32 * 0.4);
        }
        // 4. Open the secondary's list, move one option down, and choose it.
        pulse(&mut keys, Key::Enter, 4.0);
        pulse(&mut keys, Key::Down, 4.4);
        pulse(&mut keys, Key::Enter, 4.8);
        // 5. Walk the rest of the way to the Save row (SecondaryAtts .. Save: eight downs) and save.
        for i in 0..8 {
            pulse(&mut keys, Key::Down, 5.2 + i as f32 * 0.4);
        }
        pulse(&mut keys, Key::Enter, 8.4);
        Scripted { keys, ..Default::default() }
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
