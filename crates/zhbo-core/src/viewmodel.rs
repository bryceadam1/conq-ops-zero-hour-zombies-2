//! The first-person viewmodel (BO2 arms and weapon): which animation plays, and how far into it.
//!
//! The engine side (`host/zhbo/ZhboViewmodel.cpp`) owns the model and the clips; this decides from
//! the player's state and this frame's events. Idle and sprint loop; the others play once and hold
//! their last frame until the state moves on.
//!
//! Using the field computer is a BO2 scorestreak: the weapon is swapped for the tablet, which comes
//! up before the view leaves for the overhead camera. Coming back to first person, the tablet goes
//! down first, then the weapon comes up.

use crate::math::Vec3;
use crate::sim::PlayerSim;

pub const NONE: i32 = -1;
pub const IDLE: i32 = 0;
pub const FIRE: i32 = 1;
pub const RELOAD: i32 = 2;
pub const RELOAD_EMPTY: i32 = 3;
pub const SPRINT: i32 = 4;
pub const RAISE: i32 = 5;
/// Tablet clips (a separate model, the arms holding BO2's scorestreak tablet).
pub const TABLET_PULLOUT: i32 = 6;
pub const TABLET_IDLE: i32 = 7;
pub const TABLET_PUTAWAY: i32 = 8;

/// Lengths of BO2's `viewmodel_tablet_pullout` (42 frames) and `_putaway` (19 frames) at 30 fps.
pub const TABLET_PULLOUT_TIME: f32 = 41.0 / 30.0;
pub const TABLET_PUTAWAY_TIME: f32 = 18.0 / 30.0;

/// The tablet clip for `t` seconds into the field computer coming up.
pub fn tablet_up(t: f32) -> (i32, f32) {
    if t < TABLET_PULLOUT_TIME { (TABLET_PULLOUT, t) } else { (TABLET_IDLE, t - TABLET_PULLOUT_TIME) }
}

/// How long the fire clip owns the viewmodel after a shot before idle resumes.
const FIRE_HOLD: f32 = 0.3;

/// What happened to the local player's weapon this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Events {
    pub shot: bool,
    pub reload_started: bool,
    pub swapped: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Viewmodel {
    pub anim: i32,
    pub t: f32,
}

impl Default for Viewmodel {
    fn default() -> Self {
        // First person starts from the field computer: the tablet goes down first.
        Viewmodel { anim: TABLET_PUTAWAY, t: 0.0 }
    }
}

impl Viewmodel {
    fn play(&mut self, anim: i32) {
        self.anim = anim;
        self.t = 0.0;
    }

    /// Advance by `dt` and return (clip, seconds into it).
    pub fn update(&mut self, dt: f32, p: &PlayerSim, ev: Events) -> (i32, f32) {
        self.t += dt;
        let w = p.arsenal.current();
        if ev.swapped {
            self.play(RAISE);
        }
        if ev.reload_started {
            self.play(if w.ammo == 0 { RELOAD_EMPTY } else { RELOAD });
        }
        if ev.shot {
            self.play(FIRE);
        }
        let done = match self.anim {
            RELOAD | RELOAD_EMPTY => w.reload_left <= 0.0 && self.t > 0.1,
            FIRE => self.t > FIRE_HOLD,
            RAISE => p.arsenal.raise_left <= 0.0 && self.t > 0.1,
            _ => false,
        };
        if done {
            self.play(IDLE);
        }
        if self.anim == TABLET_PUTAWAY && self.t >= TABLET_PUTAWAY_TIME {
            self.play(RAISE);
        }
        if self.anim == IDLE || self.anim == SPRINT {
            let want = if p.body.sprinting && p.arsenal.ads < 0.5 { SPRINT } else { IDLE };
            if want != self.anim {
                self.play(want);
            }
        }
        (self.anim, self.t)
    }
}

