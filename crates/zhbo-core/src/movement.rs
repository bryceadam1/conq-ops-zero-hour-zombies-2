//! Black Ops 2-style player movement on Zero Hour terrain.
//!
//! IW/Treyarch movement descends from Quake 3's `PM_Accelerate`/`PM_Friction`. The numbers are
//! BO2's multiplayer engine defaults from the tuning pack (`crate::tuning`, Ghidra G2: `g_speed`
//! 190, `jump_height` 39, `bg_gravity` 800, sprint x1.5, crouch x0.65, prone x0.15, back x0.7,
//! strafe x0.8, strafing while sprinting x0.667), converted with [`crate::units::inches`]. The
//! sprint meter is the engine's (`FUN_008bcfe0` SprintMax, `FUN_008bfaa0` SprintLeft,
//! `FUN_008c5840` start/stop): a sprint lasts the base time x the weapon's `sprintDurationScale`
//! (x2 with `longersprint`), recharges second for second once it ends, and starts only with more
//! than a second in the meter.
//!
//! The walk is t6mp's Pmove (`FUN_008c7af0` PM_WalkMove, `FUN_008c7810` PM_AirMove): friction
//! (`FUN_008bfea0`) and acceleration (`FUN_008c0100`, per stance on the ground) as the engine runs
//! them, the walkable slope its trace's (`FUN_006b7d40`), the jump, its landing slowdown and the
//! fall damage in [`land`], the eye's stance legs, the stance speed blend and prone turning in
//! [`stance`].
//!
//! The world is queried through [`World`], which the engine glue implements with
//! `TheTerrainLogic` and the partition manager, and which tests implement with a fake.

pub mod land;
pub mod stance;

pub use land::{LandTuning, Landing};
pub use stance::{cap_prone_yaw, prone_view_yaw, ViewLerp};

use crate::input::{buttons, FpsInput};
use crate::math::{basis_xy, Vec3};
use crate::tuning::Tuning;
use crate::units::inches;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveTuning {
    /// `g_speed`, Zero Hour units/s.
    pub walk_speed: f32,
    /// `movefaster`'s multiplier on `g_speed` (the engine truncates the product to whole inches).
    pub speed_perk: f32,
    pub sprint_scale: f32,
    pub sprint_strafe_scale: f32,
    pub crouch_scale: f32,
    pub prone_scale: f32,
    /// Seconds the speed scale blends into prone, and back up out of it.
    pub prone_down_time: f32,
    pub prone_up_time: f32,
    pub back_scale: f32,
    pub strafe_scale: f32,
    /// Ground acceleration standing, crouched and prone, and in the air (`FUN_008c7af0`,
    /// `FUN_008c7810`).
    pub accel_stand: f32,
    pub accel_crouch: f32,
    pub accel_prone: f32,
    pub air_accel: f32,
    pub friction: f32,
    /// Below this ground speed friction stops the player dead (Zero Hour units/s).
    pub friction_min_speed: f32,
    pub stop_speed: f32,
    pub gravity: f32,
    pub jump_height: f32,
    pub step_height: f32,
    /// The steepest walkable ground as rise per unit of horizontal distance: BO2's walkable
    /// normal (`MinWalkNormal`, z >= 0.7) as a gradient, `sqrt(1 / n^2 - 1)`.
    pub max_slope: f32,
    pub radius: f32,
    /// Base sprint length, seconds (before the weapon's scale and the perk).
    pub sprint_time: f32,
    /// Forward input (0..1) needed to start or keep sprinting.
    pub sprint_forward_min: f32,
    /// A sprint starts only with more than this many seconds in the meter.
    pub sprint_restart_min: f32,
    /// The meter's cap, seconds.
    pub sprint_max: f32,
    /// Seconds of meter lost after running dry.
    pub sprint_recharge_pause: f32,
    pub stand_eye: f32,
    pub crouch_eye: f32,
    pub prone_eye: f32,
    /// The stances' view heights, inches (BO2's 60 / 40 / 11), and Zero Hour units per inch of
    /// them (the General's drawn scale: the eye stays on his body's eye line).
    pub view_stand: f32,
    pub view_crouch: f32,
    pub view_prone: f32,
    pub eye_scale: f32,
    /// Seconds the eye takes between standing and crouched (`FUN_008c1040`).
    pub crouch_time: f32,
    /// The jump and the landing ([`land`]).
    pub land: LandTuning,
    /// Prone turning, radians and radians/s ([`stance::prone_view_yaw`]).
    pub prone_yaw_cap: f32,
    pub prone_swing_speed: f32,
    pub prone_swing_margin: f32,
    /// A walk's share of the top speed the bob cycle measures against ([`bob_top_speed`]).
    pub bob_walk_scale: f32,
}

impl MoveTuning {
    /// The movement numbers from the tuning pack, in Zero Hour units.
    pub fn from_tuning(t: &Tuning) -> Self {
        Self {
            walk_speed: inches(t.g_speed),
            speed_perk: t.perk_speed_multiplier,
            sprint_scale: t.player_sprint_speed_scale,
            sprint_strafe_scale: t.player_sprint_strafe_speed_scale,
            crouch_scale: t.stance_crouch_scale,
            prone_scale: t.stance_prone_scale,
            prone_down_time: t.stance_prone_down_time,
            prone_up_time: t.stance_prone_up_time,
            back_scale: t.player_back_speed_scale,
            strafe_scale: t.player_strafe_speed_scale,
            accel_stand: t.accel_stand,
            accel_crouch: t.accel_crouch,
            accel_prone: t.accel_prone,
            air_accel: t.accel_air,
            friction: t.friction,
            friction_min_speed: inches(t.friction_min_speed),
            stop_speed: inches(t.stopspeed),
            gravity: inches(t.bg_gravity),
            jump_height: inches(t.jump_height),
            step_height: inches(t.jump_step_size),
            // ZH cliffs are flagged separately by World::is_walkable.
            max_slope: (1.0 / (t.min_walk_normal * t.min_walk_normal) - 1.0).max(0.0).sqrt(),
            radius: inches(15.0),
            sprint_time: t.player_sprint_time,
            sprint_forward_min: t.player_sprint_forward_minimum / 127.0,
            sprint_restart_min: t.sprint_restart_minimum,
            sprint_max: t.sprint_max,
            sprint_recharge_pause: t.player_sprint_recharge_pause,
            stand_eye: crate::units::drawn_inches(t.player_standing_view_height),
            crouch_eye: crate::units::drawn_inches(t.view_height_crouch),
            prone_eye: crate::units::drawn_inches(t.view_height_prone),
            view_stand: t.player_standing_view_height,
            view_crouch: t.view_height_crouch,
            view_prone: t.view_height_prone,
            eye_scale: crate::units::DRAWN_ZH_PER_INCH,
            crouch_time: t.stance_crouch_time,
            land: LandTuning::from_tuning(t),
            prone_yaw_cap: t.bg_prone_yawcap.to_radians(),
            prone_swing_speed: t.prone_swing_speed.to_radians(),
            prone_swing_margin: t.prone_swing_margin.to_radians(),
            bob_walk_scale: t.bob_walk_speed_scale,
        }
    }

