/*
** Conquer Ops - Zero Hour gameplay core: BO2 Zombies' Mystery Box.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! BO2's Mystery Box (`_zm_magicbox.gsc`, rules in `zombies.ini` `[MagicBox]`) on a Zombies map.
//! The round engine gives each player start a place for it on the ring between the wall buys (the
//! mod's own map); the box stands at one at a time, like BO2's chests.
//!
//! * **A pull** (`treasure_chest_think`): the use key on the box with its cost; it spins
//!   `SpinTime` and picks the first weapon of a shuffled roster the box offers, the buyer lacks
//!   (nor its upgrade) and is under its limit across the players
//!   (`treasure_chest_chooseweightedrandomweapon`).
//! * **The teddy** (`treasure_chest_weapon_spawn`): with more than one place, a pull may fly the box
//!   away instead ([`crate::zombies::MagicBox::joker`]); the buyer gets his points back and the box
//!   shows at its next place `MoveTime` later (`treasure_chest_move`, `default_box_move_logic`).
//! * **The weapon** floats over the box for `OfferTime`; only the buyer may take it with the use
//!   key (`weapon_give`, as a wall buy gives a gun). Taken or not, the pull counts toward the teddy
//!   (`level.chest_accessed`) and the box closes for `CloseTime`.

use super::*;
use crate::zombies::MagicBox;

/// One of the box's places.
#[derive(Clone, Debug, PartialEq)]
pub struct BoxAt {
    pub pos: Vec3,
    pub yaw: f32,
}

crate::statehash::state_hash!(struct BoxAt { pos, yaw });

/// Where the box is in a pull.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum BoxPhase {
    #[default]
    Idle,
    /// Spinning for `user` (his General), who paid `cost`.
    Spinning { user: u32, player: i32, cost: u32, t: f32 },
    /// `weapon` (an id) floats for `user`.
    Offering { user: u32, weapon: String, t: f32 },
    Closing { t: f32 },
    /// Flown away; shows at its next place when the time is up.
    Moving { t: f32 },
}

crate::statehash::state_hash!(enum BoxPhase { Idle, Spinning { user, player, cost, t }, Offering { user, weapon, t }, Closing { t }, Moving { t } });

/// The box's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoxState {
    pub places: Vec<BoxAt>,
    /// The place it stands at (`level.chest_index`), and its prop there (0: none).
    pub at: u8,
    pub object: u32,
    /// The floating weapon's prop.
    pub weapon_object: u32,
    /// Pulls since it last moved (`level.chest_accessed`) and its moves (`level.chest_moves`).
    pub accessed: u32,
    pub moves: u32,
    pub phase: BoxPhase,
}

crate::statehash::state_hash!(struct BoxState { places, at, object, weapon_object, accessed, moves, phase });

impl BoxState {
    pub fn pos(&self) -> Option<Vec3> {
        if matches!(self.phase, BoxPhase::Moving { .. }) {
            return None;
        }
        self.places.get(self.at as usize).map(|b| b.pos)
    }
}

/// Whether `p` has `id` or its upgrade (`has_weapon_or_upgrade`, by base).
fn has_weapon_or_upgrade(p: &PlayerSim, id: &str, upgrade: Option<&str>) -> bool {
    p.arsenal.weapons.iter().any(|w| {
        let base = crate::weapons::base_of(&w.main.id);
        base == id || upgrade == Some(base)
    })
}

impl Sim {
    /// The box's places (the round engine's start): one per survivor start on the ring, half a
    /// wall buy's step round from the first, the box at the first.
    pub(super) fn place_magic_box(&mut self, host: &dyn LogicHost) {
        let ring = crate::zombies::WallBuys::get().ring;
        let step = std::f32::consts::TAU / crate::zombies::WallBuys::get().buys.len().max(1) as f32;
        let starts: Vec<Vec3> = self.living_survivors(host).iter().map(|o| o.pos).collect();
        if starts.is_empty() || !self.rounds.magic_box.places.is_empty() {
            return;
        }
        for start in starts {
            let a = step * 0.5;
            let (s, c) = crate::math::det::sin_cos(a);
            let pos = start + Vec3::new(c, s, 0.0) * ring;
            let pos = Vec3::new(pos.x, pos.y, host.ground_height(pos.x, pos.y));
            self.rounds.magic_box.places.push(BoxAt { pos, yaw: crate::math::wrap_angle(a + std::f32::consts::PI) });
        }
        self.show_box(host);
    }

    fn show_box(&mut self, host: &dyn LogicHost) {
        let b = &mut self.rounds.magic_box;
        let Some(place) = b.places.get(b.at as usize).cloned() else { return };
        let owner = self.rounds.spawners.first().copied().unwrap_or(0);
        b.object = host.spawn(&crate::names::magic_box_object(), owner, place.pos, place.yaw);
        host.log(&format!("zombies: the Mystery Box stands at place {} of {}", b.at + 1, b.places.len()));
    }

    /// The box in `p`'s use list now (as a dropped weapon's, `player_useRadius`), if it is.
    pub fn magic_box_in_reach(&self, p: &PlayerSim) -> bool {
        if !p.survivor || self.is_downed(p.general) {
            return false;
        }
        let Some(pos) = self.rounds.magic_box.pos() else { return false };
        let t = crate::tuning::get();
        let to = pos + Vec3::new(0.0, 0.0, crate::units::inches(t.dropped_item_half_size)) - p.body.eye();
        to.length() / crate::units::inches(1.0) <= t.player_use_radius
    }

    /// What the box's prompt says to `p` (its key), if anything: the pull, or the buyer's take.
    pub fn magic_box_prompt(&self, p: &PlayerSim) -> Option<&'static str> {
        if !self.magic_box_in_reach(p) {
            return None;
        }
        let b = MagicBox::get();
        match &self.rounds.magic_box.phase {
            BoxPhase::Idle => Some(b.hint.as_str()),
            BoxPhase::Offering { user, .. } if *user == p.general => Some(b.take_hint.as_str()),
            _ => None,
        }
    }

    /// The use key on the box: a pull, or the buyer taking his weapon. True when the press was the
    /// box's.
    pub(super) fn magic_box_input(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, input: &mut FpsInput) -> bool {
        if input.pressed & buttons::FIELD_COMPUTER == 0 || !self.magic_box_in_reach(p) {
            return false;
        }
        let rules = MagicBox::get();
        match self.rounds.magic_box.phase.clone() {
            BoxPhase::Idle => {
                input.pressed &= !buttons::FIELD_COMPUTER;
                let Some(player) = host.object(p.general, p.general).map(|o| o.owner_player) else { return true };
                if host.money(player).max(0) as u32 >= rules.cost {
                    host.add_cash(player, -(rules.cost as i32));
                    self.rounds.magic_box.phase = BoxPhase::Spinning { user: p.general, player, cost: rules.cost, t: 0.0 };
                    self.emit(idx, EventKind::ZombiePurchase { cost: rules.cost.min(u16::MAX as u32) as u16 });
                    host.log(&format!("zombies: General {} pulls the Mystery Box", p.general));
                }
                true
            }
            BoxPhase::Offering { user, weapon, .. } if user == p.general => {
                input.pressed &= !buttons::FIELD_COMPUTER;
                if let Some(def) = crate::weapons::weapon_or_none(&weapon) {
                    super::wallbuy::give(p, def);
                    host.log(&format!("zombies: General {} took {weapon} from the box", p.general));
                }
                self.close_box(host);
                true
            }
            _ => false,
        }
    }

    /// The weapon taken or timed out: the pull counts, the box closes.
    fn close_box(&mut self, host: &dyn LogicHost) {
        let b = &mut self.rounds.magic_box;
        if b.weapon_object != 0 {
            host.destroy(b.weapon_object);
            b.weapon_object = 0;
        }
        b.accessed += 1;
        b.phase = BoxPhase::Closing { t: 0.0 };
    }

    /// `treasure_chest_chooseweightedrandomweapon` for the survivor driving `user`.
    fn box_weapon(&mut self, user: u32) -> Option<String> {
        let mut keys: Vec<crate::zombies::ZmWeapon> = crate::zombies::roster().into_iter().filter(|w| crate::weapons::weapon_or_none(&w.id).is_some()).collect();
        for i in (1..keys.len()).rev() {
            let j = (self.rounds.rng.next_u32() % (i as u32 + 1)) as usize;
            keys.swap(i, j);
        }
        let me = self.players.iter().flatten().find(|p| p.general == user);
        let held_by_others = |w: &crate::zombies::ZmWeapon| self.players.iter().flatten().filter(|p| p.survivor && p.general != user && has_weapon_or_upgrade(p, &w.id, w.upgrade.as_deref())).count() as u32;
        let eligible = |w: &crate::zombies::ZmWeapon| {
            w.in_box && !me.is_some_and(|p| has_weapon_or_upgrade(p, &w.id, w.upgrade.as_deref())) && w.limit.is_none_or(|limit| held_by_others(w) < limit)
        };
        keys.iter().find(|w| eligible(w)).or(keys.first()).map(|w| w.id.clone())
    }

    /// The box's frame.
    pub(super) fn step_magic_box(&mut self, host: &dyn LogicHost) {
        let rules = MagicBox::get();
        match self.rounds.magic_box.phase.clone() {
            BoxPhase::Idle => {}
            BoxPhase::Spinning { user, player, cost, t } => {
                let t = t + LOGIC_DT;
                if t < rules.spin_time {
                    self.rounds.magic_box.phase = BoxPhase::Spinning { user, player, cost, t };
                    return;
                }
                let weapon = self.box_weapon(user);
                let b = &self.rounds.magic_box;
                let random = self.rounds.rng.next_u32() % 100;
                if b.places.len() > 1 && rules.joker(b.accessed, b.moves, random) {
                    // The teddy: his points back (not counted as earned), the box away.
                    host.add_cash(player, cost as i32);
                    let b = &mut self.rounds.magic_box;
                    b.accessed = 0;
                    b.moves += 1;
                    if b.object != 0 {
                        host.destroy(b.object);
                        b.object = 0;
                    }
                    b.phase = BoxPhase::Moving { t: 0.0 };
                    host.log("zombies: the teddy bear; the Mystery Box flies away");
                    return;
                }
                let Some(weapon) = weapon else {
                    self.rounds.magic_box.phase = BoxPhase::Closing { t: 0.0 };
                    return;
                };
                let b = &mut self.rounds.magic_box;
                if let (Some(place), Some(def)) = (b.places.get(b.at as usize), crate::weapons::weapon_or_none(&weapon)) {
                    if !def.drop_object.is_empty() {
                        let pos = place.pos + Vec3::new(0.0, 0.0, crate::units::inches(rules.float_height));
                        let owner = self.rounds.spawners.first().copied().unwrap_or(0);
                        b.weapon_object = host.spawn(&def.drop_object, owner, pos, place.yaw + std::f32::consts::PI);
                    }
                }
                host.log(&format!("zombies: the Mystery Box offers {weapon}"));
                self.rounds.magic_box.phase = BoxPhase::Offering { user, weapon, t: 0.0 };
            }
            BoxPhase::Offering { user, weapon, t } => {
                let t = t + LOGIC_DT;
                if t >= rules.offer_time {
                    self.close_box(host);
                } else {
                    self.rounds.magic_box.phase = BoxPhase::Offering { user, weapon, t };
                }
            }
            BoxPhase::Closing { t } => {
                let t = t + LOGIC_DT;
                self.rounds.magic_box.phase = if t >= rules.close_time { BoxPhase::Idle } else { BoxPhase::Closing { t } };
            }
            BoxPhase::Moving { t } => {
                let t = t + LOGIC_DT;
                if t < rules.move_time {
                    self.rounds.magic_box.phase = BoxPhase::Moving { t };
                    return;
                }
                // `default_box_move_logic`: the next place round (one other than this).
                let b = &mut self.rounds.magic_box;
                b.at = ((b.at as usize + 1) % b.places.len().max(1)) as u8;
                b.phase = BoxPhase::Idle;
                self.show_box(host);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::fakehost::{general, FakeHost};

    fn two_starts() -> (Sim, FakeHost) {
        let host = FakeHost::default();
        host.spawn_objects.set(true);
        host.add(ObjInfo { flags: objflags::INFANTRY | objflags::GENERAL | objflags::SURVIVOR, ..general(1, 0) });
        host.add(ObjInfo { id: 2, flags: objflags::INFANTRY | objflags::GENERAL | objflags::SURVIVOR, pos: Vec3::new(2000.0, 0.0, 0.0), ..general(2, 1) });
        host.wallet.borrow_mut().push((0, 5000));
        let mut sim = Sim::new();
        sim.rounds.zombies_map = true;
        sim.take_control(&host, 0, 1);
        sim.take_control(&host, 1, 2);
        sim.rounds.phase = crate::sim::zombies::Phase::Between { left: 1e6 };
        sim.place_magic_box(&host);
        (sim, host)
    }

    #[test]
    fn a_place_per_start_and_the_box_at_the_first() {
        let (sim, host) = two_starts();
        assert_eq!(sim.rounds.magic_box.places.len(), 2);
        assert_eq!(host.spawned.borrow().iter().filter(|(_, t)| *t == crate::names::magic_box_object()).count(), 1);
    }

    #[test]
    fn a_pull_spins_four_seconds_and_a_teddy_refunds_and_moves_the_box() {
        let (mut sim, host) = two_starts();
        sim.rounds.magic_box.accessed = 8; // never moved: the teddy is certain
        sim.rounds.magic_box.phase = BoxPhase::Spinning { user: 1, player: 0, cost: 950, t: 0.0 };
        host.cash.borrow_mut().push((0, -950));
        let step = |sim: &mut Sim, n: u32| {
            for _ in 0..n {
                let f = sim.frame + 1;
                sim.step(&host, f);
            }
        };
        step(&mut sim, (3.9 / LOGIC_DT) as u32);
        assert!(matches!(sim.rounds.magic_box.phase, BoxPhase::Spinning { .. }));
        step(&mut sim, 6);
        assert!(matches!(sim.rounds.magic_box.phase, BoxPhase::Moving { .. }));
        assert_eq!(host.money(0), 5000, "his 950 back");
        assert_eq!((sim.rounds.magic_box.accessed, sim.rounds.magic_box.moves), (0, 1));
        step(&mut sim, (12.2 / LOGIC_DT) as u32);
        assert_eq!((sim.rounds.magic_box.at, &sim.rounds.magic_box.phase), (1, &BoxPhase::Idle), "at the next place");
    }

    #[test]
    fn an_offer_left_alone_times_out_and_counts_as_a_pull() {
        let (mut sim, host) = two_starts();
        sim.rounds.magic_box.phase = BoxPhase::Offering { user: 1, weapon: "an94_mp".into(), t: 0.0 };
        for _ in 0..((12.0 + 1.0) / LOGIC_DT) as u32 + 3 {
            let f = sim.frame + 1;
            sim.step(&host, f);
        }
        assert_eq!((sim.rounds.magic_box.accessed, &sim.rounds.magic_box.phase), (1, &BoxPhase::Idle));
    }
}
