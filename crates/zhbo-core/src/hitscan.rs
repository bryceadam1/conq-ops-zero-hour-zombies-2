//! Instant-hit traces against Zero Hour objects and terrain.
//!
//! Zero Hour geometry is coarse (infantry are 10-unit-radius cylinders so they are easy to click),
//! so infantry are traced as BO2's soldier: the MP body's hit boxes ([`crate::hitloc::Body`],
//! converted from the install), each with its hit location. Without converted boxes (the built-in
//! table) they fall back to a BO2-sized cylinder with a head band at the top.

use crate::math::Vec3;
use crate::units::inches;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Sphere { radius: f32 },
    /// Vertical cylinder standing on the object's position.
    Cylinder { radius: f32, height: f32 },
    /// Box standing on the object's position, rotated by `yaw` about +Z.
    Box { half_x: f32, half_y: f32, height: f32, yaw: f32 },
    /// BO2's soldier: the body's hit boxes (in the pose of his body state) standing on the
    /// object's position, turned by `yaw` about +Z and grown by `scale` (the General's drawn
    /// size); `rifle`: the shot ranks locations as a rifle bullet does ([`crate::hitloc::rank`]).
    Body { body: &'static crate::hitloc::Body, yaw: f32, scale: f32, rifle: bool },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub id: u32,
    pub pos: Vec3,
    pub shape: Shape,
    /// Fraction of the height (from the top) that counts as the head; 0 for non-infantry.
    pub head_fraction: f32,
}

/// BO2-sized infantry hit cylinder: ~16 inch radius, head is the top ~15%.
pub const INFANTRY_HIT_RADIUS: f32 = inches(16.0);
pub const INFANTRY_HEAD_FRACTION: f32 = 0.15;

