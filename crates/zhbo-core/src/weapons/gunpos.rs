/*
** Conquer Ops - Zero Hour gameplay core: where BO2 holds the gun in first person.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! The viewmodel's offset and angles each frame, as BO2 computes them (Ghidra G4,
//! `~/Documents/BO2-reference/ghidra/answers/G4.md`): the weapon's stance and movement offsets
//! blended at its own rates (`FUN_008efa70`), faded out by half-way into ADS, pulled back at wide
//! fields of view (`FUN_00530ba0`), dipped on landing (`FUN_00530990`), plus the bob waveform
//! (`FUN_008eed00`/`008eee70`), view sway (`FUN_008efce0`), the idle sway (`FUN_008eefc0`,
//! `idle_sway::gun_angles`) and the gun kick spring (G5). The input
//! is what the client knows; the output is an offset (inches: forward, right, up) and angles
//! (degrees: pitch, yaw, roll) for the renderer. The sim steps its own copy for the aimed fire
//! direction ([`GunPos::fire_angles`], `sim::PlayerSim::fire_angles`): one pose draws and aims.
//!
//! The client's viewmodel motion (`viewmodel::Motion`) is the consumer: it builds [`GunInput`]
//! from the local player and the weapon def each frame and turns this pose into the renderer's
//! view-space offset (art units, forward/left/up) and angles (radians, yaw/pitch/roll). Track R6
//! places the viewmodel from it.
//!
//! Medium-confidence parts (named in G4): the bob roll term, the sway smoothing (a lerp at the
//! weapon's lerp speed stands in for the engine's DiffTrack), and the sprint-pose weight curve,
//! which the caller supplies.

use super::def::WeaponDef;
use super::recoil::GunKickSpring;
use crate::tuning::Tuning;
use std::f32::consts::PI;

/// What the client knows this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GunInput {
    /// 0 standing, 1 crouched, 2 prone.
    pub stance: usize,
    /// Horizontal speed and the run speed (`ps->speed`), inches/s.
    pub speed: f32,
    pub run_speed: f32,
    /// The signed share of the velocity to the right (`velocity . right / speed`).
    pub strafe: f32,
    /// Aim fraction 0..1.
    pub ads: f32,
    pub sprinting: bool,
    /// The sprint pose's weight 0..1 (the sprint raise/drop progress).
    pub sprint_weight: f32,
    /// The bob cycle 0..1 (the low byte of `ps->bobCycle` / 255).
    pub bob_cycle: f32,
    /// The field of view (degrees).
    pub fov: f32,
    /// View pitch and yaw (degrees, **BO2's convention**: pitch positive looking down, yaw positive
    /// turning left), for sway.
    pub view: (f32, f32),
    /// The last landing: signed fall delta (inches) and seconds since.
    pub land_change: f32,
    pub since_land: f32,
    /// The idle sway's clock and eased stance factor (the sim's `idle_sway::IdleSway`).
    pub idle_time: f32,
    pub idle_stance: f32,
}

/// The gun's pose relative to the view, in **BO2's own convention** — every term below is the
/// decompile's literal sign, and [`GunPose::view`] is the one conversion into the mod's.
///
/// BO2's frame is X forward, Y **left**, Z up (`AnglesToAxis`, `FUN_0074d870`: axis 1 is the left
/// vector; `AngleVectors`' right, `FUN_0074d6c0`, is its negative — the lean in `FUN_008ef360`
/// adds that right vector to this offset, and the sway subtracts its horizontal term from Y so the
/// gun lags a turn). Angles are `AnglesToAxis`'s: pitch positive **muzzle down** (forward.z =
/// −sin pitch), yaw positive left, roll positive right side down.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GunPose {
    /// Forward, left, up (inches).
    pub offset: [f32; 3],
    /// Pitch (down), yaw (left), roll (right side down), degrees.
    pub angles: [f32; 3],
}

/// BO2 (pitch down, yaw left, roll) degrees → the renderer's (yaw left, pitch muzzle up, roll)
/// radians (`nativevm::GunPose::from_offset_angles`, the W3D path's `Rotate_Z(yaw)`,
/// `Rotate_Y(−pitch)`, `Rotate_X(roll)`): the pitch is the only axis whose sign differs.
pub fn view_angles(a: [f32; 3]) -> [f32; 3] {
    [a[1].to_radians(), -a[0].to_radians(), a[2].to_radians()]
}

impl GunPose {
    /// The pose in the renderer's view space: offset in inches (forward, left, up — BO2's own
    /// axes) and angles in radians (yaw left, pitch muzzle up, roll). The only conversion.
    pub fn view(&self) -> ([f32; 3], [f32; 3]) {
        (self.offset, view_angles(self.angles))
    }
}

/// The state BO2 keeps between frames (`weaponState+0x24..0x38`, the sway and the spring).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GunPos {
    pub cur_ofs: [f32; 3],
    pub cur_rot: [f32; 3],
    pub spring: GunKickSpring,
    sway_angle: (f32, f32),
    sway_ofs: (f32, f32),
    last_view: Option<(f32, f32)>,
    /// The gun's angles the shot follows (degrees, pitch and yaw): see [`GunPos::fire_angles`].
    fire: [f32; 2],
}

fn add(a: &mut [f32; 3], b: [f32; 3], k: f32) {
    for i in 0..3 {
        a[i] += b[i] * k;
    }
}

impl GunPos {
    /// Advance one frame of `dt` seconds.
    pub fn step(&mut self, def: &WeaponDef, i: &GunInput, t: &Tuning, dt: f32) -> GunPose {
        let p = &def.view_pos;
        let (ducked, prone) = (i.stance == 1, i.stance == 2);
        let ads = i.ads.clamp(0.0, 1.0);

        // ---- The target offset and rotation (FUN_008ef760 / 008ef850 / 008ef5b0).
        let mut ofs = [0.0; 3];
        let mut rot = [0.0; 3];
        if ducked {
            add(&mut ofs, p.ducked_ofs, 1.0);
        } else if prone {
            add(&mut ofs, p.prone_ofs, 1.0);
        }
        rot[0] += p.ads_aim_pitch * ads;
        let min_speed = if prone { p.prone_min_speed } else if ducked { p.ducked_min_speed } else { p.stand_min_speed };
        if i.speed > min_speed && i.run_speed > min_speed {
            let f = ((i.speed - min_speed) / (i.run_speed - min_speed)).min(1.0);
            let (mv, mr) = if prone { (p.prone_move, p.prone_rot) } else if ducked { (p.ducked_move, p.ducked_rot) } else { (p.stand_move, p.stand_rot) };
            add(&mut ofs, mv, f);
            add(&mut rot, mr, f * (1.0 - ads));
            let s = i.strafe * f;
            add(&mut ofs, p.strafe_move, s);
            add(&mut rot, p.strafe_rot, s * (1.0 - ads));
        }
        add(&mut ofs, p.sprint_ofs, i.sprint_weight);
        add(&mut rot, p.sprint_rot, i.sprint_weight);
        let k_pos = ((if prone { p.prone_move_rate } else { p.move_rate }) * dt).min(1.0);
        let k_rot = ((if prone { p.prone_rot_rate } else { p.rot_rate }) * dt).min(1.0);
        for a in 0..3 {
            self.cur_ofs[a] += (ofs[a] - self.cur_ofs[a]) * k_pos;
            self.cur_rot[a] += (rot[a] - self.cur_rot[a]) * k_rot;
        }

        // ---- The applied offset: gone by half-way into ADS (FUN_008f0700's tail).
        let fade = (1.0 - 2.0 * ads).max(0.0);
        let mut offset = [self.cur_ofs[0] * fade, self.cur_ofs[1] * fade, self.cur_ofs[2] * fade];
        // Wide fields of view pull the gun back up to 2 inches (FUN_00530ba0).
        offset[0] -= 2.0 * ((i.fov - t.cg_fov) * 0.05).clamp(0.0, 1.0) * (1.0 - ads);
        // The landing dip (FUN_00530990): down over 150 ms, back over the next 300.
        let ms = i.since_land * 1000.0;
        if ms <= 149.0 {
            offset[2] += i.land_change * 0.25 * ms / 150.0;
        } else if ms < 450.0 {
            offset[2] += i.land_change * 0.25 * (450.0 - ms) / 300.0;
        }

        // ---- Bob, sway and kick angles.
        let bob = bob_angles(def, i, t);
        // The gun's idle sway (`FUN_008eefc0`).
        let idle = super::idle_sway::gun_angles(def, ads, i.idle_time, i.idle_stance);
        self.sway(def, i, dt);
        self.spring.step(dt, &def.hip_gun_kick, &def.ads_gun_kick, ads, def.gun_max);
        // The sway offset (`FUN_008f0700`): the horizontal term comes off the left axis, the
        // vertical one is added to up — so both lag the turn.
        offset[1] -= self.sway_ofs.0;
        offset[2] += self.sway_ofs.1;
        // The angles (`FUN_008f0340`): the rotation faded out by half-way into ADS (`1 − 2·posFrac`,
        // `DAT_00c6f6ac` = 2, the same fade as the offset), plus the kick spring, the sway, the bob
        // and the idle. One function draws the gun and aims the shot: the client's view weapon
        // (`FUN_005374c0` → `FUN_008f0700(.., angles)` → `FUN_00530cd0`, whose axis is
        // `AnglesToAxis(these) × AnglesToAxis(view)`) and the server's think (`FUN_005dc820`) read
        // the same output. So `fAdsAimPitch` (the MP7's 2°, the shotguns' −0.1°) and the move pose
        // dip the gun mid-transition only; at full ADS the authored aimed clip alone places the
        // sight, on the axis the shot leaves along. The sway angles are *subtracted*
        // (`AngleNormalize180(rot·fade − sway)`, weaponState+0x54/0x58).
        let rot_fade = (1.0 - 2.0 * ads).max(0.0);
        let angles = [
            self.cur_rot[0] * rot_fade - self.sway_angle.0 + bob[0] + self.spring.angle.0 + idle[0],
            self.cur_rot[1] * rot_fade - self.sway_angle.1 + bob[1] + self.spring.angle.1 + idle[1],
            self.cur_rot[2] * rot_fade + bob[2] + idle[2],
        ];
        self.fire = [angles[0], angles[1]];
        GunPose { offset, angles }
    }

    /// The gun's pitch and yaw (degrees, BO2's convention like [`GunPose`]) an aimed shot from a gun without an
    /// overlay scope follows (`FUN_005dc820` composes them onto the view's fire angles): the drawn
    /// pose's own pitch and yaw, so the shot leaves along the sights.
    pub fn fire_angles(&self) -> [f32; 2] {
        self.fire
    }

    /// View sway (`FUN_008efce0`): the view's turn rate (`AnglesSubtract(view, last)`,
    /// `FUN_0074a240`, per 60 Hz frame), clamped to the max angle and scaled, eased in at the lerp
    /// speed. The hip and aimed parameters lerp by aim. Stored as BO2 stores them; `step` applies
    /// them with BO2's signs.
    fn sway(&mut self, def: &WeaponDef, i: &GunInput, dt: f32) {
        let (h, a, f) = (&def.hip_sway, &def.ads_sway, i.ads.clamp(0.0, 1.0));
        let l = |x: f32, y: f32| x + (y - x) * f;
        let (max, lerp_speed) = (l(h.max_angle, a.max_angle), l(h.lerp_speed, a.lerp_speed));
        let last = self.last_view.replace(i.view).unwrap_or(i.view);
        let rate = |d: f32| if dt > 0.0 { (d / (dt * 60.0)).clamp(-max.max(0.0), max.max(0.0)) } else { 0.0 };
        let (dp, dy) = (rate(i.view.0 - last.0), rate(wrap(i.view.1 - last.1)));
        // weaponState+0x54/0x58 (angles: pitch·pitchScale, yaw·yawScale) and +0x64/0x68 (offset:
        // yaw·horizScale, pitch·vertScale), literally.
        let target_angle = (dp * l(h.pitch_scale, a.pitch_scale), dy * l(h.yaw_scale, a.yaw_scale));
        let target_ofs = (dy * l(h.horiz_scale, a.horiz_scale), dp * l(h.vert_scale, a.vert_scale));
        let k = (lerp_speed * dt).clamp(0.0, 1.0);
        self.sway_angle.0 += (target_angle.0 - self.sway_angle.0) * k;
        self.sway_angle.1 += (target_angle.1 - self.sway_angle.1) * k;
        self.sway_ofs.0 += (target_ofs.0 - self.sway_ofs.0) * k;
        self.sway_ofs.1 += (target_ofs.1 - self.sway_ofs.1) * k;
    }
}

fn wrap(d: f32) -> f32 {
    (d + 180.0).rem_euclid(360.0) - 180.0
}

/// BO2's weapon bob angles (pitch, yaw, roll; `FUN_008f0160`).
pub fn bob_angles(def: &WeaponDef, i: &GunInput, t: &Tuning) -> [f32; 3] {
    let c = i.bob_cycle * PI * 2.0 + 2.0 * PI + t.bg_weapon_bob_lag * PI + 2.0 * PI;
    let speed = i.speed * 0.16;
    let ads = i.ads.clamp(0.0, 1.0);
    // The amplitude (horizontal, vertical) for the stance, capped by bg_weaponBobMax.
    let amp = |h: bool| -> f32 {
        let pick = |x: f32, y: f32| if h { x } else { y };
        let a = if i.stance == 2 {
            pick(t.bob_prone_h, t.bob_prone_v) * speed
        } else if i.stance == 1 || !i.sprinting {
            let base = if i.stance == 1 { pick(t.bob_ducked_h, t.bob_ducked_v) } else { pick(t.bob_standing_h, t.bob_standing_v) };
            (1.0 - ads) * speed * base
        } else {
            let weapon = def.view_pos.sprint_bob[if h { 0 } else { 1 }];
            pick(t.bob_sprinting_h, t.bob_sprinting_v) * speed * if weapon > 0.0 { weapon } else { 1.0 }
        };
        a.min(t.bg_weapon_bob_max)
    };
    let vert = ((2.0 * c).sin() + 0.2 * (4.0 * c + PI / 2.0).sin()) * 0.75 * amp(false);
    let horiz = |c: f32, a: f32| c.sin() * a;
    // The roll keeps only the curve's non-positive half: `FUN_008f0160` at 0x008f02b0 negates the
    // result, `FCOMIP`s it with 0 and stores 0 when `−f < 0` (G4's `max(0, …)` misread it).
    let mut out = [-vert, -horiz(c, amp(true)), horiz(c - 0.4712, amp(true) * t.bob_roll).min(0.0)];
    if ads > 0.0 {
        let k = 1.0 - (1.0 - def.view_pos.ads_bob_factor) * ads;
        for a in &mut out {
            *a *= k;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gun() -> WeaponDef {
        let mut d = WeaponDef::default();
        d.view_pos.stand_move = [-1.0, 0.0, -0.5];
        d.view_pos.ducked_ofs = [0.0, 0.0, -1.0];
        d.view_pos.move_rate = 10.0;
        d.view_pos.rot_rate = 10.0;
        d.view_pos.stand_min_speed = 20.0;
        d.view_pos.ads_bob_factor = 0.2;
        d
    }

    fn settle(g: &mut GunPos, d: &WeaponDef, i: &GunInput) -> GunPose {
        let t = Tuning::default();
        let mut pose = GunPose::default();
        for _ in 0..120 {
            pose = g.step(d, i, &t, 1.0 / 60.0);
        }
        pose
    }

    #[test]
    fn moving_blends_toward_the_move_offset_and_aiming_fades_it_by_half_way() {
        let d = gun();
        let mut g = GunPos::default();
        let run = GunInput { speed: 190.0, run_speed: 190.0, fov: 65.0, ..Default::default() };
        let pose = settle(&mut g, &d, &run);
        assert!((pose.offset[0] + 1.0).abs() < 0.01 && (pose.offset[2] + 0.5).abs() < 0.2, "{pose:?}");
        let half = GunInput { ads: 0.5, ..run };
        assert_eq!(settle(&mut g, &d, &half).offset[0], 0.0, "no positional offset past half ADS");
        let crouch = GunInput { stance: 1, fov: 65.0, ..Default::default() };
        assert!((settle(&mut GunPos::default(), &d, &crouch).offset[2] + 1.0).abs() < 0.01);
    }

    #[test]
    fn wide_fov_pulls_the_gun_back_and_landing_dips_it() {
        let d = WeaponDef::default();
        let t = Tuning::default();
        let mut g = GunPos::default();
        let wide = GunInput { fov: 90.0, since_land: 10.0, ..Default::default() };
        assert!((g.step(&d, &wide, &t, 0.016).offset[0] + 2.0).abs() < 1e-5, "(90-65) x 0.05 caps at 1: 2 inches back");
        let land = GunInput { fov: 65.0, land_change: -20.0, since_land: 0.075, ..Default::default() };
        let dip = g.step(&d, &land, &t, 0.016).offset[2];
        assert!((dip + 2.5).abs() < 0.05, "-20 x 0.25 x 75/150: {dip}");
    }

    #[test]
    fn bob_follows_the_waveform_and_ads_damps_it() {
        let d = gun();
        let t = Tuning::default();
        let walk = GunInput { speed: 190.0, bob_cycle: 0.1, ..Default::default() };
        let hip = bob_angles(&d, &walk, &t);
        assert!(hip[0].abs() > 0.01 || hip[1].abs() > 0.01, "{hip:?}");
        let still = GunInput { speed: 0.0, bob_cycle: 0.1, ..Default::default() };
        assert_eq!(bob_angles(&d, &still, &t), [0.0, 0.0, 0.0]);
        let aimed = GunInput { ads: 1.0, sprinting: false, ..walk };
        let a = bob_angles(&d, &aimed, &t);
        assert_eq!(a[0], 0.0, "standing bob is x (1 - ads)");
    }

    /// Every component keeps the decompile's literal sign in BO2's frame (X forward, Y left, Z up;
    /// pitch down, yaw left, roll right side down), and [`GunPose::view`] is the only place the
    /// renderer's convention (pitch muzzle up) comes in.
    #[test]
    fn every_component_keeps_bo2s_sign_and_one_conversion_maps_it() {
        let t = Tuning::default();
        // The conversion: BO2 pitch down → renderer pitch up; yaw and roll unchanged.
        let v = view_angles([2.0, 3.0, 4.0]);
        assert!((v[0] - 3f32.to_radians()).abs() < 1e-6 && (v[1] + 2f32.to_radians()).abs() < 1e-6 && (v[2] - 4f32.to_radians()).abs() < 1e-6, "{v:?}");
        // The move pose (`FUN_008ef850`, data added as is): BO2's walk `vStandRot` (2, 0, −2) dips
        // the muzzle and rolls the right side up; strafing right (`velocity · right`, the
        // `AngleVectors` right) adds `vStrafeMove` (0, 0.5, 0) to the *left* axis — the gun lags —
        // and `vStrafeRot` (0, 0, 6) rolls the right side down, into the strafe.
        let mut d = gun();
        d.view_pos.stand_move = [0.0; 3];
        d.view_pos.stand_rot = [2.0, 0.0, -2.0];
        d.view_pos.strafe_move = [0.0, 0.5, 0.0];
        d.view_pos.strafe_rot = [0.0, 0.0, 6.0];
        let strafe = GunInput { speed: 190.0, run_speed: 190.0, strafe: 1.0, fov: 65.0, ..Default::default() };
        let p = settle(&mut GunPos::default(), &d, &strafe);
        let bob = bob_angles(&d, &strafe, &t);
        assert!((p.offset[1] - 0.5).abs() < 0.01, "strafing right moves the gun left: {p:?}");
        assert!((p.angles[0] - bob[0] - 2.0).abs() < 0.05 && (p.angles[2] - bob[2] - 4.0).abs() < 0.05, "{p:?} bob {bob:?}");
        let (_, va) = p.view();
        assert!(va[1] < 0.0, "the walk dips the muzzle on screen: {va:?}");
        // The gun kick spring (G5: `gunKickVel.pitch += f*GunKickPitch`): a positive pitch impulse
        // is a muzzle-down kick.
        let mut g = GunPos::default();
        g.spring.kick((8.0, 0.0));
        let still = GunInput { fov: 65.0, ..Default::default() };
        let k = g.step(&d, &still, &t, 1.0 / 60.0);
        assert!(k.angles[0] > 0.0 && k.view().1[1] < 0.0, "{k:?}");
        // The bob roll keeps the curve's non-positive half only (`FUN_008f0160`).
        for n in 0..64 {
            let walk = GunInput { speed: 190.0, bob_cycle: n as f32 / 64.0, ..Default::default() };
            assert!(bob_angles(&d, &walk, &t)[2] <= 0.0, "cycle {n}");
        }
        assert!((0..64).any(|n| bob_angles(&d, &GunInput { speed: 190.0, bob_cycle: n as f32 / 64.0, ..Default::default() }, &t)[2] < -0.01));
        // The fire angles are the drawn pitch and yaw.
        assert_eq!(g.fire_angles(), [k.angles[0], k.angles[1]]);
    }

    #[test]
    fn turning_sways_the_gun_behind_the_view() {
        let mut d = gun();
        d.hip_sway = super::super::Sway { max_angle: 5.0, lerp_speed: 8.0, pitch_scale: 0.5, yaw_scale: 0.5, horiz_scale: 0.1, vert_scale: 0.1 };
        let t = Tuning::default();
        let mut g = GunPos::default();
        let mut i = GunInput { fov: 65.0, ..Default::default() };
        g.step(&d, &i, &t, 1.0 / 60.0);
        // BO2's literal signs (`FUN_008efce0`/`FUN_008f0700`/`FUN_008f0340`): a left turn moves the
        // gun's offset right (−left: it lags) and turns it by −yaw·yawScale; looking down raises it
        // (+up: it lags) and pitches it by −pitch·pitchScale (positive scales here).
        i.view.1 = 2.0;
        let left = g.step(&d, &i, &t, 1.0 / 60.0);
        assert!(left.angles[1] < 0.0 && left.offset[1] < 0.0, "{left:?}");
        i.view.1 = 0.0;
        let right = g.step(&d, &i, &t, 1.0 / 60.0);
        assert!(right.angles[1] > 0.0 && right.offset[1] > 0.0, "{right:?}");
        let mut g = GunPos::default();
        let mut i = GunInput { fov: 65.0, ..Default::default() };
        g.step(&d, &i, &t, 1.0 / 60.0);
        i.view.0 = 2.0;
        let down = g.step(&d, &i, &t, 1.0 / 60.0);
        assert!(down.offset[2] > 0.0 && down.angles[0] < 0.0, "{down:?}");
    }
}
