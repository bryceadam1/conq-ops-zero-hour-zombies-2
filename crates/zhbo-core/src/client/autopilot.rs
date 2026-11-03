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

/// The short fight's bound: the shooter fires at the target until he is down or this many seconds
/// into the fight have passed, reloading every [`MP_RELOAD_EVERY`].
const MP_KILL_TIME: f32 = 30.0;
const MP_RELOAD_EVERY: f32 = 4.5;

/// How far below the aim at his chest the long run throws its tactical (radians).
const MP_TACTICAL_DIP: f32 = 0.2;

/// The number key of the match list's second default class (the long multiplayer run's mid-life
/// change: the joiner wears the first).
fn second_preset_key() -> Key {
    crate::classui::pick_rows(false)
        .iter()
        .enumerate()
        .filter(|(_, r)| matches!(r, crate::classui::PickRow::Preset(_)))
        .nth(1)
        .and_then(|(i, _)| crate::classui::pick_key(i))
        .unwrap_or(Key::Key2)
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
/// custom class before launch; the first take-control Escapes the spawn screen (declining the spawn:
/// the General goes back overhead unclassed), the next picks that class by its row key and leaves
/// via the field computer, and the one after forces the screen open and Escapes it — exercising the
/// pick-only in-match flow end to end.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Autopilot {
    /// Seconds in first person (logic ticks, set by the client from the sim's frame counter).
    pub t: f32,
    /// The logic frame first person was entered at; `t` counts from there, immune to render hitches.
    pub base_frame: u32,
    pub equipment: bool,
    pub build: bool,
    /// The create-a-class run: decline the first spawn, pick the seeded custom, then Escape a
    /// forced re-open.
    pub classpick: bool,
    /// The scope-overlay run: the AS50 in hand, hold ADS past the hand-off and report the log line.
    pub scope: bool,
    /// The classpick script's next action and whether the previous key still needs a release frame
    /// (a render hitch may jump `t` past several windows; one action per frame keeps every edge).
    pub(crate) classpick_step: usize,
    pub(crate) classpick_release: bool,
    /// After an action that leaves first person the script waits for the next first-person session
    /// (the clock resets on take-control), so the next phase cannot fire while still leaving.
    pub(crate) classpick_wait_reset: bool,
    /// The previous `t`, to notice the reset between first-person sessions.
    pub(crate) classpick_last_t: f32,
    /// The two-instance multiplayer run (`ZHBO_AUTOPILOT=mp`, [`Autopilot::mp_input`]).
    pub mp: bool,
    /// Its part: the shooter fires the long burst, the other the short one
    /// (`ZHBO_AUTOPILOT_ROLE`, [`crate::config::MpRole`]).
    pub mp_shooter: bool,
    /// How many first-person lives the run plays (`ZHBO_AUTOPILOT_ROUNDS`; 1 the short run). More
    /// is the long run: the target declines his first spawn, and every life after the first fight
    /// plays [`Autopilot::mp_round`].
    pub mp_rounds: u32,
    /// The long run's class change has been made (once, by the target), and its spawn declined.
    pub(crate) mp_changed_class: bool,
    pub(crate) mp_declined: bool,
    /// When this session's fight ended and the field computer went down (the short fight).
    pub(crate) mp_leave_at: Option<f32>,
    /// The short fight's target went down (the shooter's burst stops for good).
    pub(crate) mp_target_down: bool,
    /// First-person sessions so far (the client counts each take-control).
    pub(crate) session: u32,
    pub(crate) in_fps: bool,
    /// The view angles (yaw, pitch) at the other player's General this frame, when he is in first
    /// person too; set by the client from the sim before the script runs.
    pub(crate) mp_aim: Option<(f32, f32)>,
    /// When the fight started this session: the first moment the other General was there to aim
    /// at (the two machines take control a few seconds apart).
    pub(crate) mp_fight: Option<f32>,
    /// What his own General holds in third person, last logged (general, viewmodel, camo).
    pub(crate) mp_holds: Option<(u32, i32, Option<String>)>,
    /// Log lines for `ConquerOps.log` (drained by the FFI layer after the frame).
    pub log: Vec<String>,
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
    /// Turn the view to these angles (yaw, pitch radians) this frame: the multiplayer run aims at
    /// the other General. The look is local; the angles travel in the input message as always.
    pub look: Option<(f32, f32)>,
}