/// BO2 inches to Zero Hour units (the converter's scale).
const INCH: f32 = 11.0 / 72.0;
/// Sway: degrees of lag per degree/second of turning, its limit, and how fast it settles.
const SWAY_PER_RATE: f32 = 0.012;
const SWAY_MAX: f32 = 4.0;
const SWAY_FOLLOW: f32 = 12.0;
/// Walk bob: sideways and vertical swing (inches), and a full cycle per two strides.
const BOB_SIDE: f32 = 0.35;
const BOB_UP: f32 = 0.25;
const BOB_STRIDE: f32 = 9.0;
const SPRINT_BOB: f32 = 1.8;
/// Recoil: kick back (inches) and up (degrees) per shot, and its recovery rate.
const KICK_BACK: f32 = 0.6;
const KICK_UP: f32 = 1.2;
const KICK_RECOVER: f32 = 18.0;
/// Landing dip (inches) and recovery.
const LAND_DIP: f32 = 0.8;
const LAND_RECOVER: f32 = 8.0;

/// The viewmodel's offset from the eye this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    /// View space, Zero Hour units: forward, left, up.
    pub offset: Vec3,
    /// Radians: yaw (left positive), pitch (muzzle up positive), roll.
    pub angles: Vec3,
}

/// Sway, bob, recoil kick and landing dip.
#[derive(Clone, Debug, Default)]
pub struct Motion {
    last_view: Option<(f32, f32)>,
    sway: (f32, f32),
    phase: f32,
    kick: f32,
    land: f32,
    airborne: bool,
}

fn angle_delta(a: f32, b: f32) -> f32 {
    let mut d = a - b;
    while d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    }
    while d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    d
}

