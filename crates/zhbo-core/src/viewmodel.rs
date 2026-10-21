//! The first-person viewmodel: which BO2 model (arms with a weapon, a grenade, the knife or the
//! scorestreak tablet), which of its BO2 animations, and how far into it.
//!
//! Clips are BO2's own viewmodel animation slots (`weapAnimFiles_t`, [`slot`]); the converter turns
//! each weapon's animations into W3D clips named by slot, and the engine side
//! (`host/zhbo/ZhboViewmodel.cpp`) poses the model. The state comes from the arsenal: offhand
//! throws, melee, drop and raise, reloads (magazine or shell by shell), sprint in/loop/out, fire and
//! last shot (hip or aimed), rechamber, then idle (empty idle when the magazine is dry).
//!
//! Using the field computer is a BO2 scorestreak: the weapon is swapped for the tablet, which comes
//! up before the view leaves for the overhead camera. Coming back to first person, the tablet goes
//! down first, then the weapon comes up.
//!
//! [`Motion`] is where the gun sits each frame: the weapon def's gun-position numbers through
//! `weapons::gunpos` (Ghidra G4/G5), in the renderer's view space and angle order.

use crate::math::{basis_xy, Vec3};
use crate::movement::Stance;
use crate::sim::PlayerSim;
use crate::weapons::gunpos::{GunInput, GunPos};
use crate::weapons::{OffhandUse, RaiseKind, Reload, ShellPhase, WeaponDef};

pub const NONE: i32 = -1;

/// BO2's viewmodel animation slots (`weapAnimFiles_t`).
pub mod slot {
    pub const IDLE: i32 = 0x01;
    pub const EMPTY_IDLE: i32 = 0x02;
    pub const FIRE: i32 = 0x04;
    pub const HOLD_FIRE: i32 = 0x05;
    pub const LASTSHOT: i32 = 0x06;
    pub const RECHAMBER: i32 = 0x08;
    pub const MELEE: i32 = 0x09;
    pub const MELEE_EMPTY: i32 = 0x0D;
    pub const RELOAD: i32 = 0x10;
    pub const RELOAD_EMPTY: i32 = 0x12;
    pub const RELOAD_START: i32 = 0x13;
    pub const RELOAD_END: i32 = 0x14;
    pub const RAISE: i32 = 0x17;
    pub const FIRST_RAISE: i32 = 0x18;
    pub const DROP: i32 = 0x19;
    pub const ALT_RAISE: i32 = 0x1A;
    pub const ALT_DROP: i32 = 0x1B;
    pub const QUICK_RAISE: i32 = 0x1C;
    pub const QUICK_DROP: i32 = 0x1D;
    pub const EMPTY_RAISE: i32 = 0x1E;
    pub const EMPTY_DROP: i32 = 0x1F;
    pub const SPRINT_IN: i32 = 0x20;
    pub const SPRINT_LOOP: i32 = 0x21;
    pub const SPRINT_OUT: i32 = 0x22;
    pub const SPRINT_EMPTY_IN: i32 = 0x23;
    pub const SPRINT_EMPTY_LOOP: i32 = 0x24;
    pub const SPRINT_EMPTY_OUT: i32 = 0x25;
    pub const ADS_FIRE: i32 = 0x3D;
    pub const ADS_LASTSHOT: i32 = 0x3E;
    pub const ADS_RECHAMBER: i32 = 0x40;
    pub const COUNT: i32 = 0x58;

    /// Clips that loop (an alt mode's clips too, which sit [`crate::weapons::ALT_CLIPS`] later).
    pub fn loops(clip: i32) -> bool {
        let slot = if clip >= crate::weapons::ALT_CLIPS { clip - crate::weapons::ALT_CLIPS } else { clip };
        matches!(slot, IDLE | EMPTY_IDLE | SPRINT_LOOP | SPRINT_EMPTY_LOOP) || clip == super::TABLET_IDLE || clip == super::HAMMER_IDLE
    }
}

/// Tablet clips (a separate model, the arms holding BO2's scorestreak tablet), after BO2's slots.
pub const TABLET_PULLOUT: i32 = 0x60;
pub const TABLET_IDLE: i32 = 0x61;
pub const TABLET_PUTAWAY: i32 = 0x62;

