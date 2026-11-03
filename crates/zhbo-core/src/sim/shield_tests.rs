//! Fake-host tests for the Assault Shield on its bearer: the held and stowed blocks, melee into
//! it, and blast protection.

use super::tests::{general, spawn, FakeHost};
use super::*;
use crate::input::buttons;
use crate::weapons::{Arsenal, Loadout};

/// Player 0's General 1 at the origin with the first preset; player 1's General 2 at x = 60
/// carrying the shield (held when `held`), facing `yaw`.
fn duel(held: bool, yaw: f32) -> (FakeHost, Sim) {
    let host = FakeHost::default();
    host.add(general(1, 0));
    host.add(ObjInfo { pos: Vec3::new(60.0, 0.0, 0.0), ..general(2, 1) });
    let mut sim = Sim::new();
    spawn(&mut sim, &host, 0, 1);
    spawn(&mut sim, &host, 1, 2);
    sim.choose_class(0, 1, 0, &[]);
    let p = sim.players[1].as_mut().unwrap();
    p.arsenal = Arsenal::with_loadout(Loadout { primary: "riotshield_mp".into(), secondary: "mp7_mp".into(), ..Loadout::test_kit() });
    p.arsenal.raise_left = 0.0;
    if !held {
        p.arsenal.active = 1;
    }
    p.yaw = yaw;
    (host, sim)
}

fn step(sim: &mut Sim, host: &FakeHost, shooter: FpsInput, victim_yaw: f32, n: u32) {
    step2(sim, host, shooter, FpsInput { yaw: victim_yaw, ..Default::default() }, n);
}

fn step2(sim: &mut Sim, host: &FakeHost, shooter: FpsInput, victim: FpsInput, n: u32) {
    for _ in 0..n {
        sim.input(0, 1, shooter);
        sim.input(1, 2, victim);
        let f = sim.frame + 1;
        sim.step(host, f);
    }
}

/// Let the shooter's first raise (the tablet going down) finish.
fn ready(sim: &mut Sim, host: &FakeHost, victim_yaw: f32) {
    let n = (sim.players[0].as_ref().unwrap().arsenal.raise_left / LOGIC_DT).ceil() as u32 + 2;
    step(sim, host, FpsInput::default(), victim_yaw, n);
}

/// Raise the shooter's gun, then fire for a second at the victim's chest (42 in up).
fn shoot(sim: &mut Sim, host: &FakeHost, victim_yaw: f32) -> f32 {
    ready(sim, host, victim_yaw);
    let eye = sim.players[0].as_ref().unwrap().body.eye().z;
    let pitch = -((eye - crate::units::inches(42.0)) / 60.0).atan();
    step(sim, host, FpsInput { held: buttons::ADS, pitch, ..Default::default() }, victim_yaw, 20);
    step(sim, host, FpsInput { held: buttons::ADS | buttons::FIRE, pitch, ..Default::default() }, victim_yaw, 30);
    host.damage_log.borrow().iter().filter(|d| d.1 == 2).map(|d| d.2).sum()
}

const FACING_SHOOTER: f32 = std::f32::consts::PI;

#[test]
fn a_held_shield_stops_bullets_from_the_front() {
    let (host, mut sim) = duel(true, FACING_SHOOTER);
    assert_eq!(shoot(&mut sim, &host, FACING_SHOOTER), 0.0, "every round hits the shield");
    assert!(sim.events.iter().any(|e| matches!(e.kind, EventKind::ShieldHit { knife: false, .. })), "the rounds spark on it");
}

#[test]
fn a_held_shield_leaves_the_back_open() {
    let (host, mut sim) = duel(true, 0.0);
    assert!(shoot(&mut sim, &host, 0.0) > 0.0);
}

#[test]
fn a_stowed_shield_covers_the_back_not_the_front() {
    let (host, mut sim) = duel(false, 0.0);
    assert_eq!(shoot(&mut sim, &host, 0.0), 0.0, "shot in the back: the stowed shield takes it");
    let (host, mut sim) = duel(false, FACING_SHOOTER);
    assert!(shoot(&mut sim, &host, FACING_SHOOTER) > 0.0, "shot in the front with the shield on the back");
}