impl Motion {
    pub fn update(&mut self, dt: f32, yaw: f32, pitch: f32, p: &PlayerSim, shot: bool) -> Pose {
        let dt = dt.max(1e-4);
        let calm = 1.0 - 0.85 * p.arsenal.ads.clamp(0.0, 1.0);

        // Sway: the weapon lags behind the turn, then catches up.
        let (yaw_rate, pitch_rate) = match self.last_view {
            Some((y, pt)) => (angle_delta(yaw, y).to_degrees() / dt, (pitch - pt).to_degrees() / dt),
            None => (0.0, 0.0),
        };
        self.last_view = Some((yaw, pitch));
        let target = (
            (-yaw_rate * SWAY_PER_RATE).clamp(-SWAY_MAX, SWAY_MAX),
            (-pitch_rate * SWAY_PER_RATE).clamp(-SWAY_MAX, SWAY_MAX),
        );
        let k = (dt * SWAY_FOLLOW).min(1.0);
        self.sway.0 += (target.0 - self.sway.0) * k;
        self.sway.1 += (target.1 - self.sway.1) * k;

        // Bob: phase follows ground distance; amplitude follows speed.
        let speed = p.body.vel.length_xy();
        let sprint = if p.body.sprinting { SPRINT_BOB } else { 1.0 };
        let amp = if p.body.on_ground { (speed / 30.0).min(1.5) * sprint } else { 0.0 };
        if p.body.on_ground {
            self.phase = (self.phase + speed * dt / BOB_STRIDE * std::f32::consts::PI) % std::f32::consts::TAU;
        }

        if shot {
            self.kick = 1.0;
        }
        self.kick *= (-dt * KICK_RECOVER).exp();
        if p.body.on_ground && self.airborne {
            self.land = 1.0;
        }
        self.airborne = !p.body.on_ground;
        self.land *= (-dt * LAND_RECOVER).exp();

        let ads_kick = 1.0 - 0.6 * p.arsenal.ads.clamp(0.0, 1.0);
        let side = BOB_SIDE * amp * self.phase.sin() * calm;
        let up = -BOB_UP * amp * self.phase.cos().abs() * calm - LAND_DIP * self.land;
        Pose {
            offset: Vec3::new(-KICK_BACK * self.kick * ads_kick * INCH, side * INCH, up * INCH),
            angles: Vec3::new(
                (self.sway.0 * calm).to_radians(),
                (self.sway.1 * calm + KICK_UP * self.kick * ads_kick).to_radians(),
                (side * 1.5 * (sprint - 0.5)).to_radians(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{Mode, PlayerSim};

    fn player() -> PlayerSim {
        PlayerSim::spawn_for_tests()
    }

    #[test]
    fn plays_the_right_clips() {
        let mut p = player();
        p.mode = Mode::Fps;
        p.arsenal.raise_left = 0.0;
        let mut vm = Viewmodel::default();
        // The tablet goes down, the weapon comes up, then idle.
        assert_eq!(vm.update(0.1, &p, Events::default()).0, TABLET_PUTAWAY);
        assert_eq!(vm.update(TABLET_PUTAWAY_TIME, &p, Events::default()).0, RAISE);
        assert_eq!(vm.update(0.2, &p, Events::default()).0, IDLE);
        // A shot restarts the fire clip; idle comes back after it.
        assert_eq!(vm.update(0.05, &p, Events { shot: true, ..Default::default() }), (FIRE, 0.0));
        assert_eq!(vm.update(0.1, &p, Events::default()).0, FIRE);
        assert_eq!(vm.update(0.3, &p, Events::default()).0, IDLE);
        // Sprinting switches to the sprint loop, unless aiming.
        p.body.sprinting = true;
        assert_eq!(vm.update(0.05, &p, Events::default()).0, SPRINT);
        p.arsenal.ads = 1.0;
        assert_eq!(vm.update(0.05, &p, Events::default()).0, IDLE);
        p.arsenal.ads = 0.0;
        p.body.sprinting = false;
        // An empty reload plays the empty clip and holds while the reload runs.
        p.arsenal.weapons[p.arsenal.active].ammo = 0;
        p.arsenal.weapons[p.arsenal.active].reload_left = 2.0;
        assert_eq!(vm.update(0.05, &p, Events { reload_started: true, ..Default::default() }).0, RELOAD_EMPTY);
        assert_eq!(vm.update(1.0, &p, Events::default()).0, RELOAD_EMPTY);
        p.arsenal.weapons[p.arsenal.active].reload_left = 0.0;
        assert_eq!(vm.update(0.05, &p, Events::default()).0, IDLE);
    }

    #[test]
    fn tablet_comes_up_then_holds() {
        assert_eq!(tablet_up(0.5), (TABLET_PULLOUT, 0.5));
        let (clip, t) = tablet_up(TABLET_PULLOUT_TIME + 0.2);
        assert_eq!(clip, TABLET_IDLE);
        assert!((t - 0.2).abs() < 1e-5);
        assert!(crate::sim::FIELD_COMPUTER_RAISE > TABLET_PULLOUT_TIME, "the tablet is fully up before the view leaves");
    }

    #[test]
    fn motion_sways_bobs_and_kicks() {
        let mut p = player();
        let mut m = Motion::default();
        assert_eq!(m.update(0.016, 0.0, 0.0, &p, false), Pose::default());
        // Turning right (yaw decreasing) swings the weapon left, then it settles.
        let turned = m.update(0.016, -0.05, 0.0, &p, false);
        assert!(turned.angles.x > 0.0, "{turned:?}");
        for _ in 0..60 {
            m.update(0.016, -0.05, 0.0, &p, false);
        }
        assert!(m.update(0.016, -0.05, 0.0, &p, false).angles.x.abs() < 1e-3);
        // A shot kicks the weapon back and up; it recovers.
        let kicked = m.update(0.016, -0.05, 0.0, &p, true);
        assert!(kicked.offset.x < 0.0 && kicked.angles.y > 0.0);
        for _ in 0..30 {
            m.update(0.016, -0.05, 0.0, &p, false);
        }
        assert!(m.update(0.016, -0.05, 0.0, &p, false).offset.x.abs() < 1e-3);
        // Walking bobs it; aiming calms the bob.
        p.body.vel = Vec3::new(30.0, 0.0, 0.0);
        let mut most = 0.0f32;
        for _ in 0..60 {
            most = most.max(m.update(0.016, -0.05, 0.0, &p, false).offset.z.abs());
        }
        p.arsenal.ads = 1.0;
        let mut aimed = 0.0f32;
        for _ in 0..60 {
            aimed = aimed.max(m.update(0.016, -0.05, 0.0, &p, false).offset.z.abs());
        }
        assert!(most > 0.0 && aimed < most * 0.5, "bob {most} vs aimed {aimed}");
    }
}
