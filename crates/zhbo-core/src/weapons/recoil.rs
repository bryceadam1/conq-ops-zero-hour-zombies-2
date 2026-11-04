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
//! The view kick is an angular velocity the shot hands to [`super::view_kick::ViewKick`], whose
//! integrator recentres it. What G5 leaves open stays marked: the reduced-kick window's exact
//! meaning (G5, medium: here, the first `reduced_bullets` shots of a string).

use super::def::{FireMode, WeaponDef};
use super::feel::GunKick;
use super::rng::Rng;
use crate::tuning::Tuning;

/// One shot's kick: the view kick's angular velocity (deg/s, BO2's axes: pitch `-|p|` is up, yaw,
/// roll `-0.5 yaw`; `FUN_008e3a00` writes it to cg+0x697bc) and the gun spring's velocity
/// impulse (pitch, yaw).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShotKick {
    pub view: (f32, f32, f32),
    pub gun: (f32, f32),
}

/// Which guns are in hand for the kick: one, or a dual-wield pair (BO2's `FUN_008eb9c0`) with
/// the left gun's def (`szDualWieldWeaponName`, `FUN_008eca70`) and which hands are in a firing
/// weapon state now (`ps+0x1d0` right / `ps+0x1d4` left equal to 6 or 0x28). The dual-wield sim
/// derives the firing flags from each hand's own fire cycle; a single gun passes `Single`.
#[derive(Clone, Copy, Debug)]
pub enum Hands<'a> {
    Single,
    Dual { left: &'a WeaponDef, right_firing: bool, left_firing: bool },
}

impl Hands<'_> {
    /// The value a dual-wield read takes (`FUN_008e3a00`, `FUN_0051f150`): both hands' summed
    /// when both fire, the left gun's when only it fires, else the right (own) gun's.
    pub(crate) fn pick<T: std::ops::Add<Output = T>>(&self, own: &WeaponDef, get: impl Fn(&WeaponDef) -> T) -> T {
        match *self {
            Hands::Dual { left, right_firing: true, left_firing: true } => get(own) + get(left),
            Hands::Dual { left, left_firing: true, .. } => get(left),
            _ => get(own),
        }
    }
}

/// The kick of one shot of a single gun: [`shot_kick_hands`] with [`Hands::Single`].
pub fn shot_kick(def: &WeaponDef, ads: f32, string: u16, reduction: &mut f32, since_shot: f32, rng: &mut Rng) -> ShotKick {
    shot_kick_hands(def, Hands::Single, ads, string, reduction, since_shot, rng)
}

/// The kick of one shot. `ads` is the aim fraction, `string` the shots already fired in this
/// string, `reduction` the grip's accumulator (updated) and `since_shot` seconds since the last
/// shot. A dual-wield pair below full aim takes its hip view kick and (unaimed) hip gun kick
/// ranges from [`Hands::pick`] — summed over both guns when both fire — and has no minimum
/// magnitude (`FUN_008e3a00`'s dual branch skips it).
pub fn shot_kick_hands(def: &WeaponDef, hands: Hands, ads: f32, string: u16, reduction: &mut f32, since_shot: f32, rng: &mut Rng) -> ShotKick {
    let aimed = ads >= 1.0;
    let dual = matches!(hands, Hands::Dual { .. });
    // The reduced-kick window: the first bullets of a string kick at a percentage.
    let window = if aimed { &def.ads_gun_kick } else { &def.hip_gun_kick };
    let scale = if (string as f32) < window.reduced_bullets && window.reduced_percent > 0.0 { window.reduced_percent * 0.01 } else { 1.0 };

    // ---- view kick
    let (mut pitch, mut yaw) = if !aimed && dual {
        let p = hands.pick(def, |d| Pair(d.hip_kick.pitch)).0;
        let y = hands.pick(def, |d| Pair(d.hip_kick.yaw)).0;
        (rng.range(p), rng.range(y))
    } else {
        let (mut kick, min_mag) = if aimed { (def.ads_kick, def.view_kick_min.0) } else { (def.hip_kick, def.view_kick_min.1) };
        let (rate, limit) = def.recoil_reduction;
        let mut taken = 0.0;
        if aimed && rate > 0.0 {
            // The grip: the accumulator wears off at the return rate between shots and builds by
            // its rate per shot, up to its limit; the kick ranges shrink by what it holds.
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
        (pitch, yaw)
    };
    pitch = -pitch.abs() * scale;
    yaw *= scale;

    // ---- gun kick: the aimed set at any aim; unaimed, a dual pair's picked hip set.
    let (gp, gy) = if ads > 0.0 {
        (def.ads_gun_kick.pitch, def.ads_gun_kick.yaw)
    } else if dual {
        (hands.pick(def, |d| Pair(d.hip_gun_kick.pitch)).0, hands.pick(def, |d| Pair(d.hip_gun_kick.yaw)).0)
    } else {
        (def.hip_gun_kick.pitch, def.hip_gun_kick.yaw)
    };
    let gun = (rng.range(gp) * scale, rng.range(gy) * scale);
    ShotKick { view: (pitch, yaw, yaw * -0.5), gun }
}

/// A (min, max) range that sums component-wise (two hands' ranges add, `FUN_008e3a00`).
#[derive(Clone, Copy)]
pub(crate) struct Pair(pub (f32, f32));

impl std::ops::Add for Pair {
    type Output = Pair;
    fn add(self, o: Pair) -> Pair {
        Pair((self.0 .0 + o.0 .0, self.0 .1 + o.0 .1))
    }
}

/// The viewmodel's gun kick: angle and velocity per axis (pitch, yaw), a damped spring back to
/// centre (`FUN_008f0700`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GunKickSpring {
    pub angle: (f32, f32),
    pub vel: (f32, f32),
}