#[test]
fn a_knife_into_a_held_shield_does_nothing() {
    let (host, mut sim) = duel(true, FACING_SHOOTER);
    host.objects.borrow_mut()[1].pos = Vec3::new(8.0, 0.0, 0.0);
    ready(&mut sim, &host, FACING_SHOOTER);
    step(&mut sim, &host, FpsInput { pressed: buttons::MELEE, ..Default::default() }, FACING_SHOOTER, 1);
    step(&mut sim, &host, FpsInput::default(), FACING_SHOOTER, 30);
    assert!(host.damage_log.borrow().iter().all(|d| d.1 != 2), "{:?}", host.damage_log.borrow());
}

#[test]
fn a_blast_in_front_of_a_held_shield_is_cut_by_its_protection() {
    let def = crate::weapons::catalog().all().iter().copied().find(|d| d.effect == crate::weapons::Effect::Explode && d.explosion_radius > 0.0 && d.explosion_inner_damage > 0.0).expect("an explosive");
    let blast_at = |yaw: f32| {
        let (host, mut sim) = duel(true, yaw);
        step(&mut sim, &host, FpsInput::default(), yaw, 1);
        // A unit in front of the held shield's tag (where the General's drawn arm holds it) when
        // he faces the shooter: squarely in its 30-degree arc.
        let held = crate::weapons::catalog().get("riotshield_mp").unwrap().shield.unwrap().held;
        let at = Vec3::new(60.0 - crate::units::drawn_inches(held.pos[0]) - 1.0, 0.0, crate::units::drawn_inches(held.pos[2]));
        crate::projectiles::blast(&host, def, 1, at, crate::projectiles::Hit::NONE, 1.0, 1.0, &sim.shields);
        let sum = host.damage_log.borrow().iter().filter(|d| d.1 == 2).map(|d| d.2).sum::<f32>();
        sum
    };
    let (front, back) = (blast_at(FACING_SHOOTER), blast_at(0.0));
    let t = crate::tuning::Tuning::default();
    assert!(back > 0.0);
    assert!((front - back * (1.0 - t.shield_blast_protection_30)).abs() < 1e-3, "front {front} back {back}");
}


/// Player 1 plants the shield facing the shooter.
fn deploy(sim: &mut Sim, host: &FakeHost) {
    let fire = FpsInput { yaw: FACING_SHOOTER, held: buttons::FIRE, pressed: buttons::FIRE, ..Default::default() };
    step2(sim, host, FpsInput::default(), fire, 1);
}

/// The bearer's shield weapon's round, `None` while the weapon is out of the arsenal.
fn shield_ammo(sim: &Sim) -> Option<u16> {
    sim.players[1].as_ref().unwrap().arsenal.weapons.iter().find(|w| w.def.is_shield()).map(|w| w.ammo)
}

#[test]
fn the_trigger_plants_the_shield_where_it_was_held_and_brings_the_gun_up() {
    let (host, mut sim) = duel(true, FACING_SHOOTER);
    deploy(&mut sim, &host);
    assert!(sim.events.iter().any(|e| e.player == 1 && matches!(e.kind, EventKind::ShieldDeployed { .. })));
    assert_eq!(sim.deployed.len(), 1);
    let d = &sim.deployed[0];
    assert!(d.bearer.pos.x < 60.0 && d.bearer.pos.x > 55.0, "in front of the bearer, toward the shooter: {:?}", d.bearer.pos);
    assert_eq!(shield_ammo(&sim), None, "deployriotshield takes the weapon");
    let p = sim.players[1].as_ref().unwrap();
    assert_eq!(p.arsenal.current().def.id, "mp7_mp", "the other gun comes up");
    // Not on the back while it stands: the next frame's bearers are the deployed one only.
    step(&mut sim, &host, FpsInput::default(), FACING_SHOOTER, 1);
    assert!(sim.shields.iter().all(|b| b.general == 0), "{:?}", sim.shields);
}

