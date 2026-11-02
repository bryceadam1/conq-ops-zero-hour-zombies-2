/*
** Conquer Ops - Zero Hour gameplay core: BO2's shellshocks on the first-person General.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! BO2's shellshock: what a concussion, a flashbang, an EMP, a Shock Charge and a blast do to a
//! player (the GSC's `shellshock( name, duration )`). The shock's numbers are its
//! `shock/<name>.shock` file, converted from the player's install into the weapon table's `Shock`
//! blocks ([`ShockDef`], `zhbo_formats::shock`); which shock plays and for how long is the
//! script's (see [`rules`]). While a shock runs (t6mp, decompiled from the engine):
//!
//! * **movement** (`PM_CmdScale`, `FUN_008c4110`): the run speed is scaled by the shock's
//!   `movement` for the whole shock (`ps.pm_flags & 0x10000`), no fade — in the logic step;
//! * **look** (`FUN_005109f0`): with `lookControl`, `f = min(remaining / lookControl_fadeTime, 1)`;
//!   the mouse sensitivity is `1 + (scale − 1) · f` and the view turns at most `maxpitchspeed / f`,
//!   `maxyawspeed / f` degrees per second — on the client, whose angles the input carries to the
//!   sim (as BO2's command does);
//! * **screen** (`FUN_005105c0` flashed, `FUN_00510460` blurred): a flashed shock grabs the frame
//!   on its first frame; then its whiteout is `ease(min(remaining / screenFlashWhiteFadeTime, 1))`
//!   and the grab's `ease(min(remaining / screenFlashShotFadeTime, 1))` ([`ease`], `FUN_00510560`),
//!   both added over the view (the `shellshock_flashed` material: `ONE, ONE`, `grab · shot +
//!   white`). A blurred shock's blend time is its `screenBlurBlendTime`, `× remaining /
//!   screenBlurBlendFadeTime` in that last stretch; each frame the saved screen is laid over the
//!   live one (`shellshock`: `SRC_ALPHA, INV_SRC_ALPHA`) and the result saved again — a trail that
//!   decays over the blend time ([`blur_alpha`]). The renderer draws both (`zhbo_gfx`'s shock pass,
//!   fed through `nativevm::set_local_shock`); without it the HUD draws the whiteout alone;
//! * **view kick** (`FUN_00510b00`, applied by `FUN_00510270` to the view's axes only): a noise
//!   wobble of `viewKickRadius` at `viewKickPeriod`, fading out over the last 3 s ([`kick`]) —
//!   the camera, never the aim;
//! * **sound** (`FUN_005107c0`): the `soundLoop` alias starts with a `soundFadeInTime` fade and stops
//!   with a `soundFadeOutTime` fade `soundModEndDelay` after the shock's end ([`Presented`]).
//!
//! Not ported, with the reasons (MODLOG "the rest of the shellshock presentation"): the vision set
//! (`concussion_grenade.vision` is an identity grade in `postfx_color`), the sound duck
//! (`soundSnapshot`: `cmn_shock_tinitus` attenuates no group, it only low-passes, and Zero Hour's
//! OpenAL has no filter) and the reverb room (`soundRoomType`: no reverb in Zero Hour's audio).
//!
//! A new shock replaces the one running (`shellshock` overwrites the player's index, start and
//! duration). Only the first-person General is shocked: Zero Hour's own units and a General under
//! RTS control keep Zero Hour's `disable(PARALYZED)` stun (the closest Zero Hour has), which
//! would freeze a first-person player outright.

use crate::units::LOGIC_DT;
use crate::weapons::table::{Field, FieldKind, Role, Values};

/// The shock files the MP GSC plays that the mod's sources use, by the script's names.
pub mod names {
    /// `_weapons.gsc:1380` (`onweapondamage`, the concussion grenade).
    pub const CONCUSSION: &str = "concussion_grenade_mp";
    /// `_flashgrenades.gsc:160` (`applyflash`) and the EMP grenade's second, `_empgrenade.gsc:105`.
    pub const FLASHBANG: &str = "flashbang";
    /// `_shellshock.gsc:35` (`shellshockondamage`: explosive damage).
    pub const FRAG: &str = "frag_grenade_mp";
    /// `_proximity_grenade.gsc:189` and `:206` (the Shock Charge's zap, then its exit).
    pub const PROXIMITY: &str = "proximity_grenade";
    pub const PROXIMITY_EXIT: &str = "proximity_grenade_exit";
    /// Every one the converter carries.
    pub const ALL: [&str; 5] = [CONCUSSION, FLASHBANG, FRAG, PROXIMITY, PROXIMITY_EXIT];
}

/// One shellshock's numbers (a `Shock <name>` block of the weapon table). Times in seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct ShockDef {
    pub name: String,
    /// `screenType "flashed"` (else blurred).
    pub flashed: bool,
    pub blur: f32,
    pub blur_fade: f32,
    pub white_fade: f32,
    pub shot_fade: f32,
    pub look_control: bool,
    /// Degrees per second.
    pub max_pitch_speed: f32,
    pub max_yaw_speed: f32,
    pub sensitivity: f32,
    pub look_fade: f32,
    pub movement: f32,
    /// `viewKickPeriod` (s) and `viewKickRadius` (the kick's tangent).
    pub kick_period: f32,
    pub kick_radius: f32,
    /// `soundFadeInTime`, `soundFadeOutTime`, `soundModEndDelay` (s).
    pub sound_fade_in: f32,
    pub sound_fade_out: f32,
    pub sound_mod_end: f32,
    /// `soundLoop`: the ringing alias, empty when the install's MP banks have no such alias (BO2
    /// then plays nothing). Played as the event [`loop_event`].
    pub sound_loop: String,
}

/// A running shock's definition by identity: its name (one `Shock` block of the installed table).
impl crate::statehash::StateHash for &'static ShockDef {
    fn hash(&self, h: &mut crate::statehash::Hasher) {
        h.bytes(self.name.as_bytes());
    }
    #[cfg(test)]
    fn perturb(&mut self, n: &mut usize) -> bool {
        crate::statehash::take(n) && {
            *self = Box::leak(Box::new(ShockDef { name: format!("{}~", self.name), ..(*self).clone() }));
            true
        }
    }
}

impl Default for ShockDef {
    fn default() -> Self {
        ShockDef {
            name: String::new(),
            flashed: false,
            blur: 0.0,
            blur_fade: 0.0,
            white_fade: 0.0,
            shot_fade: 0.0,
            look_control: false,
            max_pitch_speed: 0.0,
            max_yaw_speed: 0.0,
            sensitivity: 1.0,
            look_fade: 0.0,
            movement: 1.0,
            kick_period: 0.0,
            kick_radius: 0.0,
            sound_fade_in: 0.0,
            sound_fade_out: 0.0,
            sound_mod_end: 0.0,
            sound_loop: String::new(),
        }
    }
}

fn num(v: &Values) -> Result<f32, String> {
    v.text_num()
}

/// Every `Shock` block line, in table order.
pub const SHOCK_FIELDS: &[Field<ShockDef>] = &[
    Field { key: "Flashed", doc: "1 if `bg_shock_screenType` is `flashed` (a whiteout and a screengrab), 0 for `blurred`.", kind: FieldKind::Num,
        get: |s| (s.flashed as u8).to_string(), set: |s, v| { s.flashed = num(&v)? != 0.0; Ok(()) }, role: Role::Sim },
    Field { key: "Blur", doc: "`bg_shock_screenBlurBlendTime` and `bg_shock_screenBlurBlendFadeTime` (s).", kind: FieldKind::Nums(2),
        get: |s| format!("{} {}", s.blur, s.blur_fade), set: |s, v| { v.need(2)?; (s.blur, s.blur_fade) = (v.num(0), v.num(1)); Ok(()) }, role: Role::Sim },
    Field { key: "Flash", doc: "`bg_shock_screenFlashWhiteFadeTime` and `bg_shock_screenFlashShotFadeTime` (s).", kind: FieldKind::Nums(2),
        get: |s| format!("{} {}", s.white_fade, s.shot_fade), set: |s, v| { v.need(2)?; (s.white_fade, s.shot_fade) = (v.num(0), v.num(1)); Ok(()) }, role: Role::Sim },
    Field { key: "Look", doc: "`bg_shock_lookControl`, its max pitch and yaw speeds (deg/s), sensitivity scale and fade time (s).", kind: FieldKind::Nums(5),
        get: |s| format!("{} {} {} {} {}", s.look_control as u8, s.max_pitch_speed, s.max_yaw_speed, s.sensitivity, s.look_fade),
        set: |s, v| { v.need(5)?; s.look_control = v.flag(0); (s.max_pitch_speed, s.max_yaw_speed, s.sensitivity, s.look_fade) = (v.num(1), v.num(2), v.num(3), v.num(4)); Ok(()) }, role: Role::Sim },
    Field { key: "Movement", doc: "`bg_shock_movement`: the run speed's scale while shocked.", kind: FieldKind::Num,
        get: |s| s.movement.to_string(), set: |s, v| { s.movement = num(&v)?; Ok(()) }, role: Role::Sim },
    Field { key: "Kick", doc: "`bg_shock_viewKickPeriod` (s) and `bg_shock_viewKickRadius`.", kind: FieldKind::Nums(2),
        get: |s| format!("{} {}", s.kick_period, s.kick_radius), set: |s, v| { v.need(2)?; (s.kick_period, s.kick_radius) = (v.num(0), v.num(1)); Ok(()) }, role: Role::Sim },
    Field { key: "Sound", doc: "`bg_shock_soundFadeInTime`, `bg_shock_soundFadeOutTime` and `bg_shock_soundModEndDelay` (s).", kind: FieldKind::Nums(3),
        get: |s| format!("{} {} {}", s.sound_fade_in, s.sound_fade_out, s.sound_mod_end),
        set: |s, v| { v.need(3)?; (s.sound_fade_in, s.sound_fade_out, s.sound_mod_end) = (v.num(0), v.num(1), v.num(2)); Ok(()) }, role: Role::Sim },
    Field { key: "SoundLoop", doc: "`bg_shock_soundLoop`: the ringing alias (empty when the MP banks lack it).", kind: FieldKind::Str,
        get: |s| s.sound_loop.clone(), set: |s, v| { s.sound_loop = v.text.into(); Ok(()) }, role: Role::Presentation }, // content: presentation (the schema line)
];

/// A shock running on a player: its def and the seconds left of its duration.
#[derive(Clone, Debug, PartialEq)]
pub struct Active {
    pub def: &'static ShockDef,
    pub duration: f32,
    pub left: f32,
    /// Which `shellshock` call started it ([`Shellshock::serial`]): a new shock is a new grab.
    pub serial: u32,
}

crate::statehash::state_hash!(struct Active { def, duration, left, serial });

/// The first-person General's shellshock state (in the sim, deterministic in the logic step).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shellshock {
    pub active: Option<Active>,
    /// A shock the script plays later (the Shock Charge's exit): seconds to wait, name, duration.
    pub queued: Option<(f32, &'static str, f32)>,
    /// Counts the shocks played (presentation tells one shock from the next by it).
    pub serial: u32,
}

crate::statehash::state_hash!(struct Shellshock { active, queued, serial });

/// What the look does this frame ([`Shellshock::look`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub sensitivity: f32,
    /// Degrees per second; `INFINITY` when unlimited.
    pub max_pitch_speed: f32,
    pub max_yaw_speed: f32,
}

impl Look {
    pub const FREE: Look = Look { sensitivity: 1.0, max_pitch_speed: f32::INFINITY, max_yaw_speed: f32::INFINITY };
}

/// What the screen shows this frame ([`Shellshock::screen`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Screen {
    pub white: f32,
    pub shot: f32,
    pub blur: f32,
}

impl Shellshock {
    /// `shellshock( name, duration )`: play shock `name` (a def of the catalog) for `duration`
    /// seconds, replacing the one running. An unknown shock or a non-positive duration does nothing.
    pub fn play(&mut self, name: &str, duration: f32) {
        if duration <= 0.0 {
            return;
        }
        if let Some(def) = crate::weapons::catalog().shock(name) {
            self.serial = self.serial.wrapping_add(1);
            self.active = Some(Active { def, duration, left: duration, serial: self.serial });
        }
    }

    /// Queue shock `name` to play after `wait` seconds (the script's `wait` before a second shock).
    pub fn queue(&mut self, wait: f32, name: &'static str, duration: f32) {
        self.queued = Some((wait, name, duration));
    }

    /// Advance one logic frame.
    pub fn step(&mut self) {
        if let Some(a) = self.active.as_mut() {
            a.left -= LOGIC_DT;
            if a.left <= 0.0 {
                self.active = None;
            }
        }
        if let Some((wait, name, duration)) = self.queued {
            let wait = wait - LOGIC_DT;
            if wait <= 0.0 {
                self.queued = None;
                self.play(name, duration);
            } else {
                self.queued = Some((wait, name, duration));
            }
        }
    }

    /// The running shock's name, if any.
    pub fn name(&self) -> Option<&str> {
        self.active.as_ref().map(|a| a.def.name.as_str())
    }

    /// The run speed's scale (`PM_CmdScale`): the shock's `movement` while it runs.
    pub fn movement(&self) -> f32 {
        self.active.as_ref().map_or(1.0, |a| a.def.movement)
    }

    /// The look limits (`FUN_005109f0`).
    pub fn look(&self) -> Look {
        let Some(a) = self.active.as_ref().filter(|a| a.def.look_control) else { return Look::FREE };
        let d = a.def;
        let f = if a.left >= d.look_fade { 1.0 } else { (a.left / d.look_fade.max(1e-6)).max(0.0) };
        if f <= 0.0 {
            return Look::FREE;
        }
        // At full strength the engine takes the scale as is (its `f == 1` branch).
        let sensitivity = if f >= 1.0 { d.sensitivity } else { 1.0 + (d.sensitivity - 1.0) * f };
        Look { sensitivity, max_pitch_speed: d.max_pitch_speed / f, max_yaw_speed: d.max_yaw_speed / f }
    }

    /// The screen layers (`FUN_005105c0`, `FUN_00510460`).
    pub fn screen(&self) -> Screen {
        self.active.as_ref().map_or_else(Screen::default, |a| screen(a.def, a.left))
    }
}

/// The screen layers of shock `d` with `left` seconds to run (`FUN_005105c0` flashed,
/// `FUN_00510460` blurred). A layer whose fade time is 0 stays at full (the engine's share starts
/// at 1 and only a positive fade time lowers it).
pub fn screen(d: &ShockDef, left: f32) -> Screen {
    if left <= 0.0 {
        return Screen::default();
    }
    let share = |fade: f32| if left < fade { left / fade } else { 1.0 };
    if d.flashed {
        Screen { white: ease(share(d.white_fade)), shot: ease(share(d.shot_fade)), blur: 0.0 }
    } else {
        Screen { blur: d.blur * share(d.blur_fade), ..Default::default() }
    }
}

/// The flashed layers' fade curve (`FUN_00510560`): `(sin((x − 0.5)·π) + 1) / 2`, 0 at 0 and 1 at 1.
pub fn ease(x: f32) -> f32 {
    crate::math::det::sin((x - 0.5) * std::f32::consts::PI) * 0.5 + 0.5
}

/// How much of the saved screen a blurred shock lays over this frame's (the `shellshock`
/// material's alpha): the trail decays over the blend time `blur` (s), so a frame `dt` long keeps
/// `exp(−dt / blur)` of it. 0 without a blur. (t6mp hands the backend the blend time and the
/// frame; the backend's draw was not located, so the decay is the frame-rate independent one.)
pub fn blur_alpha(blur: f32, dt: f32) -> f32 {
    if blur <= 0.0 || !blur.is_finite() || !dt.is_finite() {
        return 0.0;
    }
    crate::math::det::exp(-dt.max(0.0) / blur)
}

/// The view kick's noise: 128 points per axis, BO2's layout (`FUN_00510b00` reads a 128-entry table
/// of (x, y) pairs at `0x00b25278`, values in −1..1). That table is engine code, not install data,
/// so the mod draws its own from a fixed seed — the same shape and range, not the same numbers.
fn noise(i: usize) -> [f32; 2] {
    static TABLE: std::sync::OnceLock<[[f32; 2]; 128]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut state: u32 = 0x5eed_5bc4;
        let mut next = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
        };
        std::array::from_fn(|_| [next(), next()])
    })[i & 127]
}

/// The cubic through four noise points at `t` in 0..1 (`FUN_00510b00`'s interpolation).
fn cubic(p: [f32; 4], t: f32) -> f32 {
    let a0 = p[3] - p[2] - p[0] + p[1];
    let a1 = p[0] - p[1] - a0;
    let a2 = p[2] - p[0];
    ((a0 * t + a1) * t + a2) * t + p[1]
}

/// The shock's view kick at `elapsed` seconds into a shock of `duration` (`FUN_00510b00`): yaw
/// (left) and pitch (up), radians, added to the camera only (`FUN_00510270` turns the view's axes
/// to forward `(1, kx, ky)`). It fades out over the last `fade` seconds as `(3 − 2f)·f²`; the
/// noise runs at `1 / viewKickPeriod` points a second from a start the duration picks.
pub fn kick(d: &ShockDef, duration: f32, elapsed: f32, fade: f32) -> (f32, f32) {
    if d.kick_radius == 0.0 || !(duration.is_finite() && elapsed.is_finite()) {
        return (0.0, 0.0);
    }
    // The engine counts milliseconds.
    let (dur_ms, el_ms) = ((duration * 1000.0).round() as i32, (elapsed * 1000.0).round() as i32);
    let remaining = dur_ms - el_ms;
    if remaining < 1 {
        return (0.0, 0.0);
    }
    let fade_ms = fade * 1000.0;
    let f = if (remaining as f32) < fade_ms { remaining as f32 / fade_ms } else { 1.0 };
    let amp = (3.0 - 2.0 * f) * f * f * d.kick_radius;
    let t = el_ms as f32 * (0.001 / d.kick_period.max(0.001));
    let i = t.floor();
    let frac = t - i;
    let base = (dur_ms.wrapping_mul(61).wrapping_add(i as i32) & 127) as usize;
    let axis = |k: usize| cubic(std::array::from_fn(|j| noise(base + j)[k]), frac) * amp;
    let (kx, ky) = (axis(0), axis(1));
    (crate::math::det::atan(kx), crate::math::det::atan2(ky, (1.0 + kx * kx).sqrt()))
}

/// The AudioEvent a shock's loop alias is converted to (`zhbo_assets::sfx`, looping).
pub fn loop_event(alias: &str) -> &'static str {
    static EVENTS: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let name = format!("ZHBO_BO2_Shock_{alias}");
    let mut events = EVENTS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(e) = events.iter().find(|e| **e == name) {
        return e;
    }
    let e: &'static str = Box::leak(name.into_boxed_str());
    events.push(e);
    e
}

/// The local player's shock as the client presents it: the shock the sim runs, followed past its
/// end for the sound's tail, and the ringing loop's voice. Presentation only.
#[derive(Clone, Debug, Default)]
pub struct Presented {
    shock: Option<(u32, &'static ShockDef, f32)>,
    /// Seconds into `shock` (continuing after the sim's shock has ended).
    elapsed: f32,
    voice: Option<Voice>,
}

#[derive(Clone, Debug)]
struct Voice {
    event: &'static str,
    gain: f32,
    /// Gain per second: positive fading in, negative fading out.
    rate: f32,
}

impl Presented {
    /// Follow the sim's shock `s` this client frame: `dt` the frame, `since_logic` the seconds
    /// since the sim's last step (the shock's clock is interpolated across it, as the eye is).
    /// `play(event, gain)` sets the ringing loop's gain (0 stops it).
    pub fn update(&mut self, s: &Shellshock, since_logic: f32, dt: f32, play: &mut dyn FnMut(&'static str, f32)) {
        match s.active.as_ref() {
            Some(a) => {
                if self.shock.is_none_or(|(serial, _, _)| serial != a.serial) {
                    self.shock = Some((a.serial, a.def, a.duration));
                }
                self.elapsed = (a.duration - a.left + since_logic.max(0.0)).min(a.duration);
            }
            None => self.elapsed += dt.max(0.0),
        }
        // `FUN_005107c0`: the sound modification runs until `soundModEndDelay` past the shock's end.
        let ringing = self.shock.and_then(|(_, d, duration)| (self.elapsed < duration + d.sound_mod_end && !d.sound_loop.is_empty()).then_some(d)); // content: presentation (the client's ringing voice)
        match (&mut self.voice, ringing) {
            (Some(v), Some(d)) if v.event == loop_event(&d.sound_loop) => { // content: presentation (the client's ringing voice)
                if v.rate < 0.0 {
                    v.rate = fade_rate(d.sound_fade_in);
                }
            }
            (voice, Some(d)) => {
                if let Some(v) = voice.take() {
                    play(v.event, 0.0);
                }
                *voice = Some(Voice { event: loop_event(&d.sound_loop), gain: 0.0, rate: fade_rate(d.sound_fade_in) }); // content: presentation (the client's ringing voice)
            }
            (Some(v), None) if v.rate >= 0.0 => v.rate = -self.shock.map_or(f32::INFINITY, |(_, d, _)| fade_rate(d.sound_fade_out)),
            _ => {}
        }
        if let Some(v) = self.voice.as_mut() {
            v.gain = (v.gain + v.rate * dt.max(0.0)).clamp(0.0, 1.0);
            if v.rate.is_infinite() {
                v.gain = if v.rate > 0.0 { 1.0 } else { 0.0 };
            }
            play(v.event, v.gain);
            if v.gain <= 0.0 && v.rate < 0.0 {
                self.voice = None;
            }
        }
        if s.active.is_none() && self.voice.is_none() {
            self.shock = None;
        }
    }

    /// Forget the shock and stop its ringing at once (first person ended).
    pub fn stop(&mut self, play: &mut dyn FnMut(&'static str, f32)) {
        if let Some(v) = self.voice.take() {
            play(v.event, 0.0);
        }
        self.shock = None;
        self.elapsed = 0.0;
    }

    /// The shock being presented and the seconds into it (`None` once nothing is left to show).
    pub fn current(&self) -> Option<(&'static ShockDef, f32, f32)> {
        self.shock.map(|(_, d, duration)| (d, duration, self.elapsed))
    }

    /// The view kick now (`kick`), with the tuning pack's fade.
    pub fn kick(&self, fade: f32) -> (f32, f32) {
        self.current().map_or((0.0, 0.0), |(d, duration, elapsed)| kick(d, duration, elapsed, fade))
    }

    /// The ringing loop's gain, if one plays.
    pub fn loop_gain(&self) -> Option<(&'static str, f32)> {
        self.voice.as_ref().map(|v| (v.event, v.gain))
    }
}

/// The gain change per second of a linear fade over `seconds` (instant when 0).
fn fade_rate(seconds: f32) -> f32 {
    if seconds > 0.0 {
        1.0 / seconds
    } else {
        f32::INFINITY
    }
}

/// Which shock the script plays and for how long, from what hit the player.
pub mod rules {
    use crate::tuning::Tuning;

    /// The flashbang (`_flashgrenades.gsc:38-58`, `monitorflash_internal`): the engine's amounts
    /// (`FUN_005f1c60`) are the distance share — 1 within the inner radius, falling to 0 at the
    /// radius — and the angle share `(dot(view, to the grenade) + 1) / 2`; the script clamps the
    /// angle (under 0.5 → 0.5, over 0.8 → 1), halves the distance for the thrower, and the
    /// duration is `distance × angle × 6`, x0.1 with `flashprotection`; under 0.25 s nothing.
    pub fn flash_duration(t: &Tuning, distance_share: f32, view_dot: f32, own: bool, protected: bool) -> Option<f32> {
        let mut angle = (view_dot + 1.0) * 0.5;
        if angle < t.flash_angle_min {
            angle = t.flash_angle_min;
        } else if angle > t.flash_angle_full {
            angle = 1.0;
        }
        let distance = if own { distance_share * t.flash_self_scale } else { distance_share };
        let mut duration = distance * angle * t.flash_duration_scale;
        if duration < t.flash_min_duration {
            return None;
        }
        if protected {
            duration *= t.flash_protection_scale;
        }
        Some(duration)
    }

    /// The engine's flashbang distance share (`FUN_005f1c60`): 1 within `inner`, linear to 0 at
    /// `radius`.
    pub fn flash_distance_share(dist: f32, inner: f32, radius: f32) -> f32 {
        if dist <= inner {
            1.0
        } else {
            (1.0 - (dist - inner) / (radius - inner).max(1e-6)).clamp(0.0, 1.0)
        }
    }

    /// `shellshockondamage` (`_shellshock.gsc:14-38`): explosive damage over 10 (BO2's scale)
    /// shakes the player for 0.5 s with the frag's shock, unless a flashbang's is running.
    pub fn blast_shock(t: &Tuning, bo2_damage: f32) -> Option<f32> {
        (bo2_damage > t.blast_shock_min_damage).then_some(t.blast_shock_duration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(look_control: bool) -> &'static ShockDef {
        Box::leak(Box::new(ShockDef {
            name: "t".into(),
            flashed: true,
            white_fade: 7.5,
            shot_fade: 15.0,
            look_control,
            max_pitch_speed: 22.0,
            max_yaw_speed: 22.0,
            sensitivity: 0.1,
            look_fade: 2.0,
            movement: 0.32,
            ..Default::default()
        }))
    }

    #[test]
    fn the_look_fades_back_over_its_last_seconds() {
        let mut s = Shellshock { active: Some(Active { def: def(true), duration: 6.0, left: 6.0, serial: 1 }), ..Default::default() };
        assert_eq!(s.look(), Look { sensitivity: 0.1, max_pitch_speed: 22.0, max_yaw_speed: 22.0 });
        s.active.as_mut().unwrap().left = 1.0;
        let l = s.look();
        assert!((l.sensitivity - 0.55).abs() < 1e-6 && (l.max_yaw_speed - 44.0).abs() < 1e-4, "{l:?}");
        s.active.as_mut().unwrap().def = def(false);
        assert_eq!(s.look(), Look::FREE, "no look control");
        assert_eq!(s.movement(), 0.32, "movement for the whole shock, no fade");
    }

    #[test]
    fn a_flashed_screen_blends_out_over_its_fade_times() {
        let s = Shellshock { active: Some(Active { def: def(true), duration: 6.0, left: 3.0, serial: 1 }), ..Default::default() };
        let sc = s.screen();
        // The shares 0.4 and 0.2 through FUN_00510560's ease.
        assert!((sc.white - ease(0.4)).abs() < 1e-6 && (sc.shot - ease(0.2)).abs() < 1e-6, "{sc:?}");
        assert!((ease(0.4) - 0.345492).abs() < 1e-5 && ease(0.0).abs() < 1e-6 && (ease(1.0) - 1.0).abs() < 1e-6 && (ease(0.5) - 0.5).abs() < 1e-6);
        assert_eq!(Shellshock::default().screen(), Screen::default());
        // A fade time of 0 keeps the layer at full (the engine's share starts at 1).
        let none = ShockDef { flashed: true, white_fade: 0.0, shot_fade: 2.0, ..Default::default() };
        assert_eq!(screen(&none, 1.0).white, 1.0);
        // Blurred: the blend time, lowered over the fade's last stretch.
        let blurred = ShockDef { blur: 0.4, blur_fade: 1.0, ..Default::default() };
        assert_eq!(screen(&blurred, 2.0), Screen { blur: 0.4, ..Default::default() });
        assert!((screen(&blurred, 0.25).blur - 0.1).abs() < 1e-6);
        assert_eq!(screen(&blurred, 0.0), Screen::default());
    }

    #[test]
    fn a_shock_runs_out_and_a_queued_one_follows() {
        let mut s = Shellshock { active: Some(Active { def: def(true), duration: 0.1, left: 0.1, serial: 1 }), ..Default::default() };
        for _ in 0..4 {
            s.step();
        }
        assert!(s.active.is_none());
        s.queue(0.1, names::CONCUSSION, 1.0);
        for _ in 0..4 {
            s.step();
        }
        assert_eq!(s.name(), Some(names::CONCUSSION), "the stand-in table carries the concussion");
    }

    #[test]
    fn the_flashbang_follows_the_script() {
        let t = crate::tuning::Tuning::default();
        // Full distance, looking at it: 6 s; looking away: the angle floor 0.5 → 3 s.
        assert!((rules::flash_duration(&t, 1.0, 1.0, false, false).unwrap() - 6.0).abs() < 1e-5);
        assert!((rules::flash_duration(&t, 1.0, -1.0, false, false).unwrap() - 3.0).abs() < 1e-5);
        // The thrower: half the distance; Tactical Mask's flash protection x0.1.
        assert!((rules::flash_duration(&t, 1.0, 1.0, true, false).unwrap() - 3.0).abs() < 1e-5);
        assert!((rules::flash_duration(&t, 1.0, 1.0, false, true).unwrap() - 0.6).abs() < 1e-5);
        assert_eq!(rules::flash_duration(&t, 0.02, 1.0, false, false), None, "under 0.25 s: nothing");
        assert_eq!(rules::flash_distance_share(10.0, 33.0, 85.0), 1.0);
        assert!((rules::flash_distance_share(59.0, 33.0, 85.0) - 0.5).abs() < 1e-5);
        assert_eq!(rules::blast_shock(&t, 10.0), None);
        assert_eq!(rules::blast_shock(&t, 11.0), Some(0.5));
    }

    #[test]
    fn the_blur_trail_decays_over_its_blend_time() {
        assert_eq!(blur_alpha(0.0, 0.016), 0.0, "no blur, no trail");
        assert!((blur_alpha(0.4, 0.4) - (-1.0f32).exp()).abs() < 1e-6);
        // Frame-rate independent: two half frames keep what one whole frame keeps.
        assert!((blur_alpha(0.4, 0.008).powi(2) - blur_alpha(0.4, 0.016)).abs() < 1e-6);
        assert_eq!(blur_alpha(f32::NAN, 0.016), 0.0);
    }

    fn kicker(radius: f32) -> &'static ShockDef {
        Box::leak(Box::new(ShockDef { name: "k".into(), kick_period: 0.75, kick_radius: radius, ..Default::default() }))
    }

    #[test]
    fn the_view_kick_wobbles_within_its_radius_and_fades_out() {
        let d = kicker(0.1);
        let max = (0..400).map(|i| kick(d, 6.0, i as f32 * 0.01, 3.0)).fold(0.0f32, |m, (y, p)| m.max(y.abs()).max(p.abs()));
        // The cubic overshoots the noise's −1..1 a little; the tangent stays near the radius.
        assert!(max > 0.02 && max < 0.2, "{max}");
        // It moves: two moments a period apart differ.
        assert_ne!(kick(d, 6.0, 0.5, 3.0), kick(d, 6.0, 1.25, 3.0));
        // Continuous across a noise point (no jump at t = 1 period).
        let (a, b) = (kick(d, 6.0, 0.749, 3.0), kick(d, 6.0, 0.751, 3.0));
        assert!((a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01, "{a:?} {b:?}");
        // The last 3 s fade it as (3 - 2f)f²: 0.5 s from the end f = 1/6.
        let f = 1.0f32 / 6.0;
        let near_end = (0..50).map(|i| kick(d, 6.0, 5.5 + i as f32 * 0.0001, 3.0).0.abs()).fold(0.0f32, f32::max);
        assert!(near_end <= 0.2 * (3.0 - 2.0 * f) * f * f, "{near_end}");
        assert_eq!(kick(d, 6.0, 6.0, 3.0), (0.0, 0.0), "over at the end");
        assert_eq!(kick(kicker(0.0), 6.0, 1.0, 3.0), (0.0, 0.0), "no radius, no kick");
        // The same shock kicks the same way (deterministic presentation).
        assert_eq!(kick(d, 6.0, 1.0, 3.0), kick(d, 6.0, 1.0, 3.0));
    }

    fn ringer() -> &'static ShockDef {
        Box::leak(Box::new(ShockDef { name: "r".into(), sound_loop: "chr_flashbang_tinnitus_loop".into(), sound_fade_in: 0.25, sound_fade_out: 1.0, sound_mod_end: 0.1, ..Default::default() }))
    }

    #[test]
    fn the_ringing_fades_in_holds_and_fades_out_past_the_end() {
        let d = ringer();
        let mut s = Shellshock { active: Some(Active { def: d, duration: 1.0, left: 1.0, serial: 1 }), serial: 1, ..Default::default() };
        let mut p = Presented::default();
        let mut calls = Vec::new();
        let step = |p: &mut Presented, s: &Shellshock, dt: f32| {
            let mut last = None;
            p.update(s, 0.0, dt, &mut |e, g| last = Some((e, g)));
            last
        };
        let (event, g) = step(&mut p, &s, 0.125).unwrap();
        assert_eq!(event, loop_event("chr_flashbang_tinnitus_loop"));
        assert!((g - 0.5).abs() < 1e-5, "half the 0.25 s fade-in: {g}");
        calls.push(step(&mut p, &s, 0.2).unwrap().1);
        assert_eq!(calls[0], 1.0, "faded in");
        // The sim's shock ends; the loop holds for soundModEndDelay, then fades over soundFadeOut.
        s.active.as_mut().unwrap().left = 0.0;
        assert_eq!(step(&mut p, &s, 0.0).unwrap().1, 1.0);
        s.active = None;
        assert_eq!(step(&mut p, &s, 0.05).unwrap().1, 1.0, "within the 0.1 s mod end delay");
        let g = step(&mut p, &s, 0.5).unwrap().1;
        assert!((g - 0.5).abs() < 1e-5, "half the 1 s fade-out: {g}");
        assert_eq!(step(&mut p, &s, 0.6), Some((loop_event("chr_flashbang_tinnitus_loop"), 0.0)), "the stop");
        assert_eq!(step(&mut p, &s, 0.1), None, "nothing more to say");
        // A shock without a loop alias plays nothing.
        let quiet: &'static ShockDef = Box::leak(Box::new(ShockDef::default()));
        let s = Shellshock { active: Some(Active { def: quiet, duration: 1.0, left: 1.0, serial: 2 }), serial: 2, ..Default::default() };
        assert_eq!(step(&mut Presented::default(), &s, 0.1), None);
    }
}
