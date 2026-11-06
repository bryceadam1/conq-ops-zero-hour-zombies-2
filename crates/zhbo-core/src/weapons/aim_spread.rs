/*
** Conquer Ops - Zero Hour gameplay core: BO2's aim spread scale and the bullet's spread cone.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! BO2's hip spread is one number per player, `ps->aimSpreadScale` (0..255, `ps+0x588` in
//! `t6mp`), that places the shot inside the weapon's cone: the shot's spread in degrees is
//! `min + (max − min) · scale / 255` from the hip and `fAdsSpread + (max − fAdsSpread) · scale / 255`
//! fully aimed (`FUN_0066e590`), where min..max is the stance-blended cone of
//! [`super::recoil::spread_range`] (`FUN_008e41a0`). The weapon's rates are shares of that range:
//!
//! * **Each frame** (`FUN_008e0080`, the first thing `Pmove` does): a sum of increases — the turn
//!   (`|Δyaw| + |Δpitch|` of the command's view, degrees, `× 0.01 × fHipSpreadTurnAdd / dt`), the
//!   move (with a move key down and the ground speed over `bg_aimSpreadMoveSpeedThreshold`,
//!   `speed × fHipSpreadMoveAdd / g_speed`), the air (2.56 off the ground) and the
//!   anti-quickscope's (`FUN_008dbfb0`) — grows the scale by `sum × dt × 255`; with nothing to add
//!   it decays by `fHipSpreadDecayRate × dt × 255` (× `fHipSpreadDucked/ProneDecay` crouched or
//!   prone, × 0.5 in the air). A weapon with no decay rate zeroes it. Fully aimed with no
//!   anti-quickscope running, nothing adds and `adsZeroSpread` zeroes it.
//! * **Each shot** (`FUN_008dc710`): `+ fHipSpreadFireAdd × 255` unless fully aimed. The shot
//!   itself flies with the scale the previous frame left (`client->currentAimSpreadScale`, set
//!   before `Pmove`, `FUN_005dbfa0`).
//! * **Events**: a jump adds `jump_spreadAdd` (`FUN_008b5000`); lowering the stance
//!   (`viewHeightTarget < viewHeightCurrent`, with `bg_ads`, `FUN_008c1120`), crawling (prone with a
//!   move key down, not fully aimed) and the melee states 0x13-0x15 (`FUN_008e4da0`) max it; a
//!   weapon raise maxes it, an alt toggle raises it to at least 128 (`FUN_008df320`); damage adds
//!   `min(damage × 100 / maxHealth, 127)` (`FUN_005d1800`).
//!
//! The bullet (`Bullet_Fire`, `FUN_005d0c20`): per pellet, an angle uniform in 0..360° and a
//! radius `r` uniform in 0..1 (not area-uniform: BO2 clusters toward the centre), the end point
//! `forward + right · r·cos·tan(spread) + up · r·sin·tan(spread)` — every pellet of a shotgun in the
//! same cone.

use super::def::WeaponDef;
use crate::math::{det, Vec3};
use crate::tuning::Tuning;

/// The scale's top (`255.0f` at `0x00c733a0`).
pub const SCALE_MAX: f32 = 255.0;

/// `ps->aimSpreadScale` and the view the turn add measures from.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AimSpread {
    /// 0..[`SCALE_MAX`].
    pub scale: f32,
    /// The previous command's view (yaw, pitch; radians): BO2's `oldcmd.angles`. `None` until the
    /// first frame (a spawn's first command turns nothing).
    pub last_view: Option<(f32, f32)>,
}

crate::statehash::state_hash!(struct AimSpread { scale, last_view });

/// What the frame's update reads, from the player's state before this frame's move (BO2 updates
/// at the start of `Pmove`) and his command. Built by the sim from replicated input and sim state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpreadIn {
    /// The command's view (yaw, pitch; radians).
    pub view: (f32, f32),
    /// A move key is down (`cmd.forwardmove || cmd.rightmove`).
    pub moving: bool,
    /// Ground speed, inches/s.
    pub speed: f32,
    /// `ps->speed`: the run speed, inches/s.
    pub run_speed: f32,
    /// Off the ground (`groundEntityNum == ENTITYNUM_NONE`).
    pub airborne: bool,
    /// 0 stand, 1 crouch, 2 prone.
    pub stance: usize,
    /// The anti-quickscope's factor and timer (seconds) as the last frame left them
    /// ([`super::idle_sway::IdleSway`]'s `aqs`, `aqs_left`).
    pub aqs: f32,
    pub aqs_left: f32,
    /// The eye height the cone blends by (`ps->viewHeightCurrent`, inches); `None`: the stance's
    /// resting height.
    pub view_height: Option<f32>,
}

crate::statehash::state_hash!(struct SpreadIn { view, moving, speed, run_speed, airborne, stance, aqs, aqs_left, view_height });

impl AimSpread {
    /// The share of the cone the scale opens, 0..1.
    pub fn fraction(&self) -> f32 {
        (self.scale / SCALE_MAX).clamp(0.0, 1.0)
    }

    /// The frame's update (`FUN_008e0080`), `ads` the aim fraction.
    pub fn update(&mut self, i: &SpreadIn, def: &WeaponDef, ads: f32, t: &Tuning, dt: f32) {
        let (dyaw, dpitch) = match self.last_view {
            Some((y, p)) => (crate::math::wrap_angle(i.view.0 - y), crate::math::wrap_angle(i.view.1 - p)),
            None => (0.0, 0.0),
        };
        self.last_view = Some(i.view);
        if def.spread_decay == 0.0 {
            // No decay rate: the engine's decay stays its 1.0 default, a whole scale per frame.
            self.scale = 0.0;
            return;
        }
        let decay = def.spread_decay
            * if i.airborne {
                t.aim_spread_air_decay_scale
            } else {
                match i.stance {
                    2 => def.spread_stance_decay.1,
                    1 => def.spread_stance_decay.0,
                    _ => 1.0,
                }
            };
        let aimed = ads >= 1.0;
        let aqs_running = t.bg_aqs != 0.0 && i.aqs_left > 0.0;
        let mut add = 0.0;
        if aimed && !aqs_running {
            if t.ads_zero_spread != 0.0 {
                self.scale = 0.0;
            }
        } else {
            add += self.anti_quickscope(i, def, t);
            if def.spread_turn_add != 0.0 && dt > 0.0 {
                let turned = dyaw.to_degrees().abs() + dpitch.to_degrees().abs();
                add += turned * t.aim_spread_turn_scale * def.spread_turn_add / dt;
            }
            if def.spread_move_add != 0.0 && i.moving && i.speed > t.bg_aim_spread_move_speed_threshold && i.run_speed > 0.0 {
                add += i.speed * def.spread_move_add / i.run_speed;
            }
            if i.airborne {
                add += t.aim_spread_air_add;
            }
        }
        let grow = add * dt;
        self.scale = if grow > 0.0 { self.scale + grow * SCALE_MAX } else { self.scale - decay * dt * SCALE_MAX };
        self.scale = self.scale.clamp(0.0, SCALE_MAX);
    }

    /// `FUN_008dbfb0`: while the scope is still coming up (the timer at its full time) and the
    /// factor is over 1, `(factor − 1) × fAntiQuickScopeSpreadMultiplier`, until the scale passes
    /// `fAntiQuickScopeSpreadMax`.
    fn anti_quickscope(&self, i: &SpreadIn, def: &WeaponDef, t: &Tuning) -> f32 {
        let q = &def.zoom.anti_quickscope;
        let full = q.time > 0.0 && i.aqs_left >= q.time - 1e-4;
        if t.bg_aqs != 0.0 && i.aqs > 1.0 && self.scale <= q.spread_max && full {
            (i.aqs - 1.0) * q.spread_multiplier
        } else {
            0.0
        }
    }

    /// The shot's spread (degrees) at this scale, in the cone `(min, max)`, `ads` the aim
    /// fraction (`FUN_0066e590`).
    pub fn shot_spread(scale: f32, cone: (f32, f32), def: &WeaponDef, ads: f32) -> f32 {
        let s = (scale / SCALE_MAX).clamp(0.0, 1.0);
        let low = if ads >= 1.0 { def.ads_spread } else { cone.0 };
        low + (cone.1 - low) * s
    }

    /// A shot's add (`FUN_008dc710`).
    pub fn fired(&mut self, def: &WeaponDef, ads: f32) {
        if ads < 1.0 {
            self.scale = (self.scale + def.spread_fire_add * SCALE_MAX).min(SCALE_MAX);
        }
    }

    /// The scale a shot leaves for the next shot of the same frame.
    pub fn after_shot(scale: f32, def: &WeaponDef, ads: f32) -> f32 {
        if ads < 1.0 { (scale + def.spread_fire_add * SCALE_MAX).min(SCALE_MAX) } else { scale }
    }

    /// A jump (`FUN_008b5000`).
    pub fn jumped(&mut self, t: &Tuning) {
        self.scale = (self.scale + t.jump_spread_add).min(SCALE_MAX);
    }

    /// Lowering the stance, crawling, a melee or a weapon raise: the cone wide open.
    pub fn open(&mut self) {
        self.scale = SCALE_MAX;
    }

    /// An alt toggle's raise keeps at least half the scale (`FUN_008df320`, `128.0f` at
    /// `0x00c735a8`).
    pub fn alt_raised(&mut self, t: &Tuning) {
        self.scale = self.scale.max(t.aim_spread_alt_raise_min);
    }

    /// A hurt (`FUN_005d1800`): the frame's damage as a whole percent of the maximum health (the
    /// engine's integer `damage × 100 / maxHealth`), at most 127. Any health units: only the ratio
    /// counts.
    pub fn damaged(&mut self, damage: f32, max_health: f32, t: &Tuning) {
        if damage > 0.0 && max_health > 0.0 {
            let add = (damage * 100.0 / max_health).floor().min(t.aim_spread_damage_cap);
            self.scale = (self.scale + add).min(SCALE_MAX);
        }
    }
}

/// The direction of one pellet in a `spread_deg` cone around the aim (`Bullet_Fire`):
/// `u_angle`, `u_radius` uniform in 0..1 (BO2 draws the angle first).
pub fn cone_dir(yaw: f32, pitch: f32, spread_deg: f32, u_angle: f32, u_radius: f32) -> Vec3 {
    let forward = crate::math::dir_from_angles(yaw, pitch);
    let (_, right) = crate::math::basis_xy(yaw);
    let up = right.cross(forward);
    let theta = (u_angle * 360.0).to_radians();
    let k = det::tan(spread_deg.to_radians()) * u_radius;
    let (s, c) = det::sin_cos(theta);
    (forward + right * (c * k) + up * (s * k)).normalized_or_zero()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def() -> WeaponDef {
        WeaponDef {
            hip_spread: [(3.0, 6.75), (2.5, 5.75), (2.0, 5.0)],
            spread_decay: 4.0,
            spread_fire_add: 0.6,
            spread_move_add: 5.0,
            spread_turn_add: 0.0,
            spread_stance_decay: (1.05, 1.1),
            ads_spread: 0.5,
            ..Default::default()
        }
    }

    fn rest() -> SpreadIn {
        SpreadIn { run_speed: 190.0, ..Default::default() }
    }

    #[test]
    fn a_shot_opens_the_cone_by_its_share_and_rest_closes_it_at_the_decay_rate() {
        let (d, t) = (def(), Tuning::default());
        let mut a = AimSpread::default();
        let cone = (3.0, 6.75);
        assert_eq!(AimSpread::shot_spread(a.scale, cone, &d, 0.0), 3.0, "the first shot from rest: the minimum");
        a.fired(&d, 0.0);
        assert!((a.scale - 0.6 * 255.0).abs() < 1e-3);
        assert!((AimSpread::shot_spread(a.scale, cone, &d, 0.0) - (3.0 + 0.6 * 3.75)).abs() < 1e-4);
        a.fired(&d, 0.0);
        assert_eq!(a.scale, SCALE_MAX, "two shots open it all the way");
        // 4 shares a second: 0.25 s from the top to nothing.
        for _ in 0..7 {
            a.update(&rest(), &d, 0.0, &t, 1.0 / 30.0);
        }
        assert!((a.scale - 255.0 * (1.0 - 4.0 * 7.0 / 30.0)).abs() < 1e-2, "{}", a.scale);
        a.update(&rest(), &d, 0.0, &t, 1.0 / 30.0);
        assert_eq!(a.scale, 0.0);
    }

    #[test]
    fn moving_turning_and_the_air_grow_it_and_stances_scale_the_decay() {
        let (mut d, t) = (def(), Tuning::default());
        let dt = 1.0 / 30.0;
        // Running at g_speed: 5 shares a second.
        let mut a = AimSpread::default();
        a.update(&SpreadIn { moving: true, speed: 190.0, ..rest() }, &d, 0.0, &t, dt);
        assert!((a.scale - 5.0 * dt * 255.0).abs() < 1e-3);
        // Under the threshold, or with no key down (sliding to a stop), it decays.
        let mut b = AimSpread { scale: 100.0, last_view: None };
        b.update(&SpreadIn { moving: true, speed: 10.0, ..rest() }, &d, 0.0, &t, dt);
        assert!(b.scale < 100.0);
        let mut b = AimSpread { scale: 100.0, last_view: None };
        b.update(&SpreadIn { moving: false, speed: 190.0, ..rest() }, &d, 0.0, &t, dt);
        assert!(b.scale < 100.0);
        // In the air: 2.56 shares a second.
        let mut c = AimSpread::default();
        c.update(&SpreadIn { airborne: true, ..rest() }, &d, 0.0, &t, dt);
        assert!((c.scale - 2.56 * dt * 255.0).abs() < 1e-3);
        // Crouched and prone decays take their scales.
        for (stance, k) in [(1, 1.05), (2, 1.1)] {
            let mut e = AimSpread { scale: 200.0, last_view: None };
            e.update(&SpreadIn { stance, ..rest() }, &d, 0.0, &t, dt);
            assert!((e.scale - (200.0 - 4.0 * k * dt * 255.0)).abs() < 1e-3);
        }
        // Turning: per degree of the command's view.
        d.spread_turn_add = 1.0;
        let mut f = AimSpread::default();
        f.update(&SpreadIn { view: (0.0, 0.0), ..rest() }, &d, 0.0, &t, dt);
        f.update(&SpreadIn { view: (10f32.to_radians(), 0.0), ..rest() }, &d, 0.0, &t, dt);
        assert!((f.scale - 10.0 * 0.01 * 255.0).abs() < 1e-2, "{}", f.scale);
    }

    #[test]
    fn fully_aimed_it_zeroes_and_a_shot_adds_nothing() {
        let (d, t) = (def(), Tuning::default());
        let mut a = AimSpread { scale: 200.0, last_view: None };
        a.fired(&d, 1.0);
        assert_eq!(a.scale, 200.0);
        assert!((AimSpread::shot_spread(a.scale, (3.0, 6.75), &d, 1.0) - (0.5 + 6.25 * 200.0 / 255.0)).abs() < 1e-4);
        a.update(&SpreadIn { moving: true, speed: 190.0, ..rest() }, &d, 1.0, &t, 1.0 / 30.0);
        assert_eq!(a.scale, 0.0, "adsZeroSpread");
        // Mid-transition the hip formula holds and a shot adds.
        a.fired(&d, 0.5);
        assert!(a.scale > 0.0);
    }

    #[test]
    fn events_follow_the_engine() {
        let t = Tuning::default();
        let mut a = AimSpread::default();
        a.jumped(&t);
        assert_eq!(a.scale, 64.0);
        a.alt_raised(&t);
        assert_eq!(a.scale, 128.0);
        a.damaged(45.0, 150.0, &t);
        assert_eq!(a.scale, 158.0);
        a.scale = 0.0;
        a.damaged(250.0, 100.0, &t);
        assert_eq!(a.scale, 127.0, "capped at 127");
        a.open();
        assert_eq!(a.scale, SCALE_MAX);
    }

    #[test]
    fn the_cone_is_bo2s_tangent_plane_with_a_linear_radius() {
        let (yaw, pitch) = (0.7f32, 0.3f32);
        let f = crate::math::dir_from_angles(yaw, pitch);
        assert!((cone_dir(yaw, pitch, 5.0, 0.3, 0.0) - f).length() < 1e-6, "radius 0: the aim");
        for u in [0.0, 0.25, 0.5, 0.9] {
            let d = cone_dir(yaw, pitch, 5.0, u, 1.0);
            let ang = d.dot(f).clamp(-1.0, 1.0).acos().to_degrees();
            assert!((ang - 5.0).abs() < 1e-3, "radius 1 lands on the rim: {ang}");
        }
        let d = cone_dir(yaw, pitch, 5.0, 0.0, 0.5);
        let ang = d.dot(f).acos().to_degrees();
        assert!((ang - (0.5 * 5f32.to_radians().tan()).atan().to_degrees()).abs() < 1e-3);
        // Angle 0 is the view's right, 90° its up.
        let (_, right) = crate::math::basis_xy(yaw);
        assert!(cone_dir(yaw, pitch, 5.0, 0.0, 1.0).dot(right) > 0.08);
        assert!(cone_dir(yaw, pitch, 5.0, 0.25, 1.0).z > f.z);
    }
}
