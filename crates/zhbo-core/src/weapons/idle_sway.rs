/*
** Conquer Ops - Zero Hour gameplay core: BO2's idle sway and hold breath.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! BO2's idle sway (the slow drift of a scope while aimed) and hold breath, ported from read-only
//! t6mp decompiles (dumps in the item's plan dir, `ghidra/`):
//!
//! - **The clock** (`FUN_008e3940`, pmove): a per-player counter (`ps+0x5c`) runs at the weapon's
//!   idle speed × 1000 per second, `adsIdleSpeed`/`hipIdleSpeed` lerped by the ADS fraction
//!   (`FUN_008e38a0`, the hip speed alone for a weapon that does not aim). While aimed, a weapon
//!   with an overlay scope (`FUN_008ed670`: the overlay reticle) adds a second run scaled by the
//!   breath (`FUN_008e3810`) — so a scope's clock runs at twice the rate. The other run takes the
//!   breath scale halved above 1 (`(s − 1)·0.5 + 1`). At the hip the client advances it the same
//!   way (`FUN_00530e70`).
//! - **The view sway** (`FUN_008f0f00`): while aimed with an overlay scope, the idle amount
//!   (`fHipIdleAmount` lerped to `fAdsIdleAmount` × Π scales by the ADS fraction; 80 when a gun
//!   that cannot aim has none) × the stance factor × the ADS fraction × the breath scale gives
//!   `yaw += sin(t·0.0007)·A·0.01` and `pitch += sin(t·0.001)·A·0.01` degrees — a slow
//!   Lissajous figure. The server adds the same angles to the fire direction (`FUN_005dc820`), so
//!   the bullets follow the drifting reticle: the sim owns it, like the view kick.
//! - **The stance factor** eases toward `fIdleProneFactor` / `fIdleCrouchFactor` / 1 at 0.5 per
//!   second (`FUN_008f0f00`, `FUN_008eefc0`).
//! - **The gun's idle** (`FUN_008eefc0`, the viewmodel): the same amount without the breath, × (1 −
//!   ADS) for an overlay scope (its view sways instead), on the gun's roll `sin(t·0.0005)`, yaw
//!   `sin(t·0.0007)` and pitch `sin(t·0.001)`. [`gun_angles`]; client-side.
//! - **Hold breath** (`FUN_008e4860`): see [`Breath`].

use super::def::WeaponDef;
use crate::math::det;
use crate::tuning::Tuning;

/// `DAT_00c733ac`: the clock's 1000 per second.
const CLOCK_RATE: f32 = 1000.0;
/// `DAT_00c73410`: idle amount to degrees.
const AMOUNT_TO_DEG: f32 = 0.01;
/// `DAT_00c733b0`, `DAT_00bdd524`, `DAT_00c73658`: the pitch, yaw and roll frequencies (per clock
/// unit).
const PITCH_FREQ: f32 = 0.001;
const YAW_FREQ: f32 = 0.0007;
const ROLL_FREQ: f32 = 0.0005;
/// `DAT_00c733cc`: the stance factor's rate (per second) and the halving of a gasp's clock boost.
const HALF: f32 = 0.5;
/// `DAT_00b12cac`: a non-aiming gun's view idle amount when it authors none.
const DEFAULT_AMOUNT: f32 = 80.0;
/// `DAT_00c733b0`: [`approach`]'s snap distance.
const SNAP: f32 = 0.001;

/// The overlay scope test (`FUN_008ed670`): the view sways instead of the gun.
pub fn sways_view(def: &WeaponDef) -> bool {
    def.overlay_reticle != 0
}

/// `FUN_00747b40`: step `cur` toward `target` by `(target − cur)·rate·dt`, snapping once within
/// 0.001 or when the step would overshoot.
pub fn approach(target: f32, cur: f32, rate: f32, dt: f32) -> f32 {
    let gap = target - cur;
    let step = gap * rate * dt;
    if gap.abs() > SNAP && step.abs() <= gap.abs() {
        cur + step
    } else {
        target
    }
}

/// The weapon's idle speed at this ADS fraction (`FUN_008e3810`/`FUN_008e38a0`).
fn speed(def: &WeaponDef, ads: f32) -> f32 {
    let (a, h) = def.zoom.idle_speed;
    if def.class.is_gun() {
        h + (a - h) * ads
    } else {
        h
    }
}

/// The idle amount before the stance and breath (`FUN_008f0f00`'s `aimDownSight` lerp; the 80
/// fallback is the view path's only).
fn amount(def: &WeaponDef, ads: f32, view: bool) -> f32 {
    let (a, h) = def.zoom.idle_amount;
    if def.class.is_gun() {
        h + (a - h) * ads
    } else if view && h == 0.0 {
        DEFAULT_AMOUNT
    } else {
        h
    }
}

/// BO2's hold breath (`FUN_008e4860`, every pmove frame). The breath key (`+breath_sprint`, the
/// sprint key) holds at full ADS on a def with `bHoldBreathToSteady`, but only from a full breath
/// (`used == 0`). While held, `used` counts up; past `player_breath_hold_time` (+
/// `perk_extraBreath` with `holdbreath`) it is set to hold + gasp time and the breath lets go.
/// Otherwise it counts back down. The sway scale eases to 0 while holding (at
/// `player_breath_hold_lerp`), else to `(gasp_scale − 1)·used/(hold + gasp) + 1` (at
/// `player_breath_gasp_lerp`): after a breath the scope sways *more* until the lungs recover.
/// Both targets fade toward 1 by the ADS fraction. A shot lets the breath go (`FUN_008dea30`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Breath {
    /// `ps+0x10` bit 4.
    pub holding: bool,
    /// `ps+0x4e4`, seconds.
    pub used: f32,
    /// `ps+0x4e0`: scales the view sway and its clock.
    pub scale: f32,
}

impl Default for Breath {
    fn default() -> Self {
        Breath { holding: false, used: 0.0, scale: 1.0 }
    }
}

/// The hold-time inputs for one frame.
#[derive(Clone, Copy, Debug)]
pub struct BreathIn {
    /// The breath key is down.
    pub key: bool,
    /// The player has `specialty_holdbreath` (class perk or the weapon's).
    pub extra: bool,
    /// The anti-quickscope timer is running with `bg_aqs` on: no breath (set by
    /// [`IdleSway::step`]).
    pub aqs_block: bool,
}

impl Breath {
    /// The breath's hold time and gasp time (seconds).
    pub fn times(t: &Tuning, extra: bool) -> (f32, f32) {
        let hold = t.player_breath_hold_time + if extra { t.perk_extra_breath } else { 0.0 };
        (hold, t.player_breath_gasp_time)
    }

    pub fn step(&mut self, def: &WeaponDef, ads: f32, i: BreathIn, t: &Tuning, dt: f32) {
        let (hold, gasp) = Self::times(t, i.extra);
        // `iVar6 < 1`: a breath under a millisecond means no hold breath at all.
        if hold < 0.001 {
            *self = Breath::default();
            return;
        }
        let can = ads >= 1.0 && def.class.is_gun() && def.zoom.hold_breath;
        if can && i.key && !i.aqs_block {
            if self.used == 0.0 {
                self.holding = true;
            }
        } else {
            self.holding = false;
        }
        self.used = if self.holding { self.used + dt } else { (self.used - dt).max(0.0) };
        if self.holding && self.used > hold {
            self.used = gasp + hold;
            self.holding = false;
        }
        let (target, rate) = if self.holding {
            (0.0, t.player_breath_hold_lerp)
        } else {
            ((t.player_breath_gasp_scale - 1.0) * (self.used / (gasp + hold)) + 1.0, t.player_breath_gasp_lerp)
        };
        self.scale = approach((target - 1.0) * ads + 1.0, self.scale, rate, dt);
    }

    /// A shot (`FUN_008dea30`, from the fire start): on an overlay scope at full ADS it costs
    /// `player_breath_fire_delay` of breath (capped at the hold time) and lets the breath go.
    pub fn fired(&mut self, def: &WeaponDef, ads: f32, t: &Tuning, extra: bool) {
        if ads >= 1.0 && sways_view(def) && def.class.is_gun() {
            let (hold, _) = Self::times(t, extra);
            if self.used < hold {
                self.used = (self.used + t.player_breath_fire_delay).min(hold);
            }
            self.holding = false;
        }
    }
}

/// The idle sway's state: the clock, the eased stance factor, the breath and the view angles it
/// produced this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IdleSway {
    /// `ps+0x5c`, clock units (1000 per second at speed 1).
    pub time: f32,
    /// The eased stance factor (`cg+0x4ae48` / the server's copy).
    pub stance: f32,
    pub breath: Breath,
    /// The anti-quickscope timer (`ps+0x4f8`, seconds left) and its sway factor (`ps+0x4f4`).
    pub aqs_left: f32,
    pub aqs: f32,
    /// BO2's view idle angles this frame (degrees: pitch down, yaw left).
    pub angles: [f32; 2],
}

impl Default for IdleSway {
    fn default() -> Self {
        // The stance factor starts settled on standing.
        IdleSway { time: 0.0, stance: 1.0, breath: Breath::default(), aqs_left: 0.0, aqs: 1.0, angles: [0.0; 2] }
    }
}

impl IdleSway {
    /// One logic frame, after the weapon's ADS update. `stance` 0 stand, 1 crouch, 2 prone.
    #[allow(clippy::too_many_arguments)]
    pub fn step(&mut self, def: &WeaponDef, ads: f32, prev_ads: f32, stance: usize, breath: BreathIn, t: &Tuning, dt: f32) {
        let aqs_on = t.bg_aqs != 0.0;
        let breath = BreathIn { aqs_block: aqs_on && self.aqs_left > 0.0, ..breath };
        self.breath.step(def, ads, breath, t, dt);
        let view = sways_view(def);
        // The clock (`FUN_008e3940`; the hip run is the client's `FUN_00530e70`, the same rule).
        let s = self.breath.scale;
        let rate = speed(def, ads) * CLOCK_RATE * dt;
        if ads != 0.0 && view {
            self.time += s * rate;
        }
        self.time += if s > 1.0 { (s - 1.0) * HALF + 1.0 } else { s } * rate;
        // The stance factor eases at 0.5/s.
        let target = match stance {
            2 => def.zoom.idle_stance.1,
            1 => def.zoom.idle_stance.0,
            _ => 1.0,
        };
        let step = dt * HALF;
        self.stance = if self.stance < target { (self.stance + step).min(target) } else { (self.stance - step).max(target) };
        self.step_aqs(def, ads, prev_ads, t, dt);
        // The view's idle angles (`FUN_008f0f00`), with the anti-quickscope's push: style 1
        // scales the amount, style 2 adds `(aqs − 1)·factor·0.01` to the pitch and takes
        // `(aqs − 1)·factor·sin(t·0.0007)·0.01` off the yaw.
        self.angles = [0.0; 2];
        if ads != 0.0 && view {
            let style = t.bg_aqs_style as i32;
            let mut a = self.stance * amount(def, ads, true) * ads * s;
            if aqs_on && style == 1 {
                a *= self.aqs;
            }
            let (sp, sy) = (det::sin(self.time * PITCH_FREQ), det::sin(self.time * YAW_FREQ));
            self.angles = [sp * a * AMOUNT_TO_DEG, sy * a * AMOUNT_TO_DEG];
            if aqs_on && style == 2 {
                let push = (self.aqs - 1.0) * def.zoom.anti_quickscope.sway_factor * AMOUNT_TO_DEG;
                self.angles[0] += push;
                self.angles[1] -= push * sy;
            }
        }
    }

    /// BO2's anti-quickscope (`FUN_008dac10`, after the idle clock): on a variant with
    /// `bAntiQuickScope` and `fAntiQuickScopeTime` of a millisecond or more, the timer sits at the
    /// full time while the scope comes up, counts down once fully up and drops to 0 when the aim
    /// falls or is under 0.001. The factor eases (at `player_breath_gasp_lerp`) toward
    /// `((left/time)·(scale − 1))·posFrac + 1`, never below 1.
    fn step_aqs(&mut self, def: &WeaponDef, ads: f32, prev_ads: f32, t: &Tuning, dt: f32) {
        let q = &def.zoom.anti_quickscope;
        if q.time < 0.001 || !q.enabled {
            self.aqs_left = 0.0;
            self.aqs = 1.0;
            return;
        }
        self.aqs_left = if prev_ads <= ads && ads >= SNAP {
            if ads >= 1.0 { self.aqs_left - dt } else { q.time }
        } else {
            0.0
        }
        .max(0.0);
        let target = (self.aqs_left / q.time) * (q.scale - 1.0) * ads + 1.0;
        self.aqs = approach(target, self.aqs, t.player_breath_gasp_lerp, dt).max(1.0);
    }

    /// The aim offset in the mod's convention, radians: (yaw left, pitch up).
    pub fn aim_offset(&self) -> (f32, f32) {
        (self.angles[1].to_radians(), -self.angles[0].to_radians())
    }
}

/// The gun's idle angles (`FUN_008eefc0`; degrees pitch, yaw, roll) at clock `time` and eased
/// stance factor `stance`.
pub fn gun_angles(def: &WeaponDef, ads: f32, time: f32, stance: f32) -> [f32; 3] {
    let mut a = stance * amount(def, ads, false);
    if sways_view(def) {
        a *= 1.0 - ads;
    }
    a *= AMOUNT_TO_DEG;
    [det::sin(time * PITCH_FREQ) * a, det::sin(time * YAW_FREQ) * a, det::sin(time * ROLL_FREQ) * a]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapons::Class;

    const DT: f32 = crate::units::LOGIC_DT;

    fn sniper() -> WeaponDef {
        let mut d = WeaponDef { class: Class::Sniper, overlay_reticle: 1, ..Default::default() };
        d.zoom.idle_amount = (60.0, 20.0);
        d.zoom.idle_speed = (1.0, 0.5);
        d.zoom.idle_stance = (0.5, 0.25);
        d.zoom.hold_breath = true;
        d
    }

    fn held(key: bool) -> BreathIn {
        BreathIn { key, extra: false, aqs_block: false }
    }

    #[test]
    fn approach_steps_and_snaps() {
        assert_eq!(approach(1.0, 0.0, 2.0, 0.1), 0.2);
        assert_eq!(approach(1.0, 0.9995, 2.0, 0.1), 1.0, "within 0.001: snap");
        assert_eq!(approach(1.0, 0.0, 20.0, 0.1), 1.0, "an overshooting step snaps");
    }

    #[test]
    fn a_scope_sways_a_lissajous_at_its_amount_and_period() {
        let d = sniper();
        let t = Tuning::default();
        let mut s = IdleSway::default();
        let (mut max_p, mut max_y) = (0.0f32, 0.0f32);
        let mut crossings = 0;
        let mut last = 0.0;
        let frames = (20.0 / DT) as usize;
        for _ in 0..frames {
            s.step(&d, 1.0, 1.0, 0, held(false), &t, DT);
            max_p = max_p.max(s.angles[0].abs());
            max_y = max_y.max(s.angles[1].abs());
            if last < 0.0 && s.angles[0] >= 0.0 {
                crossings += 1;
            }
            last = s.angles[0];
        }
        // Full ADS, standing, breath 1: A = 60 × 0.01 = 0.6°.
        assert!((max_p - 0.6).abs() < 0.01 && (max_y - 0.6).abs() < 0.01, "{max_p} {max_y}");
        // The clock runs at 2 × 1000 × adsIdleSpeed: the pitch period is 2π / 0.001 / 2000 = π s.
        let want = (20.0 / std::f32::consts::PI) as i32;
        assert!((crossings - want).abs() <= 1, "{crossings} pitch periods in 20 s, want ~{want}");
        assert!((s.time - frames as f32 * DT * 2000.0).abs() < 1.0, "{}", s.time);
    }

    #[test]
    fn stance_scales_ease_in_and_the_hip_has_no_view_sway() {
        let d = sniper();
        let t = Tuning::default();
        let mut s = IdleSway::default();
        s.step(&d, 0.0, 0.0, 0, held(false), &t, DT);
        assert_eq!(s.angles, [0.0, 0.0], "no view sway from the hip");
        for _ in 0..(3.0 / DT) as usize {
            s.step(&d, 1.0, 1.0, 2, held(false), &t, DT);
        }
        assert_eq!(s.stance, 0.25, "prone factor, reached at 0.5/s");
        let mut half = IdleSway::default();
        half.step(&d, 1.0, 1.0, 1, held(false), &t, 1.0);
        assert_eq!(half.stance, 0.5);
    }

    #[test]
    fn holding_breath_steadies_for_the_hold_time_then_gasps() {
        let d = sniper();
        let t = Tuning::default();
        let mut s = IdleSway::default();
        let mut frames_held = 0;
        let mut gasped = false;
        for f in 0..(8.0 / DT) as usize {
            s.step(&d, 1.0, 1.0, 0, held(true), &t, DT);
            if s.breath.holding {
                frames_held += 1;
                if f > 60 && !gasped {
                    assert!(s.breath.scale < 0.05, "steady after 2 s of holding: {}", s.breath.scale);
                    assert!(s.angles[0].abs() < 0.03, "{:?}", s.angles);
                }
            } else if frames_held > 0 && !gasped {
                gasped = true;
                assert!((s.breath.used - 4.5).abs() < 1e-3, "out of breath: used = hold + gasp time");
            }
        }
        let held_s = frames_held as f32 * DT;
        assert!((held_s - 4.5).abs() <= DT + 1e-3, "held {held_s} s");
        assert!(gasped);
        // Still holding the key: no second breath until the lungs recover (used back to 0).
        assert!(!s.breath.holding && s.breath.used > 0.0);
        // The gasp sways harder than at rest.
        let mut g = IdleSway { breath: Breath { holding: false, used: 4.5, scale: 1.0 }, ..Default::default() };
        g.step(&d, 1.0, 1.0, 0, held(false), &t, DT);
        for _ in 0..30 {
            g.step(&d, 1.0, 1.0, 0, held(false), &t, DT);
        }
        assert!(g.breath.scale > 1.8, "gasp scale toward 2.5: {}", g.breath.scale);
        for _ in 0..(6.0 / DT) as usize {
            g.step(&d, 1.0, 1.0, 0, held(false), &t, DT);
        }
        assert_eq!(g.breath, Breath::default(), "recovered");
    }

    #[test]
    fn hold_breath_needs_full_ads_the_flag_and_the_perk_extends_it() {
        let t = Tuning::default();
        let mut b = Breath::default();
        b.step(&sniper(), 0.9, held(true), &t, DT);
        assert!(!b.holding, "not until fully aimed");
        let no_flag = WeaponDef { zoom: crate::weapons::Zoom { hold_breath: false, ..sniper().zoom }, ..sniper() };
        b.step(&no_flag, 1.0, held(true), &t, DT);
        assert!(!b.holding);
        let mut b = Breath::default();
        let mut n = 0;
        while {
            b.step(&sniper(), 1.0, BreathIn { key: true, extra: true, aqs_block: false }, &t, DT);
            b.holding
        } {
            n += 1;
        }
        assert!(((n + 1) as f32 * DT - 9.5).abs() <= DT + 1e-3, "4.5 + perk_extraBreath 5: {n} frames");
    }

    #[test]
    fn a_shot_lets_the_breath_go() {
        let d = sniper();
        let t = Tuning::default();
        let mut b = Breath::default();
        for _ in 0..30 {
            b.step(&d, 1.0, held(true), &t, DT);
        }
        assert!(b.holding);
        b.fired(&d, 1.0, &t, false);
        assert!(!b.holding);
        b.step(&d, 1.0, held(true), &t, DT);
        assert!(!b.holding, "the key cannot take a new breath until it recovers");
    }

    #[test]
    fn the_anti_quickscope_pushes_the_fresh_scope_and_blocks_the_breath() {
        // The XPR-50's numbers: 0.1 s at scale 4, sway factor 20.
        let mut d = sniper();
        d.zoom.anti_quickscope = crate::weapons::AntiQuickScope { enabled: true, time: 0.1, scale: 4.0, spread_multiplier: 1.0, spread_max: 25.0, sway_factor: 20.0 };
        let t = Tuning::default();
        let mut s = IdleSway::default();
        // Coming up: the timer holds full.
        s.step(&d, 0.5, 0.0, 0, held(false), &t, DT);
        assert_eq!(s.aqs_left, 0.1);
        assert!(s.aqs > 1.0);
        // Fully up: counts down; the breath key is refused while it runs.
        s.step(&d, 1.0, 0.5, 0, held(true), &t, DT);
        assert!(s.aqs_left > 0.0 && !s.breath.holding);
        let pushed = s.angles[0] - (s.time * PITCH_FREQ).sin() * 0.6 * s.breath.scale;
        assert!(pushed > 0.0, "style 2 pushes the pitch: {pushed}");
        for _ in 0..60 {
            s.step(&d, 1.0, 1.0, 0, held(false), &t, DT);
        }
        assert_eq!((s.aqs_left, s.aqs), (0.0, 1.0), "settled");
        // Off without the variant flag.
        let mut off = IdleSway::default();
        off.step(&sniper(), 0.5, 0.0, 0, held(false), &t, DT);
        assert_eq!((off.aqs_left, off.aqs), (0.0, 1.0));
    }

    #[test]
    fn the_gun_idles_from_the_hip_and_a_scope_hands_it_to_the_view() {
        let d = sniper();
        let hip = gun_angles(&d, 0.0, 1000.0, 1.0);
        assert!((hip[0] - 1.0f32.sin() * 0.2).abs() < 1e-6 && (hip[2] - 0.5f32.sin() * 0.2).abs() < 1e-6, "{hip:?}");
        assert_eq!(gun_angles(&d, 1.0, 1000.0, 1.0), [0.0; 3]);
        let iron = WeaponDef { overlay_reticle: 0, ..sniper() };
        let aimed = gun_angles(&iron, 1.0, 1000.0, 1.0);
        assert!((aimed[1] - 0.7f32.sin() * 0.6).abs() < 1e-6, "{aimed:?}");
    }
}
