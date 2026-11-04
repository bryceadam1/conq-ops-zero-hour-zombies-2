/*
** Conquer Ops - Zero Hour gameplay core: the victim's class perks against another General's blasts.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! BO2 applies the victim's protective perks where the damage is delivered, in the player damage
//! callback (`_globallogic_player.gsc:776` → `_class.gsc` `cac_modified_damage`), not where the
//! blast starts. So does the sim: a blast's damage and stun reach the host through [`Guarded`],
//! which knows the weapon that went off and scales what each General takes by his class:
//!
//! * **Flak Jacket** (`specialty_flakjacket`, `_class.gsc:1023-1032`): another player's explosive
//!   damage keeps `perk_flakJacket` percent (35). `cac_modified_damage` returns the damage unchanged
//!   when the victim is the attacker (`:957-958`), so a General's own blasts are not reduced; nor is
//!   a grenade stuck to him (`grenadestuck`, `:1132-1135`). Explosive is `isexplosivedamage`
//!   (`:1058-1084`): the grenade, splash and explosive means of death
//!   ([`crate::damage::MeansOfDeath::is_explosive_damage`]: not a rocket's direct hit, not an
//!   impact), minus the tacticals — in the mod's data, a def whose effect is [`Effect::Explode`].
//! * **Tactical Mask** (`specialty_stunprotection`, `_weapons.gsc:1375-1376`): a concussion
//!   grenade's stun on the victim lasts x0.1, whoever threw it; and (`specialty_proximityprotection`,
//!   `_proximity_grenade.gsc:118-124` `proximitygrenadedamageplayer`) a Shock Charge's zap does not
//!   reach him at all.

use crate::damage::MeansOfDeath;
use crate::host::{disable_kind, LogicHost, ObjInfo};
use crate::impacts::Surface;
use crate::math::Vec3;
use crate::weapons::{Arsenal, Effect, Specialty, WeaponDef};

/// One General's protection against another's blasts, from his class (the frame's snapshot, as
/// the shield bearers are).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guard {
    pub general: u32,
    /// The share of another player's explosive damage he takes (1 without Flak Jacket).
    pub blast: f32,
    /// The share of a concussion's stun he takes (1 without Tactical Mask).
    pub stun: f32,
    /// He is in first person: a stun shellshocks him instead of Zero Hour's paralysis
    /// ([`crate::shellshock`]), which would freeze the player outright.
    pub first_person: bool,
    /// `flashprotection` (Tactical Mask): a flashbang's duration x0.1 (`_flashgrenades.gsc:86-90`).
    pub flash_protected: bool,
    /// `proximityprotection` (Tactical Mask): no Shock Charge zap.
    pub proximity_protected: bool,
}

/// What a blast did to a first-person General that the sim turns into a shellshock
/// ([`crate::shellshock`]): the stun Zero Hour would have paralysed him with, or the explosive
/// damage he took (BO2's scale).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShockHit {
    pub victim: u32,
    /// The General whose weapon it was.
    pub owner: u32,
    pub def: &'static WeaponDef,
    /// Where it went off (`None` until the projectile's step reports it).
    pub at: Option<Vec3>,
    /// A thrown grenade (else placed equipment).
    pub thrown: bool,
    pub kind: ShockHitKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShockHitKind {
    /// The stun's seconds (BO2's concussion time, perks applied).
    Stun(f32),
    /// Explosive damage taken, BO2's scale.
    Damage(f32),
}

impl Guard {
    /// General `general`'s guard from his class's specialties and the tuning pack.
    pub fn of(general: u32, a: &Arsenal, first_person: bool) -> Guard {
        let class = a.class_specialties();
        let t = &a.tuning;
        Guard {
            general,
            blast: if class.has(Specialty::FlakJacket) { t.perk_flak_jacket * 0.01 } else { 1.0 },
            stun: if class.has(Specialty::StunProtection) { t.stun_protection_scale } else { 1.0 },
            first_person,
            flash_protected: class.has(Specialty::FlashProtection),
            proximity_protected: class.has(Specialty::ProximityProtection),
        }
    }
}

/// The host as one weapon's blast reaches it: damage and stuns pass through each victim's
/// [`Guard`]; everything else goes straight to the host.
pub struct Guarded<'a> {
    pub host: &'a dyn LogicHost,
    pub guards: &'a [Guard],
    /// The weapon that went off.
    pub def: &'static WeaponDef,
    /// The General whose weapon it is.
    pub owner: u32,
    /// Where it went off, when known before the blast (placed equipment).
    pub at: Option<Vec3>,
    /// The first-person Generals' shocks this blast caused, for the sim ([`ShockHit`]).
    pub hits: &'a std::cell::RefCell<Vec<ShockHit>>,
    /// What the grenade was stuck to (Flak Jacket does not cover a grenade stuck to its wearer).
    pub stuck_to: Option<u32>,
    /// A thrown grenade's stun (the concussion's, `_weapons.gsc:1360-1376`); placed equipment's
    /// (the Shock Charge) answers to a different perk (`specialty_proximityprotection`).
    pub thrown: bool,
}

impl Guarded<'_> {
    fn guard(&self, victim: u32) -> Option<&Guard> {
        self.guards.iter().find(|g| g.general == victim)
    }

    fn first_person(&self, victim: u32) -> bool {
        self.guard(victim).is_some_and(|g| g.first_person)
    }

    fn hit(&self, victim: u32, kind: ShockHitKind) {
        self.hits.borrow_mut().push(ShockHit { victim, owner: self.owner, def: self.def, at: self.at, thrown: self.thrown, kind });
    }
}

impl LogicHost for Guarded<'_> {
    fn log(&self, msg: &str) {
        self.host.log(msg)
    }
    fn ground_height(&self, x: f32, y: f32) -> f32 {
        self.host.ground_height(x, y)
    }
    fn is_walkable(&self, x: f32, y: f32) -> bool {
        self.host.is_walkable(x, y)
    }
    fn surface_at(&self, x: f32, y: f32) -> Surface {
        self.host.surface_at(x, y)
    }
    fn extent(&self) -> (Vec3, Vec3) {
        self.host.extent()
    }
    fn object(&self, id: u32, viewer: u32) -> Option<ObjInfo> {
        self.host.object(id, viewer)
    }
    fn query(&self, center: Vec3, radius: f32, viewer: u32, out: &mut Vec<ObjInfo>) {
        self.host.query(center, radius, viewer, out)
    }
    fn set_pose(&self, id: u32, pos: Vec3, yaw: f32, moving: bool) {
        self.host.set_pose(id, pos, yaw, moving)
    }
    fn set_controlled(&self, id: u32, controlled: bool) {
        self.host.set_controlled(id, controlled)
    }
    fn fire_fx(&self, shooter: u32, fx_weapon: &str, target: u32, pos: Vec3) {
        self.host.fire_fx(shooter, fx_weapon, target, pos)
    }
    fn damage(&self, shooter: u32, victim: u32, amount: f32, how: MeansOfDeath) -> bool {
        let flak = how.is_explosive_damage() && self.def.effect == Effect::Explode && victim != shooter && self.stuck_to != Some(victim);
        let scale = if flak { self.guard(victim).map_or(1.0, |g| g.blast) } else { 1.0 };
        // `_shellshock.gsc:19`: the explosive causes shake him (a rocket's direct hit too, not an
        // impact).
        if how.shellshocks() && self.first_person(victim) {
            self.hit(victim, ShockHitKind::Damage(amount * scale / crate::units::BO2_TO_ZH_DAMAGE));
        }
        self.host.damage(shooter, victim, amount * scale, how)
    }
    fn heal(&self, id: u32, amount: f32) {
        self.host.heal(id, amount)
    }
    fn spawn(&self, template: &str, owner: u32, pos: Vec3, yaw: f32) -> u32 {
        self.host.spawn(template, owner, pos, yaw)
    }
    fn destroy(&self, id: u32) {
        self.host.destroy(id)
    }
    fn disable(&self, id: u32, kind: i32, seconds: f32) {
        // A placed stun (the Shock Charge) skips a General with `proximityprotection`.
        if kind == disable_kind::PARALYZED && !self.thrown && self.def.effect == Effect::Stun && self.guard(id).is_some_and(|g| g.proximity_protected) {
            return;
        }
        let mask = kind == disable_kind::PARALYZED && self.thrown && self.def.effect == Effect::Stun;
        let scale = if mask { self.guard(id).map_or(1.0, |g| g.stun) } else { 1.0 };
        // A first-person General is shellshocked by the sim instead: Zero Hour's paralysis would
        // freeze the player outright.
        if kind == disable_kind::PARALYZED && self.first_person(id) {
            self.hit(id, ShockHitKind::Stun(seconds * scale));
            return;
        }
        self.host.disable(id, kind, seconds * scale)
    }
    fn fx(&self, name: &str, pos: Vec3) {
        self.host.fx(name, pos)
    }
    fn capture(&self, id: u32, new_owner: u32) {
        self.host.capture(id, new_owner)
    }
    fn smoke(&self, center: Vec3, radius: f32) {
        self.host.smoke(center, radius)
    }
    fn build_options(&self, general: u32) -> Vec<crate::building::BuildOption> {
        self.host.build_options(general)
    }
    fn can_place(&self, general: u32, template: u32, pos: Vec3, angle: f32) -> bool {
        self.host.can_place(general, template, pos, angle)
    }
    fn place(&self, general: u32, template: u32, pos: Vec3, angle: f32) -> u32 {
        self.host.place(general, template, pos, angle)
    }
    fn work_site(&self, general: u32, pos: Vec3, reach: f32) -> Option<crate::building::Site> {
        self.host.work_site(general, pos, reach)
    }
    fn work(&self, general: u32, site: u32, rate: f32) -> f32 {
        self.host.work(general, site, rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::LogicHost;
    use crate::input::FpsInput;
    use crate::sim::tests::{general, run, FakeHost};
    use crate::sim::Sim;
    use crate::weapons::{weapon, Perk};

    fn def(effect: Effect) -> &'static WeaponDef {
        Box::leak(Box::new(WeaponDef { id: "test_mp".into(), effect, ..Default::default() }))
    }

    /// Flak Jacket keeps `perk_flakJacket` percent of another player's explosive damage; his own
    /// blast, a grenade stuck to him and a non-explosive blast pass whole (`_class.gsc:957-958`,
    /// `:1023-1032`, `:1058-1084`, `:1132-1135`).
    #[test]
    fn flak_jacket_cuts_anothers_explosives_only() {
        let hits = std::cell::RefCell::new(Vec::new());
        let host = FakeHost::default();
        for id in [1, 2, 3] {
            host.add(general(id, id as i32 - 1));
        }
        let flak = crate::tuning::get().perk_flak_jacket * 0.01;
        let guards = [Guard { general: 2, blast: flak, stun: 1.0, first_person: false, flash_protected: false, proximity_protected: false }, Guard { general: 3, blast: 1.0, stun: 1.0, first_person: false, flash_protected: false, proximity_protected: false }];
        let frag = def(Effect::Explode);
        let g = |def: &'static WeaponDef, stuck_to: Option<u32>| Guarded { host: &host, guards: &guards, def, stuck_to, thrown: true, owner: 1, at: None, hits: &hits };
        use MeansOfDeath::*;
        g(frag, None).damage(1, 2, 100.0, GrenadeSplash);
        g(frag, None).damage(1, 3, 100.0, GrenadeSplash);
        g(frag, None).damage(2, 2, 100.0, GrenadeSplash);
        g(frag, Some(2)).damage(1, 2, 100.0, GrenadeSplash);
        g(def(Effect::Stun), None).damage(1, 2, 100.0, GrenadeSplash);
        g(frag, None).damage(1, 2, 100.0, RifleBullet);
        // A rocket's direct hit and a grenade's strike are not `isexplosivedamage`; its splash is.
        g(frag, None).damage(1, 2, 100.0, Projectile);
        g(frag, None).damage(1, 2, 100.0, Impact);
        g(frag, None).damage(1, 2, 100.0, ProjectileSplash);
        let log: Vec<f32> = host.damage_log.borrow().iter().map(|d| d.2).collect();
        assert_eq!(log, [100.0 * flak, 100.0, 100.0, 100.0, 100.0, 100.0, 100.0, 100.0, 100.0 * flak]);
    }

    /// Tactical Mask cuts a thrown concussion's stun to x0.1 whoever threw it
    /// (`_weapons.gsc:1375-1376`); placed equipment's stun answers to another perk.
    #[test]
    fn the_tactical_mask_cuts_anothers_concussion() {
        let hits = std::cell::RefCell::new(Vec::new());
        let host = FakeHost::default();
        let guards = [Guard { general: 2, blast: 1.0, stun: 0.1, first_person: false, flash_protected: false, proximity_protected: false }];
        let conc = def(Effect::Stun);
        Guarded { host: &host, guards: &guards, def: conc, stuck_to: None, thrown: true, owner: 1, at: None, hits: &hits }.disable(2, disable_kind::PARALYZED, 4.0);
        Guarded { host: &host, guards: &guards, def: conc, stuck_to: None, thrown: false, owner: 1, at: None, hits: &hits }.disable(2, disable_kind::PARALYZED, 4.0);
        let secs: Vec<f32> = host.disabled.borrow().iter().map(|d| d.2).collect();
        assert!((secs[0] - 0.4).abs() < 1e-6 && secs[1] == 4.0, "{secs:?}");
    }

    /// Tactical Mask's `proximityprotection`: a Shock Charge's zap (placed stun equipment) does not
    /// reach him at all, overhead or in first person (`_proximity_grenade.gsc:118-124`); without it,
    /// it does. A thrown concussion still stuns him (that is `stunprotection`'s x0.1).
    #[test]
    fn the_tactical_mask_stops_a_shock_charge() {
        let shock = def(Effect::Stun);
        for first_person in [false, true] {
            let hits = std::cell::RefCell::new(Vec::new());
            let host = FakeHost::default();
            let guards = [
                Guard { general: 2, blast: 1.0, stun: 1.0, first_person, flash_protected: false, proximity_protected: true },
                Guard { general: 3, blast: 1.0, stun: 1.0, first_person, flash_protected: false, proximity_protected: false },
            ];
            let placed = Guarded { host: &host, guards: &guards, def: shock, stuck_to: None, thrown: false, owner: 1, at: None, hits: &hits };
            placed.disable(2, disable_kind::PARALYZED, 1.5);
            placed.disable(3, disable_kind::PARALYZED, 1.5);
            Guarded { thrown: true, ..placed }.disable(2, disable_kind::PARALYZED, 4.0);
            let reached: Vec<u32> = if first_person { hits.borrow().iter().map(|h| h.victim).collect() } else { host.disabled.borrow().iter().map(|d| d.0).collect() };
            assert_eq!(reached, [3, 2], "first person {first_person}: only the unmasked General is zapped, the masked one still concussed");
        }
    }

    /// BO2 concusses the thrower too, his scale over half the radius (`_weapons.gsc:1364-1365`),
    /// and his Tactical Mask cuts his own stun as it does anyone's (`:1375-1376`).
    #[test]
    fn the_thrower_is_concussed_over_half_the_radius() {
        let conc = weapon("concussion_grenade_mp");
        let t = crate::tuning::get();
        let own_stun = |dist: f32, mask: bool| {
            let hits = std::cell::RefCell::new(Vec::new());
            let host = FakeHost::default();
            // `dist` from the blast to his origin, his feet (`_weapons.gsc:1366`, `self.origin`).
            host.add(crate::host::ObjInfo { pos: crate::math::Vec3::new(dist, 0.0, 0.0), ..general(1, 0) });
            let guards = [Guard { general: 1, blast: 1.0, stun: if mask { t.stun_protection_scale } else { 1.0 }, first_person: false, flash_protected: false, proximity_protected: false }];
            let g = Guarded { host: &host, guards: &guards, def: conc, stuck_to: None, thrown: true, owner: 1, at: None, hits: &hits };
            crate::projectiles::blast(&g, conc, 1, crate::math::Vec3::ZERO, crate::projectiles::Hit::NONE, 1.0, 1.0, &[]);
            let secs: Vec<f32> = host.disabled.borrow().iter().filter(|d| d.0 == 1).map(|d| d.2).collect();
            secs
        };
        let r = conc.explosion_radius;
        let expect = |dist: f32| t.concussion_time_min + t.concussion_time_scale * (1.0 - dist / (r * t.concussion_self_radius_scale)).clamp(0.0, 1.0);
        let quarter = own_stun(r * 0.25, false);
        assert_eq!(quarter.len(), 1, "the thrower is stunned");
        assert!((quarter[0] - expect(r * 0.25)).abs() < 1e-3, "{quarter:?} vs {}", expect(r * 0.25));
        assert!((expect(r * 0.25) - (t.concussion_time_min + t.concussion_time_scale * 0.5)).abs() < 1e-3, "half the scale at a quarter radius");
        let far = own_stun(r * 0.75, false);
        assert!((far[0] - t.concussion_time_min).abs() < 1e-3, "past half the radius: the edge time");
        let masked = own_stun(r * 0.25, true);
        assert!((masked[0] - expect(r * 0.25) * t.stun_protection_scale).abs() < 1e-3, "{masked:?}");
    }

    /// End to end: another General's frag going off beside a General whose class has Flak Jacket
    /// does `perk_flakJacket` percent of what it does to one without.
    #[test]
    fn a_flak_jacket_general_takes_less_from_anothers_frag() {
        let taken = |perk: bool| {
            let host = FakeHost::default();
            host.add(general(1, 0));
            host.add(crate::host::ObjInfo { pos: crate::math::Vec3::new(40.0, 0.0, 0.0), owner_player: 1, ..general(2, 1) });
            let mut sim = Sim::new();
            sim.take_control(&host, 0, 1);
            sim.take_control(&host, 1, 2);
            let a = &mut sim.players[1].as_mut().unwrap().arsenal;
            a.loadout.perks.retain(|p| *p != Perk::by_id("FlakJacket").unwrap());
            if perk {
                a.loadout.perks.push(Perk::by_id("FlakJacket").unwrap());
            }
            let frag = weapon("frag_grenade_mp");
            let mut p = crate::projectiles::Projectile::launch(frag, 1, crate::math::Vec3::new(30.0, 0.0, 0.0), crate::math::Vec3::new(0.0, 0.0, -1.0), 0.0, &host, 0.0);
            p.fuse = 0.0;
            p.resting = true;
            sim.projectiles.push((0, p));
            run(&mut sim, &host, 0, 1, FpsInput::default(), 1);
            let taken: f32 = host.damage_log.borrow().iter().filter(|d| d.1 == 2).map(|d| d.2).sum();
            taken
        };
        let (plain, flak) = (taken(false), taken(true));
        assert!(plain > 0.0);
        assert!((flak - plain * crate::tuning::get().perk_flak_jacket * 0.01).abs() < 1e-3, "{flak} vs {plain}");
    }
}