    /// The run speed: `g_speed`, or with `movefaster` `(int)(g_speed x perk_speedMultiplier)` in
    /// BO2's inches (`FUN_005dc820`; 190 -> 203).
    pub fn run_speed(&self, move_faster: bool) -> f32 {
        if move_faster {
            let bo2 = self.walk_speed / inches(1.0);
            inches((bo2 * self.speed_perk).trunc())
        } else {
            self.walk_speed
        }
    }
}

impl Default for MoveTuning {
    fn default() -> Self {
        Self::from_tuning(&Tuning::default())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Stance {
    #[default]
    Stand,
    Crouch,
    Prone,
}

crate::statehash::state_hash!(enum Stance { Stand, Crouch, Prone });

/// A solid footprint the player cannot walk through (structures, and optionally vehicles).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Blocker {
    Circle { center: Vec3, radius: f32, top: f32 },
    /// Oriented rectangle: half extents along the object's local X (facing) and Y.
    Box { center: Vec3, half_x: f32, half_y: f32, yaw: f32, top: f32 },
}

pub trait World {
    /// Ground height at a point (terrain, bridges, water surface for wading checks).
    fn ground_height(&self, x: f32, y: f32) -> f32;
    /// False for cliffs, deep water, and anything else infantry cannot stand on.
    fn is_walkable(&self, x: f32, y: f32) -> bool;
    /// Playable map extent (lo, hi).
    fn extent(&self) -> (Vec3, Vec3);
    /// Solid footprints near `pos` within `radius`.
    fn blockers(&self, pos: Vec3, radius: f32, out: &mut Vec<Blocker>);
}

#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    /// Feet position.
    pub pos: Vec3,
    pub vel: Vec3,
    pub on_ground: bool,
    pub stance: Stance,
    /// Current eye height (Zero Hour units): the view height at the General's drawn scale.
    pub eye_height: f32,
    /// BO2's view height (`ps+0x208`, inches), going to the stance's one leg at a time.
    pub view_height: f32,
    /// The eye's leg under way ([`stance`]): the stance speed blend reads it too.
    pub view_lerp: Option<ViewLerp>,
    pub sprinting: bool,
    /// The body's own clock, seconds (the sprint meter and the stance blend read it).
    pub time: f32,
    /// The engine's sprint state (`ps+0x4b8..0x4d0`): when the current or last sprint started
    /// and ended, how long it could have lasted, and whether it ran dry.
    pub sprint_started: Option<f32>,
    pub sprint_ended: f32,
    pub sprint_start_max: f32,
    pub sprint_exhausted: bool,
    /// The sprint button must be released before the next sprint.
    pub sprint_button_up: bool,
    /// BO2's movement timer (`ps->pm_time`, ms): a landing's slowdown runs on it.
    pub pm_time: f32,
    /// The jumping flag (`PMF_JUMPING`, `0x4000`), the take-off height (Zero Hour units,
    /// `ps+0xc4`) and the last jump's time (the body's clock, `ps+0xc0`).
    pub jumping: bool,
    pub jump_origin_z: f32,
    pub jump_time: Option<f32>,
    /// A hurting landing's slowdown (`PMF_TIME_HARDLANDING`, `0x80`) runs while `pm_time` does.
    pub hard_landing: bool,
    /// The last landing's view dip (BO2's `landChange`, inches, negative down) and when it was
    /// (the body's clock). The camera and the gun sink by it ([`Body::view_dip`]).
    pub land_change: f32,
    pub land_time: f32,
    /// The prone body's yaw (`ps+0x464`, radians): the view stays within `bg_prone_yawcap` of it.
    pub prone_yaw: f32,
    held_last: u32,
}

crate::statehash::state_hash!(struct Body {
    pos, vel, on_ground, stance, eye_height, view_height, view_lerp, sprinting, time, sprint_started, sprint_ended,
    sprint_start_max, sprint_exhausted, sprint_button_up, pm_time, jumping, jump_origin_z, jump_time, hard_landing,
    land_change, land_time, prone_yaw, held_last,
});

impl Body {
    pub fn new(pos: Vec3, tuning: &MoveTuning) -> Self {
        Self {
            pos,
            vel: Vec3::ZERO,
            on_ground: true,
            stance: Stance::Stand,
            eye_height: tuning.stand_eye,
            view_height: tuning.view_stand,
            view_lerp: None,
            sprinting: false,
            time: 0.0,
            sprint_started: None,
            sprint_ended: 0.0,
            sprint_start_max: tuning.sprint_time,
            sprint_exhausted: false,
            sprint_button_up: false,
            pm_time: 0.0,
            jumping: false,
            jump_origin_z: 0.0,
            jump_time: None,
            hard_landing: false,
            land_change: 0.0,
            land_time: 0.0,
            prone_yaw: 0.0,
            held_last: 0,
        }
    }

    /// Seconds since the last landing that dipped the view.
    pub fn since_land(&self) -> f32 {
        self.time - self.land_time
    }

    /// The camera's landing dip now (BO2 inches, negative down; `FUN_0051f760`).
    pub fn view_dip(&self, t: &LandTuning) -> f32 {
        if self.land_change == 0.0 {
            return 0.0;
        }
        t.view_dip(self.land_change, self.since_land() * 1000.0)
    }

    /// Where the camera is: the eye, sunk by the landing dip (at the General's drawn scale) but
    /// never closer than `LandDipViewFloor` to the feet (`FUN_0051f760`). The sim's eye (shots,
    /// sight) is [`Body::eye`], undipped, as BO2's playerstate eye is.
    pub fn view_eye(&self, t: &MoveTuning) -> Vec3 {
        let dip = self.view_dip(&t.land);
        if dip == 0.0 {
            return self.eye();
        }
        let floor = t.land.dip_floor * t.eye_scale;
        let z = (self.eye_height + dip * t.eye_scale).max(floor.min(self.eye_height));
        self.pos + Vec3::new(0.0, 0.0, z)
    }

