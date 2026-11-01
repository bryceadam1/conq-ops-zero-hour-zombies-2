/*
** Conquer Ops - Zero Hour gameplay core: bullet penetration.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! BO2's bullet penetration (t6mp `FUN_005d0350`, the server's bullet loop; the client's
//! `FUN_00538e40` plays the same walk's effects).
//!
//! A round that hit something keeps going while its damage multiplier (1 at the muzzle) stays
//! positive, up to `penetrationCount` surfaces. At each solid it read the depth of its
//! `penetrateType` for the entry surface (`FUN_008da6d0`: the rawfile `info/bullet_penetration_mp`,
//! keys `<type>_<surface>`; surface 0 is always 0), doubled by `perk_bulletPenetrationMultiplier`
//! with `bulletpenetration` (FMJ). It steps 0.135 in past the entry, traces on to the next
//! surface, then traces back from 0.01 in short of it to find where it left the solid. The
//! thickness (at least 1 in) over the smaller of the entry and exit depths comes off the multiplier.
//! The exit and the next entry play their impacts only when the gap between them is over
//! `bullet_penetrationMinFxDist`, and the client plays at most two such effects per round.
//!
//! The mod's solids are the targets' convex shapes ([`crate::hitscan::Shape`]), so the thickness is
//! the exact chord. The ground is a heightfield with no underside, so a round that strikes it stops.
//! The surfaces come from the objects' kind flags ([`Surface::of_object`]), which are sim state,
//! so the walk is deterministic.

use crate::hitscan::{self, HitResult, Target};
use crate::impacts::Surface;
use crate::math::Vec3;
use crate::units::inches;

/// BO2's `PenetrateType` (one type, the weapon table's `Traits`/attachment field).
pub use crate::weapons::Penetrate;

/// How far `FUN_008da5c0` steps a round past the surface it entered before tracing on (`0x00b27e50`).
pub const ADVANCE: f32 = inches(0.135);
/// How far short of the next surface the reverse trace starts, and past the entry it ends
/// (`0x00c73410`).
pub const BACKOFF: f32 = inches(0.01);
/// The least thickness a solid takes off the multiplier (`0x00b0c268`).
pub const MIN_THICKNESS: f32 = inches(1.0);
/// The client plays at most this many exit and onward-entry effects per round (`FUN_00538e40`'s
/// `local_ec < 2`).
pub const FX_BUDGET: u32 = 2;

/// The depth table and the loop's dvars, from the tuning pack (`tuning.rs` cites each).
#[derive(Clone, Debug, PartialEq)]
pub struct Penetration {
    /// `penetrationCount`: the most solids one round passes.
    pub count: u32,
    /// `perk_bulletPenetrationMultiplier`.
    pub perk_multiplier: f32,
    /// `bullet_penetrationMinFxDist`, Zero Hour units.
    pub min_fx_dist: f32,
    /// Depth in Zero Hour units by `Penetrate` and [`Surface`] slot (row 0 and `Default` stay 0).
    depth: [[f32; Surface::ALL.len()]; 4],
}

impl Default for Penetration {
    fn default() -> Self {
        Self::from_tuning(&crate::tuning::Tuning::default())
    }
}

impl Penetration {
    pub fn from_tuning(t: &crate::tuning::Tuning) -> Self {
        let mut depth = [[0.0; Surface::ALL.len()]; 4];
        let rows = [
            (Penetrate::Small, [t.pen_small_flesh, t.pen_small_metal, t.pen_small_concrete, t.pen_small_dirt, t.pen_small_grass, t.pen_small_asphalt, t.pen_small_sand, t.pen_small_snow, t.pen_small_rock, t.pen_small_wood, t.pen_small_water]),
            (Penetrate::Medium, [t.pen_medium_flesh, t.pen_medium_metal, t.pen_medium_concrete, t.pen_medium_dirt, t.pen_medium_grass, t.pen_medium_asphalt, t.pen_medium_sand, t.pen_medium_snow, t.pen_medium_rock, t.pen_medium_wood, t.pen_medium_water]),
            (Penetrate::Large, [t.pen_large_flesh, t.pen_large_metal, t.pen_large_concrete, t.pen_large_dirt, t.pen_large_grass, t.pen_large_asphalt, t.pen_large_sand, t.pen_large_snow, t.pen_large_rock, t.pen_large_wood, t.pen_large_water]),
        ];
        for (p, row) in rows {
            for (s, v) in TABLE_SURFACES.iter().zip(row) {
                depth[p as usize][*s as usize] = inches(v);
            }
        }
        Self {
            count: t.penetration_count.max(0.0) as u32,
            perk_multiplier: t.perk_bullet_penetration_multiplier,
            min_fx_dist: inches(t.bullet_penetration_min_fx_dist),
            depth,
        }
    }

    /// How deep a round of `pen` goes through `surface` (Zero Hour units; 0: it stops), with the
    /// perk's multiplier when the shooter has `bulletpenetration` and the multiplier is positive.
    pub fn depth(&self, pen: Penetrate, surface: Surface, perk: bool) -> f32 {
        let d = self.depth[pen as usize][surface as usize];
        if perk && self.perk_multiplier > 0.0 {
            d * self.perk_multiplier
        } else {
            d
        }
    }
}

/// The mod surfaces the depth table has keys for, in the tuning rows' order (`Default` has none:
/// BO2's surface 0 reads 0).
pub const TABLE_SURFACES: [Surface; 11] = [
    Surface::Flesh,
    Surface::Metal,
    Surface::Concrete,
    Surface::Dirt,
    Surface::Grass,
    Surface::Asphalt,
    Surface::Sand,
    Surface::Snow,
    Surface::Rock,
    Surface::Wood,
    Surface::Water,
];

/// One step of a penetrating round after its first hit, in order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// The round left target `id` at `at` through `surface`; `fx`: it plays the exit impact.
    Exit { id: u32, at: Vec3, surface: Surface, fx: bool },
    /// The round struck `hit` (an object or the ground) with damage multiplier `mult`; `fx`: it
    /// plays the impact.
    Enter { hit: HitResult, mult: f32, fx: bool },
}

/// What the walk needs of the shot.
pub struct Shot<'a> {
    pub pen: Penetrate,
    /// The shooter has `bulletpenetration`.
    pub perk: bool,
    pub origin: Vec3,
    pub dir: Vec3,
    /// The weapon's range from `origin`.
    pub range: f32,
    pub targets: &'a [Target],
    /// The shooter (never a target).
    pub ignore: u32,
    pub ground: &'a dyn Fn(f32, f32) -> f32,
    /// The surface of a target by id (its kind flags).
    pub surface_of: &'a dyn Fn(u32) -> Surface,
}

/// The ray distance of a hit from `origin`.
fn along(origin: Vec3, dir: Vec3, p: Vec3) -> f32 {
    (p - origin).dot(dir)
}

/// BO2's loop after the first hit (`FUN_005d0350` from its `penetrationCount` loop): `first` is the
/// target the round struck at distance `t` with `surface`. Writes the steps to `out` (cleared).
pub fn walk(table: &Penetration, shot: &Shot, first: (u32, f32, Surface), out: &mut Vec<Step>) {
    out.clear();
    if shot.pen == Penetrate::None {
        return;
    }
    let (o, dir) = (shot.origin, shot.dir);
    let (mut entry_t, mut entry_surface) = (first.1, first.2);
    let mut mult = 1.0f32;
    let mut fx_left = FX_BUDGET;
    for _ in 0..table.count {
        let depth = table.depth(shot.pen, entry_surface, shot.perk);
        if depth <= 0.0 {
            return;
        }
        // On from just past the entry, through the solids the round is not inside.
        let start = entry_t + ADVANCE;
        if start >= shot.range {
            return;
        }
        let from = o + dir * start;
        let next = hitscan::trace_where(from, dir, shot.range - start, shot.targets, shot.ignore, shot.ground, &|t| !hitscan::contains(t, from));
        let hit_on = !matches!(next, HitResult::Nothing { .. });
        let next_t = along(o, dir, next.point());
        // Back from just short of the next surface (or from the range's end) to the entry.
        let back_from_t = if hit_on { next_t - BACKOFF } else { next_t };
        let back_from = o + dir * back_from_t;
        let back_len = back_from_t - (entry_t - BACKOFF);
        let no_ground = |_: f32, _: f32| f32::NEG_INFINITY;
        let solid = shot.targets.iter().any(|t| t.id != shot.ignore && hitscan::contains(t, back_from));
        let back = if solid { HitResult::Nothing { end: back_from } } else { hitscan::trace(back_from, -dir, back_len.max(0.0), shot.targets, shot.ignore, &no_ground) };
        let mut fx = false;
        match back {
            // The reverse trace starts inside a solid: the whole segment counts, and nothing plays.
            _ if solid => {
                mult -= back_len.max(MIN_THICKNESS) / depth;
                if mult <= 0.0 {
                    return;
                }
            }
            HitResult::Object { id, point, .. } => {
                let exit_t = along(o, dir, point);
                let exit_surface = (shot.surface_of)(id);
                let used = depth.min(table.depth(shot.pen, exit_surface, shot.perk));
                if used <= 0.0 {
                    return;
                }
                mult -= (exit_t - entry_t).max(MIN_THICKNESS) / used;
                if mult <= 0.0 {
                    return;
                }
                fx = next_t - exit_t > table.min_fx_dist;
                let play = fx && fx_left > 0;
                fx_left -= play as u32;
                out.push(Step::Exit { id, at: point, surface: exit_surface, fx: play });
            }
            // Nothing behind the entry (a surface with no back): the round goes on unscaled, and
            // the next entry plays when it is far enough from the last one.
            _ => fx = next_t - entry_t > table.min_fx_dist,
        }
        if !hit_on {
            return;
        }
        let play = fx && fx_left > 0;
        fx_left -= play as u32;
        out.push(Step::Enter { hit: next, mult, fx: play });
        match next {
            HitResult::Object { id, distance, .. } => {
                entry_t = start + distance;
                entry_surface = (shot.surface_of)(id);
            }
            // The ground has no underside.
            _ => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hitscan::{Shape, INFANTRY_HEAD_FRACTION, INFANTRY_HIT_RADIUS};

    fn wall(id: u32, x: f32, thick: f32) -> Target {
        Target { id, pos: Vec3::new(x + thick * 0.5, 0.0, 0.0), shape: Shape::Box { half_x: thick * 0.5, half_y: 40.0, height: 40.0, yaw: 0.0 }, head_fraction: 0.0 }
    }
    fn soldier(id: u32, x: f32) -> Target {
        Target { id, pos: Vec3::new(x, 0.0, 0.0), shape: Shape::Cylinder { radius: INFANTRY_HIT_RADIUS, height: 12.0 }, head_fraction: INFANTRY_HEAD_FRACTION }
    }
    fn run(pen: Penetrate, perk: bool, targets: &[Target], surface: &dyn Fn(u32) -> Surface) -> Vec<Step> {
        let ground = |_: f32, _: f32| -100.0;
        let o = Vec3::new(0.0, 0.0, 6.0);
        let dir = Vec3::new(1.0, 0.0, 0.0);
        let shot = Shot { pen, perk, origin: o, dir, range: 2000.0, targets, ignore: 0, ground: &ground, surface_of: surface };
        let HitResult::Object { id, distance, .. } = hitscan::trace(o, dir, 2000.0, targets, 0, &ground) else { panic!("no first hit") };
        let mut out = Vec::new();
        walk(&Penetration::default(), &shot, (id, distance, surface(id)), &mut out);
        out
    }
    fn concrete_then_flesh(id: u32) -> Surface {
        if id == 1 { Surface::Concrete } else { Surface::Flesh }
    }

    #[test]
    fn the_table_is_bo2s_mp_rawfile() {
        let t = Penetration::default();
        assert_eq!(t.depth(Penetrate::Small, Surface::Flesh, false), inches(32.0));
        assert_eq!(t.depth(Penetrate::Medium, Surface::Concrete, false), inches(9.0));
        assert_eq!(t.depth(Penetrate::Large, Surface::Metal, true), inches(40.0));
        assert_eq!(t.depth(Penetrate::Large, Surface::Default, true), 0.0);
        assert_eq!(t.depth(Penetrate::None, Surface::Flesh, true), 0.0);
        assert_eq!(t.depth(Penetrate::Large, Surface::Dirt, false), 0.0);
        assert_eq!((t.count, t.perk_multiplier, t.min_fx_dist), (5, 2.0, inches(30.0)));
    }

    #[test]
    fn a_thin_wall_costs_its_thickness_over_the_depth() {
        // 4.5 in of concrete: medium (9 in) keeps half, and the soldier 10 ft behind takes it.
        let targets = [wall(1, 50.0, inches(4.5)), soldier(2, 50.0 + inches(120.0))];
        let steps = run(Penetrate::Medium, false, &targets, &concrete_then_flesh);
        let Step::Exit { id: 1, surface: Surface::Concrete, fx: true, at } = steps[0] else { panic!("{steps:?}") };
        assert!((at.x - (50.0 + inches(4.5))).abs() < 1e-3);
        let Step::Enter { hit: HitResult::Object { id: 2, .. }, mult, fx: true } = steps[1] else { panic!("{steps:?}") };
        assert!((mult - 0.5).abs() < 1e-3, "{mult}");
    }

    #[test]
    fn a_wall_thicker_than_the_depth_stops_the_round_and_fmj_doubles_it() {
        // 7 in of concrete is past small's 6 in, inside FMJ's 12 in.
        let targets = [wall(1, 50.0, inches(7.0)), soldier(2, 50.0 + inches(120.0))];
        assert!(run(Penetrate::Small, false, &targets, &concrete_then_flesh).is_empty());
        let fmj = run(Penetrate::Small, true, &targets, &concrete_then_flesh);
        let Step::Enter { hit: HitResult::Object { id: 2, .. }, mult, .. } = fmj[1] else { panic!("{fmj:?}") };
        assert!((mult - (1.0 - 7.0 / 12.0)).abs() < 1e-3, "{mult}");
    }

    #[test]
    fn no_penetration_type_goes_nowhere() {
        let targets = [wall(1, 50.0, inches(1.0)), soldier(2, 80.0)];
        assert!(run(Penetrate::None, true, &targets, &concrete_then_flesh).is_empty());
    }

    #[test]
    fn close_surfaces_play_no_effects_and_the_client_plays_two() {
        // Three 1-in boards 10 in apart (under the 30 in), then a soldier far behind.
        let wood = |id: u32| if id < 4 { Surface::Wood } else { Surface::Flesh };
        let targets = [wall(1, 50.0, inches(1.0)), wall(2, 50.0 + inches(11.0), inches(1.0)), wall(3, 50.0 + inches(22.0), inches(1.0)), soldier(4, 50.0 + inches(200.0))];
        let steps = run(Penetrate::Large, false, &targets, &wood);
        let fx: Vec<bool> = steps.iter().map(|s| match *s { Step::Exit { fx, .. } | Step::Enter { fx, .. } => fx }).collect();
        // exit 1, enter 2, exit 2, enter 3 are close; exit 3 and the soldier are far, and they
        // spend the budget: the round leaves the soldier with nothing left to play.
        assert_eq!(fx, [false, false, false, false, true, true, false], "{steps:?}");
        let Step::Enter { mult, .. } = steps[5] else { panic!() };
        assert!((mult - (1.0 - 3.0 / 16.0)).abs() < 1e-3, "{mult}");
    }

    #[test]
    fn the_ground_stops_a_round() {
        let ground = |x: f32, _: f32| if x > 60.0 { 10.0 } else { -100.0 };
        let targets = [wall(1, 50.0, inches(1.0))];
        let o = Vec3::new(0.0, 0.0, 6.0);
        let dir = Vec3::new(1.0, 0.0, 0.0);
        let surface = |_: u32| Surface::Wood;
        let shot = Shot { pen: Penetrate::Large, perk: false, origin: o, dir, range: 2000.0, targets: &targets, ignore: 0, ground: &ground, surface_of: &surface };
        let mut out = Vec::new();
        walk(&Penetration::default(), &shot, (1, 50.0, Surface::Wood), &mut out);
        assert!(matches!(out.as_slice(), [Step::Exit { .. }, Step::Enter { hit: HitResult::Terrain { .. }, .. }]), "{out:?}");
    }
}
