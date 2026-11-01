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
//!   (`:1058-1084`): the grenade, splash and explosive means of death, minus the tacticals — in the
//!   mod's data, a def whose effect is [`Effect::Explode`].
//! * **Tactical Mask** (`specialty_stunprotection`, `_weapons.gsc:1375-1376`): a concussion
//!   grenade's stun on the victim lasts x0.1, whoever threw it.

use crate::host::{damage_kind, disable_kind, LogicHost, ObjInfo};
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
}

impl Guard {
    /// General `general`'s guard from his class's specialties and the tuning pack.
    pub fn of(general: u32, a: &Arsenal) -> Guard {
        let class = a.class_specialties();
        let t = &a.tuning;
        Guard {
            general,
            blast: if class.has(Specialty::FlakJacket) { t.perk_flak_jacket * 0.01 } else { 1.0 },
            stun: if class.has(Specialty::StunProtection) { t.stun_protection_scale } else { 1.0 },
        }
    }
}

/// The host as one weapon's blast reaches it: damage and stuns pass through each victim's
/// [`Guard`]; everything else goes straight to the host.
pub struct Guarded<'a> {
    pub host: &'a dyn LogicHost,
    pub guards: &'a [Guard],
    /// The weapon that went off.
    pub def: &'a WeaponDef,
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
    fn damage(&self, shooter: u32, victim: u32, amount: f32, kind: i32) -> bool {
        let flak = kind == damage_kind::EXPLOSION && self.def.effect == Effect::Explode && victim != shooter && self.stuck_to != Some(victim);
        let scale = if flak { self.guard(victim).map_or(1.0, |g| g.blast) } else { 1.0 };
        self.host.damage(shooter, victim, amount * scale, kind)
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
        let mask = kind == disable_kind::PARALYZED && self.thrown && self.def.effect == Effect::Stun;
        let scale = if mask { self.guard(id).map_or(1.0, |g| g.stun) } else { 1.0 };
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
        let host = FakeHost::default();
        for id in [1, 2, 3] {
            host.add(general(id, id as i32 - 1));
        }
        let flak = crate::tuning::get().perk_flak_jacket * 0.01;
        let guards = [Guard { general: 2, blast: flak, stun: 1.0 }, Guard { general: 3, blast: 1.0, stun: 1.0 }];
        let frag = def(Effect::Explode);
        let g = |def: &'static WeaponDef, stuck_to: Option<u32>| Guarded { host: &host, guards: &guards, def, stuck_to, thrown: true };
        g(frag, None).damage(1, 2, 100.0, damage_kind::EXPLOSION);
        g(frag, None).damage(1, 3, 100.0, damage_kind::EXPLOSION);
        g(frag, None).damage(2, 2, 100.0, damage_kind::EXPLOSION);
        g(frag, Some(2)).damage(1, 2, 100.0, damage_kind::EXPLOSION);
        g(def(Effect::Stun), None).damage(1, 2, 100.0, damage_kind::EXPLOSION);
        g(frag, None).damage(1, 2, 100.0, damage_kind::SMALL_ARMS);
        let log: Vec<f32> = host.damage_log.borrow().iter().map(|d| d.2).collect();
        assert_eq!(log, [100.0 * flak, 100.0, 100.0, 100.0, 100.0, 100.0]);
    }

    /// Tactical Mask cuts a thrown concussion's stun to x0.1 whoever threw it
    /// (`_weapons.gsc:1375-1376`); placed equipment's stun answers to another perk.
    #[test]
    fn the_tactical_mask_cuts_anothers_concussion() {
        let host = FakeHost::default();
        let guards = [Guard { general: 2, blast: 1.0, stun: 0.1 }];
        let conc = def(Effect::Stun);
        Guarded { host: &host, guards: &guards, def: conc, stuck_to: None, thrown: true }.disable(2, disable_kind::PARALYZED, 4.0);
        Guarded { host: &host, guards: &guards, def: conc, stuck_to: None, thrown: false }.disable(2, disable_kind::PARALYZED, 4.0);
        let secs: Vec<f32> = host.disabled.borrow().iter().map(|d| d.2).collect();
        assert!((secs[0] - 0.4).abs() < 1e-6 && secs[1] == 4.0, "{secs:?}");
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
