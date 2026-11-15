/*
** Conquer Ops - Zero Hour gameplay core: BO2 Zombies' riot shield.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! Zombies' riot shield (`_zm_weap_riotshield.gsc`, `zombies.ini` `[RiotShield]`), on action slot 1
//! as equipment. A zombie's blow (or a player's) on a survivor whose shield is up and faces the
//! attacker, or is on his back with the attacker behind him, wears the shield 100 and does him no
//! harm (`player_damage_override`, `_zm.gsc:4071-4087`): the engine's damage hook decides it
//! ([`crate::ffi`]'s `zhbo_logic_shield_damage`, from [`Sim::zm_shield_bearers`]) and queues the wear
//! for [`Sim::zm_shield_wear`]. Worn to its hit points it breaks and is taken
//! (`player_take_riotshield`). Its bash flings the zombies in front within 90 inches (2500 each,
//! 100 wear each, `riotshield_melee`).

use super::*;
use crate::zombies::RiotShield;

/// A survivor's Zombies shield as the damage hook sees it: up or on his back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZmShieldBearer {
    pub general: u32,
    pub pos: Vec3,
    pub yaw: f32,
    pub held: bool,
}

impl Sim {
    /// Every survivor carrying Zombies' shield (not deployed): up, or on his back.
    pub fn zm_shield_bearers(&self) -> Vec<ZmShieldBearer> {
        let id = &RiotShield::get().weapon;
        self.players
            .iter()
            .flatten()
            .filter(|p| p.survivor)
            .filter_map(|p| {
                let held = p.arsenal.current().main.id == *id;
                let owned = held || p.arsenal.items.iter().any(|w| w.main.id == *id);
                owned.then_some(ZmShieldBearer { general: p.general, pos: p.body.pos, yaw: p.yaw, held })
            })
            .collect()
    }

    /// The wear `amount` on survivor `general`'s shield (`player_damage_shield`): worn to its hit
    /// points, it breaks and is taken.
    pub fn zm_shield_wear(&mut self, host: &dyn LogicHost, general: u32, amount: f32) {
        let Some(idx) = self.seat_of(general) else { return };
        let Some(p) = self.players[idx].as_mut() else { return };
        wear_shield(host, p, amount);
    }

    /// `riotshield_melee`: the bash flings every zombie in front of `p` within the reach (2500
    /// each, `MOD_IMPACT`), each fling wearing the shield.
    pub(super) fn zm_shield_bash(&mut self, host: &dyn LogicHost, p: &mut PlayerSim) {
        let rules = RiotShield::get();
        let eye = p.body.eye();
        let reach = crate::units::inches(rules.melee_range);
        let fwd = dir_from_angles(p.yaw, p.pitch);
        let mut near = Vec::new();
        host.query(eye, reach, p.general, &mut near);
        near.retain(|o| o.has(objflags::ZOMBIE) && !o.has(objflags::DEAD));
        near.sort_by_key(|o| o.id);
        let mut flung = 0;
        for o in &near {
            let centre = Vec3::new(o.pos.x, o.pos.y, o.pos.z + o.height * 0.5);
            let to = centre - eye;
            if to.length() > reach || fwd.dot(to) < 0.0 {
                continue;
            }
            // Within the cylinder about the line ahead.
            let along = fwd * fwd.dot(to);
            if (to - along).length() > crate::units::inches(rules.cylinder_radius) {
                continue;
            }
            let amount = crate::damage::to_zh(&p.arsenal.tuning, o, rules.fling_damage);
            host.damage(p.general, o.id, amount, crate::damage::Blow::new(crate::damage::MeansOfDeath::Impact, p.arsenal.current().def, to.normalized_or_zero()));
            flung += 1;
        }
        if flung > 0 {
            wear_shield(host, p, rules.fling_wear * flung as f32);
        }
    }
}

/// Wear `p`'s shield `amount`; worn to its hit points it breaks and is taken
/// (`player_take_riotshield`): the weapon goes, its slot with it.
fn wear_shield(host: &dyn LogicHost, p: &mut PlayerSim, amount: f32) {
    let rules = RiotShield::get();
    p.shield_wear += amount;
    if p.shield_wear < rules.hit_points {
        return;
    }
    p.shield_wear = 0.0;
    if p.arsenal.item_up.is_some_and(|d| d.id == rules.weapon) {
        p.arsenal.stow_item();
    }
    p.arsenal.items.retain(|w| w.main.id != rules.weapon);
    for s in p.action_slots.slots.iter_mut() {
        if matches!(s, crate::weapons::actionslot::ActionSlot::Weapon { def } if def.id == rules.weapon) {
            *s = crate::weapons::actionslot::ActionSlot::Empty;
        }
    }
    host.log(&format!("zombies: General {}'s riot shield broke", p.general));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shield_takes_blows_from_its_side_only() {
        let r = RiotShield::get();
        // Facing +x: an attacker ahead hits the held shield; behind, the stowed one.
        assert!(r.blocks(0.0, (10.0, 0.0), true));
        assert!(!r.blocks(0.0, (-10.0, 0.0), true));
        assert!(r.blocks(0.0, (-10.0, 0.0), false));
        assert!(!r.blocks(0.0, (10.0, 0.0), false));
        // The held arc ends at a dot of 0.2 (about 78 degrees off the view).
        assert!(!r.blocks(0.0, (0.1, 1.0), true));
    }
}

#[cfg(test)]
mod wear_tests {
    use super::*;
    use crate::sim::fakehost::{general, FakeHost};

    /// Each blocked blow wears it 100; at its hit points it breaks and its slot is empty.
    #[test]
    fn worn_out_the_shield_breaks_and_is_taken() {
        let host = FakeHost::default();
        host.add(ObjInfo { flags: objflags::INFANTRY | objflags::GENERAL | objflags::SURVIVOR, ..general(1, 0) });
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        let shield: &'static crate::weapons::WeaponDef = Box::leak(Box::new(crate::weapons::WeaponDef { id: RiotShield::get().weapon.clone(), class: crate::weapons::Class::Shield, clip: 1, ..Default::default() }));
        {
            let p = sim.players[0].as_mut().unwrap();
            p.survivor = true;
            p.arsenal.give_item(shield, 1);
            p.action_slots.slots[0] = crate::weapons::actionslot::ActionSlot::Weapon { def: shield };
        }
        assert_eq!(sim.zm_shield_bearers().len(), 1, "on his back");
        let r = RiotShield::get();
        let blows = (r.hit_points / r.block_wear).ceil() as usize;
        for _ in 0..blows - 1 {
            sim.zm_shield_wear(&host, 1, r.block_wear);
        }
        assert!(sim.players[0].as_ref().unwrap().action_slots.holds_weapon(&r.weapon));
        sim.zm_shield_wear(&host, 1, r.block_wear);
        let p = sim.players[0].as_ref().unwrap();
        assert!(!p.action_slots.holds_weapon(&r.weapon) && p.arsenal.items.is_empty(), "broken: taken");
    }
}
