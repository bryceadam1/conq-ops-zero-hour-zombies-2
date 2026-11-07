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
    LAST_HOW.with(|h| *h.borrow_mut() = host.damage_how.borrow().clone());
    log
}

thread_local! {
    /// The means of death of [`shoot`]'s last round's hits, in order.
    static LAST_HOW: std::cell::RefCell<Vec<(u32, crate::damage::MeansOfDeath)>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn last_how() -> Vec<crate::damage::MeansOfDeath> {
    LAST_HOW.with(|h| h.borrow().iter().map(|x| x.1).collect())
}

/// BO2's means of death for a round (`FUN_005cfc60`, `_globallogic_player.gsc:802`): a rifle's
/// `MOD_RIFLE_BULLET`, a pistol's `MOD_PISTOL_BULLET`, a soldier's head `MOD_HEAD_SHOT`; all three
/// reach Zero Hour's armour as small arms (the tuning pack's `[MeansOfDeath]`).
#[test]
fn a_round_carries_bo2s_means_of_death() {
    use crate::damage::MeansOfDeath::*;
    let boxes = [hit_box(hitloc::HEAD, 62.0, [4.0, 4.0, 5.0]), hit_box(hitloc::TORSO_MID, 40.0, [5.0, 8.0, 15.0])];
    let rifle = |_: &mut crate::weapons::WeaponDef| {};
    let pistol = |d: &mut crate::weapons::WeaponDef| d.weap_class = crate::weapons::WeapClass::Pistol;
    assert_eq!(shoot(&boxes, &[(2, 60.0)], (60.0, 62.0), rifle).len(), 1);
    assert_eq!(last_how(), [HeadShot]);
    shoot(&boxes, &[(2, 60.0)], (60.0, 40.0), rifle);
    assert_eq!(last_how(), [RifleBullet]);
    shoot(&boxes, &[(2, 60.0)], (60.0, 40.0), pistol);
    assert_eq!(last_how(), [PistolBullet]);
    let table = crate::tuning::get().means_of_death;
    for m in [HeadShot, RifleBullet, PistolBullet] {
        assert_eq!(table.zh_name(m), "SMALL_ARMS");
    }
}

/// A round into a Zero Hour vehicle: a rifle bullet whatever it strikes (no head, no location
/// multiplier), the full curve's damage under BO2's vehicle rule (`vehicledamagescalar`).
#[test]
fn a_round_into_a_vehicle_is_a_rifle_bullet_at_full_damage() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    host.add(ObjInfo { id: 2, owner_player: 1, flags: objflags::VEHICLE, geom_type: geom::BOX, major_radius: 26.0, minor_radius: 13.0, height: 15.0, health: 5000.0, max_health: 5000.0, pos: Vec3::new(60.0, 0.0, 0.0), ..Default::default() });
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    sim.choose_class(0, 1, 0, &[]);
    wait_until_ready(&mut sim, &host);
    let w = sim.players[0].as_mut().unwrap().arsenal.current_mut();
    let mut def = w.def.clone();
    def.damage = [40.0; 6];
    def.damage_range = [0.0; 6];
    def.pellets = 1;
    def.location_multipliers = [3.0; crate::hitloc::COUNT];
    def.specialties = crate::weapons::Specialties::default();
    let def: &'static crate::weapons::WeaponDef = Box::leak(Box::new(def));
    (w.def, w.main) = (def, def);
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS, ..Default::default() }, 20);
    host.damage_log.borrow_mut().clear();
    host.damage_how.borrow_mut().clear();
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS | buttons::FIRE, ..Default::default() }, 1);
    let log: Vec<(u32, f32)> = host.damage_log.borrow().iter().map(|d| (d.1, d.2)).collect();
    assert_eq!(log, [(2, 40.0 * crate::units::BO2_TO_ZH_DAMAGE)], "no location multiplier on a vehicle");
    assert_eq!(host.damage_how.borrow().as_slice(), [(2, crate::damage::MeansOfDeath::RifleBullet)]);
}

