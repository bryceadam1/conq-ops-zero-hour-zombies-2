//! Fake-host tests for the killing blow's report (`EventKind::KilledBy`): every damage site names
//! its weapon and the way the blow travelled, so the victim's body falls by BO2's `dmgType` and
//! `dmgDirection` (`crate::death`).

use super::tests::{general, spawn, FakeHost};
use super::*;
use crate::death::{dmg_direction, DmgType};
use crate::input::buttons;
use crate::weapons::{Arsenal, Loadout, WeapClass};

/// Player 0's General 1 at the origin carrying `loadout`, facing +X; player 1's General 2 at
/// `x` on the X axis, facing `yaw`, one hit from death.
fn duel(loadout: Loadout, x: f32, yaw: f32) -> (FakeHost, Sim) {
    let host = FakeHost::default();
    host.add(general(1, 0));
    host.add(ObjInfo { pos: Vec3::new(x, 0.0, 0.0), health: 1.0, ..general(2, 1) });
    let mut sim = Sim::new();
    spawn(&mut sim, &host, 0, 1);
    spawn(&mut sim, &host, 1, 2);
    let p = sim.players[0].as_mut().unwrap();
    p.arsenal = Arsenal::with_loadout(loadout);
    p.arsenal.raise_left = 0.0;
    sim.players[1].as_mut().unwrap().yaw = yaw;
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

/// The victim's death report, and the side BO2 names for it in his view.
fn killed(sim: &Sim, victim_yaw: f32) -> Option<(DmgType, u8)> {
    sim.events.iter().find_map(|e| match e.kind {
        EventKind::KilledBy { dmg, dir } if e.player == 1 => Some((dmg, dmg_direction(dir, victim_yaw))),
        _ => None,
    })
}

const FACING: f32 = std::f32::consts::PI;

/// The class knife, and the shield's own bash (`melee_weapon` is the shield while it is held):
/// melee and meleebash, the blow travelling the way the attacker faces.
#[test]
fn the_knife_and_the_shields_bash_report_melee_and_meleebash() {
    for (primary, want) in [("an94_mp", DmgType::Melee), ("riotshield_mp", DmgType::MeleeBash)] {
        let (host, mut sim) = duel(Loadout { primary: primary.into(), ..Loadout::test_kit() }, 8.0, FACING);
        step(&mut sim, &host, FpsInput { pressed: buttons::MELEE, ..Default::default() }, FACING, 1);
        step(&mut sim, &host, FpsInput::default(), FACING, 20);
        assert_eq!(killed(&sim, FACING), Some((want, 1)), "{primary}: from the front, facing his killer");
        // His back to the attacker: the same blow is from behind.
        let (host, mut sim) = duel(Loadout { primary: primary.into(), ..Loadout::test_kit() }, 8.0, 0.0);
        step(&mut sim, &host, FpsInput { pressed: buttons::MELEE, ..Default::default() }, 0.0, 1);
        step(&mut sim, &host, FpsInput::default(), 0.0, 20);
        assert_eq!(killed(&sim, 0.0), Some((want, 2)), "{primary}: from behind");
    }
}

/// A gun's round is `normal`, a spread class's pellet `normal_shotgun`; it travels from the
/// shooter, so it comes from the side of the victim the shooter stands on.
#[test]
fn a_rounds_kill_reports_its_class_and_the_side_it_came_from() {
    for (spread, want) in [(false, DmgType::Normal), (true, DmgType::NormalShotgun)] {
        // The victim faces +Y, so his left is -X, where the shooter stands.
        let yaw = std::f32::consts::FRAC_PI_2;
        let (host, mut sim) = duel(Loadout::test_kit(), 60.0, yaw);
        if spread {
            let p = sim.players[0].as_mut().unwrap();
            let mut d = p.arsenal.current().def.clone();
            d.weap_class = WeapClass::Spread;
            // A spread class's pellets fly to its curve's last range: past the victim.
            d.damage_range[5] = d.damage_range[5].max(1000.0);
            p.arsenal.current_mut().def = Box::leak(Box::new(d));
        }
        let eye = sim.players[0].as_ref().unwrap().body.eye().z;
        let pitch = -((eye - crate::units::inches(42.0)) / 60.0).atan();
        step(&mut sim, &host, FpsInput { held: buttons::ADS, pitch, ..Default::default() }, yaw, 20);
        step(&mut sim, &host, FpsInput { held: buttons::ADS | buttons::FIRE, pitch, ..Default::default() }, yaw, 30);
        assert_eq!(killed(&sim, yaw), Some((want, 3)), "spread {spread}: from his left");
    }
}

/// A blast reports `explosive` from where it went off (BO2's radius damage: `vDir` is his origin
/// less the blast's), not from the thrower.
#[test]
fn a_blasts_kill_reports_explosive_from_where_it_went_off() {
    let (host, mut sim) = duel(Loadout::test_kit(), 60.0, 0.0);
    let frag = crate::weapons::catalog().get("frag_grenade_mp").expect("frag");
    // Off his right side (-Y; he faces +X): the thrower is behind him at the origin.
    let at = Vec3::new(60.0, -20.0, 0.0);
    sim.scored(&host, |_, h| {
        crate::projectiles::blast(h, frag, 1, at, crate::projectiles::Hit::NONE, 1.0, 1.0, &[]);
    });
    assert_eq!(killed(&sim, 0.0), Some((DmgType::Explosive, 4)), "from his right");
    // Every blow that landed named the frag.
    assert!(host.damage_blows.borrow().iter().all(|(_, b)| b.weapon.is_some_and(|w| w.id == "frag_grenade_mp")));
}
