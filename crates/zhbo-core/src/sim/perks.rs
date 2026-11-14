/*
** Conquer Ops - Zero Hour gameplay core: BO2 Zombies' perks.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! BO2 Zombies' perk machines (`_zm_perks.gsc`, `zombies.ini` `[Perks]`) on a Zombies map. The
//! round engine stands each of TranZit classic's machines on the wall-buy ring around every start
//! (the mod's own map; every machine is on, the map having no power switch).
//!
//! * **Buying** (`vending_trigger_think`): the use key on a machine in his use list, its price, while
//!   he has fewer than `PerkLimit` and not that one. Quick Revive costs its solo price alone, at
//!   most `SoloLives` times. (BO2's drinking bottle is not ported: the perk comes at once.)
//! * **What they do:** Speed Cola, Double Tap and Stamin-Up are the engine's specialties
//!   (`fastreload`, `rof`, `longersprint`), which the arsenal already plays; Jugger-Nog makes his
//!   health `JuggHealth` of the player's 100 (the damage hook scales each blow,
//!   [`crate::laststand`]); Quick Revive halves his revives of others, and alone it revives him
//!   after `SoloReviveTime` instead of the game ending (`wait_and_revive`).
//! * **Lost** when he goes down (`perk_think`).

use super::*;
use crate::zombies::Perks;

/// One perk machine on the map.
#[derive(Clone, Debug, PartialEq)]
pub struct MachineAt {
    /// Its row in [`Perks`]'s list.
    pub perk: u8,
    pub pos: Vec3,
    pub yaw: f32,
    pub object: u32,
}

crate::statehash::state_hash!(struct MachineAt { perk, pos, yaw, object });

/// The perks' state: the machines, who owns what (in the order bought), and the solo Quick Revives
/// given (`level.solo_lives_given`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PerkState {
    pub machines: Vec<MachineAt>,
    pub owned: Vec<(u32, Vec<u8>)>,
    pub solo_lives_given: u32,
}

crate::statehash::state_hash!(struct PerkState { machines, owned, solo_lives_given });

impl PerkState {
    /// The perks `general` owns, in the order bought.
    pub fn of(&self, general: u32) -> &[u8] {
        self.owned.iter().find(|(g, _)| *g == general).map_or(&[], |(_, p)| p.as_slice())
    }

    /// Whether `general` owns the perk of specialty `specialty`.
    pub fn has(&self, general: u32, specialty: &str) -> bool {
        Perks::get().index(specialty).is_some_and(|i| self.of(general).contains(&(i as u8)))
    }
}

/// The arsenal specialty a perk is (the engine's; Jugger-Nog and Quick Revive are the script's).
fn specialty_of(name: &str) -> Option<crate::weapons::Specialty> {
    use crate::weapons::Specialty;
    match name {
        "specialty_fastreload" => Some(Specialty::FastReload),
        "specialty_rof" => Some(Specialty::Rof),
        "specialty_longersprint" => Some(Specialty::LongerSprint),
        _ => None,
    }
}

impl Sim {
    /// Stand the machines (the round engine's start): around each start on the wall-buy ring, in
    /// the half steps after the box's.
    pub(super) fn place_perk_machines(&mut self, host: &dyn LogicHost) {
        let w = crate::zombies::WallBuys::get();
        let step = std::f32::consts::TAU / w.buys.len().max(1) as f32;
        let starts: Vec<Vec3> = self.living_survivors(host).iter().map(|o| o.pos).collect();
        if starts.is_empty() || !self.rounds.perks.machines.is_empty() {
            return;
        }
        let owner = self.rounds.spawners.first().copied().unwrap_or(0);
        for start in starts {
            for (k, perk) in Perks::get().perks.iter().enumerate() {
                let a = step * (k as f32 + 1.5);
                let (s, c) = crate::math::det::sin_cos(a);
                let pos = start + Vec3::new(c, s, 0.0) * w.ring;
                let pos = Vec3::new(pos.x, pos.y, host.ground_height(pos.x, pos.y));
                let yaw = crate::math::wrap_angle(a + std::f32::consts::PI);
                let object = host.spawn(&crate::names::perk_machine_object(&perk.specialty), owner, pos, yaw);
                self.rounds.perks.machines.push(MachineAt { perk: k as u8, pos, yaw, object });
            }
        }
        host.log(&format!("zombies: {} perk machines stood", self.rounds.perks.machines.len()));
    }

    /// The machine in `p`'s use list now (as a dropped weapon's), if any.
    pub fn perk_machine_target(&self, p: &PlayerSim) -> Option<usize> {
        if !p.survivor || self.is_downed(p.general) {
            return None;
        }
        let t = crate::tuning::get();
        let eye = p.body.eye();
        self.rounds.perks.machines.iter().enumerate().map(|(i, m)| (i, (m.pos + Vec3::new(0.0, 0.0, crate::units::inches(t.dropped_item_half_size)) - eye).length())).filter(|(_, d)| d / crate::units::inches(1.0) <= t.player_use_radius).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i)
    }

    /// A machine's price and prompt key for `p` (`reset_vending_hint_string`), or `None` when he
    /// may not buy from it (he has it; solo Quick Revive used up).
    pub fn perk_offer(&self, p: &PlayerSim, machine: usize) -> Option<(u32, &'static str)> {
        let perks = Perks::get();
        let m = self.rounds.perks.machines.get(machine)?;
        let perk = perks.perks.get(m.perk as usize)?;
        if self.rounds.perks.of(p.general).contains(&m.perk) {
            return None;
        }
        let solo = self.rounds.players.len() <= 1;
        if solo && perk.solo_cost.is_some() {
            if self.rounds.perks.solo_lives_given >= perks.solo_lives {
                return None;
            }
            return Some((perk.solo_cost.unwrap_or(perk.cost), "ZOMBIE_PERK_QUICKREVIVE_SOLO"));
        }
        Some((perk.cost, perk.hint.as_str()))
    }

    /// The use key on a perk machine: the purchase. True when the press was the machine's.
    pub(super) fn perk_machine_input(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, input: &mut FpsInput) -> bool {
        if input.pressed & buttons::FIELD_COMPUTER == 0 {
            return false;
        }
        let Some(i) = self.perk_machine_target(p) else { return false };
        input.pressed &= !buttons::FIELD_COMPUTER;
        let perks = Perks::get();
        let Some((cost, _)) = self.perk_offer(p, i) else { return true };
        if self.rounds.perks.of(p.general).len() as u32 >= perks.limit {
            return true;
        }
        let Some(player) = host.object(p.general, p.general).map(|o| o.owner_player) else { return true };
        if (host.money(player).max(0) as u32) < cost {
            return true;
        }
        host.add_cash(player, -(cost as i32));
        let k = self.rounds.perks.machines[i].perk;
        let perk = &perks.perks[k as usize];
        match self.rounds.perks.owned.iter_mut().find(|(g, _)| *g == p.general) {
            Some((_, list)) => list.push(k),
            None => self.rounds.perks.owned.push((p.general, vec![k])),
        }
        if let Some(s) = specialty_of(&perk.specialty) {
            p.arsenal.granted = p.arsenal.granted.union(crate::weapons::Specialties::of(&[s]));
        }
        if perk.solo_cost.is_some() && self.rounds.players.len() <= 1 {
            self.rounds.perks.solo_lives_given += 1;
        }
        self.emit(idx, EventKind::ZombiePurchase { cost: cost.min(u16::MAX as u32) as u16 });
        host.log(&format!("zombies: General {} bought {} for {cost}", p.general, perk.specialty));
        true
    }

    /// `perk_think`: every perk of `general` gone (he went down), from `arsenal` too.
    pub(super) fn lose_perks(&mut self, general: u32, arsenal: &mut crate::weapons::Arsenal) {
        self.rounds.perks.owned.retain(|(g, _)| *g != general);
        arsenal.granted = crate::weapons::Specialties::NONE;
    }

    /// Whether going down alone revives `general` (`wait_and_revive`: Quick Revive, solo).
    pub fn self_revives(&self, general: u32) -> bool {
        self.rounds.players.len() <= 1 && self.rounds.perks.has(general, "specialty_quickrevive")
    }

    /// The damage hook's mirror: the Generals with Jugger-Nog, and those whom going down alone
    /// revives.
    pub fn perk_mirror(&self) -> (Vec<u32>, Vec<u32>) {
        let jugg = self.rounds.perks.owned.iter().filter(|(g, _)| self.rounds.perks.has(*g, "specialty_armorvest")).map(|(g, _)| *g).collect();
        let solo = self.rounds.perks.owned.iter().filter(|(g, _)| self.self_revives(*g)).map(|(g, _)| *g).collect();
        (jugg, solo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::fakehost::{general, FakeHost};

    fn survivor(players: usize) -> (Sim, FakeHost) {
        let host = FakeHost::default();
        host.spawn_objects.set(true);
        host.add(ObjInfo { flags: objflags::INFANTRY | objflags::GENERAL | objflags::SURVIVOR, ..general(1, 0) });
        host.wallet.borrow_mut().push((0, 20000));
        let mut sim = Sim::new();
        sim.rounds.zombies_map = true;
        sim.take_control(&host, 0, 1);
        sim.rounds.players = (0..players as i32).collect();
        sim.place_perk_machines(&host);
        (sim, host)
    }

    fn buy(sim: &mut Sim, host: &FakeHost, specialty: &str) -> bool {
        let i = sim.rounds.perks.machines.iter().position(|m| Perks::get().perks[m.perk as usize].specialty == specialty).unwrap();
        let mut p = sim.players[0].take().unwrap();
        p.body.pos = sim.rounds.perks.machines[i].pos;
        let mut input = FpsInput { pressed: buttons::FIELD_COMPUTER, ..Default::default() };
        let before = sim.rounds.perks.of(1).len();
        sim.perk_machine_input(host, 0, &mut p, &mut input);
        sim.players[0] = Some(p);
        sim.rounds.perks.of(1).len() > before
    }

    #[test]
    fn a_machine_sells_its_perk_once_and_four_at_most() {
        let (mut sim, host) = survivor(2);
        assert_eq!(sim.rounds.perks.machines.len(), 5);
        assert!(buy(&mut sim, &host, "specialty_fastreload"));
        assert!(sim.players[0].as_ref().unwrap().arsenal.has_specialty(crate::weapons::Specialty::FastReload));
        assert!(!buy(&mut sim, &host, "specialty_fastreload"), "not twice");
        assert_eq!(host.cash.borrow()[0], (0, -3000));
        assert!(buy(&mut sim, &host, "specialty_armorvest") && buy(&mut sim, &host, "specialty_rof") && buy(&mut sim, &host, "specialty_longersprint"));
        assert!(!buy(&mut sim, &host, "specialty_quickrevive"), "four at most");
        let (jugg, solo) = sim.perk_mirror();
        assert_eq!((jugg, solo), (vec![1], vec![]));
    }

    #[test]
    fn quick_revive_alone_costs_500_and_revives_him_three_times_at_most() {
        let (mut sim, host) = survivor(1);
        let p = sim.players[0].clone().unwrap();
        let i = sim.rounds.perks.machines.iter().position(|m| Perks::get().perks[m.perk as usize].specialty == "specialty_quickrevive").unwrap();
        assert_eq!(sim.perk_offer(&p, i), Some((500, "ZOMBIE_PERK_QUICKREVIVE_SOLO")));
        assert!(buy(&mut sim, &host, "specialty_quickrevive"));
        assert!(sim.self_revives(1));
        sim.rounds.perks.owned.clear();
        sim.rounds.perks.solo_lives_given = 3;
        assert_eq!(sim.perk_offer(&p, i), None, "used up");
    }

    #[test]
    fn going_down_loses_every_perk() {
        let (mut sim, host) = survivor(2);
        assert!(buy(&mut sim, &host, "specialty_rof"));
        let mut a = sim.players[0].as_ref().unwrap().arsenal.clone();
        sim.lose_perks(1, &mut a);
        assert!(sim.rounds.perks.of(1).is_empty() && !a.has_specialty(crate::weapons::Specialty::Rof));
    }
}
