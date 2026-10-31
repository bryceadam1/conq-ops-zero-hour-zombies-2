//! Fake-host tests for the Assault Shield on its bearer: the held and stowed blocks, melee into
//! it, and blast protection.

use super::tests::{general, FakeHost};
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
    sim.take_control(&host, 0, 1);
    sim.take_control(&host, 1, 2);
    sim.choose_class(0, 1, 0, &[]);
    let p = sim.players[1].as_mut().unwrap();
    p.arsenal = Arsenal::with_loadout(Loadout { primary: "riotshield_mp".into(), secondary: "mp7_mp".into(), ..Loadout::default() });
    if !held {
        p.arsenal.active = 1;
    }
    p.yaw = yaw;
    (host, sim)
}

fn step(sim: &mut Sim, host: &FakeHost, shooter: FpsInput, victim_yaw: f32, n: u32) {
    for _ in 0..n {
        sim.input(0, 1, shooter);
        sim.input(1, 2, FpsInput { yaw: victim_yaw, ..Default::default() });
        let f = sim.frame + 1;
        sim.step(host, f);
    }
}

/// Let the shooter's first raise (the tablet going down) finish.
fn ready(sim: &mut Sim, host: &FakeHost, victim_yaw: f32) {
    let n = (sim.players[0].as_ref().unwrap().arsenal.raise_left / LOGIC_DT).ceil() as u32 + 2;
    step(sim, host, FpsInput::default(), victim_yaw, n);
}

/// Raise the shooter's gun, then fire for a second at the victim's chest.
fn shoot(sim: &mut Sim, host: &FakeHost, victim_yaw: f32) -> f32 {
    let pitch = -(2.0f32 / 60.0).atan();
    ready(sim, host, victim_yaw);
    step(sim, host, FpsInput { held: buttons::ADS, pitch, ..Default::default() }, victim_yaw, 20);
    step(sim, host, FpsInput { held: buttons::ADS | buttons::FIRE, pitch, ..Default::default() }, victim_yaw, 30);
    host.damage_log.borrow().iter().filter(|d| d.1 == 2).map(|d| d.2).sum()
}

const FACING_SHOOTER: f32 = std::f32::consts::PI;

#[test]
fn a_held_shield_stops_bullets_from_the_front() {
    let (host, mut sim) = duel(true, FACING_SHOOTER);
    assert_eq!(shoot(&mut sim, &host, FACING_SHOOTER), 0.0, "every round hits the shield");
    assert!(sim.events.iter().any(|e| matches!(e.kind, EventKind::Impact { surface: crate::impacts::Surface::Metal, .. })), "the rounds spark on it");
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
        let at = Vec3::new(56.0, 0.0, 6.0);
        crate::projectiles::blast(&host, def, 1, at, None, 1.0, 1.0, &sim.shields);
        let sum = host.damage_log.borrow().iter().filter(|d| d.1 == 2).map(|d| d.2).sum::<f32>();
        sum
    };
    let (front, back) = (blast_at(FACING_SHOOTER), blast_at(0.0));
    let t = crate::tuning::Tuning::default();
    assert!(back > 0.0);
    assert!((front - back * (1.0 - t.shield_blast_protection_30)).abs() < 1e-3, "front {front} back {back}");
}

