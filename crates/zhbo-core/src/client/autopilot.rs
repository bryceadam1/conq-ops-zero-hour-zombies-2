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

/// The number key for the profile's `n`-th custom class (0-based), from the live pick list.
fn nth_custom_class_key(n: usize) -> Option<Key> {
    let rows = crate::classui::pick_rows(false);
    rows.iter().position(|r| matches!(r, crate::classui::PickRow::Custom(i) if *i == n)).and_then(crate::classui::pick_key)
}

/// The short fight's bound: the shooter fires at the target until he is down or this many seconds
/// into the fight have passed, reloading every [`MP_RELOAD_EVERY`].
const MP_KILL_TIME: f32 = 30.0;
/// The equipment run's back-off after the C4's throw, seconds (then the tactical).
const EQ_BACK_OFF: f32 = 1.7;
const MP_RELOAD_EVERY: f32 = 4.5;

/// How long the other General must be out of the field before his return starts a long-run round
/// over (seconds): longer than a weapon swap or a sprint out of sight, shorter than a new General.
const MP_AWAY: f32 = 2.0;

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

/// One phase of the flicker run: from `from` seconds in first person, the view turned by
/// (`yaw`, `pitch`) from where it was when the phases began, nudged every other frame or held.
pub struct FlickerPhase {
    pub name: &'static str,
    pub from: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub nudge: bool,
    /// The run is over: the script lets go of the view.
    pub end: bool,
}

/// The flicker run's phases (the autotest reads their log lines, never these numbers). The first
/// waits out the raise; each still phase gives the view a frame to settle before the nudged one.
pub const FLICKER_PHASES: [FlickerPhase; 6] = [
    FlickerPhase { name: "ground, still", from: 3.0, yaw: 0.0, pitch: -0.12, nudge: false, end: false },
    FlickerPhase { name: "ground, nudged", from: 4.0, yaw: 0.0, pitch: -0.12, nudge: true, end: false },
    FlickerPhase { name: "turning", from: 6.0, yaw: std::f32::consts::FRAC_PI_2, pitch: -0.15, nudge: false, end: false },
    FlickerPhase { name: "base, still", from: 6.5, yaw: std::f32::consts::PI, pitch: -0.15, nudge: false, end: false },
    FlickerPhase { name: "base, nudged", from: 7.5, yaw: std::f32::consts::PI, pitch: -0.15, nudge: true, end: false },
    FlickerPhase { name: "done", from: 9.5, yaw: 0.0, pitch: 0.0, nudge: false, end: true },
];

/// The flicker run's nudge (radians of yaw): about a fifth of a pixel at the game's field of view
/// on a 1280-pixel window.
pub const FLICKER_NUDGE: f32 = 0.0002;