    /// Seconds of sprint in the meter now, of `max` (`FUN_008bfaa0`): while sprinting, what the
    /// sprint started with less the time since; after one, what was left plus the time since it
    /// ended (less the recharge pause when it ran dry).
    pub fn sprint_left(&self, max: f32, pause: f32) -> f32 {
        let Some(start) = self.sprint_started else { return max };
        let left = if self.sprinting {
            self.sprint_start_max - (self.time - start)
        } else {
            let mut l = self.sprint_start_max - (self.sprint_ended - start) + (self.time - self.sprint_ended);
            if self.sprint_exhausted {
                l -= pause;
            }
            l
        };
        left.clamp(0.0, max)
    }

    pub fn eye(&self) -> Vec3 {
        self.pos + Vec3::new(0.0, 0.0, self.eye_height)
    }

    pub fn horizontal_speed(&self) -> f32 {
        self.vel.length_xy()
    }
}

/// Extra per-step context from the weapon layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveModifiers {
    /// Weapon/ADS move speed multiplier.
    pub speed_scale: f32,
    /// Sprinting is not allowed (ADS, reloading with no sprint-reload, firing).
    pub block_sprint: bool,
    /// Sprint duration multiplier (the weapon's `sprintDurationScale`, and `longersprint`).
    pub sprint_time_scale: f32,
    /// The player has `movefaster` (Lightweight).
    pub move_faster: bool,
    /// The player has `fallheight` (Lightweight): landings never hurt.
    pub fall_height: bool,
    /// A melee lunge in progress ([`Charge`]): it replaces the walk this frame.
    pub charge: Option<Charge>,
}

/// BO2's melee charge (`FUN_008c34f0`): each frame the player moves straight at the target at the
/// speed that stops him at the melee reach under the charge friction, `sqrt(2 x friction x gap)`,
/// never faster than the charge's top speed. Zero Hour units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Charge {
    pub target: Vec3,
    /// Stop this far from the target's centre (the reach plus its hit radius).
    pub stop: f32,
    pub max_speed: f32,
    pub friction: f32,
}

impl Charge {
    /// The charge velocity from `pos` (horizontal).
    pub fn velocity(&self, pos: Vec3) -> Vec3 {
        let to = Vec3::new(self.target.x - pos.x, self.target.y - pos.y, 0.0);
        let d = to.length_xy();
        let gap = (d - self.stop).max(0.0);
        if d < 1e-3 || gap <= 0.0 {
            return Vec3::ZERO;
        }
        to * ((2.0 * self.friction * gap).sqrt().min(self.max_speed) / d)
    }
}

impl Default for MoveModifiers {
    fn default() -> Self {
        Self { speed_scale: 1.0, block_sprint: false, sprint_time_scale: 1.0, move_faster: false, fall_height: false, charge: None }
    }
}

/// One frame of movement. Returns the landing when the feet touched down this frame
/// ([`land::crash_land`]): its fall damage is the caller's to deal (`MOD_FALLING`).
pub fn step(
    body: &mut Body,
    input: &FpsInput,
    mods: MoveModifiers,
    tuning: &MoveTuning,
    world: &dyn World,
    dt: f32,
) -> Option<Landing> {
    let pressed = input.pressed | (input.held & !body.held_last);
    body.held_last = input.held;
    body.time += dt;
    land::drop_timers(body, dt);

    let (fwd_axis, right_axis) = input.move_axes();
    // BO2 stands a crouched or prone player up only by starting a sprint, and the sprint start
    // (`FUN_008c3c40`) refuses while the aim button is held (usercmd bit 0x80) or the stick is short
    // of `player_sprintForwardMinimum` — so the sprint/hold-breath key while aimed (the Hybrid's
    // sight swap, hold breath) leaves the stance alone.
    let sprint_could_start = fwd_axis >= tuning.sprint_forward_min && !mods.block_sprint;
    let before = body.stance;
    change_stance(body, input.held, pressed, sprint_could_start);
    if body.stance == Stance::Prone && before != Stance::Prone {
        // He lies down facing where he looks.
        body.prone_yaw = input.yaw;
    }
    stance::adjust_view_height(body, tuning);

    update_sprint(body, input.held, fwd_axis, mods, tuning);

    // Wish direction and speed, BO2 style (`FUN_008c4110`): forward full speed, back and strafe
    // scaled, strafing while sprinting by the sprint strafe scale.
    let (fwd, right) = basis_xy(input.yaw);
    let fwd_scale = if fwd_axis >= 0.0 { 1.0 } else { tuning.back_scale };
    let strafe_scale = if body.sprinting { tuning.sprint_strafe_scale } else { tuning.strafe_scale };
    let mut wish = fwd * (fwd_axis * fwd_scale) + right * (right_axis * strafe_scale);
    let wish_len = wish.length_xy();
    if wish_len > 1.0 {
        wish = wish * (1.0 / wish_len);
    }
    let wish_dir = wish.normalized_or_zero();
    let mut max_speed = tuning.run_speed(mods.move_faster) * mods.speed_scale * stance::speed_scale(body, tuning);
    if body.sprinting {
        max_speed *= tuning.sprint_scale;
    }
    let wish_speed = max_speed * wish.length_xy().min(1.0);

    // Where an airborne frame started (`pml.previous_origin`/`previous_velocity`): the landing
    // solves the fall from it.
    let mut fall_from = None;
    if let Some(c) = mods.charge.filter(|_| body.on_ground) {
        // The lunge owns the horizontal velocity (no walk, no jump) while it runs.
        let v = c.velocity(body.pos);
        body.vel = Vec3::new(v.x, v.y, body.vel.z);
    } else if body.on_ground {
        // PM_WalkMove (`FUN_008c7af0`): a jump's landing slows first, then the jump check, then
        // friction and the ground acceleration (the jump frame is an air move instead).
        if body.jumping {
            land::apply_slowdown(body, &tuning.land, tuning.step_height);
        }
        let jump = if pressed & buttons::JUMP != 0 {
            match body.stance {
                Stance::Stand => land::try_jump(body, tuning),
                // BO2: jump from crouch/prone stands you up instead.
                Stance::Crouch | Stance::Prone => {
                    body.stance = Stance::Stand;
                    None
                }
            }
        } else {
            None
        };
        if let Some(vz) = jump {
            body.vel.z = vz;
            body.on_ground = false;
            fall_from = Some((body.pos.z, body.vel.z));
            accelerate(body, wish_dir, wish_speed, tuning.air_accel, tuning, dt);
            body.vel.z -= tuning.gravity * dt;
        } else {
            apply_friction(body, tuning, dt);
            let accel = match body.stance {
                Stance::Stand => tuning.accel_stand,
                Stance::Crouch => tuning.accel_crouch,
                Stance::Prone => tuning.accel_prone,
            } * if body.hard_landing { tuning.land.hard_accel } else { 1.0 };
            accelerate(body, wish_dir, wish_speed, accel, tuning, dt);
        }
    } else {
        fall_from = Some((body.pos.z, body.vel.z));
        accelerate(body, wish_dir, wish_speed, tuning.air_accel, tuning, dt);
        body.vel.z -= tuning.gravity * dt;
    }

    move_horizontally(body, tuning, world, dt);
    let landed = move_vertically(body, fall_from.map(|f| f.1), tuning, world, dt);
    match fall_from {
        Some((z0, vz0)) if landed => land::crash_land(body, z0, vz0, mods.fall_height, tuning),
        _ => None,
    }
}