/// Lengths of BO2's `viewmodel_tablet_pullout` (42 frames) and `_putaway` (19 frames) at 30 fps.
pub const TABLET_PULLOUT_TIME: f32 = 41.0 / 30.0;
pub const TABLET_PUTAWAY_TIME: f32 = 18.0 / 30.0;

/// The tablet clip for `t` seconds into the field computer coming up.
pub fn tablet_up(t: f32) -> (i32, f32) {
    if t < TABLET_PULLOUT_TIME { (TABLET_PULLOUT, t) } else { (TABLET_IDLE, t - TABLET_PULLOUT_TIME) }
}

/// Hammer clips (a separate model, the arms holding a hammer with BO2's knife animations).
pub const HAMMER_PULLOUT: i32 = 0x64;
pub const HAMMER_IDLE: i32 = 0x65;
pub const HAMMER_SWING: i32 = 0x66;
pub const HAMMER_PUTAWAY: i32 = 0x67;
/// Lengths of BO2's `viewmodel_strider_pullout` and `_melee_swipe` (21 frames each at 30 fps).
pub const HAMMER_PULLOUT_TIME: f32 = 21.0 / 30.0;
pub const HAMMER_SWING_TIME: f32 = 21.0 / 30.0;

/// How long the fire clip owns the viewmodel after a shot before idle resumes.
const FIRE_HOLD: f32 = 0.3;
/// BO2's sprint-in transition before the sprint loop.
const SPRINT_IN_TIME: f32 = 0.25;

/// What happened to the local player's weapon this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Events {
    pub shot: bool,
    pub reload_started: bool,
    pub swapped: bool,
}

/// One frame's viewmodel: which converted viewmodel (weapon), which clip, seconds into it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub model: i32,
    pub clip: i32,
    pub t: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Viewmodel {
    /// The tablet going down as first person starts (seconds in), then the weapon's own clips.
    tablet_putaway: Option<f32>,
    /// Seconds since the last shot (fire, then rechamber), and whether it emptied the magazine.
    since_shot: f32,
    shot_was_last: bool,
    shot_aimed: bool,
    since_sprint_start: f32,
    /// Seconds the hammer has been out, and into building (one swing after another).
    hammer_t: f32,
    swing_t: f32,
}

impl Default for Viewmodel {
    fn default() -> Self {
        // First person starts from the field computer: the tablet goes down first.
        Viewmodel { tablet_putaway: Some(0.0), since_shot: 10.0, shot_was_last: false, shot_aimed: false, since_sprint_start: 0.0, hammer_t: 0.0, swing_t: 0.0 }
    }
}

/// `clip` if the weapon has it, else `fallback`.
fn or(def: &WeaponDef, clip: i32, fallback: i32) -> i32 {
    if def.anims == 0 || def.has_anim(clip as usize) { clip } else { fallback }
}

