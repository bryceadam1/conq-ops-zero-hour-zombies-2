//! Instant-hit traces against Zero Hour objects and terrain.
//!
//! Zero Hour geometry is coarse (infantry are 10-unit-radius cylinders so they are easy to click),
//! so infantry get a tighter BO2-sized hit cylinder with a head band at the top.

use crate::math::Vec3;
use crate::units::inches;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Sphere { radius: f32 },
    /// Vertical cylinder standing on the object's position.
    Cylinder { radius: f32, height: f32 },
    /// Box standing on the object's position, rotated by `yaw` about +Z.
    Box { half_x: f32, half_y: f32, height: f32, yaw: f32 },
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HitResult {
    Nothing { end: Vec3 },
    Terrain { point: Vec3 },
    Object { id: u32, point: Vec3, distance: f32, headshot: bool },
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
    let o = origin - target.pos;
    match target.shape {
        Shape::Sphere { radius } => {
            // Sphere centred half a radius above the ground point.
            let c = Vec3::new(0.0, 0.0, radius);
            ray_sphere(o - c, dir, radius).filter(|&t| t <= max_t)
        }
        Shape::Cylinder { radius, height } => ray_cylinder(o, dir, radius, height).filter(|&t| t <= max_t),
        Shape::Box { half_x, half_y, height, yaw } => {
            let (s, c) = yaw.sin_cos();
            let rot = |v: Vec3| Vec3::new(v.x * c + v.y * s, -v.x * s + v.y * c, v.z);
            ray_aabb(
                rot(o),
                rot(dir),
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

fn ray_aabb(o: Vec3, d: Vec3, lo: Vec3, hi: Vec3) -> Option<f32> {
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

/// Full trace: nearest of terrain and targets (skipping `ignore`).
pub fn trace(
    origin: Vec3,
    dir: Vec3,
    range: f32,
    targets: &[Target],
    ignore: u32,
    ground: &dyn Fn(f32, f32) -> f32,
) -> HitResult {
    let terrain_t = trace_terrain(origin, dir, range, ground);
    let limit = terrain_t.unwrap_or(range);
    let mut best: Option<(f32, &Target)> = None;
    for t in targets.iter().filter(|t| t.id != ignore) {
        if let Some(d) = ray_vs_target(origin, dir, limit, t) {
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, t));
            }
        }
    }
    match (best, terrain_t) {
        (Some((d, t)), _) => {
            let point = origin + dir * d;
            let height = match t.shape {
                Shape::Cylinder { height, .. } | Shape::Box { height, .. } => height,
                Shape::Sphere { radius } => radius * 2.0,
            };
            let headshot = t.head_fraction > 0.0 && point.z >= t.pos.z + height * (1.0 - t.head_fraction);
            HitResult::Object { id: t.id, point, distance: d, headshot }
        }
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
            HitResult::Object { id, headshot, distance, .. } => {
                assert_eq!(id, 1);
                assert!(!headshot);
                assert!((distance - (50.0 - INFANTRY_HIT_RADIUS)).abs() < 1e-3);
            }
            other => panic!("expected hit, got {other:?}"),
        }
        let head = trace(Vec3::new(0.0, 0.0, 11.5), Vec3::new(1.0, 0.0, 0.0), 500.0, &targets, 0, &flat);
        assert!(matches!(head, HitResult::Object { id: 1, headshot: true, .. }));
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