/// A knife reaches a vehicle's side, not a circle of its length (BO2's melee trace meets the
/// entity's bounds): beside a tank turned side-on, the blow lands on its side when the side is in
/// reach, by `MOD_MELEE`, and not when the side is a unit past it (its centre is then well inside
/// the reach plus its length, which used to strike).
#[test]
fn a_knife_reaches_a_vehicles_box() {
    let knife_at = |pos: Vec3, yaw: f32| {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(ObjInfo { id: 2, owner_player: 1, flags: objflags::VEHICLE, geom_type: geom::BOX, major_radius: 26.0, minor_radius: 13.0, height: 15.0, yaw, health: 5000.0, max_health: 5000.0, pos, ..Default::default() });
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        wait_until_ready(&mut sim, &host);
        run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::MELEE, ..Default::default() }, 1);
        run(&mut sim, &host, 0, 1, FpsInput::default(), 20);
        let how = host.damage_how.borrow().clone();
        how
    };
    let reach = super::MELEE_RANGE;
    // Side-on (its long axis across the knife): the side half a unit inside the reach.
    let side = knife_at(Vec3::new(13.0 + reach - 0.5, 0.0, 0.0), std::f32::consts::FRAC_PI_2);
    assert_eq!(side, [(2, crate::damage::MeansOfDeath::Melee)]);
    // Its side a unit past the reach.
    let past = knife_at(Vec3::new(13.0 + reach + 1.0, 0.0, 0.0), std::f32::consts::FRAC_PI_2);
    assert!(13.0 + reach + 1.0 < reach + 26.0 && past.is_empty(), "{past:?}");
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
    assert_eq!(p.body_state(None), state::id(p.arsenal.current().def.body_anims, state::STAND_IDLE));
    let mut pistol = p.arsenal.current().def.clone();
    pistol.body_anims = 5;
    p.arsenal.current_mut().def = Box::leak(Box::new(pistol));
    assert_eq!(p.body_state(None), state::id(5, state::STAND_IDLE));
    p.mode = super::Mode::Rts;
    let walking = super::EngineMotion { general: p.general, moving: true, speed: 0.3 * run };
    assert_eq!(p.body_state(Some(walking)), state::id(5, state::STAND_WALK), "moving under RTS control walks the set's walk");
}

/// Under RTS control Zero Hour walks the General: his drawn body (and his hit pose) stands — never
/// the crouch, prone, ADS or sprint his body kept from his last first person — and walks or runs
/// at the engine's speed while Zero Hour says he moves (the same split as first person's,
/// `nativebody::state_of`), idle when it says he does not.
#[test]
fn an_overhead_general_walks_and_runs_at_the_engines_speed_standing() {
    let mut p = PlayerSim::spawn_for_tests();
    let run = 31.7;
    let set = p.arsenal.current().def.body_anims;
    p.body.stance = crate::movement::Stance::Prone;
    p.body.sprinting = true;
    p.arsenal.ads = 1.0;
    p.mode = super::Mode::Rts;
    let motion = |moving: bool, speed: f32| Some(super::EngineMotion { general: p.general, moving, speed });
    assert_eq!(p.body_state(None), state::id(set, state::STAND_IDLE), "no motion reported: he stands idle");
    assert_eq!(p.body_state(motion(false, 0.0)), state::id(set, state::STAND_IDLE));
    assert_eq!(p.body_state(motion(true, 0.4 * run)), state::id(set, state::STAND_WALK));
    assert_eq!(p.body_state(motion(true, run)), state::id(set, state::STAND_RUN), "Zero Hour's speed past the walk/run split runs");
    // Back in first person his own stance rules again.
    p.mode = super::Mode::Fps;
    assert_eq!(p.body_state(motion(true, run)), state::id(set, state::PRONE_IDLE), "first person ignores the engine's motion");
}