/// An object's solid without hit boxes: infantry as BO2's soldier-sized cylinder (Zero Hour's own
/// infantry cylinders are click-sized) with its head band, anything else as its Zero Hour geometry
/// (box, sphere or cylinder).
pub fn solid(o: &crate::host::ObjInfo) -> Target {
    use crate::host::{geom, objflags};
    if o.has(objflags::INFANTRY) {
        return Target {
            id: o.id,
            pos: o.pos,
            shape: Shape::Cylinder { radius: INFANTRY_HIT_RADIUS.min(o.major_radius.max(1.0)), height: o.height.max(6.0) },
            head_fraction: INFANTRY_HEAD_FRACTION,
        };
    }
    let shape = match o.geom_type {
        geom::BOX => Shape::Box { half_x: o.major_radius, half_y: o.minor_radius, height: o.height.max(1.0), yaw: o.yaw },
        geom::SPHERE => Shape::Sphere { radius: o.major_radius },
        _ => Shape::Cylinder { radius: o.major_radius, height: o.height.max(1.0) },
    };
    Target { id: o.id, pos: o.pos, shape, head_fraction: 0.0 }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HitResult {
    Nothing { end: Vec3 },
    Terrain { point: Vec3 },
    /// `location`: the BO2 hit location struck ([`crate::hitloc`]; `NONE` for anything but a soldier).
    Object { id: u32, point: Vec3, distance: f32, location: u8 },
}

impl HitResult {
    pub fn point(&self) -> Vec3 {
        match *self {
            HitResult::Nothing { end } => end,
            HitResult::Terrain { point } => point,
            HitResult::Object { point, .. } => point,
        }
    }
}

/// Distance along the ray to the first intersection with the shape, if within `max_t`.
pub fn ray_vs_target(origin: Vec3, dir: Vec3, max_t: f32, target: &Target) -> Option<f32> {
    ray_location(origin, dir, max_t, target).map(|(t, _)| t)
}

/// A turn by `-yaw` about +Z: world offsets into a target's own frame.
fn unyaw(v: Vec3, yaw: f32) -> Vec3 {
    let (s, c) = crate::math::det::sin_cos(yaw);
    Vec3::new(v.x * c + v.y * s, -v.x * s + v.y * c, v.z)
}

/// Where the ray strikes the target and the hit location there: a soldier's box by BO2's pick, the
/// fallback cylinder's head band or body (`TORSO_UPPER`), `NONE` for anything else.
pub fn ray_location(origin: Vec3, dir: Vec3, max_t: f32, target: &Target) -> Option<(f32, u8)> {
    use crate::hitloc::{HEAD, NONE, TORSO_UPPER};
    let o = origin - target.pos;
    if let Shape::Body { body, yaw, scale, rifle } = target.shape {
        // The ray into the unscaled body's frame: positions shrink by the scale, the distance
        // along the (unit) direction with them.
        let k = 1.0 / scale;
        return body.pick(unyaw(o, yaw) * k, unyaw(dir, yaw), max_t * k, rifle).map(|(t, l)| (t * scale, l));
    }
    let t = ray_shape(o, dir, max_t, target.shape)?;
    let location = match target.shape {
        Shape::Cylinder { height, .. } if target.head_fraction > 0.0 => {
            if o.z + dir.z * t >= height * (1.0 - target.head_fraction) {
                HEAD
            } else {
                TORSO_UPPER
            }
        }
        _ => NONE,
    };
    Some((t, location))
}

fn ray_shape(o: Vec3, dir: Vec3, max_t: f32, shape: Shape) -> Option<f32> {
    match shape {
        Shape::Body { .. } => None,
        Shape::Sphere { radius } => {
            // Sphere centred half a radius above the ground point.
            let c = Vec3::new(0.0, 0.0, radius);
            ray_sphere(o - c, dir, radius).filter(|&t| t <= max_t)
        }
        Shape::Cylinder { radius, height } => ray_cylinder(o, dir, radius, height).filter(|&t| t <= max_t),
        Shape::Box { half_x, half_y, height, yaw } => {
            ray_aabb(
                unyaw(o, yaw),
                unyaw(dir, yaw),
                Vec3::new(-half_x, -half_y, 0.0),
                Vec3::new(half_x, half_y, height),
            )
            .filter(|&t| t <= max_t)
        }
    }
}

fn ray_sphere(o: Vec3, d: Vec3, r: f32) -> Option<f32> {
    let b = o.dot(d);
    let c = o.dot(o) - r * r;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let sq = disc.sqrt();
    let t0 = -b - sq;
    let t1 = -b + sq;
    if t0 >= 0.0 {
        Some(t0)
    } else if t1 >= 0.0 {
        Some(0.0)
    } else {
        None
    }
}

fn ray_cylinder(o: Vec3, d: Vec3, r: f32, h: f32) -> Option<f32> {
    let mut best: Option<f32> = None;
    let mut consider = |t: f32| {
        if t >= 0.0 && best.is_none_or(|b| t < b) {
            best = Some(t);
        }
    };
    // Side wall.
    let a = d.x * d.x + d.y * d.y;
    if a > 1e-9 {
        let b = o.x * d.x + o.y * d.y;
        let c = o.x * o.x + o.y * o.y - r * r;
        let disc = b * b - a * c;
        if disc >= 0.0 {
            let sq = disc.sqrt();
            for t in [(-b - sq) / a, (-b + sq) / a] {
                let z = o.z + d.z * t;
                if (0.0..=h).contains(&z) {
                    consider(t);
                }
            }
        }
    }
    // Caps.
    if d.z.abs() > 1e-9 {
        for cap in [0.0, h] {
            let t = (cap - o.z) / d.z;
            let x = o.x + d.x * t;
            let y = o.y + d.y * t;
            if x * x + y * y <= r * r {
                consider(t);
            }
        }
    }
    // Origin inside.
    if o.x * o.x + o.y * o.y <= r * r && (0.0..=h).contains(&o.z) {
        consider(0.0);
    }
    best
}

pub(crate) fn ray_aabb(o: Vec3, d: Vec3, lo: Vec3, hi: Vec3) -> Option<f32> {
    let mut tmin = 0.0f32;
    let mut tmax = f32::INFINITY;
    for (oi, di, l, h) in [(o.x, d.x, lo.x, hi.x), (o.y, d.y, lo.y, hi.y), (o.z, d.z, lo.z, hi.z)] {
        if di.abs() < 1e-9 {
            if oi < l || oi > h {
                return None;
            }
        } else {
            let inv = 1.0 / di;
            let (mut t0, mut t1) = ((l - oi) * inv, (h - oi) * inv);
            if t0 > t1 {
                std::mem::swap(&mut t0, &mut t1);
            }
            tmin = tmin.max(t0);
            tmax = tmax.min(t1);
            if tmin > tmax {
                return None;
            }
        }
    }
    Some(tmin)
}

/// First terrain intersection along the ray within `max_t`, by marching and bisecting.
pub fn trace_terrain(origin: Vec3, dir: Vec3, max_t: f32, ground: &dyn Fn(f32, f32) -> f32) -> Option<f32> {
    const STEP: f32 = 2.5;
    let below = |t: f32| {
        let p = origin + dir * t;
        p.z < ground(p.x, p.y)
    };
    if below(0.0) {
        return Some(0.0);
    }
    let mut prev = 0.0;
    let mut t = STEP;
    while prev < max_t {
        let tc = t.min(max_t);
        if below(tc) {
            let (mut lo, mut hi) = (prev, tc);
            for _ in 0..12 {
                let mid = 0.5 * (lo + hi);
                if below(mid) {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            return Some(hi);
        }
        prev = tc;
        t += STEP;
    }
    None
}

/// Whether `point` is inside the target's solid (on its surface counts).
pub fn contains(target: &Target, point: Vec3) -> bool {
    let o = point - target.pos;
    match target.shape {
        Shape::Sphere { radius } => (o - Vec3::new(0.0, 0.0, radius)).length() <= radius,
        Shape::Cylinder { radius, height } => o.x * o.x + o.y * o.y <= radius * radius && (0.0..=height).contains(&o.z),
        Shape::Box { half_x, half_y, height, yaw } => {
            let (s, c) = crate::math::det::sin_cos(yaw);
            let (x, y) = (o.x * c + o.y * s, -o.x * s + o.y * c);
            x.abs() <= half_x && y.abs() <= half_y && (0.0..=height).contains(&o.z)
        }
        Shape::Body { body, yaw, scale, .. } => body.contains(unyaw(o, yaw) * (1.0 / scale)),
    }
}

/// How far `point` is from the target's solid: 0 on or inside it, else the distance to its nearest
/// point. BO2's radius damage measures each entity this way, to its bounds rather than its origin
/// (t6mp `FUN_005f1fd0`, called by `FUN_005f2130`: the blast's distance to the entity's
/// `absmin`/`absmax`, clamped per axis), so a blast against a tank's hull is at the hull.
pub fn distance_to(target: &Target, point: Vec3) -> f32 {
    let o = point - target.pos;
    // Outside the interval [lo, hi] by how much (0 within it).
    let out = |v: f32, lo: f32, hi: f32| (lo - v).max(v - hi).max(0.0);
    match target.shape {
        Shape::Sphere { radius } => ((o - Vec3::new(0.0, 0.0, radius)).length() - radius).max(0.0),
        Shape::Cylinder { radius, height } => {
            let side = ((o.x * o.x + o.y * o.y).sqrt() - radius).max(0.0);
            let up = out(o.z, 0.0, height);
            (side * side + up * up).sqrt()
        }
        Shape::Box { half_x, half_y, height, yaw } => {
            let l = unyaw(o, yaw);
            Vec3::new(out(l.x, -half_x, half_x), out(l.y, -half_y, half_y), out(l.z, 0.0, height)).length()
        }
        Shape::Body { body, yaw, scale, .. } => body.distance(unyaw(o, yaw) * (1.0 / scale)) * scale,
    }
}

/// Full trace: nearest of terrain and targets (skipping `ignore`).
pub fn trace(
    origin: Vec3,
    dir: Vec3,
    range: f32,
    targets: &[Target],
    ignore: u32,
    ground: &dyn Fn(f32, f32) -> f32,
) -> HitResult {
    trace_where(origin, dir, range, targets, ignore, ground, &|_| true)
}

/// [`trace`] against the targets `keep` accepts: a penetrating round's trace onward from inside
/// the solid it entered skips the solids it starts in, as a world trace leaving a brush does.
pub fn trace_where(
    origin: Vec3,
    dir: Vec3,
    range: f32,
    targets: &[Target],
    ignore: u32,
    ground: &dyn Fn(f32, f32) -> f32,
    keep: &dyn Fn(&Target) -> bool,
) -> HitResult {
    let terrain_t = trace_terrain(origin, dir, range, ground);
    let limit = terrain_t.unwrap_or(range);
    let mut best: Option<(f32, u32, u8)> = None;
    for t in targets.iter().filter(|t| t.id != ignore && keep(t)) {
        if let Some((d, location)) = ray_location(origin, dir, limit, t) {
            if best.is_none_or(|(bd, _, _)| d < bd) {
                best = Some((d, t.id, location));
            }
        }
    }
    match (best, terrain_t) {
        (Some((d, id, location)), _) => HitResult::Object { id, point: origin + dir * d, distance: d, location },
        (None, Some(tt)) => HitResult::Terrain { point: origin + dir * tt },
        (None, None) => HitResult::Nothing { end: origin + dir * range },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn soldier(id: u32, x: f32) -> Target {
        Target {
            id,
            pos: Vec3::new(x, 0.0, 0.0),
            shape: Shape::Cylinder { radius: INFANTRY_HIT_RADIUS, height: 12.0 },
            head_fraction: INFANTRY_HEAD_FRACTION,
        }
    }

    #[test]
    fn hits_nearest_and_detects_headshots() {
        let flat = |_: f32, _: f32| 0.0;
        let targets = [soldier(2, 100.0), soldier(1, 50.0)];
        let body = trace(Vec3::new(0.0, 0.0, 6.0), Vec3::new(1.0, 0.0, 0.0), 500.0, &targets, 0, &flat);
        match body {
            HitResult::Object { id, location, distance, .. } => {
                assert_eq!(id, 1);
                assert_eq!(location, crate::hitloc::TORSO_UPPER);
                assert!((distance - (50.0 - INFANTRY_HIT_RADIUS)).abs() < 1e-3);
            }
            other => panic!("expected hit, got {other:?}"),
        }
        let head = trace(Vec3::new(0.0, 0.0, 11.5), Vec3::new(1.0, 0.0, 0.0), 500.0, &targets, 0, &flat);
        assert!(matches!(head, HitResult::Object { id: 1, location: crate::hitloc::HEAD, .. }));
    }

    #[test]
    fn ignores_shooter_and_respects_terrain() {
        let hill = |x: f32, _: f32| if x > 30.0 && x < 40.0 { 20.0 } else { 0.0 };
        let targets = [soldier(7, 0.0), soldier(1, 50.0)];
        let r = trace(Vec3::new(0.0, 0.0, 6.0), Vec3::new(1.0, 0.0, 0.0), 500.0, &targets, 7, &hill);
        match r {
            HitResult::Terrain { point } => assert!((point.x - 30.0).abs() < 2.6),
            other => panic!("hill should block, got {other:?}"),
        }
    }

    /// BO2's radius damage measures to the entity's bounds (`FUN_005f1fd0`): 0 on or inside the
    /// solid, else the distance to its nearest point, for every shape.
    #[test]
    fn distance_to_a_solid_is_to_its_nearest_point() {
        let at = |shape| Target { id: 1, pos: Vec3::new(100.0, 50.0, 0.0), shape, head_fraction: 0.0 };
        let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
        // A box 30 long and 20 wide turned a quarter: its long side runs along y.
        let tank = at(Shape::Box { half_x: 15.0, half_y: 10.0, height: 10.0, yaw: std::f32::consts::FRAC_PI_2 });
        assert!(near(distance_to(&tank, Vec3::new(100.0, 50.0, 5.0)), 0.0), "inside");
        assert!(near(distance_to(&tank, Vec3::new(117.0, 50.0, 5.0)), 7.0), "off its narrow side");
        assert!(near(distance_to(&tank, Vec3::new(100.0, 70.0, 5.0)), 5.0), "off its long end");
        assert!(near(distance_to(&tank, Vec3::new(100.0, 50.0, 13.0)), 3.0), "over its roof");
        assert!(near(distance_to(&tank, Vec3::new(113.0, 69.0, 5.0)), 5.0), "off a corner, diagonally");
        let cyl = at(Shape::Cylinder { radius: 4.0, height: 12.0 });
        assert!(near(distance_to(&cyl, Vec3::new(110.0, 50.0, 6.0)), 6.0));
        assert!(near(distance_to(&cyl, Vec3::new(107.0, 50.0, 16.0)), 5.0), "past the rim: (3, 4) off it");
        let ball = at(Shape::Sphere { radius: 5.0 });
        assert!(near(distance_to(&ball, Vec3::new(100.0, 50.0, 15.0)), 5.0));
        assert!(near(distance_to(&ball, Vec3::new(100.0, 52.0, 5.0)), 0.0));
    }

    #[test]
    fn rotated_box_and_sphere() {
        let tank = Target {
            id: 9,
            pos: Vec3::new(40.0, 0.0, 0.0),
            shape: Shape::Box { half_x: 15.0, half_y: 8.0, height: 10.0, yaw: std::f32::consts::FRAC_PI_2 },
            head_fraction: 0.0,
        };
        // Rotated 90 degrees: the 15 half-extent now lies along Y, so the near face is at x = 32.
        let d = ray_vs_target(Vec3::new(0.0, 0.0, 5.0), Vec3::new(1.0, 0.0, 0.0), 100.0, &tank).unwrap();
        assert!((d - 32.0).abs() < 1e-3, "{d}");
        let ball = Target { id: 3, pos: Vec3::new(20.0, 0.0, 0.0), shape: Shape::Sphere { radius: 5.0 }, head_fraction: 0.0 };
        let d = ray_vs_target(Vec3::new(0.0, 0.0, 5.0), Vec3::new(1.0, 0.0, 0.0), 100.0, &ball).unwrap();
        assert!((d - 15.0).abs() < 1e-3);
    }
}