/// The top speed (world units/s) the bob cycle measures the ground speed against (t6mp
/// `FUN_008c4690`, called by `PM_Footsteps`): the run speed scaled by the movement keys' direction
/// (both a forward/back and a side key: `((strafe − 1)·0.75 + 2)·0.5`, and backwards also
/// `(back + 1)·0.5`; a side key alone `(strafe − 1)·0.75 + 1`; backwards alone `back`), then by a
/// walk's [`MoveTuning::bob_walk_scale`] or the sprint scale, the weapon's move scale
/// (`speed_scale`) and the stance's.
pub fn bob_top_speed(body: &Body, fwd_axis: f32, right_axis: f32, speed_scale: f32, move_faster: bool, walking: bool, t: &MoveTuning) -> f32 {
    let mut s = t.run_speed(move_faster);
    let (strafe, back) = (t.strafe_scale, t.back_scale);
    if fwd_axis != 0.0 {
        if right_axis != 0.0 {
            s *= ((strafe - 1.0) * 0.75 + 2.0) * 0.5;
            if fwd_axis < 0.0 {
                s *= (back + 1.0) * 0.5;
            }
        } else if fwd_axis < 0.0 {
            s *= back;
        }
    } else if right_axis != 0.0 {
        s *= (strafe - 1.0) * 0.75 + 1.0;
    }
    if walking {
        s *= t.bob_walk_scale;
    } else if body.sprinting {
        s *= t.sprint_scale;
    }
    s * speed_scale * stance::speed_scale(body, t)
}

fn change_stance(body: &mut Body, held: u32, pressed: u32, sprint_could_start: bool) {
    if pressed & buttons::PRONE != 0 {
        body.stance = if body.stance == Stance::Prone { Stance::Stand } else { Stance::Prone };
    } else if pressed & buttons::CROUCH != 0 {
        body.stance = match body.stance {
            Stance::Crouch => Stance::Stand,
            Stance::Stand | Stance::Prone => Stance::Crouch,
        };
    }
    // A sprint start stands you up (BO2), when one could start.
    if sprint_could_start && held & buttons::SPRINT != 0 && body.stance != Stance::Stand && pressed & buttons::SPRINT != 0 {
        body.stance = Stance::Stand;
    }
}

/// BO2's sprint (`FUN_008c5840`): the button, enough forward input and more than
/// `sprint_restart_min` seconds in the meter start one; it ends when the meter runs dry (the
/// sprint is "exhausted"), the button is released, or something blocks it. Holding the button
/// through the end needs a release before the next sprint.
fn update_sprint(body: &mut Body, held: u32, fwd_axis: f32, mods: MoveModifiers, t: &MoveTuning) {
    let button = held & buttons::SPRINT != 0;
    if !button {
        body.sprint_button_up = false;
    }
    let max = (t.sprint_time * mods.sprint_time_scale).min(t.sprint_max);
    let keep = button && fwd_axis >= t.sprint_forward_min && body.stance == Stance::Stand && !mods.block_sprint;
    if body.sprinting {
        let start = body.sprint_started.unwrap_or(body.time);
        let dry = body.time - start >= body.sprint_start_max;
        if dry || !keep {
            body.sprinting = false;
            body.sprint_ended = body.time;
            body.sprint_exhausted = dry;
            body.sprint_button_up = button;
        }
        return;
    }
    // A sprint starts on the ground only, but carries through a jump.
    let left = body.sprint_left(max, t.sprint_recharge_pause);
    if keep && !body.sprint_button_up && body.on_ground && left > t.sprint_restart_min {
        body.sprinting = true;
        body.sprint_started = Some(body.time);
        body.sprint_start_max = left;
        body.sprint_exhausted = false;
    }
}

/// PM_Friction on the ground (`FUN_008bfea0`): the control speed `max(speed, stopspeed)`, scaled by
/// a hard landing or the jump slowdown ([`land::friction_scale`]), times `friction` comes off the
/// speed each second; under `FrictionMinSpeed` the player stops.
fn apply_friction(body: &mut Body, t: &MoveTuning, dt: f32) {
    let speed = body.vel.length_xy();
    if speed < t.friction_min_speed {
        body.vel = Vec3::ZERO;
        return;
    }
    let control = speed.max(t.stop_speed) * land::friction_scale(body, &t.land);
    let drop = control * t.friction * dt;
    let scale = ((speed - drop) / speed).max(0.0);
    body.vel.x *= scale;
    body.vel.y *= scale;
}

/// PM_Accelerate (`FUN_008c0100`): toward the wish speed along the wish direction, by
/// `accel x max(wish speed, stopspeed)` a second at most.
fn accelerate(body: &mut Body, wish_dir: Vec3, wish_speed: f32, accel: f32, t: &MoveTuning, dt: f32) {
    let current = body.vel.x * wish_dir.x + body.vel.y * wish_dir.y;
    let add = wish_speed - current;
    if add <= 0.0 {
        return;
    }
    let amount = (accel * wish_speed.max(t.stop_speed) * dt).min(add);
    body.vel.x += wish_dir.x * amount;
    body.vel.y += wish_dir.y * amount;
}

fn can_stand_at(body: &Body, t: &MoveTuning, world: &dyn World, x: f32, y: f32, dist: f32) -> bool {
    if !world.is_walkable(x, y) {
        return false;
    }
    let rise = world.ground_height(x, y) - body.pos.z;
    // Airborne players can clear anything below their feet; grounded ones obey step/slope limits,
    // and walk up only ground BO2's trace calls walkable (`FUN_006b7d40`: normal z >= 0.7).
    if !body.on_ground && rise <= 0.0 {
        return true;
    }
    rise <= t.step_height.max(dist * t.max_slope) && (rise <= 0.0 || ground_gradient(world, x, y) <= t.max_slope)
}

/// The ground's steepest rise per unit of horizontal distance at a point: the terrain's facet
/// slope, from its heights a unit either side.
fn ground_gradient(world: &dyn World, x: f32, y: f32) -> f32 {
    const D: f32 = 1.0;
    let gx = (world.ground_height(x + D, y) - world.ground_height(x - D, y)) / (2.0 * D);
    let gy = (world.ground_height(x, y + D) - world.ground_height(x, y - D)) / (2.0 * D);
    (gx * gx + gy * gy).sqrt()
}