#[test]
fn a_deployed_shield_stops_rounds_and_wears_down_at_the_bullet_scale() {
    let (host, mut sim) = duel(true, FACING_SHOOTER);
    deploy(&mut sim, &host);
    // The bearer stays behind it, facing away (its back open, the shield not on it), so only the
    // deployed shield is between.
    let p = sim.players[1].as_mut().unwrap();
    p.yaw = 0.0;
    assert_eq!(shoot(&mut sim, &host, 0.0), 0.0, "every round stops in the deployed shield");
    let taken = sim.deployed[0].damage_taken;
    assert!(taken > 0.0 && taken < crate::tuning::Tuning::default().riotshield_deployed_health, "{taken}");
}

/// The planted shield blocks exactly where its model is drawn: the deployed box (the planted
/// model's collision surfaces) on its `riotshield_placement_zoffset` origin, at the General's drawn
/// scale — the scale the converter draws the shield's object at and the body's hit boxes use. A
/// round skimming just under its top edge or just inside its side stops; just over or outside, it
/// passes.
#[test]
fn a_planted_shield_blocks_where_its_model_is_drawn() {
    let (host, mut sim) = duel(true, FACING_SHOOTER);
    deploy(&mut sim, &host);
    let d = &sim.deployed[0];
    let (g, zoffset) = (d.def.shield.unwrap().deployed, crate::tuning::Tuning::default().riotshield_placement_zoffset);
    let k = crate::units::DRAWN_ZH_PER_INCH;
    assert_eq!(k, crate::units::inches(1.0) * crate::units::GENERAL_SCALE, "the body's drawn scale");
    let foot = d.bearer.pos;
    let (top, bottom) = (foot.z + (zoffset + g.maxs[2]) * k, foot.z + (zoffset + g.mins[2]) * k);
    let half_width = g.maxs[1] * k;
    let shot = |z: f32, y: f32| d.bearer.ray(Vec3::new(foot.x - 30.0, foot.y + y, z), Vec3::new(1.0, 0.0, 0.0), 60.0);
    let eps = 0.05;
    assert!(shot(top - eps, 0.0).is_some() && shot(top + eps, 0.0).is_none(), "top edge at {top}");
    assert!(shot(bottom + eps, 0.0).is_some() && shot(bottom - eps, 0.0).is_none(), "bottom edge at {bottom}");
    assert!(shot(top - 1.0, half_width - eps).is_some() && shot(top - 1.0, half_width + eps).is_none(), "side edge at {half_width}");
}

#[test]
fn the_use_key_beside_it_gives_the_shield_back_and_brings_it_up() {
    let (host, mut sim) = duel(true, FACING_SHOOTER);
    deploy(&mut sim, &host);
    step2(&mut sim, &host, FpsInput::default(), FpsInput { yaw: FACING_SHOOTER, ..Default::default() }, 40);
    let take = FpsInput { yaw: FACING_SHOOTER, pressed: buttons::FIELD_COMPUTER, ..Default::default() };
    step2(&mut sim, &host, FpsInput::default(), take, 1);
    assert!(sim.deployed.is_empty(), "picked up with the use key");
    assert_eq!(shield_ammo(&sim), Some(1));
    let p = sim.players[1].as_ref().unwrap();
    assert!(p.arsenal.current().def.is_shield(), "the shield comes up");
    assert_eq!(p.arsenal.weapons.len(), 2);
    assert!(!matches!(p.mode, Mode::FieldComputer { .. }), "the use key did not open the field computer");
    assert!(sim.events.iter().any(|e| matches!(e.kind, EventKind::ShieldPickedUp)));
}

