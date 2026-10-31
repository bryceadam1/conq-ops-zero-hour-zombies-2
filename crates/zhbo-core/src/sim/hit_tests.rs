/*
** Conquer Ops - Zero Hour gameplay core: fake-host tests of the soldier's hit boxes and locations.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::tests::{general, run, soldier, wait_until_ready, FakeHost};
use super::*;
use crate::hitloc::{self, Body, HitBox};
use crate::units::inches;

/// A box `half` inches each way along X/Y and `half_z` along Z, centred `z` inches up.
fn hit_box(location: u8, z: f32, half: [f32; 3]) -> HitBox {
    HitBox { location, bone: "b".into(), rotation: [0.0, 0.0, 0.0, 1.0], position: [0.0, 0.0, z], mins: half.map(|h| -h), maxs: half }
}

/// The General (object 1) fires one aimed round of a flat 40-damage gun at height `aim_z` (BO2
/// inches) on the soldier `x` units out, the soldiers traced as `boxes`. `tune` edits the gun.
/// Returns the damage log `(victim, amount)`.
fn shoot(boxes: &[HitBox], others: &[(u32, f32)], aim: (f32, f32), tune: impl Fn(&mut crate::weapons::WeaponDef)) -> Vec<(u32, f32)> {
    let host = FakeHost::default();
    host.add(general(1, 0));
    for &(id, x) in others {
        host.add(ObjInfo { health: 5000.0, max_health: 5000.0, ..soldier(id, 1, x) });
    }
    let mut sim = Sim::new();
    sim.hit_body = Some(Box::leak(Box::new(Body::new(boxes))));
    sim.take_control(&host, 0, 1);
    sim.choose_class(0, 1, 0, &[]);
    wait_until_ready(&mut sim, &host);
    let w = sim.players[0].as_mut().unwrap().arsenal.current_mut();
    let mut def = w.def.clone();
    def.damage = [40.0; 6];
    def.damage_range = [0.0; 6];
    def.pellets = 1;
    def.specialties = crate::weapons::Specialties::default();
    tune(&mut def);
    let def: &'static crate::weapons::WeaponDef = Box::leak(Box::new(def));
    (w.def, w.main) = (def, def);
    let eye = sim.players[0].as_ref().unwrap().body.eye().z;
    let (x, z) = aim;
    let pitch = -((eye - inches(z)) / x).atan();
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS, pitch, ..Default::default() }, 20);
    host.damage_log.borrow_mut().clear();
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS | buttons::FIRE, pitch, ..Default::default() }, 1);
    let log = host.damage_log.borrow().iter().map(|d| (d.1, d.2)).collect();
    log
}

/// BO2's player damage (`FUN_005f05c0`): the weapon's multiplier for the location the trace
/// picked. A head box and a leg box on one soldier: the head takes ×2, the leg ×0.5.
#[test]
fn a_head_shot_and_a_leg_shot_take_their_locations_multipliers() {
    let boxes = [hit_box(hitloc::HEAD, 62.0, [4.0, 4.0, 5.0]), hit_box(hitloc::RIGHT_LEG_UPPER, 20.0, [4.0, 4.0, 10.0])];
    let tune = |d: &mut crate::weapons::WeaponDef| {
        d.location_multipliers[hitloc::HEAD as usize] = 2.0;
        d.location_multipliers[hitloc::RIGHT_LEG_UPPER as usize] = 0.5;
    };
    let head = shoot(&boxes, &[(2, 60.0)], (60.0, 62.0), tune);
    let leg = shoot(&boxes, &[(2, 60.0)], (60.0, 20.0), tune);
    assert_eq!((head.len(), leg.len()), (1, 1), "{head:?} {leg:?}");
    assert!((head[0].1 / leg[0].1 - 4.0).abs() < 1e-3, "head {head:?}, leg {leg:?}");
    // Between the boxes there is nothing to hit (the old cylinder filled it).
    assert!(shoot(&boxes, &[(2, 60.0)], (60.0, 40.0), tune).is_empty());
}

/// Penetration through a soldier crosses his torso box's real thickness, not the old 32-inch
/// cylinder: a medium round (64 in of flesh) through a 10-inch torso keeps `1 - 10/64` of its damage
/// for the soldier behind (the slanted ray crosses `10 / cos(pitch)`).
#[test]
fn a_round_through_a_soldier_crosses_his_torso_box() {
    let boxes = [hit_box(hitloc::TORSO_MID, 40.0, [5.0, 8.0, 20.0])];
    let log = shoot(&boxes, &[(2, 40.0), (3, 60.0)], (40.0, 40.0), |d| d.penetrate = crate::penetration::Penetrate::Medium);
    let to = |id: u32| log.iter().filter(|d| d.0 == id).map(|d| d.1).sum::<f32>();
    let (front, behind) = (to(2), to(3));
    assert!(front > 0.0 && behind > 0.0, "{log:?}");
    let eye = crate::units::inches(60.0) * crate::units::GENERAL_SCALE;
    let pitch = ((eye - inches(40.0)) / 40.0).atan();
    let kept = 1.0 - 10.0 / pitch.cos() / 64.0;
    assert!((behind / front - kept).abs() < 0.01, "front {front}, behind {behind}, expected {kept}");
}