/// The key a script re-picks with on a later General's class screen: every General is picked for
/// (`sim/spawn.rs`), and the screen opens on the player's last class, so Enter takes it again in
/// one press (and wears a class changed mid-life, which is what the screen opens on).
const REPICK_KEY: Key = Key::Enter;

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
    /// The smoke-screen run: throw the seeded class's smoke grenade ahead and stand watching while
    /// the bridge's scene tests what Zero Hour's units do about it (`ZHBO_AUTOPILOT=smoke`).
    pub smoke: bool,
    /// The splash run ([`Autopilot::splash_input`]): the seeded SMAW class fires one rocket into
    /// the enemy armour group the bridge stands ahead of him.
    pub splash: bool,
    /// The weapon-drop run ([`Autopilot::drops_input`], `ZHBO_AUTOPILOT=drops`): the first General
    /// dies and drops his rifle; the next walks over it and swaps for it.
    pub drops: bool,
    /// The flicker run ([`Autopilot::flicker_input`], `ZHBO_AUTOPILOT=flicker`): no enemies; the
    /// General picks his class and stands, holding the view still and then nudging it a fraction of
    /// a pixel each frame, first facing the open ground and then the Command Center, so the
    /// autotest can diff consecutive captures of a static scene.
    pub flicker: bool,
    /// The view the client holds (yaw, pitch radians), set each frame; the flicker run holds its
    /// look relative to the view it had when the phases began.
    pub(crate) view: (f32, f32),
    pub(crate) flicker_base: Option<(f32, f32)>,
    pub(crate) flicker_phase: Option<usize>,
    /// The hip-fire run ([`Autopilot::hipfire_input`], `ZHBO_AUTOPILOT=hipfire`): taps from rest,
    /// a held string, then a running string, at the ground ahead; the bridge logs each shot's place
    /// in its cone (`ffi`'s `hip shot` lines).
    pub hipfire: bool,
    /// The drops run: where the nearest dropped weapon is from the eye (yaw, pitch, the distance
    /// across the ground), set by the client from the sim, and when the use key was pressed.
    pub(crate) drop_aim: Option<(f32, f32, f32)>,
    pub(crate) drop_used: Option<f32>,
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
    /// Its part: the shooter fires the long burst, the other a single round
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
    /// Whether the use key would take a dropped weapon now (`Sim::use_target`): the field
    /// computer's key is the use key, so over a drop it swaps instead of leaving.
    pub(crate) mp_over_drop: bool,
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
    /// The long run's round began with nobody to aim at (it starts after 20 s anyway): when the
    /// other General comes in, the round starts over for him once, so its closing burst is not
    /// fired into a General just taking the field.
    pub(crate) mp_fight_alone: bool,
    /// How many times this session's long-run round has started (each start over counts). The
    /// shooter holds his aimed bursts on every second start, so the target's kill him: at BO2's
    /// health whoever fires first settles a round, and without it the shooter's General outlived
    /// every round, his deaths left to chance (a tactical's strike, a late burst).
    pub(crate) mp_round_starts: u32,
    /// The equipment run's view of his arsenal, set by the client from the sim each frame: no
    /// offhand, raise or put-away running, and the lethals left.
    pub(crate) eq_idle: bool,
    pub(crate) eq_lethal_left: u16,
    /// When this session's lethal press began, and when the tactical's turn came (the lethal down).
    pub(crate) eq_lethal_at: Option<f32>,
    pub(crate) eq_tactical_from: Option<f32>,
    /// What his own General holds in third person, last logged (general, viewmodel, camo).
    pub(crate) mp_holds: Option<(u32, i32, Option<String>)>,
    /// The General and class his record last wore (logged on a change: every General is picked
    /// for, and the scripts check that each one got his class, and which).
    pub(crate) mp_worn: Option<(u32, String)>,
    /// His General is classed and the class's first raise is done: a round waits for it (every
    /// General is picked for, so the kit comes up a moment into each life, and an offhand pressed
    /// during the raise is refused).
    pub(crate) mp_armed: bool,
    /// When the other General left the field during this round (died, went overhead, or is back
    /// behind a new General's class screen): his return a while later starts the round over.
    pub(crate) mp_aim_lost: Option<f32>,
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
        Autopilot { equipment: self.equipment, build: self.build, classpick: self.classpick, scope: self.scope, smoke: self.smoke, splash: self.splash, drops: self.drops, flicker: self.flicker, hipfire: self.hipfire, mp: self.mp, mp_shooter: self.mp_shooter, mp_rounds: self.mp_rounds, ..Default::default() }
    }

    /// What it does at time `t`.
    pub fn input_at(&mut self, b: &Bindings) -> Scripted {
        if self.mp {
            return self.mp_input(b);
        }
        if self.scope {
            return self.scope_input(b);
        }
        if self.smoke {
            return self.smoke_input(b);
        }
        if self.splash {
            return self.splash_input(b);
        }
        if self.drops {
            return self.drops_input(b);
        }
        if self.flicker {
            return self.flicker_input();
        }
        if self.hipfire {
            return self.hipfire_input(b);
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
        // One notch: on the one logic frame at 3.6 s (a time window caught one frame or two by
        // rounding, turning it 30 or 45 degrees from run to run).
        let frame = |at: f32| (at / crate::units::LOGIC_DT).round() as i64;
        let wheel = if frame(t) == frame(3.6) { 1.0 } else { 0.0 };
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
    /// first-person play continues for a classed General (it walks after). Phase four is the next
    /// General's (the host kills this one and stands a new one in his place): every General is
    /// picked for, so his spawn screen is up, opened on the class picked last, and Enter takes it
    /// in one press; he walks after.
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
        let schedule: [(f32, ClasspickAction, bool); 6] = [
            // Phase one: decline the spawn.
            (0.6, Press(Key::Escape), true),
            // Phase two: pick the seeded class, then leave overhead.
            (0.3, Press(custom_key), false),
            (1.6, Press(b.field_computer), true),
            // Phase three: force the screen open and Escape it (the session ends with his death).
            (0.3, Reopen, false),
            (0.9, Press(Key::Escape), true),
            // Phase four, the next General: re-pick the pre-selected class (after a couple of
            // seconds on the screen, so the logs and captures see him waiting behind it).
            (2.0, Press(REPICK_KEY), false),
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
        // After phase three's Escape and after phase four's pick, walk forward for a couple of
        // seconds: the autotest checks the position changes, proving first-person play resumed (a
        // still-open screen would eat the input).
        let walk = |from: f32, t: f32| {
            let mut s = Scripted::default();
            if (from..from + 1.8).contains(&t) {
                s.keys.set_down(b.forward, true);
            }
            s
        };
        if self.classpick_wait_reset {
            // Phase three's walk (its Escape ends the script's session; the death ends his).
            return if self.classpick_step == 5 { walk(1.2, self.t) } else { Scripted::default() };
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
        if self.classpick_step >= schedule.len() && scripted.keys == KeySet::default() {
            return walk(2.6, self.t);
        }
        scripted
    }

    /// The two-instance multiplayer run (`ZHBO_AUTOPILOT=mp`): one script per machine, each for its
    /// own player only; the C++ side builds his General at the Command Center, walks him to the
    /// meeting point and takes control. The first session picks the class (the seeded custom
    /// when the profile has one, else the first preset), steps forward and sideways, then — once
    /// the other General is in first person to aim at — swaps to the sidearm and fires it, swaps
    /// back, throws the lethal wide of him (so the throw is covered without settling the fight),
    /// fires the primary at him (the shooter a long aimed burst, the target one round from the
    /// hip), and opens the field computer. A General who died comes back from the Command Center:
    /// his second session meets the class screen again (every General is picked for), opened on the
    /// last class: Enter re-picks it; he walks and leaves overhead.
    /// Every time is measured from the take-control; the fight's from its start.
    pub fn mp_input(&mut self, b: &Bindings) -> Scripted {
        let t = self.t;
        let mut keys = KeySet::default();
        // The pick keys are only pressed while the spawn's pick is really up (the client gates them).
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
        // The first General picks the custom by its row; a later one re-picks the last class (the
        // screen opens on it).
        let pick = (0.3..0.4).contains(&t).then(|| if life >= 2 { REPICK_KEY } else { custom_class_key() });
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
        // The primary at him: the target's round from the hip first, then the shooter's
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
        } else if (5.0..5.0 + crate::units::LOGIC_DT).contains(&c) {
            // One logic frame of trigger, one round (no MP gun fires faster than one round a
            // logic frame): a hit, never a kill, on a whole General — at BO2's health three of
            // the old 0.3 s burst's rounds killed him.
            keys.set_down(b.fire, true);
        }
        // A long run of two lives changes class here, in the first fight, after the target's throw
        // (before his burst; he stays in the fight until he is down, so his next General's screen
        // opens on it).
        let (pick_force, change_pick) = if long { self.mp_class_change(life, c, 4.2) } else { (0, None) };
        // Out by the field computer once the fight is over: the shooter when the target is down
        // (or the time is up), the target only if he outlived it.
        let done = (self.mp_shooter && !target_up && c >= 15.0) || c >= MP_KILL_TIME + 2.0;
        if done {
            self.mp_leave(b, c, &mut keys);
        }
        Scripted { keys, class_pick: change_pick.or(pick), pick_force, look, ..Default::default() }
    }

    /// The target's life in which the long run changes class mid-life: his third (the second round)
    /// when he plays four or more, else his next-to-last, so every run of two lives or more has a
    /// next General to wear it (two lives: the first fight).
    pub fn mp_change_life(&self) -> u32 {
        self.mp_rounds.saturating_sub(1).clamp(1, 3)
    }

    /// The mid-life class change (the long run's target, in [`Self::mp_change_life`]), once the
    /// throw has made him a combatant: the screen forced open at `force` seconds into the fight,
    /// the second default class picked half a second on. `(pick_force, class_pick)`.
    fn mp_class_change(&mut self, life: u32, c: f32, force: f32) -> (i32, Option<Key>) {
        if self.mp_shooter || self.mp_rounds < 2 || life != self.mp_change_life() || self.mp_changed_class {
            return (0, None);
        }
        if (force..force + 0.1).contains(&c) {
            return (1, None);
        }
        if (force + 0.5..force + 0.6).contains(&c) {
            self.mp_changed_class = true;
            self.log.push("autopilot mp: changing class mid-life after combat".to_string());
            return (0, Some(second_preset_key()));
        }
        (0, None)
    }

    /// A life of the long multiplayer run after the first fight: at the meeting point again (the
    /// bridge walks every General there), each throws his tactical at the other (the concussion:
    /// a shellshock on both machines' sims), fires a hip burst wide of him and an aimed one at him, reloads,
    /// knifes, and leaves by the field computer, or dies; the shooter ends with a long aimed burst.
    /// In [`Self::mp_change_life`] the target changes class mid-life once his throw has made him a
    /// combatant (BO2's `MP_CHANGE_CLASS_NEXT_SPAWN`) and stays for the bursts, so his next
    /// General wears it.
    pub fn mp_round(&mut self, b: &Bindings, life: u32) -> Scripted {
        let t = self.t;
        let mut keys = KeySet::default();
        // Every General is picked for: this one's screen opens on the last class (the changed one
        // after the mid-life change), and Enter takes it.
        let repick = (0.3..0.4).contains(&t).then_some(REPICK_KEY);
        let fight = match self.mp_fight {
            Some(_) if self.mp_fight_alone && self.mp_aim.is_some() => {
                self.mp_fight_alone = false;
                self.mp_fight = Some(t);
                self.mp_round_starts += 1;
                self.log.push(format!("autopilot mp: round {life} starts over (the other General came in)"));
                t
            }
            // He came back after a while away (a new General, picked for and armed): the round
            // starts over with him, so both scripts play it together again.
            Some(_) if self.mp_aim.is_some() && self.mp_aim_lost.is_some_and(|lost| t - lost >= MP_AWAY) => {
                self.mp_aim_lost = None;
                self.mp_fight = Some(t);
                self.mp_round_starts += 1;
                self.log.push(format!("autopilot mp: round {life} starts over (the other General is back)"));
                t
            }
            Some(start) => start,
            None if t >= 1.0 && self.mp_armed && (self.mp_aim.is_some() || t >= 20.0) => {
                self.mp_fight = Some(t);
                self.mp_round_starts += 1;
                self.mp_fight_alone = self.mp_aim.is_none();
                self.log.push(format!("autopilot mp: round {life} starts ({})", if self.mp_aim.is_some() { "the other General is in first person" } else { "nobody to aim at" }));
                t
            }
            None => return Scripted { class_pick: repick, ..Default::default() },
        };
        let c = t - fight;
        if self.mp_aim.is_none() {
            self.mp_aim_lost.get_or_insert(t);
        } else if self.mp_aim_lost.is_some_and(|lost| t - lost < MP_AWAY) {
            self.mp_aim_lost = None;
        }
        let mut look = self.mp_aim;
        let mut pick_force = 0;
        let mut class_pick = repick;
        // The tactical, thrown at his feet (the throw leaves above the crosshair and flies far).
        if (0.2..1.4).contains(&c) {
            look = self.mp_aim.map(|(yaw, pitch)| (yaw, pitch - MP_TACTICAL_DIP));
        }
        if (0.4..0.8).contains(&c) {
            keys.set_down(b.tactical, true);
        }
        // The hip burst a little wide of him (as the first fight's sidearm): at BO2's health a
        // burst on target settles the fight, and the aimed one is the round's.
        if (2.3..3.3).contains(&c) {
            look = self.mp_aim.map(|(yaw, pitch)| (yaw + 0.2, pitch));
        }
        // Every burst is at him while he is in first person: one left running where he fell
        // would kill his next General standing there (at BO2's health, in a few rounds).
        let at_him = self.mp_aim.is_some();
        // The shooter's turn to yield ([`Autopilot::mp_round_starts`]): no aimed fire this round.
        let yields = self.mp_shooter && self.mp_round_starts % 2 == 0;
        if (2.5..3.2).contains(&c) && at_him {
            keys.set_down(b.fire, true);
        }
        if (4.0..7.0).contains(&c) {
            keys.set_down(b.ads, true);
        }
        if (4.5..6.5).contains(&c) && at_him && !yields {
            keys.set_down(b.fire, true);
        }
        if (7.2..7.3).contains(&c) {
            keys.set_down(b.reload, true);
        }
        if (9.5..9.6).contains(&c) {
            keys.set_down(b.melee, true);
        }
        // The mid-life class change, once the throw has made him a combatant.
        let (force, pick) = self.mp_class_change(life, c, 1.3);
        if force != 0 {
            pick_force = force;
        }
        if pick.is_some() {
            class_pick = pick;
        }
        // The shooter ends each round with a long aimed burst at him; the target stays for it in
        // the life he changed class in (his next General's screen opens on the new class).
        let changing = !self.mp_shooter && life == self.mp_change_life();
        if self.mp_shooter && (14.0..19.0).contains(&c) && at_him && !yields {
            keys.set_down(b.ads, true);
            if c >= 14.5 {
                keys.set_down(b.fire, true);
            }
        }
        let leave_at = if self.mp_shooter { 21.0 } else { 13.0 };
        if !changing && c >= leave_at {
            self.mp_leave(b, c, &mut keys);
        }
        Scripted { keys, class_pick, pick_force, look, ..Default::default() }
    }

    /// Out by the field computer, once, from `c` on. Its key is also the use key (`sim::drops`):
    /// standing over a dropped weapon (where the other General died) it would swap for it and
    /// keep him in first person, so he first backs off until nothing is in reach.
    fn mp_leave(&mut self, b: &Bindings, c: f32, keys: &mut KeySet) {
        if self.mp_leave_at.is_none() && self.mp_over_drop {
            keys.set_down(b.back, true);
            return;
        }
        let start = *self.mp_leave_at.get_or_insert(c);
        if (start..start + 0.1).contains(&c) {
            keys.set_down(b.field_computer, true);
        }
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

    /// The hip-fire run: pick the first default class (the script seeds none), look down at the
    /// ground ahead, then from the hip: twelve taps half a second apart (each a first shot from
    /// rest), a held two-second string, and a string while running forward. The bridge logs every
    /// shot's spread and its offset from the aim; the autotest holds them to BO2's cone.
    pub fn hipfire_input(&self, b: &Bindings) -> Scripted {
        let t = self.t;
        let mut keys = KeySet::default();
        let pick = (0.3..0.4).contains(&t).then(custom_class_key);
        // About 12 degrees down: the rounds meet the ground a few dozen units ahead.
        let dy = if (2.0..2.3).contains(&t) { 6.0 } else { 0.0 };
        let tap = (3.0..9.0).contains(&t) && ((t - 3.0) % 0.5) < 0.07;
        let string = (9.5..11.5).contains(&t);
        let running = (12.5..15.0).contains(&t);
        if tap || string || (13.5..15.0).contains(&t) {
            keys.set_down(b.fire, true);
        }
        if running {
            keys.set_down(b.forward, true);
        }
        if (16.0..16.1).contains(&t) {
            keys.set_down(b.field_computer, true);
        }
        Scripted { keys, dy, class_pick: pick, ..Default::default() }
    }

    /// The smoke-screen run: pick the seeded class (its tactical the smoke grenade), throw it ahead,
    /// and stand looking at the cloud for the screen's whole life and after (the bridge spawns the
    /// units around it, `ZhboAutopilot.cpp`'s smoke scene; the captures show the cloud).
    pub fn smoke_input(&self, b: &Bindings) -> Scripted {
        let mut keys = KeySet::default();
        let pick = (0.3..0.4).contains(&self.t).then(custom_class_key);
        if (1.5..1.6).contains(&self.t) {
            keys.set_down(b.tactical, true);
        }
        Scripted { keys, class_pick: pick, ..Default::default() }
    }

    /// The equipment run: throw the lethal (C4), hold the tactical up for four seconds (the Black
    /// Hat on the drone the test puts in front; a Trophy System goes down when it is let go),
    /// double-tap reload to set the C4 off, fire a burst and reload, wait while enemy rockets come
    /// in, then open the field computer.
    /// `ZHBO_AUTOPILOT=splash`: pick the seeded class (the SMAW as its secondary), swap to the
    /// SMAW, fire it level into the Overlord ahead once it is raised, then throw the frag after it
    /// and leave by the field computer. The bridge logs every health change in the group.
    pub fn splash_input(&self, b: &Bindings) -> Scripted {
        let t = self.t;
        let mut keys = KeySet::default();
        let pick = (0.3..0.4).contains(&t).then(custom_class_key);
        if (1.0..1.1).contains(&t) {
            keys.set_down(b.swap, true);
        }
        if (5.0..5.1).contains(&t) {
            keys.set_down(b.fire, true);
        }
        if (9.0..9.5).contains(&t) {
            keys.set_down(b.lethal, true);
        }
        if (16.0..16.1).contains(&t) {
            keys.set_down(b.field_computer, true);
        }
        Scripted { keys, class_pick: pick, ..Default::default() }
    }

    /// `ZHBO_AUTOPILOT=flicker`: pick the seeded class, wait for the gun to come up, then hold the
    /// view through [`FLICKER_PHASES`]: a still look a little down at the open ground ahead, the same
    /// look nudged by [`FLICKER_NUDGE`] every logic frame (a correct picture moves by a fraction of a
    /// pixel; z-fighting flips whole patches), then the same two facing the Command Center behind
    /// him (its shadow, its ground decals). Each phase is logged with its logic frame so the autotest
    /// diffs exactly its captures.
    pub fn flicker_input(&mut self) -> Scripted {
        let t = self.t;
        let pick = (0.3..0.4).contains(&t).then(custom_class_key);
        let Some(first) = FLICKER_PHASES.first() else { return Scripted { class_pick: pick, ..Default::default() } };
        if t < first.from {
            return Scripted { class_pick: pick, ..Default::default() };
        }
        let (yaw0, pitch0) = *self.flicker_base.get_or_insert(self.view);
        let Some(i) = FLICKER_PHASES.iter().rposition(|p| t >= p.from) else { return Scripted::default() };
        let phase = &FLICKER_PHASES[i];
        if self.flicker_phase != Some(i) {
            self.flicker_phase = Some(i);
            let frame = self.base_frame + (t / crate::units::LOGIC_DT).round() as u32;
            self.log.push(format!("autopilot flicker: phase {} ({}) from logic frame {frame}", i, phase.name));
        }
        if phase.end {
            return Scripted::default();
        }
        let frame = (t / crate::units::LOGIC_DT).round() as u32;
        let nudge = if phase.nudge && frame % 2 == 1 { FLICKER_NUDGE } else { 0.0 };
        Scripted { look: Some((yaw0 + phase.yaw + nudge, pitch0 + phase.pitch)), ..Default::default() }
    }

    /// `ZHBO_AUTOPILOT=drops`, two lives. The first General picks the profile's first custom (the
    /// AN-94 with a grenade launcher), fires a burst, and past the grace period re-opens the class
    /// screen and picks the second custom (the AN-94 with a reflex sight): it waits for the next
    /// spawn. The bridge then kills him (his rifle drops) and stands the next General behind the
    /// spot; his class screen opens on the second custom and Enter takes it; he fires and reloads (room for spare rounds), walks onto the dropped rifle (its stock
    /// is his: the same base weapon) and presses the use key over it: the swap.
    pub fn drops_input(&mut self, b: &Bindings) -> Scripted {
        let t = self.t;
        let mut keys = KeySet::default();
        if self.session <= 1 {
            let mut pick = (0.3..0.4).contains(&t).then(custom_class_key);
            if (2.0..2.6).contains(&t) {
                keys.set_down(b.fire, true);
            }
            let force = (18.0..18.1).contains(&t) as i32;
            if (18.5..18.6).contains(&t) {
                pick = nth_custom_class_key(1);
            }
            return Scripted { keys, class_pick: pick, pick_force: force, ..Default::default() };
        }
        // The next General is picked for: his screen opens on the class changed for him (the
        // second custom), and Enter takes it.
        let pick = (0.3..0.4).contains(&t).then_some(REPICK_KEY);
        if (2.0..2.5).contains(&t) {
            keys.set_down(b.fire, true);
        }
        if (3.0..3.1).contains(&t) {
            keys.set_down(b.reload, true);
        }
        let mut look = None;
        if t >= 7.0 {
            if let Some((yaw, pitch, dist)) = self.drop_aim {
                look = Some((yaw, pitch));
                if dist > crate::units::inches(6.0) && self.drop_used.is_none() {
                    keys.set_down(b.forward, true);
                } else if self.drop_used.is_none() && t >= 9.0 {
                    self.drop_used = Some(t);
                    self.log.push(format!("autopilot drops: over the drop at t {t:.1}, pressing use"));
                }
            }
        }
        if self.drop_used.is_some_and(|u| t < u + 0.1) {
            keys.set_down(b.field_computer, true);
        }
        Scripted { keys, look, class_pick: pick, ..Default::default() }
    }

    /// Each step waits for the state it needs, not a clock: the lethal goes once the class is
    /// armed and the hands are free; the tactical once the lethal is down and the hands are free
    /// again (BO2 refuses an offhand press during another offhand, `FUN_008e2bb0`), held long enough
    /// for the Black Hat to finish on the drone (`hackertoollockstart` + `vehiclehackertooltimems`,
    /// two seconds' margin); the rest is timed from its release.
    pub fn equipment_input(&mut self, b: &Bindings) -> KeySet {
        let t = self.t;
        let mut keys = KeySet::default();
        if t < 0.5 {
            return keys;
        }
        if self.eq_lethal_at.is_none() && self.eq_idle && self.eq_lethal_left > 0 {
            self.eq_lethal_at = Some(t);
        }
        if let Some(at) = self.eq_lethal_at {
            if (at..at + 0.5).contains(&t) {
                keys.set_down(b.lethal, true);
            }
        }
        let lethal_done = self.eq_lethal_at.is_some_and(|at| t > at + 0.5 && self.eq_lethal_left == 0) || (self.eq_lethal_at.is_none() && self.eq_lethal_left == 0);
        if self.eq_tactical_from.is_none() && lethal_done && self.eq_idle {
            self.eq_tactical_from = Some(t + EQ_BACK_OFF);
        }
        let Some(from) = self.eq_tactical_from else { return keys };
        // Out of his own C4's blast before he sets it off: at BO2's health its 205 at the centre
        // kills him (his own explosive hurts him in BO2 too), so he backs off while it settles.
        if self.eq_lethal_at.is_some() && (from - EQ_BACK_OFF + 0.2..from).contains(&t) {
            keys.set_down(b.back, true);
        }
        let tn = crate::tuning::get();
        let hold = tn.hacker_tool_lock_start + tn.vehicle_hacker_tool_time_ms / 1000.0 + 2.0;
        let end = from + hold;
        if (from..end).contains(&t) {
            keys.set_down(b.tactical, true);
        }
        let c = t - end;
        if (1.0..1.08).contains(&c) || (1.2..1.28).contains(&c) {
            keys.set_down(b.reload, true);
        }
        // A burst and a reload (the reload animation's magazines).
        if (3.0..4.0).contains(&c) {
            keys.set_down(b.fire, true);
        }
        if (4.3..4.4).contains(&c) {
            keys.set_down(b.reload, true);
        }
        if (14.0..14.1).contains(&c) {
            keys.set_down(b.field_computer, true);
        }
        keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The equipment run waits for the state each step needs: no tactical while the lethal is
    /// still in the hands (BO2 refuses that press), the Black Hat held past the drone's lock, and a
    /// lethal press only once the class is armed. A late class (the spawn's pick landing a second
    /// in, as in game) shifts everything with it. The run used fixed windows: the C4's throw, late
    /// behind the pick, swallowed the tactical's press.
    #[test]
    fn the_equipment_run_waits_for_the_hands() {
        let b = Bindings::default();
        let tn = crate::tuning::get();
        let lock = tn.hacker_tool_lock_start + tn.vehicle_hacker_tool_time_ms / 1000.0;
        for armed_at in [0.5f32, 1.5] {
            let mut ap = Autopilot { equipment: true, ..Default::default() };
            let (mut lethal_at, mut tactical) = (None, Vec::new());
            let mut thrown_at = None;
            for f in 0..(30 * 30) {
                ap.t = f as f32 * crate::units::LOGIC_DT;
                // The arsenal as the sim shows it: armed at `armed_at`, the C4's offhand states
                // running 1.5 s from its press.
                let busy = lethal_at.is_some_and(|at: f32| ap.t < at + 1.5);
                ap.eq_idle = ap.t >= armed_at && !busy;
                ap.eq_lethal_left = if thrown_at.is_some() { 0 } else { 1 };
                let keys = ap.equipment_input(&b);
                if keys.is_down(b.lethal) && lethal_at.is_none() {
                    assert!(ap.t >= armed_at, "the lethal waits for the class");
                    lethal_at = Some(ap.t);
                }
                if lethal_at.is_some_and(|at| ap.t >= at + 0.8) {
                    thrown_at.get_or_insert(ap.t);
                }
                if keys.is_down(b.tactical) {
                    assert!(!busy, "no tactical press during the lethal's offhand ({armed_at}: t {})", ap.t);
                    tactical.push(ap.t);
                }
            }
            let held = tactical.last().unwrap() - tactical.first().unwrap();
            assert!(held >= lock, "{armed_at}: held {held} s, the drone's lock needs {lock}");
        }
    }

    /// The long run's lives end by the field computer, whose key is also the use key: over a
    /// dropped weapon (the meeting point, where the other General died) the press would swap for it
    /// and keep him in first person for good. He backs off until nothing is in reach, then presses
    /// once.
    #[test]
    fn the_mp_leave_backs_off_a_dropped_weapon_first() {
        let b = Bindings::default();
        let mut ap = Autopilot { mp: true, mp_rounds: 5, session: 3, mp_fight: Some(0.0), mp_aim: Some((0.0, 0.0)), mp_over_drop: true, ..Default::default() };
        let (mut presses, mut backed) = (Vec::new(), false);
        for f in 0..(30 * 20) {
            ap.t = f as f32 * crate::units::LOGIC_DT;
            if ap.t >= 14.0 {
                ap.mp_over_drop = false;
            }
            let keys = ap.mp_round(&b, 2).keys;
            if keys.is_down(b.field_computer) {
                assert!(!ap.mp_over_drop, "no press over a drop (t {})", ap.t);
                presses.push(ap.t);
            }
            backed |= keys.is_down(b.back);
        }
        assert!(backed, "he steps back off the drop");
        assert!(!presses.is_empty() && presses.iter().all(|t| *t >= 14.0 && *t < 14.15), "one press (0.1 s) once clear: {presses:?}");
    }
}