impl Autopilot {
    /// The same run, its state cleared (a new game).
    pub fn fresh(&self) -> Autopilot {
        Autopilot { equipment: self.equipment, build: self.build, classpick: self.classpick, scope: self.scope, mp: self.mp, mp_shooter: self.mp_shooter, mp_rounds: self.mp_rounds, ..Default::default() }
    }

    /// What it does at time `t`.
    pub fn input_at(&mut self, b: &Bindings) -> Scripted {
        if self.mp {
            return self.mp_input(b);
        }
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
        // The rifle burst sweeps the aim down and back up across the rebels ahead: BO2's hit boxes
        // (`hitloc`, a 72 in soldier) are the body's own, so a level shot from the eye skims the
        // heads and passes over them on lower ground; the sweep crosses their bodies wherever the
        // terrain puts them (about 6 degrees at the default sensitivity and the ADS zoom).
        let mut dy = 0.0;
        if (5.0..6.0).contains(&t) {
            dy = 3.0;
        }
        if (6.0..7.0).contains(&t) {
            dy = -3.0;
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
        Scripted { keys, dx, dy, class_pick: pick, ..Default::default() }
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
    /// the seeded custom ("Custom 2", written to the profile before launch) leads the list
    /// (`changeclass.lua`: the set's custom slots, then the defaults).
    /// Phase one Escapes the first spawn screen without picking: BO2 never spawns a classless
    /// player, so the General goes back overhead (the host takes control again).
    /// Phase two presses the custom's number key and the sim assumes it (no editor and no profile
    /// write), then opens the field computer to return overhead. Phase three, on the next
    /// take-control, forces the screen open (the command-bar path) and Escapes it, proving
    /// first-person play continues for a classed General.
    ///
    /// The schedule is a list of actions rather than time windows: a render hitch can jump the
    /// logic clock past a window (the macOS runs do), so each due action is emitted on a frame of
    /// its own and followed by a release frame, which preserves the fresh-press edge. Actions fire
    /// no earlier than their times. The clock resets with each take-control, so after the
    /// field-computer action the script waits for the reset before its second phase.
    pub fn classpick_input(&mut self, b: &Bindings) -> Scripted {
        use ClasspickAction::*;
        let custom_key = custom_class_key();
        // Each action, its earliest time in its session, and whether it ends the session (the
        // General leaves first person, so the next action waits for the next take-control).
        let schedule: [(f32, ClasspickAction, bool); 5] = [
            // Phase one: decline the spawn.
            (0.6, Press(Key::Escape), true),
            // Phase two: pick the seeded class, then leave overhead.
            (0.3, Press(custom_key), false),
            (1.6, Press(b.field_computer), true),
            // Phase three: force the screen open and Escape it.
            (0.3, Reopen, false),
            (0.9, Press(Key::Escape), false),
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
        if let Some((when, action, ends)) = schedule.get(self.classpick_step).copied() {
            if self.t >= when {
                match action {
                    Press(k) => scripted.keys.set_down(k, true),
                    Reopen => scripted.pick_force = 1,
                }
                // The General goes back overhead: the next phase is a new session.
                if ends {
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

    /// The two-instance multiplayer run (`ZHBO_AUTOPILOT=mp`): one script per machine, each for its
    /// own player only; the C++ side builds his General at the Command Center, walks him to the
    /// meeting point and takes control. The first session picks the class (the seeded custom
    /// when the profile has one, else the first preset), steps forward and sideways, then — once
    /// the other General is in first person to aim at — swaps to the sidearm and fires it, swaps
    /// back, throws the lethal wide of him (so the throw is covered without settling the fight),
    /// fires the primary at him (the shooter a long aimed burst, the target a short one from the
    /// hip), and opens the field computer. A General who died comes back from the Command Center:
    /// his second session must arrive with the class (no pick screen), walks, and leaves overhead.
    /// Every time is measured from the take-control; the fight's from its start.
    pub fn mp_input(&mut self, b: &Bindings) -> Scripted {
        let t = self.t;
        let mut keys = KeySet::default();
        // Only pressed while the spawn's pick is really up: a respawned General has his class.
        let long = self.mp_rounds > 1;
        // The long run's target declines his first spawn: Escape on the spawn's class screen sends
        // the classless General back overhead (BO2 never spawns a player without a class), and
        // the bridge takes control of him again.
        if long && !self.mp_shooter && self.session == 1 {
            let mut keys = KeySet::default();
            if (0.6..0.7).contains(&t) {
                if !self.mp_declined {
                    self.mp_declined = true;
                    self.log.push("autopilot mp: declining the spawn (Escape on the class screen)".to_string());
                }
                keys.set_down(Key::Escape, true);
            }
            return Scripted { keys, ..Default::default() };
        }
        // Which life this is: the declined session does not count.
        let life = if long && !self.mp_shooter { self.session - 1 } else { self.session };
        if long && life >= 2 {
            return self.mp_round(b, life);
        }
        let pick = (0.3..0.4).contains(&t).then(custom_class_key);
        if life >= 2 {
            if (0.6..1.6).contains(&t) {
                keys.set_down(b.forward, true);
            }
            if (2.4..2.5).contains(&t) {
                keys.set_down(b.field_computer, true);
            }
            return Scripted { keys, class_pick: pick, ..Default::default() };
        }
        if (0.6..1.2).contains(&t) {
            keys.set_down(b.forward, true);
        }
        if (1.2..1.6).contains(&t) {
            keys.set_down(b.right, true);
        }
        let fight = match self.mp_fight {
            Some(start) => start,
            // Wait for him (at most half a minute: a lone run still exercises the rest).
            None if t >= 1.8 && (self.mp_aim.is_some() || t >= 30.0) => {
                self.mp_fight = Some(t);
                self.log.push(format!("autopilot mp: the fight starts ({})", if self.mp_aim.is_some() { "the other General is in first person" } else { "nobody to aim at" }));
                t
            }
            None => return Scripted { keys, class_pick: pick, ..Default::default() },
        };
        let c = t - fight;
        let mut look = self.mp_aim;
        // The sidearm: up, two shots once its first raise is done (a little wide of him: the
        // fight is the primaries'), back to the primary.
        if (0.2..0.3).contains(&c) || (2.6..2.7).contains(&c) {
            keys.set_down(b.swap, true);
        }
        if (1.6..2.4).contains(&c) {
            look = self.mp_aim.map(|(yaw, pitch)| (yaw + 0.2, pitch));
        }
        if (1.8..1.9).contains(&c) || (2.1..2.2).contains(&c) {
            keys.set_down(b.fire, true);
        }
        // The lethal, cooked and thrown a quarter turn off the line to him.
        if (3.2..4.2).contains(&c) {
            look = self.mp_aim.map(|(yaw, pitch)| (yaw + std::f32::consts::FRAC_PI_2, pitch.max(0.2)));
        }
        if (3.4..3.8).contains(&c) {
            keys.set_down(b.lethal, true);
        }
        // The primary at him: the target's short burst from the hip first, then the shooter's
        // long aimed one. The shooter's fight starts a couple of seconds before the target's (he
        // waited at the meeting point; the target's clock starts after his own entry), so the
        // windows are far enough apart that the target fires before he is killed.
        // The shooter fires until the target is down (no first-person General left to aim at),
        // reloading as he goes, for at most MP_KILL_TIME; the target stays in the fight until then.
        // Once he is down the fight is over: the next General he brings is left alone.
        if self.mp_shooter && c >= 9.0 && self.mp_aim.is_none() {
            self.mp_target_down = true;
        }
        let target_up = self.mp_aim.is_some() && !self.mp_target_down;
        if self.mp_shooter {
            let firing = (8.0..MP_KILL_TIME).contains(&c) && (c < 9.0 || target_up);
            if (7.5..MP_KILL_TIME).contains(&c) && (c < 9.0 || target_up) {
                keys.set_down(b.ads, true);
            }
            // A reload every few seconds of the burst (the magazine runs dry in about 2.5 s).
            let reloading = c >= 8.0 && ((c - 8.0) % MP_RELOAD_EVERY) < 0.1 && c > 9.0;
            if reloading {
                keys.set_down(b.reload, true);
            } else if firing {
                keys.set_down(b.fire, true);
            }
        } else if (5.0..5.3).contains(&c) {
            keys.set_down(b.fire, true);
        }
        // Out by the field computer once the fight is over: the shooter when the target is down
        // (or the time is up), the target only if he outlived it.
        let done = (self.mp_shooter && !target_up && c >= 15.0) || c >= MP_KILL_TIME + 2.0;
        if done {
            let at = c;
            let start = *self.mp_leave_at.get_or_insert(at);
            if (start..start + 0.1).contains(&c) {
                keys.set_down(b.field_computer, true);
            }
        }
        Scripted { keys, class_pick: pick, look, ..Default::default() }
    }

    /// A life of the long multiplayer run after the first fight: at the meeting point again (the
    /// bridge walks every General there), each throws his tactical at the other (the concussion:
    /// a shellshock on both machines' sims), fires a hip burst and an aimed one at him, reloads,
    /// knifes, and leaves by the field computer, or dies; the shooter ends with a long aimed burst.
    /// On his second such life the target changes class mid-life once his throw has made him a
    /// combatant (BO2's `MP_CHANGE_CLASS_NEXT_SPAWN`) and stays for the bursts, so his next
    /// General wears it.
    pub fn mp_round(&mut self, b: &Bindings, life: u32) -> Scripted {
        let t = self.t;
        let mut keys = KeySet::default();
        let fight = match self.mp_fight {
            Some(start) => start,
            None if t >= 1.0 && (self.mp_aim.is_some() || t >= 20.0) => {
                self.mp_fight = Some(t);
                self.log.push(format!("autopilot mp: round {life} starts ({})", if self.mp_aim.is_some() { "the other General is in first person" } else { "nobody to aim at" }));
                t
            }
            None => return Scripted::default(),
        };
        let c = t - fight;
        let mut look = self.mp_aim;
        let mut pick_force = 0;
        let mut class_pick = None;
        // The tactical, thrown at his feet (the throw leaves above the crosshair and flies far).
        if (0.2..1.4).contains(&c) {
            look = self.mp_aim.map(|(yaw, pitch)| (yaw, pitch - MP_TACTICAL_DIP));
        }
        if (0.4..0.8).contains(&c) {
            keys.set_down(b.tactical, true);
        }
        if (2.5..3.2).contains(&c) {
            keys.set_down(b.fire, true);
        }
        if (4.0..7.0).contains(&c) {
            keys.set_down(b.ads, true);
        }
        if (4.5..6.5).contains(&c) {
            keys.set_down(b.fire, true);
        }
        if (7.2..7.3).contains(&c) {
            keys.set_down(b.reload, true);
        }
        if (9.5..9.6).contains(&c) {
            keys.set_down(b.melee, true);
        }
        // The mid-life class change, once the throw has made him a combatant: the screen forced
        // open, the next default class picked.
        if !self.mp_shooter && life == 3 && !self.mp_changed_class {
            if (1.3..1.4).contains(&c) {
                pick_force = 1;
            }
            if (1.8..1.9).contains(&c) {
                class_pick = Some(second_preset_key());
                self.mp_changed_class = true;
                self.log.push("autopilot mp: changing class mid-life after combat".to_string());
            }
        }
        // The shooter ends each round with a long aimed burst at him; the target stays for it in
        // the life he changed class in (the new class comes in with his next General).
        let changing = !self.mp_shooter && life == 3;
        if self.mp_shooter && (14.0..19.0).contains(&c) {
            keys.set_down(b.ads, true);
            if c >= 14.5 {
                keys.set_down(b.fire, true);
            }
        }
        let leave_at = if self.mp_shooter { 21.0 } else { 13.0 };
        if !changing && (leave_at..leave_at + 0.1).contains(&c) {
            keys.set_down(b.field_computer, true);
        }
        Scripted { keys, class_pick, pick_force, look, ..Default::default() }
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
