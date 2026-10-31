/*
** Conquer Ops - Zero Hour gameplay core: the Assault Shield on its bearer.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! The Assault Shield on a General: BO2's riot-shield collmap as the converter's posed box
//! ([`ShieldBox`]), turned with the bearer.
//!
//! * **Bullets, melee and thrown impacts** (`_globallogic_player.gsc:840-888`): a trace that
//!   crosses the shield is a hit on location `riotshield` — rank 101 in the engine's trace, above
//!   every body box, so it wins wherever on that entity the ray crosses it — and the script
//!   returns without damage.
//! * **Blasts** (`FUN_005f2130` → `FUN_00670940`): the radius damage keeps `1 − protection` of
//!   itself, protection by how close the blast is to the shield tag's forward axis
//!   (`shieldBlastDamageProtection_30/60/120/180`: within half of each arc).
//!
//! Positions are Zero Hour world units; the box is inches on the body (x forward, y left, z up).

use crate::math::Vec3;
use crate::tuning::Tuning;
use crate::units::ZH_PER_INCH;
use crate::weapons::{ShieldBox, WeaponDef};

/// A General carrying a shield this frame, in the pose its box was measured in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bearer {
    pub general: u32,
    /// A deployed shield's host object (0 for a carried shield).
    pub object: u32,
    /// The General's feet.
    pub pos: Vec3,
    pub yaw: f32,
    /// Held (the weapon up) or stowed (on the back): the box that applies.
    pub held: bool,
    pub b: ShieldBox,
}

fn qmul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

fn qrot(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let c = [-q[0], -q[1], -q[2], q[3]];
    let p = qmul(qmul(q, [v[0], v[1], v[2], 0.0]), c);
    [p[0], p[1], p[2]]
}

fn qinv(q: [f32; 4]) -> [f32; 4] {
    [-q[0], -q[1], -q[2], q[3]]
}

impl Bearer {
    /// The shield `def` carries, held when it is the weapon up, else stowed. `None` for a weapon
    /// that is no shield.
    pub fn of(general: u32, pos: Vec3, yaw: f32, def: &WeaponDef, held: bool) -> Option<Bearer> {
        let g = def.shield?;
        Some(Bearer { general, object: 0, pos, yaw, held, b: if held { g.held } else { g.stowed } })
    }

    /// The mirror's words ([`MIRROR_WORDS`] of them): ids, floats as bits.
    pub fn to_words(&self) -> [u32; MIRROR_WORDS] {
        let f = |x: f32| x.to_bits();
        let mut w = [0u32; MIRROR_WORDS];
        w[0] = self.general;
        w[1] = self.object;
        (w[2], w[3], w[4], w[5], w[6]) = (f(self.pos.x), f(self.pos.y), f(self.pos.z), f(self.yaw), self.held as u32);
        for (k, v) in self.b.to_nums().into_iter().enumerate() {
            w[7 + k] = f(v);
        }
        w
    }

    pub fn from_words(w: &[u32; MIRROR_WORDS]) -> Bearer {
        let f = |i: usize| f32::from_bits(w[i]);
        let nums: Vec<f32> = (7..MIRROR_WORDS).map(f).collect();
        Bearer { general: w[0], object: w[1], pos: Vec3::new(f(2), f(3), f(4)), yaw: f(5), held: w[6] != 0, b: ShieldBox::from_nums(&nums) }
    }

    /// World → body inches.
    fn body_of(&self, v: Vec3, point: bool) -> [f32; 3] {
        let d = if point { v - self.pos } else { v };
        let (s, c) = self.yaw.sin_cos();
        let k = if point { 1.0 / ZH_PER_INCH } else { 1.0 };
        [(d.x * c + d.y * s) * k, (-d.x * s + d.y * c) * k, d.z * k]
    }

    /// Body inches → world.
    fn world_of(&self, v: [f32; 3], point: bool) -> Vec3 {
        let (s, c) = self.yaw.sin_cos();
        let k = if point { ZH_PER_INCH } else { 1.0 };
        let w = Vec3::new((v[0] * c - v[1] * s) * k, (v[0] * s + v[1] * c) * k, v[2] * k);
        if point {
            self.pos + w
        } else {
            w
        }
    }

    /// Distance (world units) along the ray `origin + dir·t` (`dir` a unit vector) at which it
    /// enters the shield box, if within `max_t`.
    pub fn ray(&self, origin: Vec3, dir: Vec3, max_t: f32) -> Option<f32> {
        let o = self.body_of(origin, true);
        let d = self.body_of(dir, false);
        let inv = qinv(self.b.rot);
        let o = qrot(inv, [o[0] - self.b.pos[0], o[1] - self.b.pos[1], o[2] - self.b.pos[2]]);
        let d = qrot(inv, d);
        // Slabs; `t` in inches along a unit direction.
        let (mut lo, mut hi) = (0.0f32, max_t / ZH_PER_INCH);
        for k in 0..3 {
            if d[k].abs() < 1e-9 {
                if o[k] < self.b.mins[k] || o[k] > self.b.maxs[k] {
                    return None;
                }
                continue;
            }
            let (a, b) = ((self.b.mins[k] - o[k]) / d[k], (self.b.maxs[k] - o[k]) / d[k]);
            lo = lo.max(a.min(b));
            hi = hi.min(a.max(b));
            if lo > hi {
                return None;
            }
        }
        Some(lo * ZH_PER_INCH)
    }

    /// The shield tag's origin and forward axis (axis 0 of its matrix), in the world.
    pub fn tag(&self) -> (Vec3, Vec3) {
        (self.world_of(self.b.pos, true), self.world_of(qrot(self.b.rot, [1.0, 0.0, 0.0]), false))
    }

    /// The share of a blast at `at` the shield takes away (`FUN_00670940`): by the cosine between
    /// the tag's forward axis and the direction to the blast, within half of each protection arc.
    pub fn blast_protection(&self, at: Vec3, t: &Tuning) -> f32 {
        let (origin, fwd) = self.tag();
        let dot = (at - origin).normalized_or_zero().dot(fwd);
        let arcs = [(30.0f32, t.shield_blast_protection_30), (60.0, t.shield_blast_protection_60), (120.0, t.shield_blast_protection_120), (180.0, t.shield_blast_protection_180)];
        arcs.into_iter().find(|(arc, _)| dot > (arc * 0.5).to_radians().cos()).map_or(0.0, |(_, p)| p)
    }
}

/// The first shield of `bearers` (filtered by `counts`) a ray blocks, as (index, distance).
///
/// BO2 traces the shield's collmap and the body's hit boxes together and the nearest wins; the
/// held shield stands in front of the boxes, the stowed one behind them. The mod's body is a
/// coarse cylinder, so on the entity the ray hit (`hit`) the shield is compared with where the ray
/// passes the body's axis (the boxes centre on it), not with the cylinder's skin: a ray through
/// the shield before it reaches the body's middle is blocked, one that meets the shield only on
/// the way out (a shot in the back of a held shield) is not. Any other bearer blocks when its
/// shield comes before `reach`.
pub fn blocking(bearers: &[Bearer], counts: &dyn Fn(&Bearer) -> bool, origin: Vec3, dir: Vec3, reach: f32, max_t: f32, hit: Option<u32>) -> Option<(usize, f32)> {
    bearers
        .iter()
        .enumerate()
        .filter(|(_, b)| counts(b))
        .filter_map(|(i, b)| {
            let limit = if hit == Some(b.general) { axis_passing(b.pos, origin, dir).min(max_t) } else { reach };
            b.ray(origin, dir, max_t).filter(|&t| t <= limit).map(|t| (i, t))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
}

/// Words per bearer in the engine's lock-free mirror (`ffi.rs`).
pub const MIRROR_WORDS: usize = 7 + ShieldBox::NUMS;

/// Where a Zero Hour unit's shot at a General is aimed: the middle of the sim's 72-inch body.
pub const BODY_MIDDLE: f32 = crate::units::inches(36.0);

/// The core's vocabulary for engine damage reaching a General ([`crate::host::damage_kind`];
/// the bridge maps Zero Hour's `DamageType` onto it): what a shield does to it.
///
/// What share of a Zero Hour unit's damage reaches General `victim` through his shield:
/// a round or blow whose line from the source (`from`, the source's feet and height) to the
/// body's middle crosses the shield is stopped (0); an explosion keeps `1 − protection` with the
/// source standing in for the blast; anything else, or no shield, passes whole (1).
pub fn engine_scale(bearers: &[Bearer], victim: u32, from: Option<(Vec3, f32)>, kind: i32, t: &Tuning) -> f32 {
    use crate::host::damage_kind;
    let Some(b) = bearers.iter().find(|b| b.general == victim && b.object == 0) else { return 1.0 };
    let Some((src, height)) = from else { return 1.0 };
    match kind {
        damage_kind::SMALL_ARMS | damage_kind::MELEE => {
            let origin = src + Vec3::new(0.0, 0.0, height * 0.8);
            let aim = b.pos + Vec3::new(0.0, 0.0, BODY_MIDDLE);
            let to = aim - origin;
            let len = to.length();
            let all = |_: &Bearer| true;
            if blocking(std::slice::from_ref(b), &all, origin, to.normalized_or_zero(), len, len, Some(victim)).is_some() {
                0.0
            } else {
                1.0
            }
        }
        damage_kind::EXPLOSION => 1.0 - b.blast_protection(src + Vec3::new(0.0, 0.0, height * 0.5), t),
        _ => 1.0,
    }
}

/// Distance along the ray to its closest approach to the vertical axis through `foot`.
fn axis_passing(foot: Vec3, origin: Vec3, dir: Vec3) -> f32 {
    let flat = Vec3::new(dir.x, dir.y, 0.0);
    let len2 = flat.dot(flat);
    if len2 < 1e-9 {
        return f32::INFINITY;
    }
    let to = foot - origin;
    ((to.x * dir.x + to.y * dir.y) / len2).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::inches;

    fn bearer(held: bool, yaw: f32) -> Bearer {
        let def = crate::weapons::catalog().get("riotshield_mp").unwrap();
        Bearer::of(7, Vec3::new(100.0, 50.0, 0.0), yaw, def, held).unwrap()
    }

    #[test]
    fn a_held_shield_stops_rays_from_the_front_not_from_behind() {
        let b = bearer(true, std::f32::consts::FRAC_PI_2); // facing +Y
        let chest = Vec3::new(100.0, 50.0, inches(48.0));
        let front = chest + Vec3::new(0.0, 200.0, 0.0);
        let behind = chest - Vec3::new(0.0, 200.0, 0.0);
        let t = b.ray(front, (chest - front).normalized_or_zero(), 1000.0).expect("blocked from the front");
        assert!(t < 200.0);
        assert!(b.ray(behind, (chest - behind).normalized_or_zero(), 199.0).is_none(), "the back is open until the ray reaches the front box");
        // Over the top of the shield: a ray at head height plus a margin passes.
        let high = Vec3::new(100.0, 250.0, inches(90.0));
        assert!(b.ray(high, Vec3::new(0.0, -1.0, 0.0), 1000.0).is_none());
    }

    #[test]
    fn a_stowed_shield_covers_the_back() {
        let b = bearer(false, 0.0); // facing +X, stowed behind
        let chest = Vec3::new(100.0, 50.0, inches(44.0));
        let behind = chest - Vec3::new(200.0, 0.0, 0.0);
        assert!(b.ray(behind, Vec3::new(1.0, 0.0, 0.0), 1000.0).is_some());
        let front = chest + Vec3::new(200.0, 0.0, 0.0);
        assert!(b.ray(front, Vec3::new(-1.0, 0.0, 0.0), 199.0).is_none());
    }

    #[test]
    fn blast_protection_follows_the_arcs() {
        let t = Tuning::default();
        let b = bearer(true, 0.0);
        let (origin, fwd) = b.tag();
        assert_eq!(b.blast_protection(origin + fwd * 100.0, &t), t.shield_blast_protection_30);
        let side = Vec3::new(-fwd.y, fwd.x, 0.0);
        assert_eq!(b.blast_protection(origin + side * 100.0 + fwd * 1.0, &t), t.shield_blast_protection_180);
        assert_eq!(b.blast_protection(origin - fwd * 100.0, &t), 0.0);
        let at_45 = origin + (fwd + side) * 100.0;
        assert_eq!(b.blast_protection(at_45, &t), t.shield_blast_protection_120);
    }

    #[test]
    fn the_engine_mirror_round_trips_a_bearer() {
        let b = Bearer { object: 9, ..bearer(true, 1.0) };
        assert_eq!(Bearer::from_words(&b.to_words()), b);
    }

    #[test]
    fn a_zero_hour_units_fire_is_stopped_from_the_shields_side_only() {
        use crate::host::damage_kind;
        let t = Tuning::default();
        let b = bearer(true, std::f32::consts::PI); // at x 100, facing -X
        let rifleman = |x: f32| Some((Vec3::new(x, 50.0, 0.0), 12.0));
        assert_eq!(engine_scale(&[b], 7, rifleman(40.0), damage_kind::SMALL_ARMS, &t), 0.0, "in front");
        assert_eq!(engine_scale(&[b], 7, rifleman(160.0), damage_kind::SMALL_ARMS, &t), 1.0, "behind");
        assert_eq!(engine_scale(&[b], 7, rifleman(40.0), damage_kind::MELEE, &t), 0.0, "a blow in front");
        assert_eq!(engine_scale(&[b], 8, rifleman(40.0), damage_kind::SMALL_ARMS, &t), 1.0, "another General");
        assert_eq!(engine_scale(&[b], 7, None, damage_kind::SMALL_ARMS, &t), 1.0, "no source, no side");
        assert_eq!(engine_scale(&[b], 7, rifleman(40.0), -1, &t), 1.0, "fire, poison: through");
        let blast = engine_scale(&[b], 7, rifleman(40.0), damage_kind::EXPLOSION, &t);
        assert!((blast - (1.0 - t.shield_blast_protection_30)).abs() < 1e-6, "{blast}");
    }
}
