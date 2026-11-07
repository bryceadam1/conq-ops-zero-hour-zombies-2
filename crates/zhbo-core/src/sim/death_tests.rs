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
        EventKind::KilledBy { dmg, dir, .. } if e.player == 1 => Some((dmg, dmg_direction(dir, victim_yaw))),
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

/// A hurt that does not kill: the victim's flinch kicks by the side the blow came from
/// (`flinch_kick`, BO2's `FUN_005d1800`/`FUN_004e8ab0`) and his hurt carries the blow's way (the
/// third-person flinch's `dmgDirection`), whichever of the two seats steps first.
#[test]
fn a_hurt_flinches_by_the_side_the_blow_came_from() {
    // Seat 0 knifes seat 1 (stepped after him: the step's own blow) facing him: from the front,
    // the view kicks up.
    let (host, mut sim) = duel(Loadout::test_kit(), 8.0, FACING);
    for o in host.objects.borrow_mut().iter_mut().filter(|o| o.id == 2) {
        o.health = 1000.0;
    }
    sim.players[1].as_mut().unwrap().health = 1000.0;
    step(&mut sim, &host, FpsInput { pressed: buttons::MELEE, ..Default::default() }, FACING, 1);
    step(&mut sim, &host, FpsInput::default(), FACING, 20);
    let hurt: Vec<EventKind> = sim.events.iter().filter(|e| e.player == 1).map(|e| e.kind).filter(|k| matches!(k, EventKind::Damaged { .. } | EventKind::Flinch { .. })).collect();
    let (dir, (yaw, pitch)) = match hurt[..] {
        [EventKind::Damaged { dir, .. }, EventKind::Flinch { yaw_deg, pitch_deg }] => (dir, (yaw_deg, pitch_deg)),
        _ => panic!("{hurt:?}"),
    };
    assert_eq!(dmg_direction(dir, FACING), 1, "from the front: {dir:?}");
    assert!(pitch > 0.0 && yaw.abs() < 1e-3, "kicked up: {yaw} {pitch}");
}

/// Zero Hour's own weapons: the engine's damage hook's blow gives the hurt its side and the death
/// its report — a tank's shell (`ARMOR_PIERCING`, `MOD_EXPLOSIVE`) explosive from where it came.
#[test]
fn zero_hours_own_weapons_hurt_and_kill_by_bo2s_rules() {
    let (host, mut sim) = duel(Loadout::test_kit(), 60.0, 0.0);
    for o in host.objects.borrow_mut().iter_mut().filter(|o| o.id == 2) {
        o.health = 200.0;
    }
    sim.players[1].as_mut().unwrap().health = 200.0;
    step(&mut sim, &host, FpsInput::default(), 0.0, 1);
    sim.events.clear();
    // A Ranger's round from his left (+Y: it travels -Y): his view turns left.
    let rifle = crate::damage::zh_code("SMALL_ARMS").unwrap() as i32;
    sim.engine_blow(2, rifle, Vec3::new(0.0, -1.0, 0.0));
    host.objects.borrow_mut().iter_mut().filter(|o| o.id == 2).for_each(|o| o.health -= 30.0);
    step(&mut sim, &host, FpsInput::default(), 0.0, 1);
    let flinch = sim.events.iter().find_map(|e| match e.kind {
        EventKind::Flinch { yaw_deg, .. } if e.player == 1 => Some(yaw_deg),
        _ => None,
    });
    assert!(flinch.is_some_and(|y| y > 0.0), "{flinch:?}");
    assert!(sim.blows.is_empty(), "read by the step");
    // A tank's shell from behind kills him: the report is Zero Hour's, explosive from the back.
    let shell = crate::damage::zh_code("ARMOR_PIERCING").unwrap() as i32;
    sim.engine_blow(2, shell, Vec3::new(1.0, 0.0, 0.0));
    host.objects.borrow_mut().iter_mut().filter(|o| o.id == 2).for_each(|o| {
        o.health = 0.0;
        o.flags |= crate::host::objflags::DEAD;
    });
    step(&mut sim, &host, FpsInput::default(), 0.0, 1);
    assert_eq!(killed(&sim, 0.0), Some((DmgType::Explosive, 2)));
}