impl Viewmodel {
    /// Advance by `dt` and pick this frame's model, clip and time.
    pub fn update(&mut self, dt: f32, p: &PlayerSim, ev: Events) -> Frame {
        let a = &p.arsenal;
        let w = a.current();
        let def = w.def;
        let model = def.viewmodel;
        let frame = |model: i32, clip: i32, t: f32| Frame { model, clip, t: t.max(0.0) };
        // The held weapon's own clips (an alt mode's sit after its weapon's on the same viewmodel).
        let wf = |clip: i32, t: f32| Frame { model, clip: clip + def.clip_base, t: t.max(0.0) };

        if let Some(t) = self.tablet_putaway.as_mut() {
            *t += dt;
            if *t < TABLET_PUTAWAY_TIME {
                return frame(NONE, TABLET_PUTAWAY, *t);
            }
            self.tablet_putaway = None;
        }

        // The hammer: up, then idle; building swings it again and again.
        if p.hammer.is_some() || p.building {
            self.hammer_t += dt;
            if p.building {
                self.swing_t += dt;
                return frame(NONE, HAMMER_SWING, self.swing_t % HAMMER_SWING_TIME);
            }
            self.swing_t = 0.0;
            return if self.hammer_t < HAMMER_PULLOUT_TIME {
                frame(NONE, HAMMER_PULLOUT, self.hammer_t)
            } else {
                frame(NONE, HAMMER_IDLE, self.hammer_t - HAMMER_PULLOUT_TIME)
            };
        }
        self.hammer_t = 0.0;
        self.swing_t = 0.0;

        if ev.shot {
            self.since_shot = 0.0;
            self.shot_was_last = w.ammo == 0;
            self.shot_aimed = a.ads > 0.5;
        } else {
            self.since_shot += dt;
        }
        self.since_sprint_start = if p.body.sprinting { self.since_sprint_start + dt } else { 0.0 };
        let empty = w.ammo == 0;

        // Offhands: the grenade's own viewmodel pulls the pin (hold) and throws (fire).
        if let Some(use_) = a.offhand {
            let (lethal, clip, t) = match use_ {
                OffhandUse::Holding { lethal, held_for } => (lethal, slot::HOLD_FIRE, held_for),
                OffhandUse::Throwing { lethal, t } => (lethal, slot::FIRE, t),
            };
            let off = if lethal { a.lethal.def } else { a.tactical.def };
            return frame(off.viewmodel, clip, t);
        }
        // Melee: the weapon's own (pistol whip) or BO2's knife.
        if a.melee_left > 0.0 {
            let t = a.melee.melee_time.max(0.3) - a.melee_left;
            let own = if empty { slot::MELEE_EMPTY } else { slot::MELEE };
            if def.has_anim(own as usize) {
                return wf(own, t);
            }
            if def.has_anim(slot::MELEE as usize) {
                return wf(slot::MELEE, t);
            }
            return frame(a.melee.viewmodel, slot::MELEE, t);
        }
        if a.drop_left > 0.0 {
            let prev = &a.weapons[a.previous];
            let clip = if a.drop_total <= prev.def.quick_drop_time + 1e-4 { slot::QUICK_DROP } else if prev.ammo == 0 { slot::EMPTY_DROP } else { slot::DROP };
            return frame(prev.def.viewmodel, or(prev.def, clip, slot::DROP) + prev.def.clip_base, a.drop_total - a.drop_left);
        }
        if a.raise_left > 0.0 {
            let clip = match a.raise_kind {
                RaiseKind::First => slot::FIRST_RAISE,
                RaiseKind::Quick => slot::QUICK_RAISE,
                RaiseKind::Normal if empty => slot::EMPTY_RAISE,
                RaiseKind::Normal => slot::RAISE,
                RaiseKind::Alt => slot::ALT_RAISE,
            };
            return wf(or(def, clip, slot::RAISE), a.raise_total - a.raise_left);
        }
        match w.reload {
            Some(Reload::Magazine { t, empty, .. }) => return wf(if empty { or(def, slot::RELOAD_EMPTY, slot::RELOAD) } else { slot::RELOAD }, t),
            Some(Reload::Shells { phase, t, .. }) => {
                let clip = match phase {
                    ShellPhase::Start => slot::RELOAD_START,
                    ShellPhase::Insert => slot::RELOAD,
                    ShellPhase::End => slot::RELOAD_END,
                };
                return wf(clip, t);
            }
            None => {}
        }
        if p.body.sprinting && a.ads < 0.5 {
            let (sin, sloop) = if empty { (slot::SPRINT_EMPTY_IN, slot::SPRINT_EMPTY_LOOP) } else { (slot::SPRINT_IN, slot::SPRINT_LOOP) };
            return if self.since_sprint_start < SPRINT_IN_TIME && def.has_anim(sin as usize) {
                wf(sin, self.since_sprint_start)
            } else {
                wf(or(def, sloop, slot::SPRINT_LOOP), (self.since_sprint_start - SPRINT_IN_TIME).max(0.0))
            };
        }
        if a.since_sprint < def.sprint_out_time && def.has_anim(slot::SPRINT_OUT as usize) {
            return wf(if empty { or(def, slot::SPRINT_EMPTY_OUT, slot::SPRINT_OUT) } else { slot::SPRINT_OUT }, a.since_sprint);
        }
        // Fire (or the last shot), then the pump/bolt rechamber if the weapon has one.
        let fire_len = FIRE_HOLD.max(def.fire_time);
        if self.since_shot < fire_len {
            let clip = match (self.shot_aimed, self.shot_was_last) {
                (true, true) => or(def, slot::ADS_LASTSHOT, or(def, slot::LASTSHOT, slot::FIRE)),
                (true, false) => or(def, slot::ADS_FIRE, slot::FIRE),
                (false, true) => or(def, slot::LASTSHOT, slot::FIRE),
                (false, false) => slot::FIRE,
            };
            return wf(clip, self.since_shot);
        }
        let rechamber = if self.shot_aimed { or(def, slot::ADS_RECHAMBER, slot::RECHAMBER) } else { slot::RECHAMBER };
        if def.has_anim(rechamber as usize) && !self.shot_was_last && self.since_shot < fire_len + def.fire_time {
            return wf(rechamber, self.since_shot - FIRE_HOLD.min(def.fire_time * 0.3));
        }
        wf(if empty { or(def, slot::EMPTY_IDLE, slot::IDLE) } else { slot::IDLE }, self.since_shot)
    }
}

