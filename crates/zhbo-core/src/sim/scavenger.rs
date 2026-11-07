/*
** Conquer Ops - Zero Hour gameplay core: the Scavenger pack.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! BO2's Scavenger: a kill drops a pack where the victim fell, and a Scavenger player who walks
//! over it is refilled.
//!
//! * **The drop** (`_globallogic_player.gsc:1911-1914` → `_weapons.gsc:1919-1940`
//!   `dropscavengerfordeath`): outside the grace period, unless the means of death was a grenade's,
//!   an explosive's or a projectile's splash ([`MeansOfDeath::drops_scavenger`]), when the
//!   attacker is a player other than the victim with `specialty_scavenger`, the victim drops
//!   `scavenger_item_mp` (`dropscavengeritem`). Unlike his weapon it drops for a team kill too: the
//!   script asks only `attacker != self`. The weapon drop is a separate rule (`sim::drops`); the
//!   two never wait on each other. A soldier's kill drops one (the mod's players and Zero Hour's
//!   infantry, as Scavenger always counted them); a vehicle's none.
//! * **The pack** is an item like a dropped weapon (`LaunchItem`, t6mp `FUN_005fab50`): tossed by
//!   `Drop_Item` (`FUN_005fae60`) from the middle of his box along his facing, it falls and lies
//!   where it lands, with no timer of its own (no script deletes it); it goes when a Scavenger
//!   player takes it or BO2's item cleanup frees it (`FUN_005f9230`, the cap it shares with the
//!   dropped weapons, [`Sim::make_room_for_an_item`]). It shows as `scavenger_item_mp`'s world
//!   model (`p_glo_scavenger_pack`), converted as [`crate::names::SCAVENGER_PACK_OBJECT`].
//! * **The pickup** (`scavenger_think`, `:1791-1890`): a first-person General with Scavenger whose
//!   box touches the pack's (the item touch the dropped weapons use) takes it: every gun gets a
//!   magazine and each offhand one more ([`crate::weapons::Arsenal::scavenge`]), the pickup sounds
//!   (`fly_equipment_pickup_plr` for him, `_npc` at him) and the `hud_scavenger_pickup` icon fading
//!   over 2.5 s ([`EventKind::ScavengerPickedUp`]). A General without the perk walks over it.
//!
//! The toss's random push is drawn from a stream seeded by the frame and the victim (every
//! machine draws the same, and no seat's own stream moves). The packs are hashed in the world's
//! part of the fingerprint.

use super::drops::Killing;
use super::*;

/// A Scavenger pack lying in the world (or falling to it).
#[derive(Clone, Debug, PartialEq)]
pub struct Pack {
    pub pos: Vec3,
    pub vel: Vec3,
    pub resting: bool,
    pub yaw: f32,
    /// Its Zero Hour object (0: none, the art is not converted).
    pub object: u32,
}

crate::statehash::state_hash!(struct Pack { pos, vel, resting, yaw, object });

impl Sim {
    /// The step's kills that drop a pack ([`Killing`], in the order they were dealt).
    pub(super) fn drop_scavenger_packs(&mut self, host: &dyn LogicHost, killings: &[Killing]) {
        if self.in_grace_period() {
            return;
        }
        for k in killings {
            if !k.soldier || k.by == k.victim || !k.how.drops_scavenger() {
                continue;
            }
            let scavenger = self.players.iter().flatten().any(|p| p.general == k.by && p.arsenal.has_specialty(Specialty::Scavenger));
            if !scavenger {
                continue;
            }
            self.put_pack(host, k);
        }
    }

    /// A pack tossed from `k`'s victim (`Drop_Item`'s toss, the tuning pack's `DropItem*`).
    fn put_pack(&mut self, host: &dyn LogicHost, k: &Killing) {
        self.make_room_for_an_item(host);
        let t = crate::tuning::get();
        let from = k.at + Vec3::new(0.0, 0.0, inches(t.player_box_height) * 0.5);
        let mut rng = crate::weapons::rng::Rng::new(self.frame.wrapping_mul(0x9E37_79B9) ^ k.victim.wrapping_mul(0x85EB_CA6B) ^ 0x5CA7);
        let (c, s) = (crate::math::det::cos(k.yaw), crate::math::det::sin(k.yaw));
        let forward = inches(t.drop_item_forward_speed);
        let vel = Vec3::new(
            c * forward + rng.signed() * inches(t.drop_item_random_speed),
            s * forward + rng.signed() * inches(t.drop_item_random_speed),
            inches(t.drop_item_up_speed) + rng.signed() * inches(t.drop_item_random_up_speed),
        );
        let object = host.spawn(crate::names::SCAVENGER_PACK_OBJECT, k.by, from, k.yaw);
        host.log(&format!("scavenger: a pack down at ({:.0}, {:.0}) from {}'s kill, object {object}", from.x, from.y, k.by));
        self.packs.push(Pack { pos: from, vel, resting: false, yaw: k.yaw, object });
    }

    /// Each logic frame: the falling packs fall, and a Scavenger General walking over one takes it.
    pub(super) fn step_packs(&mut self, host: &dyn LogicHost) {
        let g = crate::projectiles::gravity();
        for k in self.packs.iter_mut().filter(|k| !k.resting) {
            k.vel.z -= g * LOGIC_DT;
            k.pos += k.vel * LOGIC_DT;
            let ground = host.ground_height(k.pos.x, k.pos.y);
            if k.pos.z <= ground {
                k.pos.z = ground;
                k.vel = Vec3::ZERO;
                k.resting = true;
            }
            if k.object != 0 {
                host.set_pose(k.object, k.pos, k.yaw, false);
            }
        }
        let t = crate::tuning::get();
        let half = inches(t.dropped_item_half_size);
        let reach = self.tuning.radius + half;
        let top = inches(t.player_box_height) + half;
        for idx in 0..self.players.len() {
            let Some(mut p) = self.players[idx].take() else { continue };
            if matches!(p.mode, Mode::Fps) && !p.awaiting_class() && p.arsenal.has_specialty(Specialty::Scavenger) {
                let mut i = 0;
                while i < self.packs.len() {
                    let rel = self.packs[i].pos - p.body.pos;
                    let touches = rel.x.abs() <= reach && rel.y.abs() <= reach && rel.z >= -half && rel.z <= top;
                    if !touches {
                        i += 1;
                        continue;
                    }
                    let k = self.packs.remove(i);
                    if k.object != 0 {
                        host.destroy(k.object);
                    }
                    let c4_down = self.equipment.placed.iter().any(|q| q.owner == idx && q.kind == crate::equipment::Kind::C4);
                    p.arsenal.scavenge(c4_down);
                    self.emit(idx, EventKind::ScavengerPickedUp { at: k.pos });
                }
            }
            self.players[idx] = Some(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::damage::MeansOfDeath;
    use crate::sim::tests::{general, soldier, spawn, FakeHost};
    use crate::weapons::{Arsenal, Loadout, Perk};

    /// After the grace period, when a kill drops a pack.
    const LATE: u32 = 1000;

    fn step(sim: &mut Sim, host: &FakeHost, frames: u32) {
        for _ in 0..frames {
            let f = sim.frame + 1;
            sim.step(host, f);
        }
    }

    /// Seat `seat`'s General `id` in first person at `x` with an AN-94 kit, Scavenger when `perk`.
    fn armed(sim: &mut Sim, host: &FakeHost, seat: i32, id: u32, x: f32, perk: bool) {
        let mut g = general(id, seat);
        g.pos = Vec3::new(x, 0.0, 0.0);
        host.add(g);
        spawn(sim, host, seat, id);
        let p = sim.players[seat as usize].as_mut().unwrap();
        p.body.pos = Vec3::new(x, 0.0, 0.0);
        p.arsenal = Arsenal::with_loadout(Loadout::unarmed().with_spec("primary=an94_mp;lethal=frag_grenade_mp;tactical=concussion_grenade_mp"));
        if perk {
            p.arsenal.loadout.perks.push(Perk::SCAVENGER);
        }
        p.arsenal.raise_left = 0.0;
    }

    fn killing(victim: u32, by: u32, how: MeansOfDeath, x: f32) -> Killing {
        Killing { victim, by, how, at: Vec3::new(x, 0.0, 0.0), yaw: 0.0, soldier: true }
    }

    /// A Scavenger kill drops a pack where the victim fell (not a refill on the kill); it falls and
    /// lies; a grenade, an explosive or a splash kill drops none, nor one in the grace period, nor
    /// a killer without the perk, nor a suicide; a team kill does (`attacker != self` only).
    #[test]
    fn a_scavenger_kill_drops_a_pack_by_bo2s_rules() {
        let host = FakeHost::default();
        let mut sim = Sim::new();
        sim.frame = LATE;
        armed(&mut sim, &host, 0, 1, 0.0, true);
        armed(&mut sim, &host, 1, 2, 500.0, false);
        let reserve = sim.player(0).unwrap().arsenal.weapons[0].reserve;
        sim.drop_scavenger_packs(&host, &[killing(7, 1, MeansOfDeath::RifleBullet, 100.0)]);
        assert_eq!(sim.packs.len(), 1, "a pack");
        assert_eq!(sim.player(0).unwrap().arsenal.weapons[0].reserve, reserve, "no refill on the kill");
        assert!(host.spawned.borrow().iter().any(|s| s.1 == crate::names::SCAVENGER_PACK_OBJECT), "drawn as the pack");
        for how in [MeansOfDeath::Grenade, MeansOfDeath::GrenadeSplash, MeansOfDeath::Explosive, MeansOfDeath::ProjectileSplash] {
            sim.drop_scavenger_packs(&host, &[killing(7, 1, how, 100.0)]);
        }
        sim.drop_scavenger_packs(&host, &[killing(7, 2, MeansOfDeath::RifleBullet, 100.0), killing(1, 1, MeansOfDeath::PistolBullet, 0.0)]);
        sim.drop_scavenger_packs(&host, &[Killing { soldier: false, ..killing(7, 1, MeansOfDeath::RifleBullet, 100.0) }]);
        assert_eq!(sim.packs.len(), 1, "none for a blast, a killer without the perk, a suicide or a vehicle");
        sim.drop_scavenger_packs(&host, &[killing(2, 1, MeansOfDeath::Melee, 500.0)]);
        assert_eq!(sim.packs.len(), 2, "any player's death, the script asks only attacker != self");
        let mut early = Sim::new();
        armed(&mut early, &host, 0, 1, 0.0, true);
        early.drop_scavenger_packs(&host, &[killing(7, 1, MeansOfDeath::RifleBullet, 100.0)]);
        assert!(early.packs.is_empty(), "none in the grace period");
        // It falls from the middle of his box and lies where it lands.
        step(&mut sim, &host, 60);
        assert!(sim.packs.iter().all(|k| k.resting && k.pos.z == 0.0), "{:?}", sim.packs);
    }

    /// Walking over a pack: a Scavenger General takes it and is refilled (a magazine per gun, one
    /// more of each offhand up to the class's count); one without the perk walks over it and it
    /// stays. It has no timer of its own.
    #[test]
    fn a_scavenger_general_walking_over_a_pack_takes_it() {
        let host = FakeHost::default();
        let mut sim = Sim::new();
        sim.frame = LATE;
        armed(&mut sim, &host, 0, 1, 0.0, false);
        sim.packs.push(Pack { pos: Vec3::ZERO, vel: Vec3::ZERO, resting: true, yaw: 0.0, object: 0 });
        step(&mut sim, &host, 3);
        assert_eq!(sim.packs.len(), 1, "no perk: he walks over it");
        let p = sim.players[0].as_mut().unwrap();
        p.arsenal.loadout.perks.push(Perk::SCAVENGER);
        p.arsenal.weapons[0].reserve = 0;
        p.arsenal.lethal.count = 0;
        let clip = p.arsenal.weapons[0].main.clip;
        step(&mut sim, &host, 1);
        assert!(sim.packs.is_empty(), "taken");
        let p = sim.player(0).unwrap();
        assert_eq!(p.arsenal.weapons[0].reserve, clip, "a magazine");
        assert_eq!(p.arsenal.lethal.count, 1, "one frag");
        assert!(sim.events.iter().any(|e| matches!(e.kind, EventKind::ScavengerPickedUp { .. })));
        // An untouched pack lies on.
        sim.packs.push(Pack { pos: Vec3::new(900.0, 0.0, 0.0), vel: Vec3::ZERO, resting: true, yaw: 0.0, object: 0 });
        step(&mut sim, &host, 3000);
        assert_eq!(sim.packs.len(), 1);
    }

    /// The real step: a Scavenger General's bullet kill on Zero Hour infantry drops a pack at the
    /// soldier, independent of any weapon drop (infantry drop none).
    #[test]
    fn a_kill_in_the_step_drops_the_pack_at_the_victim() {
        let host = FakeHost::default();
        let mut sim = Sim::new();
        sim.frame = LATE;
        armed(&mut sim, &host, 0, 1, 0.0, true);
        let mut s = soldier(7, 1, 40.0);
        s.health = 1.0;
        host.add(s);
        let p = sim.players[0].as_mut().unwrap();
        p.arsenal.weapons[0].ammo = 30;
        crate::sim::tests::run(&mut sim, &host, 0, 1, crate::input::FpsInput { held: crate::input::buttons::FIRE, ..Default::default() }, 10);
        assert!(host.objects.borrow().iter().any(|o| o.id == 7 && o.has(objflags::DEAD)), "the soldier died");
        assert_eq!(sim.packs.len(), 1, "{:?}", sim.packs);
        assert!((sim.packs[0].pos.x - 40.0).abs() < 10.0, "at him: {:?}", sim.packs[0].pos);
        assert!(sim.drops.is_empty(), "infantry drop no weapon; the pack is its own rule");
    }
}