/// A blow scored after the victim's seat stepped (his seat before his attacker's) gives his hurt
/// its side at the next step.
#[test]
fn a_blow_scored_after_his_step_gives_the_next_steps_hurt_its_side() {
    let (host, mut sim) = duel(Loadout::test_kit(), 60.0, 0.0);
    for o in host.objects.borrow_mut().iter_mut().filter(|o| o.id == 2) {
        o.health = 200.0;
    }
    sim.players[1].as_mut().unwrap().health = 200.0;
    step(&mut sim, &host, FpsInput::default(), 0.0, 1);
    sim.events.clear();
    let knife = crate::weapons::catalog().get("knife_mp").expect("knife");
    sim.scored(&host, |_, h| {
        h.damage(1, 2, 30.0, crate::damage::Blow::new(crate::damage::MeansOfDeath::Melee, knife, Vec3::new(1.0, 0.0, 0.0)));
    });
    step(&mut sim, &host, FpsInput::default(), 0.0, 1);
    let dir = sim.events.iter().find_map(|e| match e.kind {
        EventKind::Damaged { dir, .. } if e.player == 1 => Some(dir),
        _ => None,
    });
    assert_eq!(dir, Some(Vec3::new(1.0, 0.0, 0.0)), "from behind, the blow's own way");
}

/// The Sensor Grenade's timed fuse (`timedDetonation`, conformance-final) runs out beside him: its
/// radius damage (`iExplosionInnerDamage` 15, `FUN_005f2130`) is a grenade's splash
/// (`MOD_GRENADE_SPLASH`, so `dmgType` `explosive`) by the sensor, from where it lay (BO2's radius
/// `vDir`), not from the thrower: his hurt flinches from that side.
#[test]
fn a_sensor_grenades_detonation_is_explosive_from_where_it_lay() {
    let (host, mut sim) = duel(Loadout::test_kit(), 60.0, 0.0);
    // Whole, so its 15 hurts him without killing him.
    for o in host.objects.borrow_mut().iter_mut().filter(|o| o.id == 2) {
        o.health = o.max_health;
    }
    let frag = crate::weapons::catalog().get("frag_grenade_mp").expect("frag");
    let sensor: &'static crate::weapons::WeaponDef = Box::leak(Box::new(crate::weapons::WeaponDef {
        id: "sensor_grenade_mp".into(),
        effect: crate::weapons::Effect::None,
        equipment_kind: crate::equipment::Kind::Sensor,
        timed_detonation: true,
        laser_sensor: true,
        explosion_inner_damage: 15.0,
        explosion_outer_damage: 15.0,
        explosion_radius: 256.0,
        ..frag.clone()
    }));
    // Off his right side (-Y; he faces +X); the thrower stands behind him at the origin.
    let mut p = crate::equipment::Placed::new(sensor, 0, 1, 0, Vec3::new(60.0, -20.0, 0.0), 0.0, None);
    p.fuse = Some(2.0 * crate::units::LOGIC_DT);
    sim.equipment.add(p);
    step(&mut sim, &host, FpsInput::default(), 0.0, 4);
    assert!(sim.equipment.placed.is_empty(), "it went off");
    let blows = host.damage_blows.borrow();
    let (_, b) = blows.iter().find(|(v, _)| *v == 2).expect("its radius damage reached him");
    assert_eq!(b.how, crate::damage::MeansOfDeath::GrenadeSplash);
    assert_eq!(b.weapon.map(|w| w.id.as_str()), Some("sensor_grenade_mp"));
    assert_eq!(DmgType::of(b.how, b.weapon, false), DmgType::Explosive);
    let hurt = sim.events.iter().find_map(|e| match e.kind {
        EventKind::Damaged { dir, .. } if e.player == 1 => Some(dir),
        _ => None,
    });
    assert_eq!(hurt.map(|d| dmg_direction(d, 0.0)), Some(4), "from his right, where it lay: {hurt:?}");
}

