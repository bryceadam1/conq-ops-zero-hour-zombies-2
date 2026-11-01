/*
** Conquer Ops - Zero Hour gameplay core: fake-host tests of the soldier's hit boxes and locations.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::tests::{general, run, soldier, wait_until_ready, FakeHost};
use super::*;
use crate::hitloc::{self, Body, HitBox, Poses};
use crate::units::{inches, GENERAL_SCALE};
use zhbo_native::thirdperson::state;

/// A box `half` inches each way along X/Y and `half_z` along Z, centred `z` inches up.
fn hit_box(location: u8, z: f32, half: [f32; 3]) -> HitBox {
    HitBox { state: state::STAND_IDLE, location, bone: "b".into(), rotation: [0.0, 0.0, 0.0, 1.0], position: [0.0, 0.0, z], mins: half.map(|h| -h), maxs: half }
}

/// A box posed for body state `s`, centred at (`x`, 0, `z`) inches.
fn posed(s: u16, location: u8, x: f32, z: f32, half: [f32; 3]) -> HitBox {
    HitBox { state: s, position: [x, 0.0, z], ..hit_box(location, z, half) }
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
    sim.hit_body = Some(Box::leak(Box::new(Poses::single(Body::new(boxes)))));
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
    let log = shoot(&boxes, &[(2, 40.0), (3, 60.0)], (40.0, 40.0), |d| d.traits.penetrate = crate::penetration::Penetrate::Medium);
    let to = |id: u32| log.iter().filter(|d| d.0 == id).map(|d| d.1).sum::<f32>();
    let (front, behind) = (to(2), to(3));
    assert!(front > 0.0 && behind > 0.0, "{log:?}");
    let eye = crate::units::inches(60.0) * crate::units::GENERAL_SCALE;
    let pitch = ((eye - inches(40.0)) / 40.0).atan();
    let kept = 1.0 - 10.0 / pitch.cos() / 64.0;
    assert!((behind / front - kept).abs() < 0.01, "front {front}, behind {behind}, expected {kept}");
}

/// The poses of the tests below: the head 62 in up standing, 40 crouched, and prone a head 8 in
/// off the ground with the body lying back along -X — each state's own boxes, as the converter
/// writes them from the first frame of its clip.
fn stance_poses() -> Vec<HitBox> {
    vec![
        posed(state::STAND_IDLE, hitloc::HEAD, 0.0, 62.0, [4.0, 4.0, 5.0]),
        posed(state::STAND_IDLE, hitloc::TORSO_MID, 0.0, 40.0, [5.0, 8.0, 12.0]),
        posed(state::CROUCH_IDLE, hitloc::HEAD, 0.0, 40.0, [4.0, 4.0, 5.0]),
        posed(state::CROUCH_IDLE, hitloc::TORSO_MID, 0.0, 22.0, [5.0, 8.0, 10.0]),
        posed(state::PRONE_IDLE, hitloc::HEAD, 10.0, 8.0, [4.0, 4.0, 4.0]),
        posed(state::PRONE_IDLE, hitloc::TORSO_MID, -12.0, 6.0, [14.0, 8.0, 5.0]),
    ]
}

/// The General (player 0, object 1) fires one aimed flat-40 round at height `z` (BO2 inches) on
/// an enemy General (player 1, object 2) `x` units out in `stance`. Returns the damage log.
fn duel(stance: crate::movement::Stance, aim: (f32, f32), tune: impl Fn(&mut crate::weapons::WeaponDef)) -> Vec<(u32, f32)> {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let (x, z) = aim;
    host.add(ObjInfo { pos: Vec3::new(x, 0.0, 0.0), yaw: std::f32::consts::PI, health: 5000.0, max_health: 5000.0, ..general(2, 1) });
    let mut sim = Sim::new();
    sim.hit_body = Some(Box::leak(Box::new(Poses::new(&stance_poses()))));
    sim.take_control(&host, 0, 1);
    sim.choose_class(0, 1, 0, &[]);
    sim.take_control(&host, 1, 2);
    sim.choose_class(1, 2, 0, &[]);
    wait_until_ready(&mut sim, &host);
    sim.players[1].as_mut().unwrap().body.stance = stance;
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
    let pitch = -((eye - inches(z)) / x).atan();
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS, pitch, ..Default::default() }, 20);
    host.damage_log.borrow_mut().clear();
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS | buttons::FIRE, pitch, ..Default::default() }, 1);
    let log = host.damage_log.borrow().iter().filter(|d| d.1 == 2).map(|d| (d.1, d.2)).collect();
    log
}

fn head_x2(d: &mut crate::weapons::WeaponDef) {
    d.location_multipliers[hitloc::HEAD as usize] = 2.0;
}

/// BO2 traces a player as the server posed him; the mod poses an enemy General's boxes from the
/// sim's body state, at his drawn size. Standing, the head is at 62 in; crouched, a shot there
/// passes over him and the head is at 40 in.
#[test]
fn a_crouched_generals_head_is_lower_and_a_shot_over_it_misses() {
    use crate::movement::Stance;
    let s = GENERAL_SCALE;
    let stand = duel(Stance::Stand, (60.0, 62.0 * s), head_x2);
    let over = duel(Stance::Crouch, (60.0, 62.0 * s), head_x2);
    let head = duel(Stance::Crouch, (60.0, 40.0 * s), head_x2);
    let body = duel(Stance::Crouch, (60.0, 22.0 * s), head_x2);
    assert_eq!(stand.len(), 1, "standing head: {stand:?}");
    assert!(over.is_empty(), "over a crouched head: {over:?}");
    assert_eq!((head.len(), body.len()), (1, 1), "{head:?} {body:?}");
    assert!((head[0].1 / body[0].1 - 2.0).abs() < 1e-3, "the crouched head takes the head's x2: {head:?} {body:?}");
    assert!((stand[0].1 - head[0].1).abs() < 1e-3);
}

/// Prone, the General is hit as BO2's prone pose: the head a few inches off the ground, the torso
/// lying behind it — a shot at standing chest height misses.
#[test]
fn a_prone_general_is_hit_as_the_prone_pose() {
    use crate::movement::Stance;
    let s = GENERAL_SCALE;
    assert!(duel(Stance::Prone, (60.0, 40.0 * s), head_x2).is_empty(), "nothing at chest height");
    let head = duel(Stance::Prone, (90.0, 8.0 * s), head_x2);
    assert_eq!(head.len(), 1, "{head:?}");
    let stand_body = duel(Stance::Stand, (90.0, 40.0 * s), head_x2);
    assert!((head[0].1 / stand_body[0].1 - 2.0).abs() < 1e-3, "{head:?} {stand_body:?}");
}

/// Zero Hour's infantry have no BO2 animation state and stand, whatever a General near them does.
#[test]
fn zero_hour_infantry_stand() {
    let boxes = stance_poses();
    let poses: &'static Poses = Box::leak(Box::new(Poses::new(&boxes)));
    let bodies = HitBodies { poses: Some(poses), states: &[(2, state::PRONE_IDLE)] };
    let soldier = soldier(3, 1, 60.0);
    let t = fire::target_from(&soldier, &bodies, false);
    assert!(matches!(t.shape, crate::hitscan::Shape::Body { scale, .. } if scale == 1.0));
    let o = Vec3::new(0.0, 0.0, inches(62.0));
    let hit = crate::hitscan::ray_location(o, Vec3::new(1.0, 0.0, 0.0), 1000.0, &t);
    assert_eq!(hit.map(|h| h.1), Some(hitloc::HEAD), "{hit:?}");
}

/// The body (and so the hit boxes and the third-person clip) takes the held weapon's animation
/// set: a weapon BO2's script animates as a pistol poses the pistol's clip ids, the rifle set 0's.
#[test]
fn the_body_state_takes_the_held_weapons_animation_set() {
    let mut p = PlayerSim::spawn_for_tests();
    let run = 31.7;
    assert_eq!(p.body_state(false, run), state::id(p.arsenal.current().def.body_anims, state::STAND_IDLE));
    let mut pistol = p.arsenal.current().def.clone();
    pistol.body_anims = 5;
    p.arsenal.current_mut().def = Box::leak(Box::new(pistol));
    assert_eq!(p.body_state(false, run), state::id(5, state::STAND_IDLE));
    assert_eq!(p.body_state(true, run), state::id(5, state::STAND_WALK), "moving under RTS control walks the set's walk");
}