crate::statehash::state_hash!(struct GunKickSpring { angle, vel });

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

/// The damage flinch's view kick, `(yaw, pitch)` in degrees in the mod's view convention (yaw
/// counter-clockwise, pitch up), for a kick of `kick` degrees ([`flinch_degrees`]) by a blow that
/// travelled along `dir` (BO2's `vDir`; zero: none) into a view at `view_yaw`/`view_pitch`
/// (radians). BO2: `finishplayerdamage` (t6mp `FUN_005ed360`) keeps the blow's `vDir` as the
/// damage direction (none: "from the world"); `P_DamageFeedback` (`FUN_005d1800`) and
/// `CG_DamageFeedback` (`FUN_004e8ab0`) kick the view by it against the view's axes:
/// `yaw = -(dir · left) × kick`, `pitch = (dir · forward) × kick` in the engine's angles (pitch
/// down), and `pitch = -kick` (straight up) with no direction. So a blow from in front kicks the
/// view up, one from behind down, one from the left turns it left.
pub fn flinch_kick(kick: f32, dir: crate::math::Vec3, view_yaw: f32, view_pitch: f32) -> (f32, f32) {
    let d = dir.normalized_or_zero();
    if d == crate::math::Vec3::ZERO {
        return (0.0, kick);
    }
    let forward = crate::math::dir_from_angles(view_yaw, view_pitch);
    let (_, right) = crate::math::basis_xy(view_yaw);
    let left = right * -1.0;
    // BO2's pitch grows downward; the mod's upward.
    (-d.dot(left) * kick, -d.dot(forward) * kick)
}

