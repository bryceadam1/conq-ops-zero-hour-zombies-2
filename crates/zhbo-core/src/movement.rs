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
//! The world is queried through [`World`], which the engine glue implements with
//! `TheTerrainLogic` and the partition manager, and which tests implement with a fake.

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
    pub ground_accel: f32,
    pub air_accel: f32,
    pub friction: f32,
    pub stop_speed: f32,
    pub gravity: f32,
    pub jump_height: f32,
    pub step_height: f32,
    /// Maximum climbable terrain rise per unit of horizontal distance.
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
    /// Seconds to blend the eye between stances.
    pub stance_blend_time: f32,
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
            // The mod's acceleration (BO2's `PM_Accelerate` rates were not part of G2).
            ground_accel: 9.0,
            air_accel: 1.0,
            friction: t.friction,
            stop_speed: inches(t.stopspeed),
            gravity: inches(t.bg_gravity),
            jump_height: inches(t.jump_height),
            step_height: inches(t.jump_step_size),
            max_slope: 1.4, // ~55 degrees; ZH cliffs are flagged separately by World::is_walkable
            radius: inches(15.0),
            sprint_time: t.player_sprint_time,
            sprint_forward_min: t.player_sprint_forward_minimum / 127.0,
            sprint_restart_min: t.sprint_restart_minimum,
            sprint_max: t.sprint_max,
            sprint_recharge_pause: t.player_sprint_recharge_pause,
            stand_eye: inches(t.player_standing_view_height) * crate::units::GENERAL_SCALE,
            crouch_eye: inches(t.view_height_crouch) * crate::units::GENERAL_SCALE,
            prone_eye: inches(t.view_height_prone) * crate::units::GENERAL_SCALE,
            stance_blend_time: 0.2,
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
    /// Current eye height, blending toward the stance target.
    pub eye_height: f32,
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
    /// The stance's speed-scale blend: (start time, from, to, duration).
    pub stance_lerp: Option<(f32, f32, f32, f32)>,
    held_last: u32,
}