/// An overhead General (his body drawn for everyone, as in first person) hurt by Zero Hour's own
/// weapons: the hurt is the same `Damaged` event with the blow's way, so his pain voice and the
/// third-person flinch by `dmgDirection` play; no view flinch (there is no first-person view);
/// and a death to them reports its `KilledBy`.
#[test]
fn an_overhead_general_hurt_by_zero_hour_units_flinches_by_the_blows_side() {
    let host = FakeHost::default();
    host.add(ObjInfo { health: 200.0, max_health: 200.0, ..general(1, 0) });
    let mut sim = Sim::new();
    sim.rts_update(&host, 0, 1, 0, [0, 0, 0], None);
    sim.choose_class(0, 1, 0, &[]);
    let mut frame = 0;
    let mut step = |sim: &mut Sim| {
        frame += 1;
        sim.step(&host, frame);
    };
    step(&mut sim);
    assert!(!sim.player(0).unwrap().is_first_person());
    sim.events.clear();
    // A Ranger's round from his left (+Y side: it travels -Y).
    let rifle = crate::damage::zh_code("SMALL_ARMS").unwrap() as i32;
    sim.engine_blow(1, rifle, Vec3::new(0.0, -1.0, 0.0));
    host.objects.borrow_mut()[0].health -= 30.0;
    step(&mut sim);
    let hurt: Vec<EventKind> = sim.events.iter().filter(|e| e.player == 0).map(|e| e.kind).filter(|k| matches!(k, EventKind::Damaged { .. } | EventKind::Flinch { .. })).collect();
    assert_eq!(hurt, vec![EventKind::Damaged { amount: 30.0, dir: Vec3::new(0.0, -1.0, 0.0) }], "the hurt, no view flinch");
    // A tank's shell from behind kills him: Zero Hour's report, explosive from the back.
    sim.events.clear();
    let shell = crate::damage::zh_code("ARMOR_PIERCING").unwrap() as i32;
    sim.engine_blow(1, shell, Vec3::new(1.0, 0.0, 0.0));
    host.objects.borrow_mut().iter_mut().for_each(|o| {
        o.health = 0.0;
        o.flags |= crate::host::objflags::DEAD;
    });
    step(&mut sim);
    let killed = sim.events.iter().find_map(|e| match e.kind {
        EventKind::KilledBy { dmg, dir, .. } if e.player == 0 => Some((dmg, dmg_direction(dir, 0.0))),
        _ => None,
    });
    assert_eq!(killed, Some((DmgType::Explosive, 2)));
}

