/*
** Conquer Ops - Zero Hour gameplay core: BO2 Zombies' buildables.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! TranZit's buildable equipment (`_zm_buildables.gsc`, `zombies.ini` `[Buildables]`): the riot
//! shield and the jetgun. Each part lies on the map; a survivor picks one up with the use key (one
//! at a time: taking another drops the one he holds where he stands, `player_take_piece`; going
//! down drops it). At the buildable's workbench, holding the use key with a part it lacks, the
//! builder comes up (`zombie_builder_zm`, [`super::items`]) and after `BuildTime` the part is on
//! (`buildable_use_hold_think_internal`); letting go, walking off or going down stops it. With every
//! part on, the bench offers the equipment free (`bptrigger_think_persistent`): onto action slot 1,
//! one equipment at a time, a limited one only while none is out.

use super::*;
use crate::weapons::actionslot::ActionSlot;
use crate::zombies::Buildables;

/// One part: where it lies (its home: where it was laid), who carries it (0: none), and whether it
/// is on the bench.
#[derive(Clone, Debug, PartialEq)]
pub struct PartAt {
    pub pos: Vec3,
    pub home: Vec3,
    pub object: u32,
    pub carrier: u32,
    pub built: bool,
}

crate::statehash::state_hash!(struct PartAt { pos, home, object, carrier, built });

/// One buildable: its row in [`Buildables`], its bench, its parts, the build under way (who, which
/// part, seconds in) and the finished equipment's prop.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildableAt {
    pub index: u8,
    pub bench: Vec3,
    pub yaw: f32,
    pub bench_object: u32,
    pub built_object: u32,
    pub parts: Vec<PartAt>,
    pub building: Option<(u32, u8, f32)>,
}

crate::statehash::state_hash!(struct BuildableAt { index, bench, yaw, bench_object, built_object, parts, building });

impl BuildableAt {
    pub fn all_built(&self) -> bool {
        self.parts.iter().all(|p| p.built)
    }
}

/// The seed of the parts' places (every machine lays them alike).
const PART_SEED: u32 = 0x5A4D_B11D;

impl Sim {
    /// Stand the benches and lay the parts (the round engine's start).
    pub(super) fn place_buildables(&mut self, host: &dyn LogicHost) {
        let rules = Buildables::get();
        let starts = self.survivor_starts(host);
        if starts.is_empty() || !self.rounds.buildables.is_empty() {
            return;
        }
        let w = crate::zombies::WallBuys::get();
        let step = std::f32::consts::TAU / w.buys.len().max(1) as f32;
        let owner = self.rounds.spawners.first().copied().unwrap_or(0);
        let ground = |p: Vec3| Vec3::new(p.x, p.y, host.ground_height(p.x, p.y));
        // After Pack-a-Punch on the first start's ring (`place_pack_a_punch` stands at 1.5 slots
        // past the perk machines).
        let first = crate::zombies::Perks::get().perks.len() as f32 + 2.5;
        // Where the parts lie: a stream of their own (the round engine's draws stay its own).
        let mut rng = crate::weapons::Rng::new(PART_SEED);
        for (k, b) in rules.list.iter().enumerate() {
            let a = step * (first + k as f32);
            let (s, c) = crate::math::det::sin_cos(a);
            let bench = ground(starts[0] + Vec3::new(c, s, 0.0) * w.ring);
            let yaw = crate::math::wrap_angle(a + std::f32::consts::PI);
            let bench_object = host.spawn(&crate::names::buildable_table_object(), owner, bench, yaw);
            let mut parts = Vec::new();
            for (model, _) in &b.parts {
                let start = starts[(rng.next_u32() as usize) % starts.len()];
                let pa = (rng.next_u32() % 3600) as f32 / 3600.0 * std::f32::consts::TAU;
                let (s, c) = crate::math::det::sin_cos(pa);
                let pos = ground(start + Vec3::new(c, s, 0.0) * rules.part_ring);
                let object = host.spawn(&crate::names::buildable_part_object(model), owner, pos, pa);
                parts.push(PartAt { pos, home: pos, object, carrier: 0, built: false });
            }
            self.rounds.buildables.push(BuildableAt { index: k as u8, bench, yaw, bench_object, built_object: 0, parts, building: None });
        }
        host.log(&format!("zombies: {} buildables stood, their parts laid", self.rounds.buildables.len()));
    }

    /// The part `general` carries: (buildable, part).
    pub fn carried_part(&self, general: u32) -> Option<(usize, usize)> {
        self.rounds.buildables.iter().enumerate().find_map(|(b, at)| at.parts.iter().position(|p| p.carrier == general && !p.built).map(|i| (b, i)))
    }

    /// The part lying in `p`'s reach, if any.
    pub fn part_target(&self, p: &PlayerSim) -> Option<(usize, usize)> {
        if !p.survivor || self.is_downed(p.general) {
            return None;
        }
        let t = crate::tuning::get();
        let eye = p.body.eye();
        let reach = t.player_use_radius * crate::units::inches(1.0);
        let mut best: Option<((usize, usize), f32)> = None;
        for (b, at) in self.rounds.buildables.iter().enumerate() {
            for (i, part) in at.parts.iter().enumerate().filter(|(_, part)| part.carrier == 0 && !part.built) {
                let d = (part.pos - eye).length();
                if d <= reach && best.is_none_or(|(_, bd)| d < bd) {
                    best = Some(((b, i), d));
                }
            }
        }
        best.map(|(at, _)| at)
    }

    /// The bench in `p`'s reach, if any.
    pub fn bench_target(&self, p: &PlayerSim) -> Option<usize> {
        if !p.survivor || self.is_downed(p.general) {
            return None;
        }
        let t = crate::tuning::get();
        let eye = p.body.eye();
        let reach = t.player_use_radius * crate::units::inches(1.0);
        self.rounds.buildables.iter().position(|b| (b.bench + Vec3::new(0.0, 0.0, crate::units::inches(t.dropped_item_half_size)) - eye).length() <= reach)
    }

    /// What the bench `b` prompts `p` with (`buildablestub_update_prompt`): the build hint while he
    /// carries a part it lacks, the grab hint once built and he may take it, else nothing.
    pub fn bench_prompt(&self, p: &PlayerSim, b: usize) -> Option<&'static str> {
        let at = self.rounds.buildables.get(b)?;
        let row = Buildables::get().list.get(at.index as usize)?;
        if at.all_built() {
            return self.may_take(p, at).then_some(row.grab_hint.as_str());
        }
        let (cb, _) = self.carried_part(p.general)?;
        (cb == b).then_some(if at.building.is_some() { row.building_hint.as_str() } else { row.build_hint.as_str() })
    }

    /// Whether `p` may take bench `at`'s equipment: he has not got it, and a limited one is not out.
    fn may_take(&self, p: &PlayerSim, at: &BuildableAt) -> bool {
        let Some(row) = Buildables::get().list.get(at.index as usize) else { return false };
        if p.action_slots.holds_weapon(&row.weapon) {
            return false;
        }
        let out = self.players.iter().flatten().filter(|o| o.action_slots.holds_weapon(&row.weapon)).count() as u32;
        row.limit == 0 || out < row.limit
    }

    /// The use key at a part or a bench (before the item rules block it), and the build's frame.
    pub(super) fn buildable_input(&mut self, host: &dyn LogicHost, p: &mut PlayerSim, input: &mut FpsInput) {
        let use_key = crate::input::buttons::FIELD_COMPUTER;
        let used = self.buildable_use(host, p, input);
        if used {
            input.pressed &= !use_key;
            input.held &= !use_key;
        }
    }

    /// [`Sim::buildable_input`]'s work; whether the use key went to it.
    fn buildable_use(&mut self, host: &dyn LogicHost, p: &mut PlayerSim, input: &FpsInput) -> bool {
        let use_key = crate::input::buttons::FIELD_COMPUTER;
        // The build under way: held on, in reach, he keeps building; done, the part is on.
        if let Some(b) = self.rounds.buildables.iter().position(|b| b.building.is_some_and(|(g, ..)| g == p.general)) {
            let rules = Buildables::get();
            let near = self.bench_target(p) == Some(b);
            let held = input.held & use_key != 0 && near && !self.is_downed(p.general);
            let at = &mut self.rounds.buildables[b];
            let Some((g, part, t)) = at.building else { return false };
            if !held {
                at.building = None;
                Self::end_item(p);
                return false;
            }
            let t = t + LOGIC_DT;
            if t < rules.build_time {
                at.building = Some((g, part, t));
                return true;
            }
            at.building = None;
            at.parts[part as usize].built = true;
            at.parts[part as usize].carrier = 0;
            Self::end_item(p);
            let row = &rules.list[at.index as usize];
            host.log(&format!("zombies: General {} built a part of the {}", p.general, row.weapon));
            if at.all_built() {
                let (pos, yaw) = (at.bench + Vec3::new(0.0, 0.0, crate::units::inches(40.0)), at.yaw);
                let owner = self.rounds.spawners.first().copied().unwrap_or(0);
                let built = host.spawn(&crate::names::buildable_built_object(&row.weapon), owner, pos, yaw);
                self.rounds.buildables[b].built_object = built;
                host.log(&format!("zombies: the {} is built", row.weapon));
            }
            return true;
        }
        if input.pressed & use_key == 0 || p.zm_item.is_some() {
            return false;
        }
        // A part in reach: take it, dropping the one he holds where he stands.
        if let Some((b, i)) = self.part_target(p) {
            if self.carried_part(p.general).is_some() {
                self.drop_part(host, p.general, p.body.pos);
            }
            let part = &mut self.rounds.buildables[b].parts[i];
            part.carrier = p.general;
            if part.object != 0 {
                host.destroy(part.object);
                part.object = 0;
            }
            host.log(&format!("zombies: General {} picked up a part", p.general));
            return true;
        }
        let Some(b) = self.bench_target(p) else { return false };
        if self.rounds.buildables[b].all_built() {
            // `bptrigger_think_persistent`: the equipment, free.
            let at = self.rounds.buildables[b].clone();
            if !self.may_take(p, &at) {
                return false;
            }
            let row = &Buildables::get().list[at.index as usize];
            if let Some(def) = crate::weapons::weapon_or_none(&row.weapon) {
                give_equipment(p, def);
                host.log(&format!("zombies: General {} took the {}", p.general, row.weapon));
            }
            return true;
        }
        // A part it lacks in hand: the build starts, the builder up.
        if let Some((_, ci)) = self.carried_part(p.general).filter(|(cb, _)| *cb == b) {
            if self.rounds.buildables[b].building.is_none() {
                // The builder up for the build (a table without it: the build all the same).
                Self::start_item(p, super::items::ItemKind::Build, &Buildables::get().builder, false);
                self.rounds.buildables[b].building = Some((p.general, ci as u8, 0.0));
                return true;
            }
        }
        false
    }

    /// The buildables' frame: a part whose carrier is gone (dead, or no longer a General) goes
    /// back where it was laid.
    pub(super) fn step_buildables(&mut self, host: &dyn LogicHost) {
        let owner = self.rounds.spawners.first().copied().unwrap_or(0);
        let rules = Buildables::get();
        for at in self.rounds.buildables.iter_mut() {
            let row = &rules.list[at.index as usize];
            for (part, (model, _)) in at.parts.iter_mut().zip(&row.parts) {
                if part.carrier != 0 && host.object(part.carrier, part.carrier).is_none_or(|o| o.has(objflags::DEAD)) {
                    part.carrier = 0;
                    part.pos = part.home;
                    part.object = host.spawn(&crate::names::buildable_part_object(model), owner, part.home, 0.0);
                }
            }
        }
    }

    /// A survivor going down or dying drops the part he carries where he is
    /// (`player_drop_piece_on_death`).
    pub(super) fn drop_part(&mut self, host: &dyn LogicHost, general: u32, at: Vec3) {
        let Some((b, i)) = self.carried_part(general) else { return };
        let model = Buildables::get().list[self.rounds.buildables[b].index as usize].parts[i].0.clone();
        let owner = self.rounds.spawners.first().copied().unwrap_or(0);
        let part = &mut self.rounds.buildables[b].parts[i];
        part.carrier = 0;
        part.pos = at;
        part.object = host.spawn(&crate::names::buildable_part_object(&model), owner, at, 0.0);
        if self.rounds.buildables[b].building.is_some_and(|(g, ..)| g == general) {
            self.rounds.buildables[b].building = None;
        }
    }

    /// `unbuild_buildable` with its parts sent home: the equipment `weapon` is gone (an overheated
    /// jetgun), its bench bare again and its parts back where they were laid.
    pub(super) fn unbuild(&mut self, host: &dyn LogicHost, weapon: &str, _at: Vec3) {
        let rules = Buildables::get();
        let owner = self.rounds.spawners.first().copied().unwrap_or(0);
        let Some(b) = self.rounds.buildables.iter().position(|b| rules.list.get(b.index as usize).is_some_and(|r| r.weapon == weapon)) else { return };
        let row = &rules.list[self.rounds.buildables[b].index as usize];
        let at = &mut self.rounds.buildables[b];
        if at.built_object != 0 {
            host.destroy(at.built_object);
            at.built_object = 0;
        }
        for (part, (model, _)) in at.parts.iter_mut().zip(&row.parts) {
            if part.built {
                part.built = false;
                part.pos = part.home;
                part.object = host.spawn(&crate::names::buildable_part_object(model), owner, part.home, 0.0);
            }
        }
    }
}

/// `equipment_give`: `def` onto `p`'s action slot 1, the equipment he had before taken
/// (`equipment_take`), with one round in its clip.
pub(super) fn give_equipment(p: &mut PlayerSim, def: &'static crate::weapons::WeaponDef) {
    if let ActionSlot::Weapon { def: old } = p.action_slots.slots[0] {
        if p.arsenal.item_up.is_some_and(|d| d.id == old.id) {
            p.arsenal.stow_item();
        }
        p.arsenal.items.retain(|w| w.main.id != old.id);
    }
    p.arsenal.give_item(def, 1);
    p.action_slots.slots[0] = ActionSlot::Weapon { def };
    if def.id == crate::zombies::RiotShield::get().weapon {
        // `onbuyweapon_riotshield`: a new shield, unworn.
        p.shield_wear = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::sim::fakehost::{general, FakeHost};

    fn survivor() -> (Sim, FakeHost) {
        let host = FakeHost::default();
        host.spawn_objects.set(true);
        host.add(ObjInfo { flags: objflags::INFANTRY | objflags::GENERAL | objflags::SURVIVOR, ..general(1, 0) });
        let mut sim = Sim::new();
        sim.rounds.zombies_map = true;
        sim.take_control(&host, 0, 1);
        sim.rounds.players = vec![0];
        sim.place_buildables(&host);
        (sim, host)
    }

    fn press(sim: &mut Sim, host: &FakeHost, p: &mut PlayerSim, held: bool, pressed: bool) {
        let use_key = crate::input::buttons::FIELD_COMPUTER;
        let mut input = FpsInput { held: if held { use_key } else { 0 }, pressed: if pressed { use_key } else { 0 }, ..Default::default() };
        sim.buildable_input(host, p, &mut input);
    }

    /// The shield's two parts picked up one at a time, each built on in `BuildTime` with the use
    /// key held, letting go stopping it; built, the bench gives the shield onto action slot 1.
    #[test]
    fn parts_are_carried_one_at_a_time_built_on_and_the_shield_taken() {
        let (mut sim, host) = survivor();
        let rules = Buildables::get();
        let b = sim.rounds.buildables.iter().position(|b| rules.list[b.index as usize].weapon == "riotshield_zm").unwrap();
        let mut p = sim.players[0].take().unwrap();
        let bench = sim.rounds.buildables[b].bench;
        for i in 0..2 {
            p.body.pos = sim.rounds.buildables[b].parts[i].pos;
            assert!(sim.part_target(&p).is_some());
            press(&mut sim, &host, &mut p, true, true);
            assert_eq!(sim.carried_part(1), Some((b, i)), "carried");
            p.body.pos = bench;
            press(&mut sim, &host, &mut p, true, true);
            assert!(sim.rounds.buildables[b].building.is_some(), "building");
            if i == 0 {
                // Let go halfway: it stops, the part still in hand.
                for _ in 0..30 {
                    press(&mut sim, &host, &mut p, true, false);
                }
                press(&mut sim, &host, &mut p, false, false);
                assert!(sim.rounds.buildables[b].building.is_none() && !sim.rounds.buildables[b].parts[0].built);
                press(&mut sim, &host, &mut p, true, true);
            }
            for _ in 0..(rules.build_time / LOGIC_DT).round() as usize + 1 {
                press(&mut sim, &host, &mut p, true, false);
            }
            assert!(sim.rounds.buildables[b].parts[i].built, "part {i} on");
        }
        assert!(sim.rounds.buildables[b].all_built());
        assert_eq!(sim.bench_prompt(&p, b), Some(rules.list[sim.rounds.buildables[b].index as usize].grab_hint.as_str()));
        press(&mut sim, &host, &mut p, true, true);
        assert!(p.action_slots.holds_weapon("riotshield_zm") || crate::weapons::weapon_or_none("riotshield_zm").is_none());
        sim.players[0] = Some(p);
    }

    /// Going down drops the part he carries where he is.
    #[test]
    fn going_down_drops_the_part() {
        let (mut sim, host) = survivor();
        let mut p = sim.players[0].take().unwrap();
        p.body.pos = sim.rounds.buildables[0].parts[0].pos;
        press(&mut sim, &host, &mut p, true, true);
        sim.players[0] = Some(p);
        assert!(sim.carried_part(1).is_some());
        sim.drop_part(&host, 1, Vec3::new(5.0, 6.0, 0.0));
        assert!(sim.carried_part(1).is_none());
        assert_eq!(sim.rounds.buildables[0].parts[0].pos, Vec3::new(5.0, 6.0, 0.0));
    }

    #[test]
    fn tranzit_has_the_shield_and_the_jetgun_to_build() {
        let b = Buildables::get();
        let shield = b.list.iter().find(|r| r.weapon == "riotshield_zm").unwrap();
        assert_eq!(shield.parts.len(), 2);
        let jet = b.list.iter().find(|r| r.weapon == "jetgun_zm").unwrap();
        assert_eq!((jet.parts.len(), jet.limit), (4, 1));
        assert_eq!(b.build_time, 3.0);
        assert_eq!(b.builder, "zombie_builder_zm");
    }
}