impl Body {
    pub fn new(pos: Vec3, tuning: &MoveTuning) -> Self {
        Self {
            pos,
            vel: Vec3::ZERO,
            on_ground: true,
            stance: Stance::Stand,
            eye_height: tuning.stand_eye,
            sprinting: false,
            time: 0.0,
            sprint_started: None,
            sprint_ended: 0.0,
            sprint_start_max: tuning.sprint_time,
            sprint_exhausted: false,
            sprint_button_up: false,
            stance_lerp: None,
            held_last: 0,
        }
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
}

impl Default for MoveModifiers {
    fn default() -> Self {
        Self { speed_scale: 1.0, block_sprint: false, sprint_time_scale: 1.0, move_faster: false }
    }
}

pub fn step(
    body: &mut Body,
    input: &FpsInput,
    mods: MoveModifiers,
    tuning: &MoveTuning,
    world: &dyn World,
    dt: f32,
) {
    let pressed = input.pressed | (input.held & !body.held_last);
    body.held_last = input.held;
    body.time += dt;

    update_stance(body, input.held, pressed, tuning);

    let (fwd_axis, right_axis) = input.move_axes();
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
    let mut max_speed = tuning.run_speed(mods.move_faster) * mods.speed_scale * stance_scale(body, tuning);
    if body.sprinting {
        max_speed *= tuning.sprint_scale;
    }
    let wish_speed = max_speed * wish.length_xy().min(1.0);

    if body.on_ground {
        apply_friction(body, tuning, dt);
        accelerate(body, wish_dir, wish_speed, tuning.ground_accel, dt);
        if pressed & buttons::JUMP != 0 {
            match body.stance {
                Stance::Stand => {
                    body.vel.z = (2.0 * tuning.gravity * tuning.jump_height).sqrt();
                    body.on_ground = false;
                }
                // BO2: jump from crouch/prone stands you up instead.
                Stance::Crouch | Stance::Prone => body.stance = Stance::Stand,
            }
        }
    } else {
        accelerate(body, wish_dir, wish_speed, tuning.air_accel, dt);
        body.vel.z -= tuning.gravity * dt;
    }

    move_horizontally(body, tuning, world, dt);
    move_vertically(body, tuning, world, dt);
    blend_eye(body, tuning, dt);
}

/// The stance's speed scale (`FUN_008c3e00` PM_CmdScaleForStance): crouch and prone scale the
/// speed, and going prone or getting up from it blends the scale (crouched <-> prone).
fn stance_scale(body: &Body, t: &MoveTuning) -> f32 {
    if let Some((start, from, to, dur)) = body.stance_lerp {
        let f = ((body.time - start) / dur.max(1e-3)).clamp(0.0, 1.0);
        if f < 1.0 {
            return from + (to - from) * f;
        }
    }
    match body.stance {
        Stance::Stand => 1.0,
        Stance::Crouch => t.crouch_scale,
        Stance::Prone => t.prone_scale,
    }
}

fn update_stance(body: &mut Body, held: u32, pressed: u32, t: &MoveTuning) {
    let before = body.stance;
    change_stance(body, held, pressed);
    if body.stance != before {
        body.stance_lerp = match (before, body.stance) {
            (_, Stance::Prone) => Some((body.time, t.crouch_scale, t.prone_scale, t.prone_down_time)),
            (Stance::Prone, _) => Some((body.time, t.prone_scale, t.crouch_scale, t.prone_up_time)),
            _ => None,
        };
    }
}

fn change_stance(body: &mut Body, held: u32, pressed: u32) {
    if pressed & buttons::PRONE != 0 {
        body.stance = if body.stance == Stance::Prone { Stance::Stand } else { Stance::Prone };
    } else if pressed & buttons::CROUCH != 0 {
        body.stance = match body.stance {
            Stance::Crouch => Stance::Stand,
            Stance::Stand | Stance::Prone => Stance::Crouch,
        };
    }
    // Sprinting always stands you up (BO2).
    if held & buttons::SPRINT != 0 && body.stance != Stance::Stand && pressed & buttons::SPRINT != 0 {
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

fn apply_friction(body: &mut Body, t: &MoveTuning, dt: f32) {
    let speed = body.vel.length_xy();
    if speed < 1e-4 {
        body.vel.x = 0.0;
        body.vel.y = 0.0;
        return;
    }
    let control = speed.max(t.stop_speed);
    let drop = control * t.friction * dt;
    let scale = ((speed - drop) / speed).max(0.0);
    body.vel.x *= scale;
    body.vel.y *= scale;
}

fn accelerate(body: &mut Body, wish_dir: Vec3, wish_speed: f32, accel: f32, dt: f32) {
    let current = body.vel.x * wish_dir.x + body.vel.y * wish_dir.y;
    let add = wish_speed - current;
    if add <= 0.0 {
        return;
    }
    let amount = (accel * wish_speed * dt).min(add);
    body.vel.x += wish_dir.x * amount;
    body.vel.y += wish_dir.y * amount;
}

fn can_stand_at(body: &Body, t: &MoveTuning, world: &dyn World, x: f32, y: f32, dist: f32) -> bool {
    if !world.is_walkable(x, y) {
        return false;
    }
    let rise = world.ground_height(x, y) - body.pos.z;
    // Airborne players can clear anything below their feet; grounded ones obey step/slope limits.
    rise <= t.step_height.max(dist * t.max_slope) || (!body.on_ground && rise <= 0.0)
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
            let (s, c) = yaw.sin_cos();
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

fn move_vertically(body: &mut Body, t: &MoveTuning, world: &dyn World, dt: f32) {
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
    }
    if !body.on_ground {
        body.pos.z += body.vel.z * dt;
        if body.pos.z <= ground {
            body.pos.z = ground;
            body.vel.z = 0.0;
            body.on_ground = true;
        }
    }
}

fn blend_eye(body: &mut Body, t: &MoveTuning, dt: f32) {
    let target = match body.stance {
        Stance::Stand => t.stand_eye,
        Stance::Crouch => t.crouch_eye,
        Stance::Prone => t.prone_eye,
    };
    let rate = (t.stand_eye - t.prone_eye) / t.stance_blend_time.max(1e-3);
    let d = target - body.eye_height;
    let max = rate * dt;
    body.eye_height += d.clamp(-max, max);
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
        let world = FlatWorld::default();
        let mut i = forward();
        i.pressed = buttons::PRONE;
        i.held = buttons::PRONE;
        step(&mut b, &i, MoveModifiers::default(), &t, &world, LOGIC_DT);
        assert_eq!(b.stance, Stance::Prone);
        let mid = stance_scale(&Body { time: b.time + 0.3, ..b.clone() }, &t);
        assert!((mid - (t.crouch_scale + t.prone_scale) * 0.5).abs() < 0.02, "half-way at 300 ms: {mid}");
        let end = stance_scale(&Body { time: b.time + 0.7, ..b.clone() }, &t);
        assert_eq!(end, t.prone_scale);
    }

    #[test]
    fn jump_reaches_bo2_height_and_lands() {
        let t = MoveTuning::default();
        let mut b = Body::new(Vec3::ZERO, &t);
        let mut input = FpsInput::default();
        input.held = buttons::JUMP;
        input.pressed = buttons::JUMP;
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
}