fn move_horizontally(body: &mut Body, t: &MoveTuning, world: &dyn World, dt: f32) {
    let delta = Vec3::new(body.vel.x * dt, body.vel.y * dt, 0.0);
    let dist = delta.length_xy();
    if dist < 1e-6 {
        resolve_blockers(body, t, world);
        return;
    }
    // Sub-step so fast movement cannot tunnel through a cliff cell (ZH cells are 10 units).
    let steps = (dist / 2.0).ceil().max(1.0) as i32;
    let sub = delta * (1.0 / steps as f32);
    let sub_len = sub.length_xy();
    for _ in 0..steps {
        let target = body.pos + sub;
        if can_stand_at(body, t, world, target.x, target.y, sub_len) {
            body.pos.x = target.x;
            body.pos.y = target.y;
        } else if can_stand_at(body, t, world, target.x, body.pos.y, sub.x.abs()) {
            // Slide along X.
            body.pos.x = target.x;
            body.vel.y = 0.0;
        } else if can_stand_at(body, t, world, body.pos.x, target.y, sub.y.abs()) {
            body.pos.y = target.y;
            body.vel.x = 0.0;
        } else {
            body.vel.x = 0.0;
            body.vel.y = 0.0;
            break;
        }
        resolve_blockers(body, t, world);
    }
    clamp_to_extent(body, t, world);
}

fn resolve_blockers(body: &mut Body, t: &MoveTuning, world: &dyn World) {
    let mut blockers = Vec::new();
    world.blockers(body.pos, t.radius + 64.0, &mut blockers);
    for b in &blockers {
        if let Some(push) = push_out(body.pos, t.radius, b) {
            body.pos.x += push.x;
            body.pos.y += push.y;
            // Remove the velocity component going into the blocker.
            let n = push.normalized_or_zero();
            let into = body.vel.x * n.x + body.vel.y * n.y;
            if into < 0.0 {
                body.vel.x -= n.x * into;
                body.vel.y -= n.y * into;
            }
        }
    }
}

/// Minimum XY translation that separates a circle of `radius` at `p` from the blocker, if overlapping.
pub fn push_out(p: Vec3, radius: f32, blocker: &Blocker) -> Option<Vec3> {
    match *blocker {
        Blocker::Circle { center, radius: r, top } => {
            if p.z >= top {
                return None;
            }
            let d = Vec3::new(p.x - center.x, p.y - center.y, 0.0);
            let dist = d.length_xy();
            let min = radius + r;
            if dist >= min {
                return None;
            }
            let n = if dist > 1e-5 { d * (1.0 / dist) } else { Vec3::new(1.0, 0.0, 0.0) };
            Some(n * (min - dist))
        }
        Blocker::Box { center, half_x, half_y, yaw, top } => {
            if p.z >= top {
                return None;
            }
            let (s, c) = crate::math::det::sin_cos(yaw);
            let rel = Vec3::new(p.x - center.x, p.y - center.y, 0.0);
            // Into box-local space.
            let lx = rel.x * c + rel.y * s;
            let ly = -rel.x * s + rel.y * c;
            let cx = lx.clamp(-half_x, half_x);
            let cy = ly.clamp(-half_y, half_y);
            let (dx, dy) = (lx - cx, ly - cy);
            let dist2 = dx * dx + dy * dy;
            let (px, py) = if dist2 > 1e-10 {
                // Outside the box: push from closest point.
                let dist = dist2.sqrt();
                if dist >= radius {
                    return None;
                }
                let k = (radius - dist) / dist;
                (dx * k, dy * k)
            } else {
                // Centre inside the box: leave through the nearest face.
                let ex = half_x - lx.abs() + radius;
                let ey = half_y - ly.abs() + radius;
                let sign = |v: f32| if v >= 0.0 { 1.0 } else { -1.0 };
                if ex < ey {
                    (ex * sign(lx), 0.0)
                } else {
                    (0.0, ey * sign(ly))
                }
            };
            // Back to world space.
            Some(Vec3::new(px * c - py * s, px * s + py * c, 0.0))
        }
    }
}

fn clamp_to_extent(body: &mut Body, t: &MoveTuning, world: &dyn World) {
    let (lo, hi) = world.extent();
    let r = t.radius;
    if body.pos.x < lo.x + r {
        body.pos.x = lo.x + r;
        body.vel.x = body.vel.x.max(0.0);
    }
    if body.pos.x > hi.x - r {
        body.pos.x = hi.x - r;
        body.vel.x = body.vel.x.min(0.0);
    }
    if body.pos.y < lo.y + r {
        body.pos.y = lo.y + r;
        body.vel.y = body.vel.y.max(0.0);
    }
    if body.pos.y > hi.y - r {
        body.pos.y = hi.y - r;
        body.vel.y = body.vel.y.min(0.0);
    }
}