/// The hip spread cone's min and max (degrees) at an eye height (inches; `FUN_008e41a0`): blended
/// prone -> crouched below the crouched height and crouched -> standing above it, x
/// `stackFireAccuracyDecay ^ (clip - 1)` for a stacked weapon with more than one round in its
/// stack (the x87 `pow` at `0x008e4319`; `tu12_zm_stack_fire_fix`, default off, would use the
/// def's stack count instead of the clip), x `perk_weapSpreadMultiplier` with `bulletaccuracy`.
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
    let stack = if def.fire == FireMode::Stacked && def.clip > 1 { crate::math::det::powi(def.traits.stack_fire.1, def.clip as i32 - 1) } else { 1.0 };
    let m = stack * if accuracy { t.perk_weap_spread_multiplier } else { 1.0 };
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

    /// A stack's hip cone is scaled by `stackFireAccuracyDecay ^ (clip - 1)` (`FUN_008e41a0`).
    #[test]
    fn a_stacks_hip_cone_takes_the_accuracy_decay_per_extra_round() {
        let t = Tuning::default();
        let mut d = WeaponDef { hip_spread: [(2.0, 4.0); 3], clip: 3, ..Default::default() };
        d.traits.stack_fire = (1.5, 0.5);
        assert_eq!(spread_range(&d, t.player_standing_view_height, false, &t), (2.0, 4.0), "not stacked: unscaled");
        d.fire = FireMode::Stacked;
        assert_eq!(spread_range(&d, t.player_standing_view_height, false, &t), (0.5, 1.0));
    }

    #[test]
    fn the_view_kick_is_hip_until_fully_aimed_kicks_up_and_rolls_against_yaw() {
        let d = gun();
        let mut rng = Rng::new(3);
        let mut acc = 0.0;
        let hip = shot_kick(&d, 0.99, 0, &mut acc, 1.0, &mut rng);
        assert!((-60.0..=-50.0).contains(&hip.view.0), "hip set until ADS is 1, kicking up: {hip:?}");
        assert!((5.0..=6.0).contains(&hip.gun.0), "the gun kick takes the aimed set at any aim: {hip:?}");
        let aimed = shot_kick(&d, 1.0, 0, &mut acc, 1.0, &mut rng);
        assert!((-40.0..=-30.0).contains(&aimed.view.0));
        assert!((aimed.view.2 + 0.5 * aimed.view.1).abs() < 1e-6, "roll = -0.5 yaw");
    }

    #[test]
    fn a_dual_wield_pair_sums_both_guns_kicks_while_both_fire() {
        let right = gun();
        let left = WeaponDef { hip_kick: Kick { pitch: (10.0, 10.0), yaw: (5.0, 5.0) }, hip_gun_kick: GunKick { pitch: (1.0, 1.0), yaw: (2.0, 2.0), ..right.hip_gun_kick }, ..gun() };
        let right = WeaponDef { hip_kick: Kick { pitch: (50.0, 50.0), yaw: (-20.0, -20.0) }, hip_gun_kick: GunKick { pitch: (9.0, 9.0), yaw: (0.5, 0.5), ..right.hip_gun_kick }, view_kick_min: (0.0, 500.0), ..right };
        let mut acc = 0.0;
        let mut rng = Rng::new(1);
        let both = shot_kick_hands(&right, Hands::Dual { left: &left, right_firing: true, left_firing: true }, 0.0, 0, &mut acc, 1.0, &mut rng);
        assert_eq!(both.view, (-60.0, -15.0, 7.5), "summed ranges, no minimum magnitude");
        assert_eq!(both.gun, (10.0, 2.5));
        let left_only = shot_kick_hands(&right, Hands::Dual { left: &left, right_firing: false, left_firing: true }, 0.0, 0, &mut acc, 1.0, &mut rng);
        assert_eq!((left_only.view.0, left_only.view.1, left_only.gun), (-10.0, 5.0, (1.0, 2.0)));
        let right_only = shot_kick_hands(&right, Hands::Dual { left: &left, right_firing: true, left_firing: false }, 0.0, 0, &mut acc, 1.0, &mut rng);
        assert_eq!((right_only.view.0, right_only.view.1), (-50.0, -20.0), "the minimum is the single gun's rule only");
        let single = shot_kick(&right, 0.0, 0, &mut acc, 1.0, &mut rng);
        assert!(single.view.0 < -400.0, "the single gun enforces its minimum: {single:?}");
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
        // The kick's direction (`FUN_005d1800`/`FUN_004e8ab0`): facing +X level, a blow from in
        // front (travelling -X) kicks the view up, from behind down, from the left (+Y side,
        // travelling -Y) turns it left, from the right right; none kicks it straight up.
        use crate::math::Vec3;
        let k = |d: Vec3| flinch_kick(10.0, d, 0.0, 0.0);
        let near = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4;
        assert!(near(k(Vec3::ZERO), (0.0, 10.0)));
        assert!(near(k(Vec3::new(-1.0, 0.0, 0.0)), (0.0, 10.0)), "front: up, as with no direction");
        assert!(near(k(Vec3::new(1.0, 0.0, 0.0)), (0.0, -10.0)), "behind: down");
        assert!(near(k(Vec3::new(0.0, -1.0, 0.0)), (10.0, 0.0)), "left: turns left");
        assert!(near(k(Vec3::new(0.0, 1.0, 0.0)), (-10.0, 0.0)), "right: turns right");
        // Turned to face +Y, a blow from +Y is from in front.
        assert!(near(flinch_kick(10.0, Vec3::new(0.0, -1.0, 0.0), std::f32::consts::FRAC_PI_2, 0.0), (0.0, 10.0)));
        let d = WeaponDef { hip_spread: [(3.0, 6.0), (2.0, 5.0), (1.0, 4.0)], ..Default::default() };
        assert_eq!(spread_range(&d, 60.0, false, &t), (3.0, 6.0));
        assert_eq!(spread_range(&d, 40.0, false, &t), (2.0, 5.0));
        assert_eq!(spread_range(&d, 11.0, false, &t), (1.0, 4.0));
        assert_eq!(spread_range(&d, 50.0, false, &t), (2.5, 5.5), "half-way crouched -> standing");
        let (lo, hi) = spread_range(&d, 60.0, true, &t);
        assert!((lo - 1.95).abs() < 1e-5 && (hi - 3.9).abs() < 1e-5, "bulletaccuracy x0.65");
    }
}
