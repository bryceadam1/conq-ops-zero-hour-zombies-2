/*
** Conquer Ops - Zero Hour gameplay core: which shellshock a first-person General gets from a blast.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! The GSC's `shellshock()` sources, applied to the first-person General from what the frame's
//! blasts did to him ([`ShockHit`], collected by `sim::guard`): the script picks the shock by the
//! weapon's name, as BO2's does (`onweapondamage`'s and `monitorflash`'s `switch ( sweapon )`).
//!
//! * the concussion grenade — its stun time (2 + 4 × scale, the thrower's over half the radius,
//!   Tactical Mask x0.1) as `concussion_grenade_mp` (`_weapons.gsc:1360-1380`);
//! * the flashbang — `flashbang` for the script's flash duration ([`rules::flash_duration`]);
//! * the EMP grenade — `flashbang` for 1 s on anyone it reaches (`_empgrenade.gsc:105`);
//! * the Shock Charge — `proximity_grenade` for 1.5 s, then `proximity_grenade_exit` once its
//!   damage ticks and the script's wait are over (`_proximity_grenade.gsc:186-206`); the zap's own
//!   damage and the hidden HUD are its [`Taser`];
//! * explosive damage over 10 — `frag_grenade_mp` for 0.5 s unless a flashbang's is running
//!   (`_shellshock.gsc:14-38`).

use super::guard::{ShockHit, ShockHitKind};
use super::{PlayerSim, Sim};
use crate::host::{objflags, LogicHost};
use crate::math::{dir_from_angles, Vec3};
use crate::shellshock::{names, rules};
use crate::weapons::{Effect, Specialty, WeaponDef};

/// The weapons the script names (`_weapons.gsc:1362`, `_flashgrenades.gsc`).
pub const CONCUSSION_WEAPON: &str = "concussion_grenade_mp";
pub const FLASH_WEAPON: &str = "flash_grenade_mp";

/// An EMP grenade gone off at `at`: every first-person General it reaches — its thrower too,
/// not a teammate, nor one with Hard Wired's `immuneemp` (`_empgrenade.gsc:41-60`) — takes its
/// flashbang shock.
pub(super) fn emp_hits(host: &dyn LogicHost, players: &[Option<PlayerSim>], def: &'static WeaponDef, owner: u32, at: Vec3) -> Vec<ShockHit> {
    let mut out = Vec::new();
    // The script's EMP reaches a player through the radius damage (`empexplosiondamageupdate`),
    // so only one the blast sees ([`crate::cover`], `FUN_005f0f00`).
    let cover = crate::cover::Blockers::near(host, at, def.explosion_radius.max(1.0) * 2.0, owner, &[]);
    for p in players.iter().flatten().filter(|p| p.is_first_person()) {
        if (p.body.pos - at).length() > def.explosion_radius || p.arsenal.class_specialties().has(Specialty::ImmuneEmp) {
            continue;
        }
        if cover.player_fraction(host, &p.arsenal.tuning, at, p.general, p.body.pos, p.body.eye_height) <= 0.0 {
            continue;
        }
        let ally = p.general != owner && host.object(p.general, owner).is_some_and(|o| o.has(objflags::ALLY));
        if ally {
            continue;
        }
        let secs = p.arsenal.tuning.emp_shock_duration;
        out.push(ShockHit { victim: p.general, owner, def, at: Some(at), thrown: true, kind: ShockHitKind::Stun(secs) });
    }
    out
}

impl Sim {
    /// Turn the frame's [`ShockHit`]s into shellshocks on the first-person Generals.
    pub(super) fn apply_shock_hits(&mut self, host: &dyn LogicHost) {
        let hits = self.equipment.shock_hits.take();
        for h in &hits {
            // A tactical's hit is an attacker's for the assists (`score`).
            if matches!(h.kind, ShockHitKind::Stun(_)) && h.owner != h.victim {
                self.score_shock(host, h.owner, h.victim, h.def, h.thrown);
            }
        }
        for h in hits {
            let Some(p) = self.players.iter_mut().flatten().find(|p| p.general == h.victim && p.is_first_person()) else { continue };
            apply(p, &h);
        }
    }
}

/// One hit's shellshock on `p`.
fn apply(p: &mut PlayerSim, h: &ShockHit) {
    let t = p.arsenal.tuning.clone();
    match h.kind {
        ShockHitKind::Damage(bo2) => {
            if p.shock.name() != Some(names::FLASHBANG) {
                if let Some(secs) = rules::blast_shock(&t, bo2) {
                    p.shock.play(names::FRAG, secs);
                }
            }
        }
        ShockHitKind::Stun(secs) if h.def.effect == Effect::Emp => p.shock.play(names::FLASHBANG, secs),
        ShockHitKind::Stun(_) if h.def.id == FLASH_WEAPON => {
            let Some(at) = h.at else { return };
            let eye = p.body.eye();
            let to = at - eye;
            let share = rules::flash_distance_share(to.length(), h.def.explosion_radius_min, h.def.explosion_radius);
            let dot = dir_from_angles(p.yaw, p.pitch).dot(to.normalized_or_zero());
            let protected = p.arsenal.class_specialties().has(Specialty::FlashProtection);
            if let Some(secs) = rules::flash_duration(&t, share, dot, h.owner == p.general, protected) {
                p.shock.play(names::FLASHBANG, secs);
            }
        }
        ShockHitKind::Stun(_) if h.def.equipment_kind == crate::weapons::Kind::Shock => {
            // The Shock Charge: the zap, then the exit after its damage ticks and the wait.
            p.shock.play(names::PROXIMITY, t.proximity_shock_duration);
            let exit_after = t.proximity_dot_time * t.proximity_dot_instances + t.proximity_exit_wait;
            p.shock.queue(exit_after, names::PROXIMITY_EXIT, t.proximity_exit_duration);
            // A new zap starts the ticks over (`notify( "proximityGrenadeDamageStart" )` ends the
            // last one's thread, `:161-162`).
            p.taser = Some(Taser::new(h.owner, h.def, h.at.unwrap_or(p.body.pos), &t));
        }
        ShockHitKind::Stun(secs) if h.def.id == CONCUSSION_WEAPON => p.shock.play(names::CONCUSSION, secs),
        // Any other thrown stun has no shellshock in the script.
        ShockHitKind::Stun(_) => {}
    }
}

/// A Shock Charge's zap running on a first-person General (`damageplayerinradius`,
/// `_proximity_grenade.gsc:159-207`): `scr_proximityGrenadeDOTDamageInstances` ticks of
/// `scr_proximityGrenadeDOTDamageAmount` (BO2's 100-HP scale), `MOD_GAS`, one every
/// `scr_proximityGrenadeDOTDamageTime` from the owner at the charge; his HUD hidden
/// (`setclientuivisibilityflag( "hud_visible", 0 )`, `:191`) from the zap until the exit shock
/// (`:206-207`, after the ticks and `wait 0.85`), or his death (`watch_death`, `:236-241`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Taser {
    /// The charge's owner (the damage is his) and its weapon.
    pub owner: u32,
    pub def: &'static WeaponDef,
    /// Where the charge went off (each tick's `position`).
    pub at: Vec3,
    pub ticks_left: u8,
    /// Seconds to the next tick.
    pub next: f32,
    /// Seconds the HUD stays hidden.
    pub hud: f32,
}

crate::statehash::state_hash!(struct Taser { owner, def, at, ticks_left, next, hud });

impl Taser {
    pub fn new(owner: u32, def: &'static WeaponDef, at: Vec3, t: &crate::tuning::Tuning) -> Taser {
        let ticks = t.proximity_dot_instances.clamp(0.0, 255.0) as u8;
        Taser { owner, def, at, ticks_left: ticks, next: t.proximity_dot_time, hud: t.proximity_dot_time * f32::from(ticks) + t.proximity_exit_wait }
    }

    /// Whether the HUD is hidden now.
    pub fn hides_hud(&self) -> bool {
        self.hud > 1e-4
    }
}

/// One logic frame of `p`'s zap: a tick's damage when it is due, and the HUD's wait. The tick's
/// weapon is the script's `proximity_grenade_aoe_mp` when the table has it, else the charge's.
pub(super) fn step_taser(host: &dyn LogicHost, p: &mut PlayerSim) {
    let Some(mut z) = p.taser else { return };
    let dt = crate::units::LOGIC_DT;
    z.hud -= dt;
    if z.ticks_left > 0 {
        z.next -= dt;
        if z.next <= 1e-4 {
            let t = &p.arsenal.tuning;
            let def = crate::weapons::catalog().get(AOE_WEAPON).unwrap_or(z.def);
            let blow = crate::damage::Blow::from_to(crate::damage::MeansOfDeath::Gas, def, z.at, p.body.pos);
            // A tick is BO2 health on BO2's player: his share of `playerMaxHealth`.
            host.damage(z.owner, p.general, crate::damage::general_zh(t, t.proximity_dot_damage, p.max_health), blow);
            z.ticks_left -= 1;
            z.next += t.proximity_dot_time;
        }
    }
    p.taser = (z.ticks_left > 0 || z.hides_hud()).then_some(z);
}

/// The weapon the Shock Charge's ticks are dealt as (`_proximity_grenade.gsc:202`).
pub const AOE_WEAPON: &str = "proximity_grenade_aoe_mp";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::FpsInput;
    use crate::projectiles::Projectile;
    use crate::sim::tests::{general, run, FakeHost};
    use crate::weapons::weapon;

    /// Two Generals, both in first person (players 0 and 1 → Generals 1 and 2), facing +x; General
    /// 2 stands `dist` ahead of General 1.
    fn duel(dist: f32) -> (FakeHost, Sim) {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(crate::host::ObjInfo { pos: Vec3::new(dist, 0.0, 0.0), ..general(2, 1) });
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        sim.take_control(&host, 1, 2);
        (host, sim)
    }

    fn go_off(sim: &mut Sim, host: &FakeHost, id: &str, at: Vec3) {
        go_off_def(sim, host, weapon(id), at);
    }

    fn go_off_def(sim: &mut Sim, host: &FakeHost, def: &'static WeaponDef, at: Vec3) {
        let mut p = Projectile::launch(def, 1, at, Vec3::new(0.0, 0.0, -1.0), 0.0, host, 0.0);
        (p.fuse, p.resting) = (0.0, true);
        sim.projectiles.push((0, p));
        run(sim, host, 0, 1, FpsInput::default(), 1);
    }

    /// The concussion grenade shellshocks the first-person General it reaches — never Zero Hour's
    /// paralysis, which would freeze the player — for its stun time, and slows his run.
    #[test]
    fn a_concussion_shellshocks_a_first_person_general_instead_of_freezing_him() {
        let (host, mut sim) = duel(40.0);
        go_off(&mut sim, &host, CONCUSSION_WEAPON, Vec3::new(40.0, 0.0, 0.0));
        assert!(host.disabled.borrow().iter().all(|d| d.0 != 2 && d.0 != 1), "no paralysis on a first-person General: {:?}", host.disabled.borrow());
        let p = sim.player(1).unwrap();
        assert_eq!(p.shock.name(), Some(names::CONCUSSION));
        let left = p.shock.active.as_ref().unwrap().left;
        let t = crate::tuning::get();
        assert!(left > t.concussion_time_min && left <= t.concussion_time_min + t.concussion_time_scale, "{left}");
        assert!(p.shock.movement() < 1.0);
        // The thrower, 40 units off, inside half the radius: his own, shorter.
        let own = sim.player(0).unwrap().shock.active.as_ref().map(|a| a.left);
        assert!(own.is_some_and(|o| o < left), "{own:?} vs {left}");
    }

    /// Under RTS control a General keeps Zero Hour's stun (the closest Zero Hour has).
    #[test]
    fn an_overhead_general_keeps_zero_hours_stun() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(crate::host::ObjInfo { pos: Vec3::new(40.0, 0.0, 0.0), ..general(2, 1) });
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        go_off(&mut sim, &host, CONCUSSION_WEAPON, Vec3::new(40.0, 0.0, 0.0));
        assert!(host.disabled.borrow().iter().any(|d| d.0 == 2), "the unclaimed General 2 is paralysed");
    }

    /// The flashbang whites out the General looking at it for the script's duration; looking away
    /// halves it.
    #[test]
    fn the_flashbang_depends_on_where_he_looks() {
        // The stand-in table has no flashbang: BO2's, a stun grenade with a 200" inner radius
        // in a 512" one (the converted table's numbers).
        let flash: &'static WeaponDef = Box::leak(Box::new(WeaponDef {
            id: FLASH_WEAPON.into(),
            explosion_radius: crate::units::inches(512.0),
            explosion_radius_min: crate::units::inches(200.0),
            ..weapon(CONCUSSION_WEAPON).clone()
        }));
        let at = Vec3::new(20.0, 0.0, 6.0);
        let (host, mut sim) = duel(40.0);
        // General 2 faces -x (towards the grenade), General 1 faces +x (also towards it).
        let p2 = sim.players[1].as_mut().unwrap();
        (p2.yaw, p2.input.yaw) = (std::f32::consts::PI, std::f32::consts::PI);
        go_off_def(&mut sim, &host, flash, at);
        let p2 = sim.player(1).unwrap();
        assert_eq!(p2.shock.name(), Some(names::FLASHBANG));
        assert!(p2.shock.screen().white > 0.9, "a whiteout: {:?} {:?}", p2.shock.screen(), p2.shock.active);
        let facing = p2.shock.active.as_ref().unwrap().left;
        let (host, mut sim) = duel(40.0);
        go_off_def(&mut sim, &host, flash, at); // General 2 faces +x: away from it
        let away = sim.player(1).unwrap().shock.active.as_ref().map_or(0.0, |a| a.left);
        assert!(away < facing, "{away} vs {facing}");
    }

    /// Hard Wired (`immuneemp`): an EMP grenade's flashbang shock skips him (`_empgrenade.gsc:43`).
    #[test]
    fn hard_wired_ignores_the_emp_grenade() {
        let emp: &'static WeaponDef = Box::leak(Box::new(WeaponDef { id: "emp_grenade_mp".into(), effect: Effect::Emp, explosion_radius: 100.0, ..weapon(CONCUSSION_WEAPON).clone() }));
        let (host, mut sim) = duel(40.0);
        let shocked = |sim: &Sim| sim.players.iter().flatten().map(|p| p.general).filter(|g| emp_hits(&host, &sim.players, emp, 1, Vec3::new(40.0, 0.0, 0.0)).iter().any(|h| h.victim == *g)).collect::<Vec<_>>();
        assert_eq!(shocked(&sim), [1, 2], "both in reach, the thrower too");
        sim.players[1].as_mut().unwrap().arsenal.loadout.perks.push(crate::weapons::Perk::HARD_WIRED);
        assert_eq!(shocked(&sim), [1], "Hard Wired keeps it off him");
    }

    /// A frag's blast shakes the first-person General (`frag_grenade_mp`, 0.5 s).
    #[test]
    fn a_blast_shakes_him() {
        let (host, mut sim) = duel(30.0);
        go_off(&mut sim, &host, "frag_grenade_mp", Vec3::new(30.0, 0.0, 0.0));
        let p = sim.player(1).unwrap();
        assert_eq!(p.shock.name(), Some(names::FRAG));
        assert!(p.shock.active.as_ref().unwrap().left <= crate::tuning::get().blast_shock_duration);
    }

    /// The Shock Charge's zap on a first-person General is BO2's `damageplayerinradius`
    /// (`_proximity_grenade.gsc:159-207`): the charge's own 1-point radius damage (no more `> 10`
    /// gate that left it harmless), then four `MOD_GAS` ticks of 1 every 0.15 s from its owner,
    /// the HUD hidden until the exit shock (0.6 + 0.85 s after the zap); a new zap starts over.
    #[test]
    fn the_shock_charge_zaps_four_ticks_and_hides_the_hud() {
        let charge: &'static WeaponDef = Box::leak(Box::new(WeaponDef {
            id: "proximity_grenade_mp".into(),
            effect: Effect::Stun,
            equipment_kind: crate::weapons::Kind::Shock,
            explosion_radius: crate::units::inches(200.0),
            explosion_inner_damage: 1.0,
            explosion_outer_damage: 1.0,
            ..weapon(CONCUSSION_WEAPON).clone()
        }));
        let t = crate::tuning::get();
        let (host, mut sim) = duel(20.0);
        let at = Vec3::new(20.0, 0.0, 0.0);
        let mut p = crate::equipment::Placed::new(charge, 0, 1, 0, at, 0.0, None);
        p.triggered = Some(0.0);
        sim.equipment.add(p);
        let hits = |host: &FakeHost| host.damage_log.borrow().iter().filter(|d| d.0 == 1 && d.1 == 2).map(|d| d.2).collect::<Vec<f32>>();
        run(&mut sim, &host, 0, 1, FpsInput::default(), 1);
        // BO2's 1 on BO2's player: his share of `playerMaxHealth` (`damage::general_zh`).
        let max = host.objects.borrow().iter().find(|o| o.id == 2).unwrap().max_health;
        let unit = crate::damage::general_zh(&t, 1.0, max);
        assert_eq!(hits(&host), [unit], "the charge's own radius damage, BO2's 1");
        let z = sim.player(1).unwrap().taser.expect("zapped");
        assert!(z.hides_hud() && z.ticks_left == 4 && z.owner == 1, "{z:?}");
        assert_eq!(sim.player(1).unwrap().shock.name(), Some(names::PROXIMITY));
        let frames = |s: f32| (s / crate::units::LOGIC_DT).round() as u32;
        run(&mut sim, &host, 0, 1, FpsInput::default(), frames(t.proximity_dot_time * 4.0) + 1);
        assert_eq!(hits(&host), [unit; 5], "and four ticks of 1");
        assert!(host.damage_how.borrow().iter().filter(|d| d.0 == 2).skip(1).all(|d| d.1 == crate::damage::MeansOfDeath::Gas), "the ticks are MOD_GAS");
        assert!(sim.player(1).unwrap().taser.is_some_and(|z| z.hides_hud()), "the HUD waits out the 0.85 s");
        run(&mut sim, &host, 0, 1, FpsInput::default(), frames(t.proximity_exit_wait) + 1);
        assert_eq!(sim.player(1).unwrap().taser, None, "the HUD is back with the exit shock");
    }

    /// A Shock Charge thrown into an enemy player goes off at once (`watchproximitygrenadehitplayer`,
    /// `_proximity_grenade.gsc:127-145`); into Zero Hour infantry it bounces on and lands.
    #[test]
    fn a_shock_charge_that_strikes_an_enemy_player_goes_off() {
        let charge: &'static WeaponDef = Box::leak(Box::new(WeaponDef {
            id: "proximity_grenade_mp".into(),
            effect: Effect::Stun,
            equipment_kind: crate::weapons::Kind::Shock,
            explosion_radius: crate::units::inches(200.0),
            explosion_inner_damage: 1.0,
            explosion_outer_damage: 1.0,
            ..weapon(CONCUSSION_WEAPON).clone()
        }));
        for (target_is_player, goes_off) in [(true, true), (false, false)] {
            let host = FakeHost::default();
            host.add(general(1, 0));
            let mut target = if target_is_player { general(2, 1) } else { crate::sim::tests::soldier(2, 1, 0.0) };
            target.pos = Vec3::new(10.0, 0.0, 0.0);
            host.add(target);
            let mut p = Projectile::launch(charge, 1, Vec3::new(0.0, 0.0, 3.0), Vec3::new(1.0, 0.0, 0.0), 0.0, &host, 0.0);
            p.vel = Vec3::new(60.0, 0.0, 0.0);
            let players = if target_is_player { vec![1, 2] } else { vec![1] };
            let mut out = None;
            for _ in 0..10 {
                if let Some(o) = p.step_shielded(&host, &[], &players) {
                    out = Some(o);
                    break;
                }
            }
            assert_eq!(matches!(out, Some(crate::projectiles::Outcome::Exploded { .. })), goes_off, "player {target_is_player}: {out:?}");
        }
    }
}