/// A whole General (BO2's player, at a Zero Hour health that is not BO2's 100) dies in BO2's
/// shots, end to end through the sim: the class knife in one blow, and the class gun's aimed
/// string in the fewest rounds whose BO2 hits add up to `playerMaxHealth` — each round a whole
/// BO2 hit taken as its share of his health.
#[test]
fn a_whole_general_dies_in_bo2s_shots() {
    let whole = |host: &FakeHost, health: f32| {
        for o in host.objects.borrow_mut().iter_mut().filter(|o| o.id == 2) {
            (o.health, o.max_health) = (health, health);
        }
    };
    let t = crate::tuning::get();
    for health in [200.0, 230.0, 500.0] {
        // The knife (`knife_held_mp`'s melee damage is past 100): one blow.
        let (host, mut sim) = duel(Loadout::test_kit(), 8.0, FACING);
        whole(&host, health);
        step(&mut sim, &host, FpsInput { pressed: buttons::MELEE, ..Default::default() }, FACING, 1);
        step(&mut sim, &host, FpsInput::default(), FACING, 20);
        let blows: Vec<f32> = host.damage_log.borrow().iter().filter(|d| d.1 == 2).map(|d| d.2).collect();
        assert_eq!(blows.len(), 1, "{health}: {blows:?}");
        assert!(killed(&sim, FACING).is_some(), "{health}: one knife blow kills");

        // The gun: every round a whole BO2 hit, the last the one that reaches 100.
        let (host, mut sim) = duel(Loadout::test_kit(), 60.0, FACING);
        whole(&host, health);
        let eye = sim.players[0].as_ref().unwrap().body.eye().z;
        let pitch = -((eye - crate::units::inches(42.0)) / 60.0).atan();
        step(&mut sim, &host, FpsInput { held: buttons::ADS, pitch, ..Default::default() }, FACING, 20);
        step(&mut sim, &host, FpsInput { held: buttons::ADS | buttons::FIRE, pitch, ..Default::default() }, FACING, 60);
        assert!(killed(&sim, FACING).is_some(), "{health}: the string killed him");
        let him = ObjInfo { max_health: health, ..general(2, 1) };
        let bo2: Vec<f32> = host.damage_log.borrow().iter().filter(|d| d.1 == 2).map(|d| crate::damage::to_bo2(&t, &him, d.2)).collect();
        assert!(bo2.iter().all(|b| (b - b.round()).abs() < 0.05 && *b >= 1.0), "{health}: whole BO2 hits {bo2:?}");
        let total: f32 = bo2.iter().map(|b| b.round()).sum();
        let before_last = total - bo2.last().unwrap().round();
        assert!(total >= t.player_max_health && before_last < t.player_max_health, "{health}: killed by the round that reached 100: {bo2:?}");
    }
}