/// A click picks a General the native pass draws (`Sim::pick_general`, host patch 0029) by the hit
/// boxes the bullets strike and the body is drawn over, at his drawn size, where his logic object
/// stands and faces: a pick ray down through his drawn chest strikes him, one beside him misses,
/// and overhead he is picked standing even when his body kept a prone stance from first person.
/// Only a seat's General is picked this way.
#[test]
fn a_click_picks_the_general_by_his_drawn_hit_boxes() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.hit_body = Some(Box::leak(Box::new(Poses::new(&stance_poses()))));
    sim.take_control(&host, 0, 1);
    sim.choose_class(0, 1, 0, &[]);
    wait_until_ready(&mut sim, &host);
    let feet = Vec3::new(100.0, 50.0, 3.0);
    let s = GENERAL_SCALE;
    // The overhead camera's ray: from up and behind, down through a point.
    let pick = |sim: &Sim, through: Vec3| {
        let origin = through + Vec3::new(-150.0, 0.0, 200.0);
        let dir = (through - origin).normalized_or_zero();
        sim.pick_general(1, feet, 0.0, 14.0, origin, dir, 1000.0)
    };
    let chest = feet + Vec3::new(0.0, 0.0, inches(40.0) * s);
    let t = pick(&sim, chest).expect("a click on his chest picks him");
    assert!(t < 250.0 && 250.0 - t < inches(24.0), "it enters his torso just before the chest: {t}");
    assert!(pick(&sim, feet + Vec3::new(0.0, inches(30.0) * s, inches(40.0) * s)).is_none(), "beside him misses");
    assert!(sim.pick_general(9, feet, 0.0, 14.0, chest + Vec3::new(-150.0, 0.0, 200.0), Vec3::new(150.0, 0.0, -200.0).normalized_or_zero(), 1000.0).is_none(), "no seat's General: no pick");
    // Overhead, a prone body from first person: Zero Hour has him standing, and so does the pick.
    let p = sim.players[0].as_mut().unwrap();
    p.body.stance = crate::movement::Stance::Prone;
    p.mode = Mode::Rts;
    assert!(pick(&sim, chest).is_some(), "overhead he is picked standing");
}

/// The native third person's poses take each overhead General's engine motion by his object id
/// (`zhbo_client_native_poses_moving`): the General Zero Hour moves walks or runs, standing, and
/// one it does not stays idle; a first-person General's own state ignores it.
#[test]
fn the_native_poses_take_each_overhead_generals_engine_motion() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    host.add(general(2, 1));
    let mut sim = Sim::new();
    for (seat, g) in [(0, 1), (1, 2)] {
        sim.take_control(&host, seat, g);
        sim.choose_class(seat, g, 0, &[]);
    }
    wait_until_ready(&mut sim, &host);
    let run = sim.tuning.run_speed(false);
    for p in sim.players.iter_mut().flatten() {
        p.mode = Mode::Rts;
        p.body.stance = crate::movement::Stance::Crouch;
    }
    let motion = [EngineMotion { general: 2, moving: true, speed: run }];
    let poses = sim.native_poses(&motion, None);
    let of = |g: u32| poses.iter().find(|p| p.general == g).map(|p| (p.stance, state::split(p.state).1)).unwrap();
    assert_eq!(of(2), (0, state::STAND_RUN), "Zero Hour runs General 2: his body runs, standing");
    assert_eq!(of(1), (0, state::STAND_IDLE), "General 1 is not moving: idle, standing");
    sim.players[0].as_mut().unwrap().mode = Mode::Fps;
    let poses = sim.native_poses(&[EngineMotion { general: 1, moving: true, speed: run }], None);
    assert_eq!(poses.iter().find(|p| p.general == 1).map(|p| (p.stance, state::split(p.state).1)), Some((1, state::CROUCH_IDLE)), "first person: his own crouch, not the engine's");
}
