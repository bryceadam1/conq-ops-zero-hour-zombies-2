/*
** Conquer Ops - Zero Hour gameplay core: BO2's per-shot recoil, damage flinch and hip spread.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! BO2's recoil, from the engine (Ghidra G5, `~/Documents/BO2-reference/ghidra/answers/G5.md`):
//! each shot (`FUN_008e3a00`) rolls a **view kick** (camera pitch/yaw/roll, the aimed set only
//! when fully aimed) and a **gun kick** (an impulse into the viewmodel's own spring, the aimed
//! set at any aim). The spring (`FUN_008f0700`) integrates in 5 ms steps. Damage flinch is G7
//! (`FUN_004e8ab0`) and the hip spread cone G6 (`FUN_008e41a0`).
//!
//! What the answers leave open stays as it was and is marked: how the engine turns a view kick
//! into degrees and recentres it (G5 "Unresolved"; the client still scales by
//! [`super::KICK_TO_DEGREES`] and settles it), and the reduced-kick window's exact meaning
//! (G5, medium: here, the first `reduced_bullets` shots of a string).

use super::def::WeaponDef;
use super::feel::GunKick;
use super::rng::Rng;
use crate::tuning::Tuning;

/// One shot's kick: the view (pitch up, yaw, roll, in the weapon's kick units) and the gun
/// spring's velocity impulse (pitch, yaw).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShotKick {
    pub view: (f32, f32, f32),
    pub gun: (f32, f32),
}

/// The kick of one shot. `ads` is the aim fraction, `string` the shots already fired in this
/// string, `reduction` the grip's accumulator (updated) and `since_shot` seconds since the last
/// shot.
pub fn shot_kick(def: &WeaponDef, ads: f32, string: u16, reduction: &mut f32, since_shot: f32, rng: &mut Rng) -> ShotKick {
    let aimed = ads >= 1.0;
    // The reduced-kick window: the first bullets of a string kick at a percentage.
    let gun_set = if ads > 0.0 { &def.ads_gun_kick } else { &def.hip_gun_kick };
    let window = if aimed { &def.ads_gun_kick } else { &def.hip_gun_kick };
    let scale = if (string as f32) < window.reduced_bullets && window.reduced_percent > 0.0 { window.reduced_percent * 0.01 } else { 1.0 };

    // ---- view kick
    let (mut kick, min_mag) = if aimed { (def.ads_kick, def.view_kick_min.0) } else { (def.hip_kick, def.view_kick_min.1) };
    let (rate, limit) = def.recoil_reduction;
    let mut taken = 0.0;
    if aimed && rate > 0.0 {
        // The grip: the accumulator wears off at the return rate between shots and builds by its
        // rate per shot, up to its limit; the kick ranges shrink by what it holds.
        let acc = (*reduction - since_shot * def.recoil_return_rate).max(0.0);
        let acc = (acc + rate).clamp(0.0, limit);
        *reduction = acc;
        let k = 1.0 - acc;
        kick.pitch = (kick.pitch.0 * k, kick.pitch.1 * k);
        kick.yaw = (kick.yaw.0 * k, kick.yaw.1 * k);
        taken = rate;
    }
    let mut pitch = rng.range(kick.pitch);
    let mut yaw = rng.range(kick.yaw);
    let m = (1.0 - taken) * min_mag;
    let len = (pitch * pitch + yaw * yaw).sqrt();
    if len > 1e-6 && len < m {
        pitch *= m / len;
        yaw *= m / len;
    }
    let pitch = pitch.abs() * scale;
    let yaw = yaw * scale;

    // ---- gun kick
    let gun = (rng.range(gun_set.pitch) * scale, rng.range(gun_set.yaw) * scale);
    ShotKick { view: (pitch, yaw, yaw * -0.5), gun }
}

/// The viewmodel's gun kick: angle and velocity per axis (pitch, yaw), a damped spring back to
/// centre (`FUN_008f0700`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GunKickSpring {
    pub angle: (f32, f32),
    pub vel: (f32, f32),
}

/// The spring's step (`0.005` s).
pub const GUN_KICK_STEP: f32 = 0.005;

impl GunKickSpring {
    pub fn kick(&mut self, impulse: (f32, f32)) {
        self.vel.0 += impulse.0;
        self.vel.1 += impulse.1;
    }

    /// Advance `dt` seconds. The rates lerp hip -> aimed by `ads`; each axis is clamped to the
    /// weapon's `fGunMax{Pitch,Yaw}`.
    pub fn step(&mut self, dt: f32, hip: &GunKick, aimed: &GunKick, ads: f32, max: (f32, f32)) {
        let lerp = |a: f32, b: f32| a + (b - a) * ads;
        let accel = lerp(hip.accel, aimed.accel);
        let vmax = lerp(hip.speed_max, aimed.speed_max);
        let sdecay = lerp(hip.speed_decay, aimed.speed_decay);
        let stdec = lerp(hip.static_decay, aimed.static_decay);
        let mut left = dt;
        while left > 1e-6 {
            let step = left.min(GUN_KICK_STEP);
            left -= step;
            for (angle, vel, limit) in [(&mut self.angle.0, &mut self.vel.0, max.0), (&mut self.angle.1, &mut self.vel.1, max.1)] {
                if angle.abs() < 0.25 && vel.abs() < 1.0 {
                    *angle = 0.0;
                    *vel = 0.0;
                    continue;
                }
                *angle += step * *vel;
                if limit > 0.0 && angle.abs() > limit {
                    *angle = angle.signum() * limit;
                    if *vel * angle.signum() > 0.0 {
                        *vel = 0.0;
                    }
                }
                *vel -= angle.signum() * step * accel;
                *vel -= *vel * sdecay * step;
                let friction = step * stdec;
                *vel = if vel.abs() <= friction { 0.0 } else { *vel - vel.signum() * friction };
                if vmax > 0.0 {
                    *vel = vel.clamp(-vmax, vmax);
                }
            }
        }
    }
}

/// Damage flinch in degrees (`FUN_004e8ab0`): `damage x bg_viewKickScale`, x
/// `perk_damageKickReduction` with `bulletflinch` (Toughness), clamped to
/// `[bg_viewKickMin, bg_viewKickMax]`. `damage` is on BO2's 100-health scale. With no damage
/// direction (the sim has none) the engine kicks pitch only.
pub fn flinch_degrees(damage: f32, toughness: bool, t: &Tuning) -> f32 {
    let mut kick = damage * t.bg_view_kick_scale;
    if toughness {
        kick *= t.perk_damage_kick_reduction;
    }
    kick.clamp(t.bg_view_kick_min, t.bg_view_kick_max)
}

/// The hip spread cone's min and max (degrees) at an eye height (inches; `FUN_008e41a0`): blended
/// prone -> crouched below the crouched height and crouched -> standing above it, x
/// `perk_weapSpreadMultiplier` with `bulletaccuracy`.
pub fn spread_range(def: &WeaponDef, view_height: f32, accuracy: bool, t: &Tuning) -> (f32, f32) {
    let [stand, ducked, prone] = def.hip_spread;
    let lerp = |a: (f32, f32), b: (f32, f32), f: f32| (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f);
    let (min, max) = if view_height <= t.view_height_crouch {
        let f = ((view_height - t.view_height_prone) / (t.view_height_crouch - t.view_height_prone).max(1e-3)).clamp(0.0, 1.0);
        lerp(prone, ducked, f)
    } else {
        let f = ((view_height - t.view_height_crouch) / (t.player_standing_view_height - t.view_height_crouch).max(1e-3)).clamp(0.0, 1.0);
        lerp(ducked, stand, f)
    };
    let m = if accuracy { t.perk_weap_spread_multiplier } else { 1.0 };
    (min * m, max * m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapons::def::Kick;

    fn gun() -> WeaponDef {
        let mut d = WeaponDef { ads_kick: Kick { pitch: (30.0, 40.0), yaw: (-10.0, 10.0) }, hip_kick: Kick { pitch: (50.0, 60.0), yaw: (-20.0, 20.0) }, ..Default::default() };
        d.ads_gun_kick = GunKick { pitch: (5.0, 6.0), yaw: (-1.0, 1.0), accel: 800.0, speed_max: 2000.0, speed_decay: 16.0, static_decay: 40.0, reduced_bullets: 0.0, reduced_percent: 0.0 };
        d.hip_gun_kick = GunKick { pitch: (9.0, 10.0), ..d.ads_gun_kick };
        d.gun_max = (10.0, 10.0);
        d
    }

    #[test]
    fn the_view_kick_is_hip_until_fully_aimed_kicks_up_and_rolls_against_yaw() {
        let d = gun();
        let mut rng = Rng::new(3);
        let mut acc = 0.0;
        let hip = shot_kick(&d, 0.99, 0, &mut acc, 1.0, &mut rng);
        assert!((50.0..=60.0).contains(&hip.view.0), "hip set until ADS is 1: {hip:?}");
        assert!((5.0..=6.0).contains(&hip.gun.0), "the gun kick takes the aimed set at any aim: {hip:?}");
        let aimed = shot_kick(&d, 1.0, 0, &mut acc, 1.0, &mut rng);
        assert!((30.0..=40.0).contains(&aimed.view.0));
        assert!((aimed.view.2 + 0.5 * aimed.view.1).abs() < 1e-6, "roll = -0.5 yaw");
    }

    #[test]
    fn the_grip_builds_and_wears_off() {
        let mut d = gun();
        d.recoil_reduction = (0.1, 0.4);
        d.recoil_return_rate = 1.0;
        let mut rng = Rng::new(5);
        let mut acc = 0.0;
        for _ in 0..10 {
            shot_kick(&d, 1.0, 0, &mut acc, 0.05, &mut rng);
        }
        assert!((acc - 0.4).abs() < 1e-6, "capped at the limit: {acc}");
        shot_kick(&d, 1.0, 0, &mut acc, 0.35, &mut rng);
        assert!((acc - 0.15).abs() < 1e-5, "0.4 - 0.35 s x 1.0 + 0.1: {acc}");
    }

    #[test]
    fn reduced_kick_scales_the_first_bullets() {
        let mut d = gun();
        d.hip_gun_kick.reduced_bullets = 2.0;
        d.hip_gun_kick.reduced_percent = 50.0;
        let mut acc = 0.0;
        let first = shot_kick(&d, 0.0, 0, &mut acc, 1.0, &mut Rng::new(9));
        let later = shot_kick(&d, 0.0, 2, &mut acc, 1.0, &mut Rng::new(9));
        assert!((first.view.0 * 2.0 - later.view.0).abs() < 1e-4);
    }

    #[test]
    fn the_gun_kick_springs_back_to_centre() {
        let d = gun();
        let mut s = GunKickSpring::default();
        s.kick((200.0, 0.0));
        s.step(0.02, &d.hip_gun_kick, &d.ads_gun_kick, 0.0, d.gun_max);
        assert!(s.angle.0 > 0.0 && s.angle.0 <= 10.0, "{s:?}");
        for _ in 0..200 {
            s.step(0.02, &d.hip_gun_kick, &d.ads_gun_kick, 0.0, d.gun_max);
        }
        assert_eq!(s, GunKickSpring::default(), "settled");
    }

    #[test]
    fn flinch_and_spread_follow_the_engine() {
        let t = Tuning::default();
        assert!((flinch_degrees(40.0, false, &t) - 7.0).abs() < 1e-4, "40 x 0.175");
        assert!((flinch_degrees(40.0, true, &t) - 1.75).abs() < 1e-4, "Toughness x0.25");
        assert_eq!(flinch_degrees(1.0, false, &t), 0.5, "floor");
        let d = WeaponDef { hip_spread: [(3.0, 6.0), (2.0, 5.0), (1.0, 4.0)], ..Default::default() };
        assert_eq!(spread_range(&d, 60.0, false, &t), (3.0, 6.0));
        assert_eq!(spread_range(&d, 40.0, false, &t), (2.0, 5.0));
        assert_eq!(spread_range(&d, 11.0, false, &t), (1.0, 4.0));
        assert_eq!(spread_range(&d, 50.0, false, &t), (2.5, 5.5), "half-way crouched -> standing");
        let (lo, hi) = spread_range(&d, 60.0, true, &t);
        assert!((lo - 1.95).abs() < 1e-5 && (hi - 3.9).abs() < 1e-5, "bulletaccuracy x0.65");
    }
}
