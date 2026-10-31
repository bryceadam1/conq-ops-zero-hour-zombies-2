/*
** Conquer Ops - Zero Hour gameplay core: BO2's view kick, the camera recoil that moves the aim.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! BO2's view kick, from the engine (t6mp; the dumps are in the view-kick plan's `ghidra/`):
//!
//! * A shot **sets** the kick's angular velocity (`FUN_008e3a00` writes cg+0x697bc, called from
//!   the fire event `FUN_00539df0`): pitch `-|p|`, yaw `y`, roll `-0.5 y` in deg/s, where `p`/`y`
//!   are G5's per-shot roll of `f{Ads,Hip}ViewKick{Pitch,Yaw}{Min,Max}`
//!   ([`super::recoil::shot_kick`]). It replaces the velocity a previous shot left.
//! * The client integrator `FUN_0051f150` runs each frame in steps of at most 5 ms per axis: the
//!   velocity accelerates back toward centre by the recentre speed (deg/s², [`centre_speed`]), a
//!   move toward centre covers only [`TOWARD_CENTRE`] of its distance, the angle stops dead at the
//!   centre and is clamped to ±[`ANGLE_MAX`] degrees.
//! * The angles are added to the view angles of every user command (`FUN_004b0fd0` →
//!   `FUN_00543220` → clientActive+0x2c00; `FUN_00549fb0`: `angles = ANGLE2SHORT(viewangles +
//!   kick)`), so they move the aim the next bullets fly along — never `cl.viewangles` itself:
//!   BO2's recoil all recentres; there is no permanent part. That is why the sim owns it.
//!
//! Axes are BO2's: pitch negative is up, yaw positive is left (the mod's yaw), roll.

use super::def::WeaponDef;

/// The integrator's step, ms (`FUN_0051f150`: `min(msec, 5)`).
pub const STEP_MS: f32 = 5.0;
/// A move toward centre covers this share of its distance (`DAT_00c6f680`).
pub const TOWARD_CENTRE: f32 = 0.06;
/// The kick angle's limit per axis, degrees (`DAT_00c7340c`).
pub const ANGLE_MAX: f32 = 10.0;
/// Above this aim fraction the aimed recentre speed applies (`DAT_00c733cc`).
pub const ADS_SPLIT: f32 = 0.5;

/// The view kick: angular velocity (deg/s) and angle (degrees) per axis, pitch/yaw/roll.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewKick {
    pub vel: [f32; 3],
    pub angle: [f32; 3],
}

/// The recentre speed (deg/s²) for the weapon in hand at an aim fraction and stance (0 stand,
/// 1 crouch, 2 prone), as `FUN_0051f150` picks it. Aimed (above [`ADS_SPLIT`]): the variant's
/// aimed speed × Π attachment scale (`FUN_008ecf70`) × the crouched/prone scale (`FUN_008bfa60`:
/// 1 prone → `fAdsViewKickCenterProneScale`, 2 crouched → `…DuckedScale`). At the hip the client
/// reads the same aimed field, unscaled (`variant+0x21c`; only dual wield, which the mod does not
/// have, reads `fHipViewKickCenterSpeed`).
pub fn centre_speed(def: &WeaponDef, ads: f32, stance: usize) -> f32 {
    let base = def.view_kick_center.0;
    if ads <= ADS_SPLIT {
        return base;
    }
    let speed = base * def.ads_kick_center_scale;
    match stance {
        1 => speed * def.kick_center_stance.0,
        2 => speed * def.kick_center_stance.1,
        _ => speed,
    }
}

impl ViewKick {
    /// A shot's kick (`FUN_008e3a00`): the velocity is replaced, the angle kept.
    pub fn kick(&mut self, vel: (f32, f32, f32)) {
        self.vel = [vel.0, vel.1, vel.2];
    }

    /// Advance `dt` seconds at the recentre speed `centre` (`FUN_0051f150`).
    pub fn step(&mut self, dt: f32, centre: f32) {
        let mut ms = dt * 1000.0;
        while ms > 0.0 {
            let step = ms.min(STEP_MS) * 0.001;
            ms -= STEP_MS;
            for (v, a) in self.vel.iter_mut().zip(self.angle.iter_mut()) {
                if *a != 0.0 {
                    // Back toward centre: the sign opposes the angle.
                    let sign = if *a > 0.0 { -1.0 } else { 1.0 };
                    *v += sign * centre * step;
                } else if *v == 0.0 {
                    continue;
                }
                let mut d = *v * step;
                if *a * d < 0.0 {
                    d *= TOWARD_CENTRE;
                }
                if (*a + d) * *a < 0.0 {
                    // Crossed the centre: stop there.
                    *a = 0.0;
                } else {
                    *a += d;
                    if *a != 0.0 && a.abs() <= ANGLE_MAX {
                        continue;
                    }
                    if *a != 0.0 {
                        *a = a.signum() * ANGLE_MAX;
                    }
                }
                *v = 0.0;
            }
        }
    }

    /// The aim offset in the mod's convention, radians: (yaw left, pitch up).
    pub fn aim_offset(&self) -> (f32, f32) {
        (self.angle[1].to_radians(), -self.angle[0].to_radians())
    }

    /// The roll axis, radians, BO2's sign (positive turns the camera clockwise as seen from
    /// behind it).
    pub fn roll(&self) -> f32 {
        self.angle[2].to_radians()
    }

    pub fn is_rest(&self) -> bool {
        *self == ViewKick::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The integrator written out step by step from `FUN_0051f150`'s listing, for one axis.
    fn reference(mut v: f32, mut a: f32, centre: f32, steps: &[f32]) -> Vec<(f32, f32)> {
        let mut out = Vec::new();
        for &ms in steps {
            let dt = ms * 0.001;
            if v != 0.0 || a != 0.0 {
                if a != 0.0 {
                    v += if a > 0.0 { -centre } else { centre } * dt;
                }
                let mut d = v * dt;
                if a * d < 0.0 {
                    d *= 0.06;
                }
                if (a + d) * a < 0.0 {
                    a = 0.0;
                    v = 0.0;
                } else {
                    a += d;
                    if a == 0.0 {
                        v = 0.0;
                    } else if a.abs() > 10.0 {
                        a = if a > 0.0 { 10.0 } else { -10.0 };
                        v = 0.0;
                    }
                }
            }
            out.push((v, a));
        }
        out
    }

    #[test]
    fn the_integrator_matches_the_engine_step_by_step() {
        let mut k = ViewKick::default();
        k.kick((-47.5, 20.0, -10.0));
        let refs = reference(-47.5, 0.0, 1500.0, &[5.0; 40]);
        for (i, r) in refs.iter().enumerate() {
            k.step(0.005, 1500.0);
            assert_eq!((k.vel[0], k.angle[0]), *r, "step {i}");
        }
        // The first step only moves (no centre pull at angle 0), then the pull brakes it.
        assert_eq!(refs[0], (-47.5, -47.5 * (5.0f32 * 0.001)));
        assert!((refs[1].0 + 40.0).abs() < 1e-4);
    }

    #[test]
    fn a_frame_is_cut_into_five_ms_steps() {
        let mut a = ViewKick::default();
        let mut b = ViewKick::default();
        a.kick((-60.0, 0.0, 0.0));
        b.kick((-60.0, 0.0, 0.0));
        a.step(1.0 / 30.0, 1000.0);
        let refs = reference(-60.0, 0.0, 1000.0, &[5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 1000.0 / 30.0 - 30.0]);
        assert!((a.vel[0] - refs[6].0).abs() < 1e-3 && (a.angle[0] - refs[6].1).abs() < 1e-5, "{a:?} vs {:?}", refs[6]);
        for _ in 0..6 {
            b.step(0.005, 1000.0);
        }
        assert_ne!(a, b, "the remainder step counts");
    }

    #[test]
    fn the_kick_recentres_slowly_stops_at_centre_and_clamps() {
        let mut k = ViewKick::default();
        k.kick((-47.5, 0.0, 0.0));
        let mut peak = 0.0f32;
        let mut t = 0;
        while !k.is_rest() && t < 2000 {
            k.step(0.005, 1500.0);
            peak = peak.min(k.angle[0]);
            t += 1;
        }
        assert!(k.is_rest(), "settled exactly at centre");
        assert!((peak + 0.875).abs() < 1e-3, "the kick peaks at the braked distance: {peak}");
        assert!(t > 20, "the return is the slow part: {t} steps");
        let mut big = ViewKick::default();
        big.kick((-5000.0, 0.0, 0.0));
        big.step(0.005, 0.0);
        assert_eq!((big.angle[0], big.vel[0]), (-10.0, 0.0), "clamped, velocity dropped");
    }

    /// A 10-shot aimed string's peak climb (degrees) with shots every 100 ms.
    fn aimed_peak(d: &WeaponDef) -> f32 {
        let mut rng = super::super::rng::Rng::new(7);
        let (mut acc, mut k, mut peak) = (0.0, ViewKick::default(), 0.0f32);
        for shot in 0..10u16 {
            k.kick(super::super::recoil::shot_kick(d, 1.0, shot, &mut acc, 0.1, &mut rng).view);
            for _ in 0..20 {
                k.step(0.005, centre_speed(d, 1.0, 0));
                peak = peak.max(-k.angle[0]);
            }
        }
        peak
    }

    #[test]
    fn the_grip_and_the_aimed_set_scale_the_climb() {
        use super::super::def::Kick;
        let bare = WeaponDef { ads_kick: Kick { pitch: (30.0, 40.0), yaw: (0.0, 0.0) }, view_kick_center: (1000.0, 1000.0), ..Default::default() };
        let grip = WeaponDef { recoil_reduction: (0.1, 0.4), recoil_return_rate: 0.5, ..bare.clone() };
        let (b, g) = (aimed_peak(&bare), aimed_peak(&grip));
        assert!(g < b * 0.8 && g > 0.0, "the grip's accumulator shrinks the kick: {g} vs {b}");
        let soft = WeaponDef { ads_kick: Kick { pitch: (15.0, 20.0), yaw: (0.0, 0.0) }, ..bare.clone() };
        assert!(aimed_peak(&soft) < b * 0.6, "half the kick velocity climbs well under half as far: the pull is quadratic");
    }

    #[test]
    fn the_recentre_speed_follows_aim_and_stance() {
        let d = WeaponDef { view_kick_center: (1500.0, 900.0), ads_kick_center_scale: 0.975, kick_center_stance: (0.5, 0.25), ..Default::default() };
        assert_eq!(centre_speed(&d, 0.5, 2), 1500.0, "hip: the unscaled aimed field, any stance");
        assert_eq!(centre_speed(&d, 0.6, 0), 1500.0 * 0.975);
        assert_eq!(centre_speed(&d, 1.0, 1), 1500.0 * 0.975 * 0.5, "crouched: ducked scale");
        assert_eq!(centre_speed(&d, 1.0, 2), 1500.0 * 0.975 * 0.25, "prone: prone scale");
    }
}