/// The vertical move; true when the feet touched down this frame. An airborne frame that started
/// going up at `vz0` moves by the mean of it and the velocity after gravity, as the engine's slide
/// move does (`PM_StepSlideMove` with gravity): exact under constant gravity, so the jump peaks at
/// `jump_height` and a fall lands at its true speed.
fn move_vertically(body: &mut Body, vz0: Option<f32>, t: &MoveTuning, world: &dyn World, dt: f32) -> bool {
    let ground = world.ground_height(body.pos.x, body.pos.y);
    if body.on_ground {
        // Follow terrain down slopes; walk off ledges higher than a step.
        if body.pos.z - ground > t.step_height {
            body.on_ground = false;
            body.vel.z = 0.0;
        } else {
            body.pos.z = ground;
            body.vel.z = 0.0;
        }
        return false;
    }
    body.pos.z += vz0.map_or(body.vel.z, |v0| (v0 + body.vel.z) * 0.5) * dt;
    if body.pos.z <= ground {
        body.pos.z = ground;
        body.vel.z = 0.0;
        body.on_ground = true;
        return true;
    }
    false
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::input::pack_move;
    use crate::units::LOGIC_DT;

    /// Flat ground at z=0 with an optional cliff wall at x >= wall_x and optional blockers.
    pub struct FlatWorld {
        pub wall_x: f32,
        pub blockers: Vec<Blocker>,
        pub height: fn(f32, f32) -> f32,
    }

    impl Default for FlatWorld {
        fn default() -> Self {
            Self { wall_x: f32::INFINITY, blockers: vec![], height: |_, _| 0.0 }
        }
    }

    impl World for FlatWorld {
        fn ground_height(&self, x: f32, y: f32) -> f32 {
            (self.height)(x, y)
        }
        fn is_walkable(&self, x: f32, _y: f32) -> bool {
            x < self.wall_x
        }
        fn extent(&self) -> (Vec3, Vec3) {
            (Vec3::new(-10_000.0, -10_000.0, 0.0), Vec3::new(10_000.0, 10_000.0, 0.0))
        }
        fn blockers(&self, _pos: Vec3, _radius: f32, out: &mut Vec<Blocker>) {
            out.extend_from_slice(&self.blockers);
        }
    }

    fn run(body: &mut Body, input: FpsInput, world: &dyn World, seconds: f32) {
        let t = MoveTuning::default();
        let frames = (seconds / LOGIC_DT).round() as i32;
        for _ in 0..frames {
            step(body, &input, MoveModifiers::default(), &t, world, LOGIC_DT);
        }
    }

    fn forward() -> FpsInput {
        FpsInput { move_packed: pack_move(1.0, 0.0), ..Default::default() }
    }

    #[test]
    fn walking_reaches_bo2_speed() {
        let t = MoveTuning::default();
        let mut b = Body::new(Vec3::ZERO, &t);
        run(&mut b, forward(), &FlatWorld::default(), 2.0);
        let v = b.horizontal_speed();
        assert!((v - t.walk_speed).abs() < 0.05 * t.walk_speed, "speed {v} vs {}", t.walk_speed);
        assert!(b.pos.x > 0.0 && b.pos.y.abs() < 1e-3);
    }

    #[test]
    fn the_sprint_key_stands_you_up_only_when_a_sprint_could_start() {
        // FUN_008c3c40: the aim button (and a short stick) refuses the sprint start, and with it
        // the stand-up — the Hybrid's sight swap while crouched keeps the crouch.
        let t = MoveTuning::default();
        let world = FlatWorld::default();
        let press = |b: &mut Body, input: FpsInput, mods: MoveModifiers| {
            step(b, &FpsInput { held: input.held | buttons::SPRINT, pressed: buttons::SPRINT, ..input }, mods, &t, &world, LOGIC_DT);
        };
        for stance in [Stance::Crouch, Stance::Prone] {
            let mut b = Body::new(Vec3::ZERO, &t);
            b.stance = stance;
            press(&mut b, forward(), MoveModifiers { block_sprint: true, ..Default::default() });
            assert_eq!(b.stance, stance, "aimed: no stand-up");
            press(&mut b, FpsInput::default(), MoveModifiers::default());
            assert_eq!(b.stance, stance, "standing still: no sprint, no stand-up");
            press(&mut b, forward(), MoveModifiers::default());
            assert_eq!(b.stance, Stance::Stand, "a sprint start stands you up");
        }
    }

    #[test]
    fn sprint_is_faster_then_runs_out() {
        let t = MoveTuning::default();
        let mut b = Body::new(Vec3::ZERO, &t);
        let mut input = forward();
        input.held = buttons::SPRINT;
        run(&mut b, input, &FlatWorld::default(), 1.5);
        assert!(b.sprinting);
        assert!(b.horizontal_speed() > t.walk_speed * 1.3);
        run(&mut b, input, &FlatWorld::default(), 4.0);
        assert!(!b.sprinting, "the meter runs dry after sprint_time");
        assert!(b.sprint_exhausted);
    }

    #[test]
    fn the_sprint_meter_recharges_second_for_second_and_needs_a_second_to_restart() {
        // G2: SprintLeft recharges 1 ms per ms after the sprint ends; a new sprint needs > 1 s.
        let t = MoveTuning::default();
        let mut b = Body::new(Vec3::ZERO, &t);
        let world = FlatWorld::default();
        let mut sprint = forward();
        sprint.held = buttons::SPRINT;
        run(&mut b, sprint, &world, 5.0);
        assert!(!b.sprinting && b.sprint_exhausted);
        // Holding the button through the end: no new sprint until it is released.
        run(&mut b, sprint, &world, 2.0);
        assert!(!b.sprinting, "the button must come up first");
        run(&mut b, forward(), &world, 0.5);
        // It ran dry at 4 s and has recharged since: 1 + 2 + 0.5 seconds.
        assert!((b.sprint_left(t.sprint_time, 0.0) - 3.5).abs() < 0.15, "3.5 s back after 3.5 s: {}", b.sprint_left(t.sprint_time, 0.0));
        run(&mut b, sprint, &world, 0.2);
        assert!(b.sprinting, "3.5 s in the meter is enough");
        // A short sprint ended early leaves the rest, and a restart below a second is refused.
        let mut c = Body::new(Vec3::ZERO, &t);
        run(&mut c, sprint, &world, 3.5);
        run(&mut c, forward(), &world, 0.1);
        assert!(c.sprint_left(t.sprint_time, 0.0) < 1.0);
        run(&mut c, sprint, &world, 0.1);
        assert!(!c.sprinting, "under a second in the meter");
    }

    #[test]
    fn sprint_length_scales_with_the_weapon_and_the_perk() {
        let t = MoveTuning::default();
        let world = FlatWorld::default();
        let mut sprint = forward();
        sprint.held = buttons::SPRINT;
        // sprintDurationScale 1.5 x longersprint 2 = 12 s.
        let mods = MoveModifiers { sprint_time_scale: 1.5 * 2.0, ..Default::default() };
        let mut b = Body::new(Vec3::ZERO, &t);
        for _ in 0..(11.5 / LOGIC_DT) as i32 {
            step(&mut b, &sprint, mods, &t, &world, LOGIC_DT);
        }
        assert!(b.sprinting, "still sprinting at 11.5 s");
        for _ in 0..(1.0 / LOGIC_DT) as i32 {
            step(&mut b, &sprint, mods, &t, &world, LOGIC_DT);
        }
        assert!(!b.sprinting);
    }

    #[test]
    fn lightweight_truncates_like_the_engine_and_strafe_scales_apply() {
        let t = MoveTuning::default();
        // (int)(190 x 1.07) = 203 inches/s.
        assert!((t.run_speed(true) - inches(203.0)).abs() < 1e-4);
        assert_eq!(t.run_speed(false), inches(190.0));
        let mut b = Body::new(Vec3::ZERO, &t);
        let strafe = FpsInput { move_packed: pack_move(0.0, 1.0), ..Default::default() };
        run(&mut b, strafe, &FlatWorld::default(), 2.0);
        assert!((b.horizontal_speed() - t.walk_speed * t.strafe_scale).abs() < 0.05 * t.walk_speed);
        let mut b = Body::new(Vec3::ZERO, &t);
        let back = FpsInput { move_packed: pack_move(-1.0, 0.0), ..Default::default() };
        run(&mut b, back, &FlatWorld::default(), 2.0);
        assert!((b.horizontal_speed() - t.walk_speed * t.back_scale).abs() < 0.05 * t.walk_speed);
    }

    #[test]
    fn going_prone_blends_the_speed_scale_down_over_600_ms() {
        let t = MoveTuning::default();
        let mut b = Body::new(Vec3::ZERO, &t);
        b.stance = Stance::Crouch;
        b.view_height = t.view_crouch;
        let world = FlatWorld::default();
        let mut i = forward();
        i.pressed = buttons::PRONE;
        i.held = buttons::PRONE;
        step(&mut b, &i, MoveModifiers::default(), &t, &world, LOGIC_DT);
        assert_eq!(b.stance, Stance::Prone);
        let mid = stance::speed_scale(&Body { time: b.time + 0.3, ..b.clone() }, &t);
        assert!((mid - (t.crouch_scale + t.prone_scale) * 0.5).abs() < 0.02, "half-way at 300 ms: {mid}");
        let end = stance::speed_scale(&Body { time: b.time + 0.7, ..b.clone() }, &t);
        assert_eq!(end, t.prone_scale);
    }

    #[test]
    fn jump_reaches_bo2_height_and_lands() {
        let t = MoveTuning::default();
        let mut b = Body::new(Vec3::ZERO, &t);
        let input = FpsInput { held: buttons::JUMP, pressed: buttons::JUMP, ..Default::default() };
        let world = FlatWorld::default();
        step(&mut b, &input, MoveModifiers::default(), &t, &world, LOGIC_DT);
        assert!(!b.on_ground);
        let mut peak: f32 = 0.0;
        let idle = FpsInput { held: buttons::JUMP, ..Default::default() };
        for _ in 0..60 {
            step(&mut b, &idle, MoveModifiers::default(), &t, &world, LOGIC_DT);
            peak = peak.max(b.pos.z);
        }
        assert!(b.on_ground);
        assert!((peak - t.jump_height).abs() < 0.15 * t.jump_height, "peak {peak}");
    }

    #[test]
    fn cliff_cells_block_and_slide() {
        let t = MoveTuning::default();
        let mut b = Body::new(Vec3::ZERO, &t);
        let world = FlatWorld { wall_x: 20.0, ..Default::default() };
        let diag = FpsInput { move_packed: pack_move(1.0, -1.0), yaw: 0.0, ..Default::default() };
        run(&mut b, diag, &world, 3.0);
        assert!(b.pos.x < 20.0, "stopped at cliff");
        assert!(b.pos.y > 5.0, "slid along the wall (strafe left = +Y)");
    }

    #[test]
    fn structures_push_the_player_out() {
        let t = MoveTuning::default();
        let mut b = Body::new(Vec3::ZERO, &t);
        let world = FlatWorld {
            blockers: vec![Blocker::Box {
                center: Vec3::new(30.0, 0.0, 0.0),
                half_x: 10.0,
                half_y: 10.0,
                yaw: 0.3,
                top: 40.0,
            }],
            ..Default::default()
        };
        run(&mut b, forward(), &world, 3.0);
        for b2 in &world.blockers {
            assert!(push_out(b.pos, t.radius - 0.01, b2).is_none(), "ended outside the box at {:?}", b.pos);
        }
    }

    #[test]
    fn steep_rise_blocks_but_gentle_slope_climbs() {
        let t = MoveTuning::default();
        let world = FlatWorld { height: |x, _| if x > 10.0 { 30.0 } else { 0.0 }, ..Default::default() };
        let mut b = Body::new(Vec3::ZERO, &t);
        run(&mut b, forward(), &world, 2.0);
        assert!(b.pos.x <= 10.0 + 1e-3, "a 30-unit wall is not a step");

        let ramp = FlatWorld { height: |x, _| x.max(0.0) * 0.3, ..Default::default() };
        let mut b = Body::new(Vec3::ZERO, &t);
        run(&mut b, forward(), &ramp, 2.0);
        assert!(b.pos.x > 30.0 && (b.pos.z - b.pos.x * 0.3).abs() < 0.5);
    }

    #[test]
    fn crouch_and_prone_slow_and_lower_the_eye() {
        let t = MoveTuning::default();
        let mut b = Body::new(Vec3::ZERO, &t);
        let world = FlatWorld::default();
        let mut i = forward();
        i.pressed = buttons::CROUCH;
        i.held = buttons::CROUCH;
        step(&mut b, &i, MoveModifiers::default(), &t, &world, LOGIC_DT);
        assert_eq!(b.stance, Stance::Crouch);
        run(&mut b, forward(), &world, 2.0);
        assert!((b.horizontal_speed() - t.walk_speed * t.crouch_scale).abs() < 0.5);
        assert!((b.eye_height - t.crouch_eye).abs() < 1e-3);
    }

    /// Runs a fall from `inches` above flat ground until the feet land; the landing.
    fn fall(inches_up: f32, mods: MoveModifiers) -> (Body, Landing) {
        let t = MoveTuning::default();
        let mut b = Body::new(Vec3::new(0.0, 0.0, inches(inches_up)), &t);
        b.on_ground = false;
        let world = FlatWorld::default();
        for _ in 0..600 {
            if let Some(l) = step(&mut b, &FpsInput::default(), mods, &t, &world, LOGIC_DT) {
                return (b, l);
            }
        }
        panic!("never landed");
    }

    #[test]
    fn a_fall_hurts_on_bo2s_curve_and_lightweight_spares_it() {
        // FUN_008c0b20: the fall height is the landing speed's v^2/2g, solved from the frame's
        // start: the drop itself, whatever the frame rate.
        let (_, l) = fall(214.0, MoveModifiers::default());
        assert!((l.height - 214.0).abs() < 2.0, "fall height {}", l.height);
        assert!((49..=51).contains(&l.damage), "50% at 214 inches: {}", l.damage);
        let (b, l) = fall(120.0, MoveModifiers::default());
        assert_eq!(l.damage, 0, "below bg_fallDamageMinHeight");
        assert!(!b.hard_landing);
        assert_eq!(fall(400.0, MoveModifiers::default()).1.damage, 100);
        let (b, l) = fall(214.0, MoveModifiers { fall_height: true, ..Default::default() });
        assert_eq!(l.damage, 0, "fallheight: no fall damage");
        assert!(!b.hard_landing, "and no hard landing");
        // A hurting landing runs the hard-landing timer (35 ms per percent + 500, at most 2000)
        // and dips by the damage's height (24 at 50%).
        let (b, l) = fall(150.0, MoveModifiers::default());
        assert!(b.hard_landing && b.pm_time == 500.0 + 35.0 * f32::from(l.damage), "{} at {}%", b.pm_time, l.damage);
        let (b, _) = fall(214.0, MoveModifiers::default());
        assert!(b.hard_landing && b.pm_time == 2000.0, "{}", b.pm_time);
        assert_eq!(b.land_change, -24.0);
    }

    #[test]
    fn a_jump_landing_slows_the_run_and_eases_back_over_1800_ms() {
        let t = MoveTuning::default();
        let world = FlatWorld::default();
        let mut b = Body::new(Vec3::ZERO, &t);
        run(&mut b, forward(), &world, 1.5);
        let full = b.horizontal_speed();
        let jump = FpsInput { pressed: buttons::JUMP, held: buttons::JUMP, ..forward() };
        step(&mut b, &jump, MoveModifiers::default(), &t, &world, LOGIC_DT);
        let mut landed = None;
        for _ in 0..60 {
            if step(&mut b, &forward(), MoveModifiers::default(), &t, &world, LOGIC_DT).is_some() {
                landed = Some(b.horizontal_speed());
                break;
            }
        }
        let at_land = landed.expect("landed");
        assert!((at_land / full - t.land.soft_scale).abs() < 0.03, "the landing's 0.67: {at_land} of {full}");
        assert_eq!(b.land_change, -8.0, "a 39-inch fall dips 8");
        step(&mut b, &forward(), MoveModifiers::default(), &t, &world, LOGIC_DT);
        assert!(b.jumping && (b.pm_time - t.land.land_ms).abs() < 1.0, "the slowdown timer starts: {}", b.pm_time);
        assert!(b.horizontal_speed() < full * 0.5, "0.67 x 0.65 and 2.5x friction: {}", b.horizontal_speed());
        run(&mut b, forward(), &world, 0.6);
        let mid = b.horizontal_speed();
        assert!(mid < full * 0.95, "still slowed 0.6 s on: {mid} of {full}");
        run(&mut b, forward(), &world, 1.5);
        assert!(!b.jumping && b.pm_time == 0.0, "the timer ran out and the flag went");
        assert!((b.horizontal_speed() - full).abs() < 0.03 * full);
    }

    #[test]
    fn jumping_again_at_once_is_lower_and_the_slowdown_can_be_turned_off() {
        let peak_of_second = |slowdown: bool| {
            let mut t = MoveTuning::default();
            t.land.slowdown = slowdown;
            let world = FlatWorld::default();
            let mut b = Body::new(Vec3::ZERO, &t);
            let jump = FpsInput { pressed: buttons::JUMP, held: buttons::JUMP, ..Default::default() };
            step(&mut b, &jump, MoveModifiers::default(), &t, &world, LOGIC_DT);
            while step(&mut b, &FpsInput::default(), MoveModifiers::default(), &t, &world, LOGIC_DT).is_none() {}
            step(&mut b, &jump, MoveModifiers::default(), &t, &world, LOGIC_DT);
            let mut peak: f32 = 0.0;
            for _ in 0..60 {
                step(&mut b, &FpsInput::default(), MoveModifiers::default(), &t, &world, LOGIC_DT);
                peak = peak.max(b.pos.z);
            }
            (peak, t)
        };
        let (low, t) = peak_of_second(true);
        // FUN_008b5000: the height is divided by the slowdown's 2.5 right after a landing.
        assert!((low - t.jump_height / t.land.slowdown_max).abs() < 0.2 * t.jump_height / t.land.slowdown_max, "{low}");
        let (full, t) = peak_of_second(false);
        assert!((full - t.jump_height).abs() < 0.15 * t.jump_height, "{full}");
        // FUN_008b52f0: no second jump within 500 ms of the last.
        let mut b = Body::new(Vec3::ZERO, &t);
        assert!(land::try_jump(&mut b, &t).is_some());
        b.time += 0.4;
        assert!(land::try_jump(&mut b, &t).is_none());
        b.time += 0.2;
        assert!(land::try_jump(&mut b, &t).is_some());
    }

    #[test]
    fn bo2s_walkable_normal_bounds_the_climbable_slope() {
        let t = MoveTuning::default();
        // n_z >= 0.7: a gradient of at most sqrt(1/0.49 - 1) = 1.02 (45.6 degrees).
        assert!((t.max_slope - 1.0202).abs() < 1e-3, "{}", t.max_slope);
        let climbs = |grade: fn(f32, f32) -> f32| {
            let world = FlatWorld { height: grade, ..Default::default() };
            let mut b = Body::new(Vec3::ZERO, &t);
            run(&mut b, forward(), &world, 2.0);
            b.pos.x > 20.0
        };
        assert!(climbs(|x, _| x.max(0.0) * 0.9), "0.9 (42 degrees) is walkable");
        assert!(!climbs(|x, _| x.max(0.0) * 1.2), "1.2 (50 degrees) is not, though each sub-step is under a step");
    }

    #[test]
    fn ground_acceleration_is_per_stance_and_floored_at_stopspeed() {
        // FUN_008c7af0: 9 standing, 12 crouched, 19 prone; FUN_008c0100 accelerates by
        // accel x max(wish speed, stopspeed): prone's slow wish speed is reached in a frame or two.
        let t = MoveTuning::default();
        let world = FlatWorld::default();
        let mut b = Body::new(Vec3::ZERO, &t);
        b.stance = Stance::Prone;
        b.view_height = t.view_prone;
        step(&mut b, &forward(), MoveModifiers::default(), &t, &world, LOGIC_DT);
        let wish = t.walk_speed * t.prone_scale;
        assert!((b.horizontal_speed() - wish).abs() < 1e-3, "{} vs {wish}", b.horizontal_speed());
        let mut s = Body::new(Vec3::ZERO, &t);
        step(&mut s, &forward(), MoveModifiers::default(), &t, &world, LOGIC_DT);
        let first = t.accel_stand * t.walk_speed * LOGIC_DT;
        assert!((s.horizontal_speed() - first).abs() < 1e-3, "{}", s.horizontal_speed());
    }

    #[test]
    fn the_camera_dips_by_land_change_and_stays_above_the_feet() {
        // FUN_0051f760: the whole landChange over 150 ms, back by 450; never under 8 inches.
        let t = MoveTuning::default();
        let mut b = Body::new(Vec3::ZERO, &t);
        b.land_change = -8.0;
        b.land_time = 0.0;
        b.time = 0.15;
        assert!((b.view_eye(&t).z - (t.view_stand - 8.0) * t.eye_scale).abs() < 1e-4);
        assert_eq!(b.eye().z, t.stand_eye, "the sim's eye is not dipped");
        b.time = 0.5;
        assert_eq!(b.view_eye(&t), b.eye());
        b.view_height = t.view_prone;
        b.eye_height = t.prone_eye;
        b.land_change = -24.0;
        b.time = 0.15;
        assert!((b.view_eye(&t).z - t.land.dip_floor * t.eye_scale).abs() < 1e-4, "held at the floor");
    }
}
