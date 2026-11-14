/*
** Conquer Ops - Zero Hour gameplay core: BO2 Zombies' Pack-a-Punch.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! Pack-a-Punch (`_zm_perks.gsc` `vending_weapon_upgrade`, `zombies.ini` `[PackAPunch]`) on a
//! Zombies map: a machine on the wall-buy ring around each start, after the perk machines (TranZit's
//! is a buildable; this one stands built and on).
//!
//! * **The use key with a weapon up that has an upgrade** (`can_upgrade_weapon`: the roster's
//!   `upgrade_name`) and its price: the machine takes the weapon (`third_person_weapon_upgrade`)
//!   and works `UpgradeTime`.
//! * **The upgraded weapon** then waits `TakeTime` for its buyer alone (`wait_for_player_to_take`);
//!   the use key gives it to him with its start ammo (`weapon_give`). Left, it is lost
//!   (`wait_for_timeout`).

use super::*;
use crate::zombies::PackAPunch;

/// Where a machine is in an upgrade.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum PapPhase {
    #[default]
    Idle,
    /// Working on `weapon` (the upgrade's id) for `user`.
    Upgrading { user: u32, weapon: String, t: f32 },
    /// `weapon` waits for `user`.
    Waiting { user: u32, weapon: String, t: f32 },
}

crate::statehash::state_hash!(enum PapPhase { Idle, Upgrading { user, weapon, t }, Waiting { user, weapon, t } });

/// One machine.
#[derive(Clone, Debug, PartialEq)]
pub struct PapAt {
    pub pos: Vec3,
    pub yaw: f32,
    pub object: u32,
    pub phase: PapPhase,
}

crate::statehash::state_hash!(struct PapAt { pos, yaw, object, phase });

/// The upgrade of the weapon `id` (by base), if the roster gives it one and it is no upgrade itself.
pub fn upgrade_of(id: &str) -> Option<String> {
    let base = crate::weapons::base_of(id);
    let roster = crate::zombies::roster();
    if roster.iter().any(|w| w.upgrade.as_deref() == Some(base)) {
        return None;
    }
    roster.into_iter().find(|w| w.id == base).and_then(|w| w.upgrade).filter(|u| crate::weapons::weapon_or_none(u).is_some())
}

impl Sim {
    /// Stand the machines (the round engine's start).
    pub(super) fn place_pack_a_punch(&mut self, host: &dyn LogicHost) {
        let w = crate::zombies::WallBuys::get();
        let step = std::f32::consts::TAU / w.buys.len().max(1) as f32;
        let starts: Vec<Vec3> = self.living_survivors(host).iter().map(|o| o.pos).collect();
        if starts.is_empty() || !self.rounds.pack_a_punch.is_empty() {
            return;
        }
        let owner = self.rounds.spawners.first().copied().unwrap_or(0);
        let a = step * (crate::zombies::Perks::get().perks.len() as f32 + 1.5);
        for start in starts {
            let (s, c) = crate::math::det::sin_cos(a);
            let pos = start + Vec3::new(c, s, 0.0) * w.ring;
            let pos = Vec3::new(pos.x, pos.y, host.ground_height(pos.x, pos.y));
            let yaw = crate::math::wrap_angle(a + std::f32::consts::PI);
            let object = host.spawn(&crate::names::pack_a_punch_object(), owner, pos, yaw);
            self.rounds.pack_a_punch.push(PapAt { pos, yaw, object, phase: PapPhase::Idle });
        }
    }

    /// The machine in `p`'s use list now, if any.
    pub fn pack_a_punch_target(&self, p: &PlayerSim) -> Option<usize> {
        if !p.survivor || self.is_downed(p.general) {
            return None;
        }
        let t = crate::tuning::get();
        let eye = p.body.eye();
        self.rounds.pack_a_punch.iter().position(|m| (m.pos + Vec3::new(0.0, 0.0, crate::units::inches(t.dropped_item_half_size)) - eye).length() / crate::units::inches(1.0) <= t.player_use_radius)
    }

    /// The machine's prompt to `p`: its price with a weapon it takes up, or the buyer's take.
    pub fn pack_a_punch_prompt(&self, p: &PlayerSim) -> Option<&'static str> {
        let m = self.rounds.pack_a_punch.get(self.pack_a_punch_target(p)?)?;
        let rules = PackAPunch::get();
        match &m.phase {
            PapPhase::Idle if upgrade_of(&p.arsenal.current().main.id).is_some() => Some(rules.hint.as_str()),
            PapPhase::Waiting { user, .. } if *user == p.general => Some(rules.take_hint.as_str()),
            _ => None,
        }
    }

    /// The use key on the machine. True when the press was the machine's.
    pub(super) fn pack_a_punch_input(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, input: &mut FpsInput) -> bool {
        if input.pressed & buttons::FIELD_COMPUTER == 0 {
            return false;
        }
        let Some(i) = self.pack_a_punch_target(p) else { return false };
        let rules = PackAPunch::get();
        match self.rounds.pack_a_punch[i].phase.clone() {
            PapPhase::Idle => {
                let Some(upgrade) = upgrade_of(&p.arsenal.current().main.id) else { return false };
                input.pressed &= !buttons::FIELD_COMPUTER;
                let Some(player) = host.object(p.general, p.general).map(|o| o.owner_player) else { return true };
                if (host.money(player).max(0) as u32) < rules.cost {
                    return true;
                }
                host.add_cash(player, -(rules.cost as i32));
                let taken = p.arsenal.take_current();
                host.log(&format!("zombies: General {} packs {} into {upgrade}", p.general, taken.main.id));
                self.rounds.pack_a_punch[i].phase = PapPhase::Upgrading { user: p.general, weapon: upgrade, t: 0.0 };
                self.emit(idx, EventKind::ZombiePurchase { cost: rules.cost.min(u16::MAX as u32) as u16 });
                true
            }
            PapPhase::Waiting { user, weapon, .. } if user == p.general => {
                input.pressed &= !buttons::FIELD_COMPUTER;
                if let Some(def) = crate::weapons::weapon_or_none(&weapon) {
                    super::wallbuy::give(p, def);
                    host.log(&format!("zombies: General {} took {weapon}", p.general));
                }
                self.rounds.pack_a_punch[i].phase = PapPhase::Idle;
                true
            }
            _ => false,
        }
    }

    /// The machines' frame.
    pub(super) fn step_pack_a_punch(&mut self, host: &dyn LogicHost) {
        let rules = PackAPunch::get();
        for m in &mut self.rounds.pack_a_punch {
            m.phase = match std::mem::take(&mut m.phase) {
                PapPhase::Upgrading { user, weapon, t } if t + LOGIC_DT >= rules.upgrade_time => PapPhase::Waiting { user, weapon, t: 0.0 },
                PapPhase::Upgrading { user, weapon, t } => PapPhase::Upgrading { user, weapon, t: t + LOGIC_DT },
                PapPhase::Waiting { weapon, t, .. } if t + LOGIC_DT >= rules.take_time => {
                    host.log(&format!("zombies: {weapon} left in Pack-a-Punch, lost"));
                    PapPhase::Idle
                }
                PapPhase::Waiting { user, weapon, t } => PapPhase::Waiting { user, weapon, t: t + LOGIC_DT },
                PapPhase::Idle => PapPhase::Idle,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_weapon_with_an_upgrade_and_no_upgrade_itself_packs() {
        // The built-in table has no Zombies guns: nothing packs there.
        assert_eq!(upgrade_of("an94_mp"), None);
        let roster = crate::zombies::roster();
        let m14 = roster.iter().find(|w| w.id == "m14_zm").unwrap();
        assert_eq!(m14.upgrade.as_deref(), Some("m14_upgraded_zm"));
        assert_eq!(upgrade_of("m14_upgraded_zm"), None, "an upgrade does not pack again");
    }

    #[test]
    fn taking_the_weapon_up_brings_the_next_one_up() {
        let mut a = crate::weapons::Arsenal::with_loadout(crate::weapons::Loadout { primary: "an94_mp".into(), secondary: "fnp45_mp".into(), held_knife: false, ..crate::weapons::Loadout::unarmed() });
        let taken = a.take_current();
        assert_eq!(taken.main.id, "an94_mp");
        assert_eq!(a.weapons.len(), 1);
        assert_eq!(a.current().main.id, "fnp45_mp");
        let last = a.take_current();
        assert_eq!(last.main.id, "fnp45_mp");
        assert_eq!(a.weapons.len(), 1, "the melee weapon held in its place");
    }

    #[test]
    fn an_upgrade_left_waiting_is_lost_after_fifteen_seconds() {
        let mut sim = Sim::new();
        let host = crate::sim::fakehost::FakeHost::default();
        sim.rounds.pack_a_punch.push(PapAt { pos: Vec3::ZERO, yaw: 0.0, object: 0, phase: PapPhase::Upgrading { user: 1, weapon: "m14_upgraded_zm".into(), t: 0.0 } });
        for _ in 0..(3.4 / LOGIC_DT) as u32 {
            sim.step_pack_a_punch(&host);
        }
        assert!(matches!(sim.rounds.pack_a_punch[0].phase, PapPhase::Waiting { .. }));
        for _ in 0..(15.0 / LOGIC_DT) as u32 + 1 {
            sim.step_pack_a_punch(&host);
        }
        assert_eq!(sim.rounds.pack_a_punch[0].phase, PapPhase::Idle);
    }
}
