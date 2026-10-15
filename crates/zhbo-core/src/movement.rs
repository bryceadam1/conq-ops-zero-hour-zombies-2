//! Black Ops 2-style player movement on Zero Hour terrain.
//!
//! IW/Treyarch movement descends from Quake 3's `PM_Accelerate`/`PM_Friction`; the tuning values below
//! are BO2's multiplayer defaults (`g_speed 190`, `jump_height 39`, `g_gravity 800`, sprint scale 1.5,
//! crouch 0.65, prone 0.15) converted with [`crate::units::inches`].
//!
//! The world is queried through [`World`], which the engine glue implements with
//! `TheTerrainLogic` and the partition manager, and which tests implement with a fake.

use crate::input::{buttons, FpsInput};
use crate::math::{basis_xy, Vec3};
use crate::units::inches;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveTuning {
    pub walk_speed: f32,
    pub sprint_scale: f32,
    pub crouch_scale: f32,
    pub prone_scale: f32,
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
    pub sprint_time: f32,
    pub sprint_recover_time: f32,
    pub stand_eye: f32,
    pub crouch_eye: f32,
    pub prone_eye: f32,
    /// Seconds to blend the eye between stances.
    pub stance_blend_time: f32,
}

impl Default for MoveTuning {
    fn default() -> Self {
        Self {
            walk_speed: inches(190.0),
            sprint_scale: 1.5,
            crouch_scale: 0.65,
            prone_scale: 0.15,
            back_scale: 0.7,
            strafe_scale: 0.8,
            ground_accel: 9.0,
            air_accel: 1.0,
            friction: 5.5,
            stop_speed: inches(100.0),
            gravity: inches(800.0),
            jump_height: inches(39.0),
            step_height: inches(18.0),
            max_slope: 1.4, // ~55 degrees; ZH cliffs are flagged separately by World::is_walkable
            radius: inches(15.0),
            sprint_time: 4.0,
            sprint_recover_time: 2.0,
            stand_eye: inches(60.0) * crate::units::GENERAL_SCALE,
            crouch_eye: inches(40.0) * crate::units::GENERAL_SCALE,
            prone_eye: inches(11.0) * crate::units::GENERAL_SCALE,
            stance_blend_time: 0.2,
        }
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
    pub sprint_stamina: f32,
    /// Seconds before sprint may restart after running dry.
    pub sprint_cooldown: f32,
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
            sprint_stamina: tuning.sprint_time,
            sprint_cooldown: 0.0,
            held_last: 0,
        }
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
    /// Sprint duration multiplier (the weapon's `sprintDurationScale`, Extreme Conditioning).
    pub sprint_time_scale: f32,
}

impl Default for MoveModifiers {
    fn default() -> Self {
        Self { speed_scale: 1.0, block_sprint: false, sprint_time_scale: 1.0 }
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

    update_stance(body, input.held, pressed, tuning);

    let (fwd_axis, right_axis) = input.move_axes();
    update_sprint(body, input.held, fwd_axis, mods, tuning, dt);

    // Wish direction and speed, BO2 style: forward full speed, strafe/back scaled.
    let (fwd, right) = basis_xy(input.yaw);
    let fwd_scale = if fwd_axis >= 0.0 { 1.0 } else { tuning.back_scale };
    let mut wish = fwd * (fwd_axis * fwd_scale) + right * (right_axis * tuning.strafe_scale);
    let wish_len = wish.length_xy();
    if wish_len > 1.0 {
        wish = wish * (1.0 / wish_len);
    }
    let wish_dir = wish.normalized_or_zero();
    let mut max_speed = tuning.walk_speed * mods.speed_scale * stance_scale(body.stance, tuning);
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

fn stance_scale(stance: Stance, t: &MoveTuning) -> f32 {
    match stance {
        Stance::Stand => 1.0,
        Stance::Crouch => t.crouch_scale,
        Stance::Prone => t.prone_scale,
    }
}

fn update_stance(body: &mut Body, held: u32, pressed: u32, t: &MoveTuning) {
    let _ = t;
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

fn update_sprint(body: &mut Body, held: u32, fwd_axis: f32, mods: MoveModifiers, t: &MoveTuning, dt: f32) {
    let wants = held & buttons::SPRINT != 0 && fwd_axis > 0.5 && body.stance == Stance::Stand;
    let can = !mods.block_sprint && body.sprint_cooldown <= 0.0 && body.sprint_stamina > 0.0;
    // Sprint starts only on the ground but carries through a jump.
    body.sprinting = wants && can && (body.on_ground || body.sprinting);
    if body.sprinting {
        body.sprint_stamina -= dt;
        if body.sprint_stamina <= 0.0 {
            body.sprint_stamina = 0.0;
            body.sprinting = false;
            body.sprint_cooldown = t.sprint_recover_time;
        }
    } else {
        body.sprint_cooldown = (body.sprint_cooldown - dt).max(0.0);
        let full = t.sprint_time * mods.sprint_time_scale;
        body.sprint_stamina = (body.sprint_stamina + dt * full / t.sprint_recover_time).min(full);
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
        assert!(!b.sprinting, "stamina exhausted after sprint_time");
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