/// Every damage path round 8 added takes a General's health as BO2's share of `playerMaxHealth`,
/// each hit a whole BO2 number (`damage::to_zh`): so a General whose Zero Hour health is not BO2's
/// 100 (230 here, as a hero's toughness makes it) dies to exactly the hit whose BO2 numbers reach
/// 100 — a landing's fall damage (`MOD_FALLING`), a blast seen past cover, the nine-ray knife, the
/// Shock Charge's radius 1 and its ticks, a thrown-back frag's blast (the same `blast` as any
/// frag's, owned by the thrower) and the flashbang's and concussion's radius 1.
#[test]
fn every_new_damage_path_kills_a_general_in_bo2s_count() {
    use crate::weapons::weapon;
    let t = crate::tuning::get();
    const HEALTH: f32 = 230.0;
    let him = ObjInfo { health: HEALTH, max_health: HEALTH, ..general(2, 1) };
    // One hit of `zh` is the whole BO2 number `bo2`, and `ceil(100 / bo2)` of them kill him where
    // one fewer leaves him alive (the fake host's own sum, `health -= amount`).
    let in_bo2s_count = |what: &str, zh: f32| {
        let bo2 = crate::damage::to_bo2(&t, &him, zh);
        assert!(bo2 >= 1.0 && (bo2 - bo2.round()).abs() < 0.01 + bo2 * crate::damage::LETHAL_MARGIN * 2.0, "{what}: a whole BO2 hit, {bo2}");
        let n = (t.player_max_health / bo2.round()).ceil() as u32;
        let left = |k: u32| (0..k).fold(HEALTH, |h, _| h - zh);
        assert!(left(n) <= 0.0 && left(n - 1) > 0.0, "{what}: {n} hits of {} kill", bo2.round());
        bo2.round()
    };
    let blows_on = |host: &FakeHost, victim: u32| host.damage_log.borrow().iter().filter(|d| d.1 == victim).map(|d| d.2).collect::<Vec<f32>>();
    let blast_at = |def: &'static crate::weapons::WeaponDef, at: Vec3, wall: Option<ObjInfo>| {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(him);
        if let Some(w) = wall {
            host.add(w);
        }
        crate::projectiles::blast(&host, def, 1, at, crate::projectiles::Hit::NONE, 1.0, 1.0, &[]);
        blows_on(&host, 2)
    };

    // A landing: a 100% fall is 110 of BO2's 100 in one blow; a 49% one kills on the third.
    for (up, want) in [(400.0, Some(110.0)), (214.0, None)] {
        let host = FakeHost::default();
        host.add(ObjInfo { health: HEALTH, max_health: HEALTH, ..general(1, 0) });
        let mut sim = Sim::new();
        spawn(&mut sim, &host, 0, 1);
        super::tests::wait_until_ready(&mut sim, &host);
        let p = sim.players[0].as_mut().unwrap();
        p.body.pos.z = crate::units::inches(up);
        p.body.on_ground = false;
        super::tests::run(&mut sim, &host, 0, 1, FpsInput::default(), 90);
        let falls = blows_on(&host, 1);
        assert_eq!(falls.len(), 1, "{up}: one landing, {falls:?}");
        let bo2 = in_bo2s_count("a fall", falls[0]);
        if let Some(w) = want {
            assert_eq!(bo2, w, "a 100% fall");
        }
    }

    // A frag in the open and past a low wall that hides his middle and feet (two of five points:
    // `DamageVisibilityShare` each), the second also the thrown-back frag's blast.
    let frag = weapon("frag_grenade_mp");
    let at = Vec3::new(him.pos.x - crate::units::inches(60.0), 0.0, crate::units::inches(1.0));
    let open = blast_at(frag, at, None);
    assert_eq!(open.len(), 1, "{open:?}");
    let open_bo2 = in_bo2s_count("a frag in the open", open[0]);
    let eye = crate::cover::soldier_eye(&FakeHost::default(), &t, &him);
    let wall = ObjInfo { id: 9, flags: objflags::STRUCTURE, pos: Vec3::new(him.pos.x - crate::units::inches(20.0), 0.0, 0.0), geom_type: crate::host::geom::BOX, major_radius: 1.0, minor_radius: 30.0, height: eye * 0.6, ..Default::default() };
    let covered = blast_at(frag, Vec3::new(at.x, 0.0, eye * 0.5 + 0.01), Some(wall));
    assert_eq!(covered.len(), 1, "{covered:?}");
    assert!(in_bo2s_count("a frag past cover", covered[0]) < open_bo2, "the wall cut it");

    // The flashbang's, the concussion's and the Shock Charge's radius damage: BO2's 1.
    for id in [super::shock::FLASH_WEAPON, super::shock::CONCUSSION_WEAPON] {
        let def = weapon(id);
        assert!(def.explosion_inner_damage >= 1.0, "{id}: BO2's radius 1");
        let near = Vec3::new(him.pos.x - crate::units::inches(30.0), 0.0, crate::units::inches(40.0));
        let hits = blast_at(def, near, None);
        assert!(!hits.is_empty(), "{id}: {hits:?}");
        in_bo2s_count(id, hits[0]);
    }

    // The knife's nine rays: one blow, past his 100.
    let (host, mut sim) = duel(Loadout::test_kit(), 8.0, FACING);
    for o in host.objects.borrow_mut().iter_mut().filter(|o| o.id == 2) {
        (o.health, o.max_health) = (HEALTH, HEALTH);
    }
    step(&mut sim, &host, FpsInput { pressed: buttons::MELEE, ..Default::default() }, FACING, 1);
    step(&mut sim, &host, FpsInput::default(), FACING, 20);
    let knife = blows_on(&host, 2);
    assert_eq!(knife.len(), 1, "{knife:?}");
    assert!(in_bo2s_count("the knife", knife[0]) >= t.player_max_health && killed(&sim, FACING).is_some(), "one blow kills");

    // The Shock Charge's ticks (`proximity_dot_damage`): each BO2's 1 of his 100.
    let unit = crate::damage::general_zh(&t, t.proximity_dot_damage, HEALTH);
    assert_eq!(in_bo2s_count("a Shock Charge tick", unit), t.proximity_dot_damage.trunc());
}
