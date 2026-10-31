/*
** Conquer Ops - Zero Hour gameplay core: BO2's plant check for planted equipment.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use crate::host::LogicHost;
use crate::hitscan::trace_terrain;
use crate::impacts::Surface;
use crate::math::{dir_from_angles, Vec3};
use crate::movement::Body;
use crate::units::inches;
use crate::weapons::WeaponDef;

/// How far BO2 traces along the throw for the plant check, in inches (`FUN_008e1780`: the end is
/// `start + dir * DAT_00f6dbf0`, 1000.0).
pub const PLANT_TRACE_INCHES: f32 = 1000.0;

/// BO2's plant check (`FUN_008e1780`) in the mod's world. It applies to a `plantable` item whose
/// throw does not go up (`iProjectileSpeedUp <= 0`), and refuses ("Cannot Plant Here") when:
/// * the player is off the ground (`groundEntityNum` none);
/// * a trace from the eye along the throw (`forward * iProjectileSpeed + up *
///   iProjectileSpeedUp`, normalised, [`PLANT_TRACE_INCHES`] long) ends in water deeper than
///   `bg_plantInWaterDepth` (`bg_disableWeaponPlantingInWater` is on by default). The mod's
///   world has no water depth; water the General cannot stand in (`is_walkable` false) is the
///   deep kind, water he can stand in is shallow enough.
///
/// Not modelled: BO2's silent refusals on a vehicle and on a trace that hits a non-world entity
/// with contents `0x200`, and the Tactical Insertion's extra test (`FUN_008e16d0`: on world
/// ground, a clear box at the feet) beyond being on the ground — the mod's ground is all world.
pub fn can_plant(def: &WeaponDef, body: &Body, yaw: f32, pitch: f32, host: &dyn LogicHost) -> bool {
    if !def.plantable || def.projectile_speed_up > 0.0 {
        return true;
    }
    if !body.on_ground {
        return false;
    }
    let throw = dir_from_angles(yaw, pitch) * def.projectile_speed + Vec3::new(0.0, 0.0, def.projectile_speed_up);
    let len = throw.length();
    if len <= 1e-6 {
        // No throw (the Tactical Insertion): the trace has no length and hits nothing.
        return true;
    }
    let dir = throw * (1.0 / len);
    let start = body.eye();
    let Some(t) = trace_terrain(start, dir, inches(PLANT_TRACE_INCHES), &|x, y| host.ground_height(x, y)) else {
        return true;
    };
    let hit = start + dir * t;
    !(host.surface_at(hit.x, hit.y) == Surface::Water && !host.is_walkable(hit.x, hit.y))
}
