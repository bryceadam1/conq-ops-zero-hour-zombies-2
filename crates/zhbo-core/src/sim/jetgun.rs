/*
** Conquer Ops - Zero Hour gameplay core: BO2 Zombies' jetgun, the script's part.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! The jetgun's engine and heat are the weapon's own (fire type `jetgun` and its overheat, the
//! arsenal); what it does to the zombies is `_zm_weap_jetgun.gsc`'s (`zombies.ini` `[Jetgun]`).
//! While its engine spins past 0.2, every 0.25 s from the first fired frame (`jetgun_firing`): the
//! three closest zombies within 2048 inches that are in its cone and cylinder are ground up when
//! within 128 inches (killed; the engine let down to half) and dragged to him when farther ahead;
//! the power-ups in its cone within 500 inches are pulled 50 inches in. Overheated, TranZit's
//! jetgun blows up and is gone (`handle_overheated_jetgun`, `level.explode_overheated_jetgun`).

use super::*;
use crate::zombies::Jetgun;

impl Sim {
    /// The jetgun's frame for `p` (after his arsenal's step).
    pub(super) fn step_jetgun(&mut self, host: &dyn LogicHost, p: &mut PlayerSim) {
        let rules = Jetgun::get();
        if rules.weapon.is_empty() || p.arsenal.current().main.id != rules.weapon {
            p.jet_pulse = None;
            return;
        }
        if p.arsenal.current().overheated && rules.explode {
            self.explode_jetgun(host, p);
            return;
        }
        if p.arsenal.spin_lerp() <= rules.firing {
            p.jet_pulse = None;
            return;
        }
        let left = p.jet_pulse.map_or(0.0, |t| t - LOGIC_DT);
        if left > 1e-6 {
            p.jet_pulse = Some(left);
            return;
        }
        p.jet_pulse = Some(rules.pulse);
        self.jetgun_pulse(host, p);
    }

    /// `jetgun_fired`: one pulse.
    fn jetgun_pulse(&mut self, host: &dyn LogicHost, p: &mut PlayerSim) {
        let rules = Jetgun::get();
        let eye = p.body.eye();
        let fwd = dir_from_angles(p.yaw, p.pitch);
        let inch = crate::units::inches(1.0);
        let drag = rules.drag_range * inch;
        let mut near = Vec::new();
        host.query(eye, drag, p.general, &mut near);
        near.retain(|o| o.has(objflags::ZOMBIE) && !o.has(objflags::DEAD));
        let centre = |o: &ObjInfo| Vec3::new(o.pos.x, o.pos.y, o.pos.z + o.height * 0.5);
        near.sort_by(|a, b| (centre(a) - eye).length().total_cmp(&(centre(b) - eye).length()).then(a.id.cmp(&b.id)));
        near.truncate(rules.enemies);
        let end = eye + fwd * (rules.knockdown_range * inch);
        let def = p.arsenal.current().def;
        for o in &near {
            let to = centre(o) - eye;
            let dist = to.length();
            if dist > drag || dist <= 0.0 {
                continue;
            }
            let dot = fwd.dot(to) / dist;
            if dot.abs() < rules.cone_dot {
                continue;
            }
            // `pointonsegmentnearesttopoint`: the line `KnockdownRange` ahead.
            let along = to.dot(fwd).clamp(0.0, (end - eye).length());
            if (to - fwd * along).length() > rules.cylinder_radius * inch {
                continue;
            }
            if dist < rules.grind_range * inch {
                // `jetgun_grind_zombie`: health + 666, the engine let down to half.
                host.damage(p.general, o.id, o.health + 666.0, crate::damage::Blow::new(crate::damage::MeansOfDeath::Unknown, def, to.normalized_or_zero()));
                let i = p.arsenal.active;
                p.arsenal.weapons[i].spin *= rules.grind_let_down;
            } else if dot > 0.0 {
                // `zombie_do_drag`: he comes to the jetgun's owner.
                host.order_attack(o.id, p.general);
            }
        }
        // `try_pull_powerups`.
        for d in self.rounds.powerups.dropped.iter_mut() {
            let to = d.pos - eye;
            let dist = to.length();
            if dist > rules.powerup_range * inch || dist <= 0.0 || (fwd.dot(to) / dist).abs() < rules.cone_dot {
                continue;
            }
            let step = (rules.powerup_move * inch).min(dist);
            d.pos -= to * (step / dist);
            if d.object != 0 {
                host.set_pose(d.object, d.pos, 0.0, false);
            }
        }
    }

    /// `handle_overheated_jetgun` with `level.explode_overheated_jetgun`: the equipment is released
    /// (`equipment_release`) and blown apart.
    fn explode_jetgun(&mut self, host: &dyn LogicHost, p: &mut PlayerSim) {
        let id = &Jetgun::get().weapon;
        p.jet_pulse = None;
        if p.arsenal.item_up.is_some_and(|d| d.id == *id) {
            p.arsenal.stow_item();
        }
        p.arsenal.items.retain(|w| w.main.id != *id);
        for s in p.action_slots.slots.iter_mut() {
            if matches!(s, crate::weapons::actionslot::ActionSlot::Weapon { def } if def.id == *id) {
                *s = crate::weapons::actionslot::ActionSlot::Empty;
            }
        }
        self.unbuild(host, id, p.body.pos);
        host.log(&format!("zombies: General {}'s jetgun overheated and exploded", p.general));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::fakehost::{general, FakeHost};

    /// A pulse grinds the zombie within 128 inches ahead and lets the engine down to half, drags
    /// the one farther ahead to him, and leaves the one behind beyond the grind range alone.
    #[test]
    fn a_pulse_grinds_the_near_and_drags_the_far() {
        let rules = Jetgun::get();
        let host = FakeHost::default();
        host.add(ObjInfo { flags: objflags::INFANTRY | objflags::GENERAL | objflags::SURVIVOR, ..general(1, 0) });
        let z = |id: u32, x: f32| ObjInfo { id, owner_player: 7, flags: objflags::INFANTRY | objflags::ZOMBIE, pos: Vec3::new(x, 0.0, 0.0), height: 10.0, health: 150.0, max_health: 150.0, ..Default::default() };
        let inch = crate::units::inches(1.0);
        host.add(z(20, 60.0 * inch));
        host.add(z(21, 600.0 * inch));
        host.add(z(22, -600.0 * inch));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        let jet: &'static crate::weapons::WeaponDef = Box::leak(Box::new(crate::weapons::WeaponDef { id: rules.weapon.clone(), fire: crate::weapons::FireMode::Jetgun, clip: 500, spin_up_time: 3.0, ..Default::default() }));
        let mut p = sim.players[0].take().unwrap();
        (p.yaw, p.pitch) = (0.0, 0.0);
        let i = p.arsenal.active;
        p.arsenal.weapons[i] = crate::weapons::WeaponState::new(jet);
        p.arsenal.weapons[i].spin = 3.0;
        sim.step_jetgun(&host, &mut p);
        let hit: Vec<u32> = host.damage_log.borrow().iter().map(|&(_, v, _)| v).collect();
        assert_eq!(hit, vec![20], "ground");
        assert!((p.arsenal.spin_lerp() - rules.grind_let_down).abs() < 1e-4, "the engine let down");
        assert_eq!(host.orders.borrow().as_slice(), &[(21, 1)], "dragged to him");
        // The next pulse waits its 0.25 s.
        sim.step_jetgun(&host, &mut p);
        assert_eq!(host.damage_log.borrow().len(), 1);
        sim.players[0] = Some(p);
    }
}
