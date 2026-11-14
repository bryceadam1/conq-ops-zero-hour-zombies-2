/*
** Conquer Ops - Zero Hour gameplay core: BO2 Zombies' Tombstone.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! Tombstone's grave (`_zm_tombstone.gsc`, `zombies.ini` `[Tombstone]`): a survivor going down
//! with the perk leaves what he carries in a grave where he fell (`tombstone_laststand`,
//! `tombstone_spawn`). Revived, the grave goes. Bled out, it waits for him to come back at the next
//! round; then he alone may take it within `GrabRange`, from `GrabDelay` after for `Life` and its
//! `BlinkTime` (`tombstone_grab`, `tombstone_timeout`): in a game with others his guns (full clips,
//! the stock they had) and grenades come back, in any game his perks, Quick Revive not alone
//! (`tombstone_give`).

use super::*;
use crate::zombies::Tombstone;

/// What a grave keeps (`tombstone_laststand`).
#[derive(Clone, Debug, PartialEq)]
pub struct Grave {
    /// His Zero Hour player: the grave is his whichever General he comes back as.
    pub player: i32,
    /// The General that went down (the grave goes if he is revived).
    pub general: u32,
    pub pos: Vec3,
    pub object: u32,
    /// His guns (id, stock) and which was up; his lethal count; his perks (rows of `Perks`).
    pub weapons: Vec<(String, u16)>,
    pub current: Option<u8>,
    pub grenades: u16,
    pub perks: Vec<u8>,
    /// Seconds since he came back (`None`: he is not back yet, or still down).
    pub out: Option<f32>,
}

crate::statehash::state_hash!(struct Grave { player, general, pos, object, weapons, current, grenades, perks, out });

impl Sim {
    /// `tombstone_laststand` and `tombstone_spawn`: survivor `general` of `player` goes down with
    /// Tombstone, carrying `arsenal` and `perks` (his perks before he lost them).
    pub(super) fn dig_grave(&mut self, host: &dyn LogicHost, general: u32, player: i32, pos: Vec3, arsenal: &crate::weapons::Arsenal, perks: &[u8]) {
        let rules = Tombstone::get();
        let tombstone = crate::zombies::Perks::get().index("specialty_scavenger").map(|i| i as u8);
        let weapons: Vec<(String, u16)> = arsenal.weapons.iter().filter(|w| w.main.class.is_slot_weapon()).map(|w| (w.main.id.clone(), w.reserve)).collect();
        let current = weapons.iter().position(|(id, _)| *id == arsenal.current().main.id).map(|i| i as u8);
        let at = pos + Vec3::new(0.0, 0.0, crate::units::inches(rules.height));
        let owner = self.rounds.spawners.first().copied().unwrap_or(0);
        let object = host.spawn(&crate::names::tombstone_object(), owner, at, 0.0);
        self.rounds.graves.retain(|g| g.player != player);
        self.rounds.graves.push(Grave {
            player,
            general,
            pos: at,
            object,
            weapons,
            current,
            grenades: arsenal.lethal.count,
            perks: perks.iter().copied().filter(|&k| Some(k) != tombstone).collect(),
            out: None,
        });
        host.log(&format!("zombies: General {general} leaves a tombstone"));
    }

    /// The graves' frame: a revived owner's goes; an owner back (a living survivor of his not
    /// down) starts its clock; he takes it within reach; it times out.
    pub(super) fn step_graves(&mut self, host: &dyn LogicHost) {
        let rules = Tombstone::get();
        let mut i = 0;
        while i < self.rounds.graves.len() {
            let g = self.rounds.graves[i].clone();
            let down = self.is_downed(g.general);
            let alive = host.object(g.general, g.general).is_some_and(|o| !o.has(objflags::DEAD));
            if g.out.is_none() && alive && !down {
                // Revived (`tombstone_spawn`'s `player_revived`): the grave goes.
                self.remove_grave(host, i);
                continue;
            }
            // Back: his survivor now (another General than the one that fell).
            let back = self.players.iter().flatten().find(|p| p.survivor && p.general != g.general && host.object(p.general, p.general).is_some_and(|o| o.owner_player == g.player && !o.has(objflags::DEAD))).map(|p| p.general);
            if g.out.is_none() && back.is_some() {
                self.rounds.graves[i].out = Some(0.0);
            }
            if let Some(t) = self.rounds.graves[i].out {
                let t = t + LOGIC_DT;
                if t >= rules.life + rules.blink_time {
                    host.log(&format!("zombies: player {}'s tombstone timed out", g.player));
                    self.remove_grave(host, i);
                    continue;
                }
                self.rounds.graves[i].out = Some(t);
                if let Some(general) = back.filter(|_| t >= rules.grab_delay) {
                    let near = self.players.iter().flatten().find(|p| p.general == general).is_some_and(|p| (p.body.pos - g.pos).length() < crate::units::inches(rules.grab_range));
                    if near {
                        self.remove_grave(host, i);
                        self.give_grave(host, general, &g);
                        continue;
                    }
                }
            }
            i += 1;
        }
    }

    fn remove_grave(&mut self, host: &dyn LogicHost, i: usize) {
        let g = self.rounds.graves.remove(i);
        if g.object != 0 {
            host.destroy(g.object);
        }
    }

    /// `tombstone_give`: grave `g` back to his survivor `general`.
    fn give_grave(&mut self, host: &dyn LogicHost, general: u32, g: &Grave) {
        let Some(idx) = self.seat_of(general) else { return };
        let Some(mut p) = self.players[idx].take() else { return };
        let solo = self.rounds.players.len() <= 1;
        if !solo {
            let defs: Vec<(&'static crate::weapons::WeaponDef, u16)> = g.weapons.iter().filter_map(|(id, stock)| crate::weapons::weapon_or_none(id).map(|d| (d, *stock))).collect();
            let primaries = p.arsenal.weapons.iter().filter(|w| w.main.class.is_slot_weapon()).count();
            if defs.len() > 1 || primaries > 1 {
                p.arsenal.weapons.retain(|w| !w.main.class.is_slot_weapon());
            }
            for (k, (def, stock)) in defs.iter().enumerate() {
                if p.arsenal.weapons.iter().any(|w| w.main.id == def.id) {
                    continue;
                }
                let mut w = crate::weapons::WeaponState::new(def);
                w.ammo = def.clip;
                w.reserve = *stock;
                w.raised_once = true;
                p.arsenal.weapons.push(w);
                if g.current == Some(k as u8) {
                    p.arsenal.active = p.arsenal.weapons.len() - 1;
                }
            }
            if p.arsenal.active >= p.arsenal.weapons.len() {
                p.arsenal.active = 0;
            }
            if g.grenades > 0 && !p.arsenal.lethal.def.id.is_empty() {
                p.arsenal.lethal.count = g.grenades;
            }
        }
        let qr = crate::zombies::Perks::get().index("specialty_quickrevive").map(|i| i as u8);
        for &k in &g.perks {
            if solo && Some(k) == qr {
                continue;
            }
            self.grant_perk(host, &mut p, k);
        }
        self.players[idx] = Some(p);
        host.log(&format!("zombies: General {general} took his tombstone back"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::fakehost::{general, FakeHost};

    fn co_op() -> (Sim, FakeHost) {
        let host = FakeHost::default();
        host.spawn_objects.set(true);
        host.add(ObjInfo { flags: objflags::INFANTRY | objflags::GENERAL | objflags::SURVIVOR, ..general(1, 0) });
        host.add(ObjInfo { id: 2, flags: objflags::INFANTRY | objflags::GENERAL | objflags::SURVIVOR, pos: Vec3::new(500.0, 0.0, 0.0), ..general(2, 1) });
        let mut sim = Sim::new();
        sim.rounds.zombies_map = true;
        sim.take_control(&host, 0, 1);
        sim.take_control(&host, 1, 2);
        sim.rounds.players = vec![0, 1];
        let perks = crate::zombies::Perks::get();
        let ts = perks.index("specialty_scavenger").unwrap() as u8;
        let rof = perks.index("specialty_rof").unwrap() as u8;
        sim.rounds.perks.owned.push((1, vec![ts, rof]));
        (sim, host)
    }

    fn step(sim: &mut Sim, host: &FakeHost, n: u32) {
        for _ in 0..n {
            let f = sim.frame + 1;
            sim.step(host, f);
        }
    }

    #[test]
    fn a_revive_takes_the_grave_away() {
        let (mut sim, host) = co_op();
        step(&mut sim, &host, 1);
        sim.go_down(&host, 1);
        assert_eq!(sim.rounds.graves.len(), 1);
        assert!(host.spawned.borrow().iter().any(|(_, t)| *t == crate::names::tombstone_object()));
        let rof = crate::zombies::Perks::get().index("specialty_rof").unwrap() as u8;
        assert_eq!(sim.rounds.graves[0].perks, vec![rof], "his other perks, not Tombstone itself");
        let d = sim.rounds.downed.remove(0);
        sim.rounds.downed.clear();
        let _ = d;
        step(&mut sim, &host, 2);
        assert!(sim.rounds.graves.is_empty(), "up again: the grave goes");
    }

    #[test]
    fn bled_out_and_back_he_takes_his_perks_from_it() {
        let (mut sim, host) = co_op();
        step(&mut sim, &host, 1);
        sim.go_down(&host, 1);
        let at = sim.rounds.graves[0].pos;
        host.kill(1);
        step(&mut sim, &host, 2);
        assert_eq!(sim.rounds.graves.len(), 1, "dead: it waits for him");
        // Back at the next round as General 3, over the grave.
        host.add(ObjInfo { id: 3, flags: objflags::INFANTRY | objflags::GENERAL | objflags::SURVIVOR, pos: Vec3::new(at.x, at.y, 0.0), ..general(3, 0) });
        sim.take_control(&host, 0, 3);
        step(&mut sim, &host, (1.2 / LOGIC_DT) as u32);
        assert!(sim.rounds.graves.is_empty(), "taken");
        assert!(sim.rounds.perks.has(3, "specialty_rof"), "his perk back");
        assert!(!sim.rounds.perks.has(3, "specialty_scavenger"));
    }
}
