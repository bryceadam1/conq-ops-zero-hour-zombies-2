/*
** Conquer Ops - Zero Hour gameplay core: how much of a target a blast sees.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! BO2's damage visibility (t6mp `FUN_005f0f00`, called by the radius damage `FUN_005f2130` with
//! trace mask `0x802013`): the share of a target a blast at a point sees, which scales the blast's
//! damage (an entity it sees none of takes none, `FUN_005f2130` skips it). The traces run from the
//! blast to a few points on the target, ignoring the target and the blast's own entity; what
//! blocks them is the world: here the terrain and every solid that is not a soldier (Zero Hour's
//! structures, vehicles and other solid objects; BO2's players never block a blast).
//!
//! * **A player** (BO2's client; here any soldier): five points — his middle, halfway between his
//!   feet and his eye, and four more [`Tuning::damage_visibility_width`] to each side of it
//!   (across the way from his feet to the blast, horizontally) at his eye and his feet. None seen:
//!   0; one to three: [`Tuning::damage_visibility_share`] each; four or five: all of it.
//! * **Anything else**: the middle of its bounds and the four corners of its outline as the blast
//!   sees it (its bounds' extent across and up from the way to the blast). Any one seen: all of
//!   it; none: nothing.
//!
//! Deterministic: logic data only (the height map and the objects' Zero Hour geometry).

use crate::hitscan::{Shape, Target};
use crate::host::{objflags, LogicHost, ObjInfo};
use crate::math::Vec3;
use crate::tuning::Tuning;
use crate::units::inches;

/// What may stand between a blast and its targets: the solids near it (the terrain is the host's).
pub struct Blockers {
    solids: Vec<Target>,
}

impl Blockers {
    /// The solids within `radius` of `at` (as `viewer` sees them; allegiance does not matter to a
    /// wall), less those in `ignore` (the object the blast struck or is stuck to: it starts on or
    /// in it). A soldier never blocks (BO2's trace mask leaves players out).
    pub fn near(host: &dyn LogicHost, at: Vec3, radius: f32, viewer: u32, ignore: &[u32]) -> Blockers {
        let mut objs = Vec::new();
        host.query(at, radius.max(1.0), viewer, &mut objs);
        let mut solids: Vec<Target> = objs
            .iter()
            .filter(|o| blocks(o) && !ignore.contains(&o.id))
            .map(crate::hitscan::solid)
            .collect();
        solids.sort_by_key(|t| t.id);
        Blockers { solids }
    }

    /// Nothing but the terrain (a blast in the open).
    pub fn none() -> Blockers {
        Blockers { solids: Vec::new() }
    }

    /// Whether the line from `from` to `to` is clear of the terrain and of every blocker but
    /// `target` (`FUN_006f3250` with the target and the blast's entity ignored).
    pub fn clear(&self, host: &dyn LogicHost, from: Vec3, to: Vec3, target: u32) -> bool {
        let seg = to - from;
        let len = seg.length();
        if len < 1e-4 {
            return true;
        }
        let dir = seg * (1.0 / len);
        let ground = |x: f32, y: f32| host.ground_height(x, y);
        if crate::hitscan::trace_terrain(from, dir, len, &ground).is_some_and(|t| t < len - 1e-3) {
            return false;
        }
        !self.solids.iter().filter(|s| s.id != target).any(|s| crate::hitscan::ray_vs_target(from, dir, len, s).is_some_and(|t| t < len - 1e-3))
    }

    /// The share of `o` a blast at `at` sees (`FUN_005f0f00`). `eye`: a soldier's eye height over
    /// his feet (the sim's own for a General it moves, [`LogicHost::sim_eye`]).
    pub fn fraction(&self, host: &dyn LogicHost, t: &Tuning, at: Vec3, o: &ObjInfo) -> f32 {
        if o.has(objflags::INFANTRY) {
            return self.player_fraction(host, t, at, o.id, o.pos, soldier_eye(host, t, o));
        }
        let (lo, hi) = bounds(&crate::hitscan::solid(o));
        if entity_points(at, lo, hi).iter().any(|p| self.clear(host, at, *p, o.id)) {
            1.0
        } else {
            0.0
        }
    }

    /// The share of player `id` (feet at `feet`, eye `eye` over them) a blast at `at` sees.
    pub fn player_fraction(&self, host: &dyn LogicHost, t: &Tuning, at: Vec3, id: u32, feet: Vec3, eye: f32) -> f32 {
        let seen = player_points(t, at, feet, eye).iter().filter(|p| self.clear(host, at, **p, id)).count();
        match seen {
            0 => 0.0,
            1..=3 => seen as f32 * t.damage_visibility_share,
            _ => 1.0,
        }
    }

    /// Whether a blast at `at` sees anything of a point-sized piece at `pos` (placed equipment).
    pub fn sees_point(&self, host: &dyn LogicHost, at: Vec3, pos: Vec3, id: u32) -> bool {
        self.clear(host, at, pos, id)
    }
}

/// Whether `o` stands in a blast's way: a live solid that is not a soldier (BO2's players and
/// non-solid entities are not in the visibility trace's mask).
fn blocks(o: &ObjInfo) -> bool {
    !o.has(objflags::INFANTRY) && !o.has(objflags::DEAD) && !o.has(objflags::NO_COLLIDE) && !o.has(objflags::PROJECTILE)
}

/// A soldier's eye over his feet: the sim's own for a General it moves, else the standing eye
/// (`player_standingViewHeight`) on his height.
pub fn soldier_eye(host: &dyn LogicHost, t: &Tuning, o: &ObjInfo) -> f32 {
    host.sim_eye(o.id).unwrap_or_else(|| inches(t.player_standing_view_height).min(o.height.max(1e-3)))
}

/// A player's five visibility points (`FUN_005f0f00`'s client branch): the middle between his
/// feet and eye, then `± side` across the horizontal way from his feet to the blast, each at
/// `± half` his eye height (its order: +side up, +side down, -side up, -side down).
pub fn player_points(t: &Tuning, at: Vec3, feet: Vec3, eye: f32) -> [Vec3; 5] {
    let mid = feet + Vec3::new(0.0, 0.0, eye * 0.5);
    let half = Vec3::new(0.0, 0.0, eye * 0.5);
    let (dx, dy) = (at.x - feet.x, at.y - feet.y);
    let len = (dx * dx + dy * dy).sqrt();
    // BO2 normalises by 1 when the blast is straight above or below (no side).
    let inv = if len > 0.0 { 1.0 / len } else { 1.0 };
    let side = Vec3::new(-dy * inv, dx * inv, 0.0) * inches(t.damage_visibility_width);
    [mid, mid + side + half, mid + side - half, mid - side + half, mid - side - half]
}

/// An entity's five visibility points (`FUN_005f0f00`'s other branch): the middle of its bounds
/// `lo..hi`, then the four corners of its outline across (`p`, horizontal) and up (`u`) from the way
/// `d` to the blast, each at the bounds' extent along it.
pub fn entity_points(at: Vec3, lo: Vec3, hi: Vec3) -> [Vec3; 5] {
    let c = (lo + hi) * 0.5;
    let e = hi - c;
    let mut d = (at - c).normalized_or_zero();
    if d.x * d.x + d.y * d.y < 0.001 {
        d = Vec3::new(1.0, 0.0, 0.0);
    }
    let p = Vec3::new(-d.y, d.x, 0.0).normalized_or_zero();
    let u = d.cross(p);
    let w = (e.x * p.x).abs() + (e.y * p.y).abs();
    let h = (e.x * u.x).abs() + (e.y * u.y).abs() + (e.z * u.z).abs();
    let (pw, uh) = (p * w, u * h);
    [c, c + pw + uh, c - pw + uh, c + pw - uh, c - pw - uh]
}

/// The axis-aligned bounds of a solid (BO2's `absmin`/`absmax`).
pub fn bounds(t: &Target) -> (Vec3, Vec3) {
    let p = t.pos;
    match t.shape {
        Shape::Sphere { radius } => (Vec3::new(p.x - radius, p.y - radius, p.z), Vec3::new(p.x + radius, p.y + radius, p.z + 2.0 * radius)),
        Shape::Cylinder { radius, height } => (Vec3::new(p.x - radius, p.y - radius, p.z), Vec3::new(p.x + radius, p.y + radius, p.z + height)),
        Shape::Box { half_x, half_y, height, yaw } => {
            let (s, c) = crate::math::det::sin_cos(yaw);
            let ex = (half_x * c).abs() + (half_y * s).abs();
            let ey = (half_x * s).abs() + (half_y * c).abs();
            (Vec3::new(p.x - ex, p.y - ey, p.z), Vec3::new(p.x + ex, p.y + ey, p.z + height))
        }
        // A soldier is traced by the player branch; his body never reaches here.
        Shape::Body { .. } => (p, p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::tests::{general, FakeHost};

    fn wall(id: u32, x: f32) -> ObjInfo {
        ObjInfo { id, flags: objflags::STRUCTURE, pos: Vec3::new(x, 0.0, 0.0), geom_type: crate::host::geom::BOX, major_radius: 1.0, minor_radius: 30.0, height: 30.0, ..Default::default() }
    }

    /// In the open a soldier is all seen; behind a wall taller than him, none; a wall a little
    /// over his middle hides his middle and low points: the two high ones are seen, 2 x 1/3.
    #[test]
    fn a_wall_hides_a_player_by_bo2s_points() {
        let t = Tuning::default();
        let host = FakeHost::default();
        let soldier = ObjInfo { pos: Vec3::new(20.0, 0.0, 0.0), ..general(2, 1) };
        let at = Vec3::new(0.0, 0.0, 1.0);
        assert_eq!(Blockers::none().fraction(&host, &t, at, &soldier), 1.0);
        host.add(wall(9, 10.0));
        let b = Blockers::near(&host, at, 40.0, 1, &[]);
        assert_eq!(b.fraction(&host, &t, at, &soldier), 0.0, "a wall taller than him");
        let eye = soldier_eye(&host, &t, &soldier);
        let low = ObjInfo { height: eye * 0.6, ..wall(9, 10.0) };
        let host = FakeHost::default();
        host.add(low);
        let b = Blockers::near(&host, Vec3::new(0.0, 0.0, eye * 0.5 + 0.01), 40.0, 1, &[]);
        let f = b.fraction(&host, &t, Vec3::new(0.0, 0.0, eye * 0.5 + 0.01), &soldier);
        assert!((f - 2.0 * t.damage_visibility_share).abs() < 1e-4, "{f}");
    }

    /// An entity (a vehicle) is all seen when any of its five points is: its top corners over a
    /// low wall do it.
    #[test]
    fn an_entity_is_seen_by_any_point() {
        let t = Tuning::default();
        let host = FakeHost::default();
        let tank = ObjInfo { id: 5, flags: objflags::VEHICLE, pos: Vec3::new(30.0, 0.0, 0.0), geom_type: crate::host::geom::BOX, major_radius: 5.0, minor_radius: 5.0, height: 10.0, ..Default::default() };
        host.add(ObjInfo { height: 4.0, ..wall(9, 15.0) });
        let at = Vec3::new(0.0, 0.0, 1.0);
        let b = Blockers::near(&host, at, 60.0, 1, &[]);
        assert_eq!(b.fraction(&host, &t, at, &tank), 1.0, "the hull's top shows over the wall");
        let host = FakeHost::default();
        host.add(ObjInfo { height: 40.0, ..wall(9, 15.0) });
        let b = Blockers::near(&host, at, 60.0, 1, &[]);
        assert_eq!(b.fraction(&host, &t, at, &tank), 0.0);
        // The wall the blast is stuck to (ignored) hides nothing.
        let b = Blockers::near(&host, at, 60.0, 1, &[9]);
        assert_eq!(b.fraction(&host, &t, at, &tank), 1.0);
    }

    /// BO2's point sets: the player's five at his middle and `±15"` across the way to the blast,
    /// at `± half` his eye height; an entity's middle and the outline's four corners.
    #[test]
    fn the_point_sets_are_bo2s() {
        let t = Tuning::default();
        let p = player_points(&t, Vec3::new(10.0, 0.0, 0.0), Vec3::ZERO, 10.0);
        let w = inches(t.damage_visibility_width);
        assert_eq!(p[0], Vec3::new(0.0, 0.0, 5.0));
        assert!((p[1] - Vec3::new(0.0, w, 10.0)).length() < 1e-5 && (p[4] - Vec3::new(0.0, -w, 0.0)).length() < 1e-5, "{p:?}");
        let e = entity_points(Vec3::new(10.0, 0.0, 1.0), Vec3::new(-1.0, -2.0, 0.0), Vec3::new(1.0, 2.0, 2.0));
        assert_eq!(e[0], Vec3::new(0.0, 0.0, 1.0));
        // Seen from +x: across is y (extent 2), up is z (extent 1).
        assert!((e[1] - Vec3::new(0.0, 2.0, 2.0)).length() < 0.1, "{e:?}");
    }
}
