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
//! * **screen** (`FUN_005105c0` flashed, `FUN_00510460` blurred): a flashed shock's whiteout is
//!   `min(remaining / screenFlashWhiteFadeTime, 1)` and its frozen screengrab
//!   `min(remaining / screenFlashShotFadeTime, 1)`; a blurred shock's blur is its
//!   `screenBlurBlendTime` amount, `× remaining / screenBlurBlendFadeTime` in that last stretch —
//!   on the HUD.
//!
//! A new shock replaces the one running (`shellshock` overwrites the player's index, start and
//! duration). Only the first-person General is shocked: Zero Hour's own units and a General under
//! RTS control keep Zero Hour's `disable(PARALYZED)` stun (the closest Zero Hour has), which
//! would freeze a first-person player outright.

use crate::units::LOGIC_DT;
use crate::weapons::table::{Field, FieldKind, Values};

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
    /// `soundLoop`: the ringing alias (not played yet: the converter does not carry it).
    pub sound_loop: String,
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
        get: |s| (s.flashed as u8).to_string(), set: |s, v| { s.flashed = num(&v)? != 0.0; Ok(()) } },
    Field { key: "Blur", doc: "`bg_shock_screenBlurBlendTime` and `bg_shock_screenBlurBlendFadeTime` (s).", kind: FieldKind::Nums(2),
        get: |s| format!("{} {}", s.blur, s.blur_fade), set: |s, v| { v.need(2)?; (s.blur, s.blur_fade) = (v.num(0), v.num(1)); Ok(()) } },
    Field { key: "Flash", doc: "`bg_shock_screenFlashWhiteFadeTime` and `bg_shock_screenFlashShotFadeTime` (s).", kind: FieldKind::Nums(2),
        get: |s| format!("{} {}", s.white_fade, s.shot_fade), set: |s, v| { v.need(2)?; (s.white_fade, s.shot_fade) = (v.num(0), v.num(1)); Ok(()) } },
    Field { key: "Look", doc: "`bg_shock_lookControl`, its max pitch and yaw speeds (deg/s), sensitivity scale and fade time (s).", kind: FieldKind::Nums(5),
        get: |s| format!("{} {} {} {} {}", s.look_control as u8, s.max_pitch_speed, s.max_yaw_speed, s.sensitivity, s.look_fade),
        set: |s, v| { v.need(5)?; s.look_control = v.flag(0); (s.max_pitch_speed, s.max_yaw_speed, s.sensitivity, s.look_fade) = (v.num(1), v.num(2), v.num(3), v.num(4)); Ok(()) } },
    Field { key: "Movement", doc: "`bg_shock_movement`: the run speed's scale while shocked.", kind: FieldKind::Num,
        get: |s| s.movement.to_string(), set: |s, v| { s.movement = num(&v)?; Ok(()) } },
    Field { key: "SoundLoop", doc: "`bg_shock_soundLoop`: the ringing alias.", kind: FieldKind::Str,
        get: |s| s.sound_loop.clone(), set: |s, v| { s.sound_loop = v.text.into(); Ok(()) } },
];

/// A shock running on a player: its def and the seconds left of its duration.
#[derive(Clone, Debug, PartialEq)]
pub struct Active {
    pub def: &'static ShockDef,
    pub duration: f32,
    pub left: f32,
}

/// The first-person General's shellshock state (in the sim, deterministic in the logic step).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shellshock {
    pub active: Option<Active>,
    /// A shock the script plays later (the Shock Charge's exit): seconds to wait, name, duration.
    pub queued: Option<(f32, &'static str, f32)>,
}

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
            self.active = Some(Active { def, duration, left: duration });
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
        let Some(a) = self.active.as_ref() else { return Screen::default() };
        let d = a.def;
        let share = |fade: f32| if fade <= 0.0 { 0.0 } else { (a.left / fade).min(1.0) };
        if d.flashed {
            Screen { white: share(d.white_fade), shot: share(d.shot_fade), blur: 0.0 }
        } else {
            let fade = if a.left < d.blur_fade { a.left / d.blur_fade.max(1e-6) } else { 1.0 };
            Screen { blur: d.blur * fade, ..Default::default() }
        }
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
        let mut s = Shellshock { active: Some(Active { def: def(true), duration: 6.0, left: 6.0 }), queued: None };
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
        let s = Shellshock { active: Some(Active { def: def(true), duration: 6.0, left: 3.0 }), queued: None };
        let sc = s.screen();
        assert!((sc.white - 0.4).abs() < 1e-6 && (sc.shot - 0.2).abs() < 1e-6, "{sc:?}");
        assert_eq!(Shellshock::default().screen(), Screen::default());
    }

    #[test]
    fn a_shock_runs_out_and_a_queued_one_follows() {
        let mut s = Shellshock { active: Some(Active { def: def(true), duration: 0.1, left: 0.1 }), queued: None };
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
}