/// BO2 inches to the viewmodel's art units: the converter's scale, the same one the composed
/// model is drawn at (`zhbo_native::ART_SCALE`).
const INCH: f32 = zhbo_native::ART_SCALE;
/// The bob cycle advances one swing per this many Zero Hour units walked (`ps->bobCycle`: it
/// follows ground distance, not time).
const BOB_STRIDE: f32 = 9.0;
/// Landing dip (BO2 inches) and its recovery rate (per second).
const LAND_DIP: f32 = 0.8;
const LAND_RECOVER: f32 = 8.0;

/// The viewmodel's offset from the eye this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    /// View space, art units (`INCH` per BO2 inch): forward, left, up.
    pub offset: Vec3,
    /// Radians: yaw (left positive), pitch (muzzle up positive), roll.
    pub angles: Vec3,
}

/// The viewmodel's motion: BO2's gun-position state (the stance and movement offsets, sway and
/// the gun kick spring; Ghidra G4/G5, `weapons::gunpos`), fed this frame's stance, speed, aim and
/// view, plus the bob cycle, the sprint pose's weight and the landing dip.
#[derive(Clone, Debug, Default)]
pub struct Motion {
    gun: GunPos,
    /// The bob cycle, turns (`ps->bobCycle`).
    bob: f32,
    /// The sprint pose's weight 0..1 (G4's sprint raise/drop curve, which the caller supplies).
    sprint: f32,
    /// The landing dip 0..1, and whether the feet were off the ground last frame.
    land: f32,
    airborne: bool,
}

