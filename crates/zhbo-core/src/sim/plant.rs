/*
** Conquer Ops - Zero Hour gameplay core: BO2's plant check for planted equipment.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use crate::host::{objflags, LogicHost};
use crate::hitscan::{ray_vs_target, trace, trace_terrain, HitResult, Shape, Target};
use crate::sim::fire::target_from;
use crate::impacts::Surface;
use crate::math::{dir_from_angles, Vec3};
use crate::movement::Body;
use crate::units::inches;
use crate::weapons::WeaponDef;

/// How far BO2 traces along the throw for the plant check, in inches (`FUN_008e1780`: the end is
/// `start + dir * DAT_00f6dbf0`, 1000.0).
pub const PLANT_TRACE_INCHES: f32 = 1000.0;

/// The Tactical Insertion's clearance box (`FUN_008e16d0`: mins (-15, -15, 0), maxs (15, 15, 70)
/// at the player's origin, `DAT_00bd2434`/`DAT_00bd2440`), in inches.
const TI_BOX_HALF_INCHES: f32 = 15.0;
const TI_BOX_HEIGHT_INCHES: f32 = 70.0;

/// How far below the feet a vehicle's roof still counts as what the General stands on.
const ON_VEHICLE_INCHES: f32 = 4.0;

/// BO2's plant check (`FUN_008e1780`) in the mod's world. It applies to a `plantable` item whose
/// throw does not go up (`iProjectileSpeedUp <= 0`), and refuses — every caller then fires event
/// 0xd5, the "Cannot Plant Here" hint — when:
/// * the player is off the ground (`groundEntityNum` none);
/// * the player is on a vehicle (`ps+0xfc & 0x4000`; the flag `FUN_008deb50` reads with
///   `player_disableWeaponsOnVehicle`): in the mod, a Zero Hour vehicle's roof just under the feet;
/// * the item is the Tactical Insertion and `FUN_008e16d0` fails: the player must stand on the world
///   (`groundEntityNum` 0x3fe, not on an entity) with the 30 x 30 x 70 box at his origin clear — in
///   the mod, no colliding object (other than himself) overlapping that box;
/// * a trace from the eye along the throw (`forward * iProjectileSpeed + up *
///   iProjectileSpeedUp`, normalised, [`PLANT_TRACE_INCHES`] long, mask `0x800a31`) hits a
///   non-world entity whose contents have `0x200` — the IW-family `CONTENTS_VEHICLECLIP`; in the
///   mod, a Zero Hour vehicle, friend or foe (a structure stops the trace without refusing) — or ends in
///   water deeper than `bg_plantInWaterDepth` (`bg_disableWeaponPlantingInWater` is on by default).
///   The mod's world has no water depth; water the General cannot stand in (`is_walkable` false)
///   is the deep kind, water he can stand in is shallow enough.
pub fn can_plant(def: &WeaponDef, body: &Body, yaw: f32, pitch: f32, general: u32, host: &dyn LogicHost) -> bool {
    if !def.plantable || def.projectile_speed_up > 0.0 {
        return true;
    }
    if !body.on_ground {
        return false;
    }
    let ground = |x: f32, y: f32| host.ground_height(x, y);
    let start = body.eye();
    let reach = inches(PLANT_TRACE_INCHES);
    // Everything the traces could meet (dead and non-colliding objects never block, `NO_COLLIDE`).
    let mut objs = Vec::new();
    host.query(body.pos, reach + body.eye_height, general, &mut objs);
    let mut solids: Vec<Target> = objs.iter().filter(|o| o.id != general && !o.has(objflags::DEAD) && !o.has(objflags::NO_COLLIDE)).map(target_from).collect();
    solids.sort_by_key(|t| t.id);
    let vehicles: Vec<Target> = objs
        .iter()
        .filter(|o| o.id != general && o.has(objflags::VEHICLE) && !o.has(objflags::DEAD) && !o.has(objflags::AIRBORNE))
        .map(target_from)
        .collect();
    // On a vehicle: a roof right under the feet (a short ray down from just above them).
    let down = Vec3::new(0.0, 0.0, -1.0);
    let above_feet = body.pos + Vec3::new(0.0, 0.0, inches(ON_VEHICLE_INCHES));
    if vehicles.iter().any(|v| ray_vs_target(above_feet, down, inches(2.0 * ON_VEHICLE_INCHES), v).is_some()) {
        return false;
    }
    if def.equipment_kind == crate::equipment::Kind::Insertion && !ti_box_clear(body, &solids) {
        return false;
    }
    let throw = dir_from_angles(yaw, pitch) * def.projectile_speed + Vec3::new(0.0, 0.0, def.projectile_speed_up);
    let len = throw.length();
    if len <= 1e-6 {
        // No throw (the Tactical Insertion): the trace has no length and hits nothing.
        return true;
    }
    let dir = throw * (1.0 / len);
    match trace(start, dir, reach, &solids, general, &ground) {
        // Only an entity with vehicle clip (`0x200`) refuses; a structure stops the trace and is
        // simply not water.
        HitResult::Object { id, .. } => !objs.iter().any(|o| o.id == id && o.has(objflags::VEHICLE)),
        HitResult::Terrain { point } => !(host.surface_at(point.x, point.y) == Surface::Water && !host.is_walkable(point.x, point.y)),
        HitResult::Nothing { .. } => true,
    }
}

/// `FUN_008e16d0`'s clearance: no solid overlaps the 30 x 30 x 70 inch box standing at the feet.
fn ti_box_clear(body: &Body, solids: &[Target]) -> bool {
    let (half, height) = (inches(TI_BOX_HALF_INCHES), inches(TI_BOX_HEIGHT_INCHES));
    let (lo, hi) = (body.pos.z, body.pos.z + height);
    !solids.iter().any(|t| {
        let (reach, top) = match t.shape {
            Shape::Cylinder { radius, height } => (radius, height),
            Shape::Box { half_x, half_y, height, .. } => (half_x.max(half_y), height),
            Shape::Sphere { radius } => (radius, radius * 2.0),
        };
        let d = t.pos - body.pos;
        d.x.abs() < half + reach && d.y.abs() < half + reach && t.pos.z < hi && t.pos.z + top > lo
    })
}

/// Longest straight flight [`plant_point`] follows before setting the item at the feet.
const MAX_FLIGHT: f32 = 10.0;

/// Where BO2 puts a planted item down, if it goes straight there. Its offhand fire
/// (`FUN_008e2ac0`, event 0x30) launches the item from the eye with `forward * iProjectileSpeed +
/// up * iProjectileSpeedUp` and the item settles where it lands: an item with no launch speed (the
/// Tactical Insertion) at the feet; one without gravity, or thrown down (the claymore's 100/-10000,
/// straight down a hair in front of the feet), where that line meets the ground. `None` for a lobbed
/// one (gravity and an upward-or-level launch: the Bouncing Betty's 650, the Trophy System's 575):
/// it flies as a grenade, bouncing and settling with the projectile physics, and is placed where it
/// comes to rest ([`plant_launch`]).
pub fn plant_point(def: &WeaponDef, body: &Body, yaw: f32, pitch: f32, host: &dyn LogicHost) -> Option<Vec3> {
    let ground = |x: f32, y: f32| host.ground_height(x, y);
    let feet = Vec3::new(body.pos.x, body.pos.y, ground(body.pos.x, body.pos.y));
    let vel = plant_velocity(def, yaw, pitch);
    let len = vel.length();
    if len <= 1e-6 {
        return Some(feet);
    }
    if def.projectile_gravity && vel.z > -len * 0.5 {
        return None;
    }
    let start = body.eye();
    let dir = vel * (1.0 / len);
    Some(match trace_terrain(start, dir, len * MAX_FLIGHT, &ground) {
        Some(t) => {
            let p = start + dir * t;
            Vec3::new(p.x, p.y, ground(p.x, p.y))
        }
        None => feet,
    })
}

/// BO2's launch velocity for a planted item (`forward * iProjectileSpeed + up * iProjectileSpeedUp`).
pub fn plant_velocity(def: &WeaponDef, yaw: f32, pitch: f32) -> Vec3 {
    dir_from_angles(yaw, pitch) * def.projectile_speed + Vec3::new(0.0, 0.0, def.projectile_speed_up)
}

/// A lobbed planted item as a grenade from the eye with BO2's launch velocity; the sim's
/// projectile step bounces it and turns it into placed equipment where it rests.
pub fn plant_launch(def: &'static WeaponDef, body: &Body, yaw: f32, pitch: f32, owner: u32, host: &dyn LogicHost) -> crate::projectiles::Projectile {
    let mut proj = crate::projectiles::Projectile::launch(def, owner, body.eye(), Vec3::ZERO, def.fuse_time, host, yaw);
    proj.vel = plant_velocity(def, yaw, pitch);
    proj
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::tests::FakeHost;

    fn body() -> Body {
        Body::new(Vec3::ZERO, &crate::movement::MoveTuning::default())
    }

    fn object(id: u32, flags: u32, x: f32) -> crate::host::ObjInfo {
        crate::host::ObjInfo {
            id,
            flags,
            pos: Vec3::new(x, 0.0, 0.0),
            geom_type: crate::host::geom::BOX,
            major_radius: 20.0,
            minor_radius: 20.0,
            height: 15.0,
            health: 100.0,
            max_health: 100.0,
            ..Default::default()
        }
    }

    #[test]
    fn the_plant_check_refuses_on_and_into_vehicles_and_a_crowded_insertion() {
        let b = body();
        // A forward plant (no upward throw): the trace runs along the view.
        let ahead = WeaponDef { plantable: true, projectile_speed: inches(100.0), ..Default::default() };
        let host = FakeHost::default();
        assert!(can_plant(&ahead, &b, 0.0, 0.0, 1, &host), "nothing in the way");
        host.add(object(7, objflags::STRUCTURE, 60.0));
        assert!(can_plant(&ahead, &b, 0.0, 0.0, 1, &host), "a structure stops the trace, no refusal");
        let host = FakeHost::default();
        host.add(object(7, objflags::VEHICLE, 60.0));
        assert!(!can_plant(&ahead, &b, 0.0, 0.0, 1, &host), "the trace meets a vehicle");
        // Standing on a vehicle's roof.
        let host = FakeHost::default();
        host.add(object(7, objflags::VEHICLE, 0.0));
        let mut on_roof = body();
        on_roof.pos.z = 15.0;
        let down = WeaponDef { plantable: true, projectile_speed: inches(100.0), projectile_speed_up: inches(-10000.0), ..Default::default() };
        assert!(!can_plant(&down, &on_roof, 0.0, 0.0, 1, &host), "on a vehicle");
        // The Tactical Insertion: a soldier inside the 30 x 30 x 70 box at the feet.
        let ti = WeaponDef { plantable: true, equipment_kind: crate::equipment::Kind::Insertion, ..Default::default() };
        let host = FakeHost::default();
        assert!(can_plant(&ti, &b, 0.0, 0.0, 1, &host));
        host.add(crate::host::ObjInfo { id: 9, flags: objflags::INFANTRY, pos: Vec3::new(inches(20.0), 0.0, 0.0), geom_type: crate::host::geom::CYLINDER, major_radius: 5.0, height: 12.0, ..Default::default() });
        assert!(!can_plant(&ti, &b, 0.0, 0.0, 1, &host), "the box is not clear");
    }

    #[test]
    fn the_plant_point_follows_bo2s_launch() {
        let host = FakeHost::default();
        let b = body();
        // No launch speed (the Tactical Insertion): at the feet.
        let ti = WeaponDef::default();
        assert_eq!(plant_point(&ti, &b, 0.0, 0.0, &host), Some(Vec3::ZERO));
        // The claymore's 100 forward, -10000 up: straight down, a hair in front.
        let claymore = WeaponDef { projectile_speed: inches(100.0), projectile_speed_up: inches(-10000.0), projectile_gravity: true, ..Default::default() };
        let at = plant_point(&claymore, &b, 0.0, 0.0, &host).expect("straight down");
        assert!((at.x - b.eye_height * 0.01).abs() < 1e-3 && at.y.abs() < 1e-4 && at.z == 0.0, "{at:?}");
        // The Betty's 650 under gravity: lobbed, so it flies as a grenade.
        let betty = WeaponDef { projectile_speed: inches(650.0), projectile_gravity: true, ..Default::default() };
        assert_eq!(plant_point(&betty, &b, 0.0, 0.0, &host), None);
    }

    /// A lobbed Betty bounces with the grenade physics, comes to rest and is placed there.
    #[test]
    fn a_lobbed_betty_bounces_and_settles_as_equipment() {
        let host = FakeHost::default();
        let b = body();
        let betty: &'static WeaponDef = Box::leak(Box::new(WeaponDef {
            id: "bouncingbetty_mp".into(),
            projectile_speed: inches(650.0),
            projectile_gravity: true,
            equipment_kind: crate::equipment::Kind::Betty,
            ..Default::default()
        }));
        let mut proj = plant_launch(betty, &b, 0.0, 0.0, 1, &host);
        let mut bounced = false;
        let mut prev_vz = proj.vel.z;
        let landed = (0..2000).find_map(|_| {
            let out = proj.step(&host);
            bounced |= prev_vz < 0.0 && proj.vel.z > 0.0;
            prev_vz = proj.vel.z;
            out
        });
        match landed {
            Some(crate::projectiles::Outcome::Landed { at, .. }) => {
                let fall = (2.0 * b.eye_height / crate::projectiles::GRAVITY).sqrt();
                assert!(at.x > inches(650.0) * fall, "rolled on past the first contact: {at:?}");
                assert_eq!(at.z, 0.0);
            }
            other => panic!("{other:?}"),
        }
        assert!(bounced, "it bounced");
    }
}
