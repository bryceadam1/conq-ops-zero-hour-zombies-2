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

/// Longest flight [`plant_point`] follows before giving up and setting the item at the feet.
const MAX_FLIGHT: f32 = 10.0;

/// Where BO2 puts a planted item down. Its offhand fire (`FUN_008e2ac0`, event 0x30) launches the
/// item from the eye with `forward * iProjectileSpeed + up * iProjectileSpeedUp` — the claymore's
/// 100/-10000 drops it straight down in front of the feet, the Bouncing Betty (650) and the Trophy
/// System (575) are lobbed under gravity when the def has it — and the item settles where it first
/// meets the ground (the mod does not bounce it). An item with no launch speed (the Tactical
/// Insertion) goes down at the feet.
pub fn plant_point(def: &WeaponDef, body: &Body, yaw: f32, pitch: f32, host: &dyn LogicHost) -> Vec3 {
    let ground = |x: f32, y: f32| host.ground_height(x, y);
    let feet = Vec3::new(body.pos.x, body.pos.y, ground(body.pos.x, body.pos.y));
    let vel = dir_from_angles(yaw, pitch) * def.projectile_speed + Vec3::new(0.0, 0.0, def.projectile_speed_up);
    if vel.length() <= 1e-6 {
        return feet;
    }
    let start = body.eye();
    let landed = |p: Vec3| Vec3::new(p.x, p.y, ground(p.x, p.y));
    if !def.projectile_gravity {
        let len = vel.length();
        let dir = vel * (1.0 / len);
        return match trace_terrain(start, dir, len * MAX_FLIGHT, &ground) {
            Some(t) => landed(start + dir * t),
            None => feet,
        };
    }
    // Under gravity: step the arc a logic frame at a time, settling at the first step below
    // the ground (that step's ray found by the terrain trace).
    let (mut p, mut v) = (start, vel);
    let mut flown = 0.0;
    while flown < MAX_FLIGHT {
        v.z -= crate::projectiles::GRAVITY * crate::units::LOGIC_DT;
        let step = v * crate::units::LOGIC_DT;
        let len = step.length();
        if len > 1e-6 {
            if let Some(t) = trace_terrain(p, step * (1.0 / len), len, &ground) {
                return landed(p + step * (t / len));
            }
        }
        p = p + step;
        flown += crate::units::LOGIC_DT;
    }
    feet
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::tests::FakeHost;
    use crate::units::LOGIC_DT;

    fn body() -> Body {
        Body::new(Vec3::ZERO, &crate::movement::MoveTuning::default())
    }

    #[test]
    fn the_plant_point_follows_bo2s_launch() {
        let host = FakeHost::default();
        let b = body();
        // No launch speed (the Tactical Insertion): at the feet.
        let ti = WeaponDef::default();
        assert_eq!(plant_point(&ti, &b, 0.0, 0.0, &host), Vec3::ZERO);
        // The claymore's 100 forward, -10000 up: straight down, a hair in front.
        let claymore = WeaponDef { projectile_speed: inches(100.0), projectile_speed_up: inches(-10000.0), ..Default::default() };
        let at = plant_point(&claymore, &b, 0.0, 0.0, &host);
        assert!((at.x - b.eye_height * 0.01).abs() < 1e-3 && at.y.abs() < 1e-4 && at.z == 0.0, "{at:?}");
        // The Betty's 650 under gravity: lobbed out, landing where the arc from the eye meets the
        // ground (within one logic step).
        let betty = WeaponDef { projectile_speed: inches(650.0), projectile_gravity: true, ..Default::default() };
        let at = plant_point(&betty, &b, 0.0, 0.0, &host);
        let fall = (2.0 * b.eye_height / crate::projectiles::GRAVITY).sqrt();
        let want = inches(650.0) * fall;
        assert!((at.x - want).abs() <= inches(650.0) * LOGIC_DT * 1.5, "{} vs {want}", at.x);
        assert_eq!(plant_point(&betty, &b, 0.0, 0.0, &host), at, "deterministic");
    }
}