impl Motion {
    /// One frame of `dt` seconds. `yaw`/`pitch` are the view angles this frame (radians), `fov`
    /// the field of view `cg_fov` (degrees; G4's wide-FOV pullback), and `shot` the gun kick's
    /// impulse when a shot went off.
    pub fn update(&mut self, dt: f32, yaw: f32, pitch: f32, fov: f32, p: &PlayerSim, shot: Option<(f32, f32)>) -> Pose {
        let def = p.arsenal.current().def;
        let dt = dt.max(1e-4);
        let ads = p.arsenal.ads.clamp(0.0, 1.0);
        let speed_zh = p.body.horizontal_speed();
        // The gun-position math is BO2's, in its inches and inches/s.
        let speed = speed_zh / crate::units::ZH_PER_INCH;

        // The bob cycle follows the distance walked on the ground (`ps->bobCycle`).
        if p.body.on_ground {
            self.bob = (self.bob + speed_zh * dt / BOB_STRIDE * std::f32::consts::PI) % std::f32::consts::TAU;
        }

        // The sprint pose comes up over the sprint-in clip and drops at the def's sprint-out time.
        let target = if p.body.sprinting { 1.0 } else { 0.0 };
        let rate = if target > self.sprint { 1.0 / SPRINT_IN_TIME } else { 1.0 / def.sprint_out_time.max(1e-3) };
        let step = rate * dt;
        self.sprint = if (target - self.sprint).abs() <= step { target } else { self.sprint + step * (target - self.sprint).signum() };

        // The landing dip: down on the frame the feet touch, back up over the next eighth-second.
        if p.body.on_ground && self.airborne {
            self.land = 1.0;
        }
        self.airborne = !p.body.on_ground;
        self.land *= (-dt * LAND_RECOVER).exp();

        if let Some(impulse) = shot {
            self.gun.spring.kick(impulse);
        }

        // The signed share of the velocity to the right (`velocity . right / speed`).
        let (_, right) = basis_xy(yaw);
        let strafe = if speed_zh > 1e-3 { (p.body.vel.x * right.x + p.body.vel.y * right.y) / speed_zh } else { 0.0 };
        let input = GunInput {
            stance: match p.body.stance {
                Stance::Stand => 0,
                Stance::Crouch => 1,
                Stance::Prone => 2,
            },
            speed,
            run_speed: p.arsenal.tuning.g_speed.max(1.0),
            strafe,
            ads,
            sprinting: p.body.sprinting,
            sprint_weight: self.sprint,
            bob_cycle: (self.bob / std::f32::consts::TAU).rem_euclid(1.0),
            fov,
            view: (pitch.to_degrees(), yaw.to_degrees()),
            // The landing dip stays the client's own: the fall delta `FUN_00530990` wants is not
            // in the sim's state (no `landChange`), so gunpos's dip contributes nothing.
            land_change: 0.0,
            since_land: 0.0,
        };
        let pose = self.gun.step(def, &input, &p.arsenal.tuning, dt);
        // BO2's offset is (forward, right, up) and its angles (pitch, yaw, roll); the renderer's
        // view space is (forward, left, up) and its angles (yaw, pitch, roll).
        Pose {
            offset: Vec3::new(pose.offset[0] * INCH, -pose.offset[1] * INCH, (pose.offset[2] - LAND_DIP * self.land) * INCH),
            angles: Vec3::new(pose.angles[1].to_radians(), pose.angles[0].to_radians(), pose.angles[2].to_radians()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{Mode, PlayerSim};
    use crate::weapons::{GunKick, OffhandUse, RaiseKind, Reload, Sway};

    fn player() -> PlayerSim {
        PlayerSim::spawn_for_tests()
    }

    #[test]
    fn plays_bo2_clips_for_each_action() {
        let mut p = player();
        p.mode = Mode::Fps;
        p.arsenal.raise_left = 0.0;
        let mut vm = Viewmodel::default();
        let clip = |vm: &mut Viewmodel, p: &PlayerSim, dt: f32, ev: Events| vm.update(dt, p, ev).clip;
        // The tablet goes down, then idle.
        assert_eq!(clip(&mut vm, &p, 0.1, Events::default()), TABLET_PUTAWAY);
        assert_eq!(clip(&mut vm, &p, TABLET_PUTAWAY_TIME, Events::default()), slot::IDLE);
        // A shot plays fire; aimed, the aimed fire.
        assert_eq!(clip(&mut vm, &p, 0.03, Events { shot: true, ..Default::default() }), slot::FIRE);
        assert_eq!(clip(&mut vm, &p, 0.5, Events::default()), slot::IDLE);
        p.arsenal.ads = 1.0;
        assert_eq!(clip(&mut vm, &p, 0.03, Events { shot: true, ..Default::default() }), slot::ADS_FIRE);
        p.arsenal.ads = 0.0;
        vm.update(0.5, &p, Events::default());
        // Sprinting: sprint in, then the loop; afterwards sprint out.
        p.body.sprinting = true;
        assert_eq!(clip(&mut vm, &p, 0.05, Events::default()), slot::SPRINT_IN);
        assert_eq!(clip(&mut vm, &p, 0.5, Events::default()), slot::SPRINT_LOOP);
        p.body.sprinting = false;
        p.arsenal.since_sprint = 0.05;
        assert_eq!(clip(&mut vm, &p, 0.05, Events::default()), slot::SPRINT_OUT);
        p.arsenal.since_sprint = 10.0;
        // Reloads: magazine, then empty.
        let w = p.arsenal.active;
        p.arsenal.weapons[w].reload = Some(Reload::Magazine { t: 0.5, length: 2.0, add_at: 1.0, added: false, empty: false });
        assert_eq!(clip(&mut vm, &p, 0.05, Events::default()), slot::RELOAD);
        p.arsenal.weapons[w].reload = Some(Reload::Magazine { t: 0.5, length: 2.0, add_at: 1.0, added: false, empty: true });
        assert_eq!(clip(&mut vm, &p, 0.05, Events::default()), slot::RELOAD_EMPTY);
        p.arsenal.weapons[w].reload = None;
        // A grenade: the grenade's viewmodel pulls the pin, then throws.
        p.arsenal.offhand = Some(OffhandUse::Holding { lethal: true, held_for: 0.2 });
        let f = vm.update(0.05, &p, Events::default());
        assert_eq!((f.model, f.clip), (p.arsenal.lethal.def.viewmodel, slot::HOLD_FIRE));
        p.arsenal.offhand = Some(OffhandUse::Throwing { lethal: true, t: 0.1 });
        assert_eq!(clip(&mut vm, &p, 0.05, Events::default()), slot::FIRE);
        p.arsenal.offhand = None;
        // Melee with a rifle: BO2's knife.
        p.arsenal.melee_left = 0.3;
        let f = vm.update(0.05, &p, Events::default());
        assert_eq!((f.model, f.clip), (p.arsenal.melee.viewmodel, slot::MELEE));
        p.arsenal.melee_left = 0.0;
        // Swap: the rifle goes down, the pistol comes up.
        p.arsenal.drop_left = 0.2;
        p.arsenal.drop_total = 0.4;
        p.arsenal.previous = 0;
        p.arsenal.active = 1;
        p.arsenal.raise_left = 0.3;
        p.arsenal.raise_total = 0.3;
        p.arsenal.raise_kind = RaiseKind::Normal;
        let f = vm.update(0.05, &p, Events::default());
        assert_eq!((f.model, f.clip), (p.arsenal.weapons[0].def.viewmodel, slot::DROP));
        p.arsenal.drop_left = 0.0;
        let f = vm.update(0.05, &p, Events::default());
        assert_eq!((f.model, f.clip), (p.arsenal.weapons[1].def.viewmodel, slot::RAISE));
    }

    #[test]
    fn tablet_comes_up_then_holds() {
        assert_eq!(tablet_up(0.5), (TABLET_PULLOUT, 0.5));
        let (clip, t) = tablet_up(TABLET_PULLOUT_TIME + 0.2);
        assert_eq!(clip, TABLET_IDLE);
        assert!((t - 0.2).abs() < 1e-5);
        const { assert!(crate::sim::FIELD_COMPUTER_RAISE > TABLET_PULLOUT_TIME, "the tablet is fully up before the view leaves") };
    }

    /// A def whose G4 numbers a test can assert on (the catalogs' defs are real BO2 data).
    fn gun_def() -> &'static WeaponDef {
        let mut d = WeaponDef::default();
        d.view_pos.ducked_ofs = [0.0, 1.0, -2.0];
        d.view_pos.stand_move = [-1.0, 0.5, 0.25];
        d.view_pos.move_rate = 10.0;
        d.view_pos.rot_rate = 10.0;
        d.view_pos.stand_min_speed = 20.0;
        d.view_pos.ads_aim_pitch = 2.0;
        d.hip_gun_kick = GunKick { accel: 30.0, speed_decay: 20.0, static_decay: 20.0, ..Default::default() };
        d.hip_sway = Sway { max_angle: 5.0, lerp_speed: 8.0, pitch_scale: 0.5, yaw_scale: 0.5, horiz_scale: 0.1, vert_scale: 0.1 };
        Box::leak(Box::new(d))
    }

    fn settle(m: &mut Motion, p: &PlayerSim, yaw: f32, pitch: f32, shot: Option<(f32, f32)>) -> Pose {
        let mut pose = Pose::default();
        for _ in 0..120 {
            pose = m.update(1.0 / 60.0, yaw, pitch, 65.0, p, shot);
        }
        pose
    }

    #[test]
    fn motion_sways_bobs_and_kicks() {
        let mut p = player();
        p.arsenal.weapons[0].def = gun_def();
        p.arsenal.weapons[0].main = gun_def();
        let mut m = Motion::default();
        assert_eq!(m.update(0.016, 0.0, 0.0, 65.0, &p, None), Pose::default());
        // Turning right (yaw decreasing) leaves the weapon behind: it hangs left of the view, then
        // settles.
        let turned = m.update(0.016, -0.05, 0.0, 65.0, &p, None);
        assert!(turned.angles.x > 0.0 && turned.offset.y > 0.0, "{turned:?}");
        for _ in 0..60 {
            m.update(0.016, -0.05, 0.0, 65.0, &p, None);
        }
        assert!(m.update(0.016, -0.05, 0.0, 65.0, &p, None).angles.x.abs() < 1e-3);
        // A shot kicks the muzzle up (the impulse the sim hands over with the recoil event), then
        // the spring brings it back.
        let kicked = m.update(0.016, -0.05, 0.0, 65.0, &p, Some((8.0, 0.0)));
        assert!(kicked.angles.y > 0.0, "{kicked:?}");
        for _ in 0..180 {
            m.update(0.016, -0.05, 0.0, 65.0, &p, None);
        }
        assert!(m.update(0.016, -0.05, 0.0, 65.0, &p, None).angles.y.abs() < 1e-3);
        // Walking bobs the muzzle; aiming damps the bob (G4's `fAdsBobFactor`). The aim's own pitch
        // offset is constant, so the bob is the pitch's swing.
        let swing = |m: &mut Motion, p: &PlayerSim| {
            // Let the aim's pitch and the sway settle first, then measure the bob's own swing.
            for _ in 0..120 {
                m.update(0.016, -0.05, 0.0, 65.0, p, None);
            }
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            for _ in 0..120 {
                let y = m.update(0.016, -0.05, 0.0, 65.0, p, None).angles.y;
                (lo, hi) = (lo.min(y), hi.max(y));
            }
            hi - lo
        };
        p.body.vel = Vec3::new(30.0, 0.0, 0.0);
        let most = swing(&mut m, &p);
        p.arsenal.ads = 1.0;
        let aimed = swing(&mut m, &p);
        assert!(most > 0.0 && aimed < most * 0.5, "bob {most} vs aimed {aimed}");
    }

    #[test]
    fn the_hip_pose_is_the_defs_offsets_and_aiming_fades_them_by_half_way() {
        let mut p = player();
        p.arsenal.weapons[0].def = gun_def();
        p.arsenal.weapons[0].main = gun_def();
        p.body.stance = Stance::Crouch;
        let mut m = Motion::default();
        // Crouched at the hip: BO2's `vDuckedOfs` (forward, right, up) in inches, its Y flipped to
        // the renderer's left, at the art's scale.
        let hip = settle(&mut m, &p, 0.0, 0.0, None);
        assert!((hip.offset.x - 0.0).abs() < 1e-4, "{hip:?}");
        assert!((hip.offset.y + 1.0 * INCH).abs() < 1e-4, "{hip:?}");
        assert!((hip.offset.z + 2.0 * INCH).abs() < 1e-4, "{hip:?}");
        // Half-way into the aim the positional offsets are gone (G4: `max(0, 1 - 2 * ads)`), and
        // fully aimed the gun has the def's `fAdsAimPitch` on its pitch.
        p.arsenal.ads = 0.5;
        for _ in 0..120 {
            m.update(1.0 / 60.0, 0.0, 0.0, 65.0, &p, None);
        }
        p.arsenal.ads = 1.0;
        let aimed = settle(&mut m, &p, 0.0, 0.0, None);
        assert!(aimed.offset.length() < 1e-4, "{aimed:?}");
        assert!((aimed.angles.y - 2.0f32.to_radians()).abs() < 1e-4, "{aimed:?}");
        // Standing and walking: `vStandMove` scaled by how far into the run the speed is.
        p.body.stance = Stance::Stand;
        p.arsenal.ads = 0.0;
        p.body.vel = Vec3::new(30.0, 0.0, 0.0);
        let moving = settle(&mut m, &p, 0.0, 0.0, None);
        let f = (180.0 - 20.0) / (190.0 - 20.0);
        assert!((moving.offset.x + 1.0 * f * INCH).abs() < 1e-3, "{moving:?}");
        assert!((moving.offset.z - 0.25 * f * INCH).abs() < 1e-3, "{moving:?}");
    }
}