#[test]
fn a_shield_only_kit_gets_the_held_knife_while_it_is_deployed() {
    let (host, mut sim) = duel(true, FACING_SHOOTER);
    let p = sim.players[1].as_mut().unwrap();
    p.arsenal = Arsenal::with_loadout(Loadout { primary: "riotshield_mp".into(), secondary: String::new(), ..Loadout::test_kit() });
    p.arsenal.raise_left = 0.0;
    // Whatever the kit carries besides the shield stays; the knife is given only when nothing does.
    p.arsenal.weapons.retain(|w| w.def.is_shield());
    p.arsenal.active = 0;
    deploy(&mut sim, &host);
    let p = sim.players[1].as_ref().unwrap();
    assert_eq!(p.arsenal.weapons.len(), 1);
    assert_eq!(p.arsenal.current().def.id, p.arsenal.melee.id, "`knife_held_mp` given (riotshieldtakeweapon)");
    let take = FpsInput { yaw: FACING_SHOOTER, pressed: buttons::FIELD_COMPUTER, ..Default::default() };
    step2(&mut sim, &host, FpsInput::default(), take, 1);
    let p = sim.players[1].as_ref().unwrap();
    assert_eq!(p.arsenal.weapons.len(), 1, "the knife goes again");
    assert!(p.arsenal.current().def.is_shield());
}

#[test]
fn a_refused_placement_gives_the_round_back() {
    let (host, mut sim) = duel(true, FACING_SHOOTER);
    host.deep_water.set(true);
    deploy(&mut sim, &host);
    assert!(sim.deployed.is_empty());
    assert_eq!(shield_ammo(&sim), Some(1));
    assert!(sim.events.iter().any(|e| e.player == 1 && matches!(e.kind, EventKind::CantPlant { .. })));
}

#[test]
fn a_deployed_shield_is_destroyed_at_its_health_and_its_model_times_out() {
    let (host, mut sim) = duel(true, FACING_SHOOTER);
    deploy(&mut sim, &host);
    let t = crate::tuning::Tuning::default();
    sim.damage_deployed(&host, 0, Some(0), 1, t.riotshield_deployed_health - 1.0, deploy::ShieldDamage::Melee);
    assert!(sim.deployed[0].standing());
    sim.damage_deployed(&host, 0, Some(0), 1, 1.0, deploy::ShieldDamage::Explosive);
    assert!(!sim.deployed[0].standing());
    assert!(sim.events.iter().any(|e| e.player == 1 && matches!(e.kind, EventKind::ShieldDestroyed { by: Some(0), .. })));
    step(&mut sim, &host, FpsInput::default(), FACING_SHOOTER, 1);
    assert!(sim.shields.iter().all(|b| b.general != 0), "a destroyed shield blocks nothing");
    step(&mut sim, &host, FpsInput::default(), FACING_SHOOTER, (t.riotshield_destroyed_cleanup_time / LOGIC_DT) as u32 + 2);
    assert!(sim.deployed.is_empty());
    assert_eq!(shield_ammo(&sim), None, "the weapon went with it");
}

#[test]
fn zero_hour_damage_to_the_deployed_shields_object_wears_it_down_at_the_scripts_scales() {
    let (host, mut sim) = duel(true, FACING_SHOOTER);
    deploy(&mut sim, &host);
    sim.deployed[0].object = 42;
    let t = crate::tuning::Tuning::default();
    // A Zero Hour rifleman (no General): bullets at the bullet scale, in BO2 damage.
    sim.engine_damage_to_deployed(&host, 42, 0, crate::host::damage_kind::SMALL_ARMS, 40.0 * BO2_TO_ZH_DAMAGE);
    assert!((sim.deployed[0].damage_taken - 40.0 * t.riotshield_bullet_damage_scale).abs() < 1e-4);
    // A tank shell: explosive, full.
    sim.engine_damage_to_deployed(&host, 42, 0, crate::host::damage_kind::EXPLOSION, t.riotshield_deployed_health * BO2_TO_ZH_DAMAGE);
    assert!(!sim.deployed[0].standing(), "destroyed");
    // Another object id is not a shield.
    sim.engine_damage_to_deployed(&host, 43, 0, crate::host::damage_kind::EXPLOSION, 1.0);
}
