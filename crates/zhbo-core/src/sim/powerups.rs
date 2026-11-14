/*
** Conquer Ops - Zero Hour gameplay core: BO2 Zombies' power-ups.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! BO2 Zombies' power-ups (`_zm_powerups.gsc`, rules in `zombies.ini` `[PowerUps]`):
//!
//! * **When one is due** (`watch_for_drop`): once the points the players have earned pass the
//!   threshold (at first `players * zombie_score_start_<n>p` plus `DropIncrement`), the next kill
//!   drops one; the increment grows by `IncrementGrowth` and the threshold moves past the total.
//! * **The drop** (`powerup_drop`, from `zombie_death_points`: a kill by a player): at most
//!   `DropMaxPerRound` a round; a kill drops one anyway `RandomChance` times in 100. Which: the
//!   next of the power-ups in a shuffled order that drops with the regular ones (`get_valid_powerup`,
//!   the order shuffled again each time round, `array_randomize`).
//! * **On the ground** `DropHeight` over the kill for `Life` seconds and its `BlinkTime` of blinking
//!   (`powerup_timeout`); a survivor within `GrabRange` takes it (`powerup_grab`).
//! * **What it does:** the nuke kills every zombie of the round (0.5 s, then one after another) and
//!   pays every player 400; insta-kill makes any player hit on a zombie a kill, double points
//!   doubles every point, each for `Duration` seconds; max ammo fills every weapon and grenade of
//!   each survivor up (not one in last stand).
//!
//! The world object is the converter's prop of the power-up's model (`names::powerup_object`);
//! the record is the state (hashed with the round engine's).

use super::*;
use crate::zombies::{Drops, PowerUps};

/// A power-up lying on the ground.
#[derive(Clone, Debug, PartialEq)]
pub struct Dropped {
    /// Its row in [`PowerUps`]'s list.
    pub kind: u8,
    /// Its origin (the drop point lifted by `DropHeight`).
    pub pos: Vec3,
    pub object: u32,
    /// Seconds since it dropped.
    pub age: f32,
}

crate::statehash::state_hash!(struct Dropped { kind, pos, object, age });

/// The power-ups' state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PowerUpState {
    pub dropped: Vec<Dropped>,
    /// `level.zombie_powerup_array` (rows of [`PowerUps`], shuffled) and its next index.
    pub order: Vec<u8>,
    pub next: u8,
    /// This round's drops (`level.powerup_drop_count`).
    pub drop_count: u32,
    /// The points every player has earned (BO2's `score_total`, summed).
    pub score_total: i32,
    /// `watch_for_drop`'s threshold and increment, and `zombie_drop_item`: a drop is due.
    pub score_to_drop: f32,
    pub increment: f32,
    pub drop_item: bool,
    /// Seconds left of insta-kill and double points.
    pub insta_kill: f32,
    pub double_points: f32,
    /// The nuke's zombies still to die, each with the seconds until it does.
    pub nuke: Vec<(u32, f32)>,
    /// The logic frame of the last nuke (its white flash) and of the last max ammo (its text).
    pub nuke_frame: u32,
    pub max_ammo_frame: u32,
}

crate::statehash::state_hash!(struct PowerUpState {
    dropped, order, next, drop_count, score_total, score_to_drop, increment, drop_item, insta_kill, double_points, nuke, nuke_frame, max_ammo_frame,
});

impl PowerUpState {
    /// `array_randomize`: the order shuffled (Fisher-Yates, from the end).
    fn shuffle(&mut self, rng: &mut crate::weapons::Rng) {
        for i in (1..self.order.len()).rev() {
            let j = (rng.next_u32() % (i as u32 + 1)) as usize;
            self.order.swap(i, j);
        }
    }

    /// `get_next_powerup` until one drops with the regular ones (`get_valid_powerup`).
    fn next_valid(&mut self, rng: &mut crate::weapons::Rng) -> Option<u8> {
        let p = PowerUps::get();
        if !self.order.iter().any(|&k| p.kinds.get(k as usize).is_some_and(|k| k.drops == Drops::Always)) {
            return None;
        }
        loop {
            let k = self.order[self.next as usize];
            self.next += 1;
            if self.next as usize >= self.order.len() {
                self.next = 0;
                self.shuffle(rng);
            }
            if p.kinds.get(k as usize).is_some_and(|k| k.drops == Drops::Always) {
                return Some(k);
            }
        }
    }

    /// `get_points_multiplier`: 2 under double points.
    pub fn multiplier(&self) -> i32 {
        if self.double_points > 0.0 {
            2
        } else {
            1
        }
    }
}

/// `round_up_score( points, 5 )`.
fn round_up_to_five(points: i32) -> i32 {
    (points + 4).div_euclid(5) * 5
}

impl Sim {
    /// The power-ups' start with the round engine's (`init_powerups`, `watch_for_drop`'s first
    /// threshold).
    pub(super) fn powerups_start(&mut self) {
        let p = PowerUps::get();
        let s = &mut self.rounds.powerups;
        s.order = (0..p.kinds.len() as u8).collect();
        s.shuffle(&mut self.rounds.rng);
        s.next = 0;
        s.increment = p.drop_increment;
        let players = self.rounds.players.len().clamp(1, 8);
        s.score_to_drop = players as f32 * crate::zombies::Vars::get().score_start[players - 1] as f32 + s.increment;
    }

    /// Points paid to `player` (`player_add_points`): scaled by double points and rounded up to
    /// five, counted toward the next power-up. Returns what was paid.
    pub(super) fn pay_points(&mut self, host: &dyn LogicHost, player: i32, points: i32) -> i32 {
        let paid = self.rounds.powerups.multiplier() * round_up_to_five(points);
        host.add_cash(player, paid);
        self.rounds.powerups.score_total += paid;
        paid
    }

    /// `powerup_drop` at a kill's point `at` (on the ground).
    pub(super) fn powerup_drop(&mut self, host: &dyn LogicHost, at: Vec3) {
        let p = PowerUps::get();
        if self.rounds.powerups.drop_count >= p.drop_max_per_round {
            return;
        }
        let roll = self.rounds.rng.next_u32() % 100;
        if roll >= p.random_chance && !self.rounds.powerups.drop_item {
            return;
        }
        let Some(kind) = self.rounds.powerups.next_valid(&mut self.rounds.rng) else { return };
        self.rounds.powerups.drop_count += 1;
        let pos = at + Vec3::new(0.0, 0.0, crate::units::inches(p.drop_height));
        let owner = self.rounds.spawners.first().copied().unwrap_or(0);
        let object = host.spawn(&crate::names::powerup_object(&p.kinds[kind as usize].name), owner, pos, 0.0);
        self.rounds.powerups.dropped.push(Dropped { kind, pos, object, age: 0.0 });
        self.rounds.powerups.drop_item = false;
        host.log(&format!("zombies: a {} dropped", p.kinds[kind as usize].name));
    }

    /// The power-ups' frame: the threshold, the timers, the nuke, and each one on the ground (its
    /// timeout, a survivor taking it).
    pub(super) fn step_powerups(&mut self, host: &dyn LogicHost) {
        let p = PowerUps::get();
        let s = &mut self.rounds.powerups;
        // `watch_for_drop` looks every 0.5 s.
        if self.frame.is_multiple_of(15) && s.score_total as f32 > s.score_to_drop {
            s.increment *= p.increment_growth;
            s.score_to_drop = s.score_total as f32 + s.increment;
            s.drop_item = true;
        }
        s.insta_kill = (s.insta_kill - LOGIC_DT).max(0.0);
        s.double_points = (s.double_points - LOGIC_DT).max(0.0);
        let mut nuked = Vec::new();
        s.nuke.retain_mut(|(id, left)| {
            *left -= LOGIC_DT;
            if *left <= 0.0 {
                nuked.push(*id);
                false
            } else {
                true
            }
        });
        for id in nuked {
            host.kill(id);
        }
        let mut i = 0;
        while i < self.rounds.powerups.dropped.len() {
            let d = &mut self.rounds.powerups.dropped[i];
            d.age += LOGIC_DT;
            if d.age >= p.life + p.blink_time {
                let d = self.rounds.powerups.dropped.remove(i);
                if d.object != 0 {
                    host.destroy(d.object);
                }
                continue;
            }
            let (kind, pos) = (d.kind, d.pos);
            let range = crate::units::inches(p.grab_range);
            let taker = self.survivors().into_iter().find(|s| host.object(s.general, s.general).is_some_and(|o| !o.has(objflags::DEAD)) && (s.body.pos - pos).length() < range).map(|s| s.general);
            if let Some(general) = taker {
                let d = self.rounds.powerups.dropped.remove(i);
                if d.object != 0 {
                    host.destroy(d.object);
                }
                self.grab_powerup(host, kind, pos, general);
                continue;
            }
            i += 1;
        }
    }

    /// Survivor `general` took power-up `kind` at `pos`.
    fn grab_powerup(&mut self, host: &dyn LogicHost, kind: u8, pos: Vec3, general: u32) {
        let p = PowerUps::get();
        let name = p.kinds[kind as usize].name.clone();
        host.log(&format!("zombies: General {general} took the {name}"));
        match name.as_str() {
            "nuke" => {
                // Nearest first, 0.5 s, then each randomfloatrange( 0.1, 0.7 ) after the last.
                let mut zombies: Vec<(u32, f32)> = self.rounds.alive.iter().filter_map(|&id| host.object(id, id).map(|o| (id, (o.pos - pos).length()))).collect();
                zombies.sort_by(|a, b| a.1.total_cmp(&b.1));
                let mut t = p.nuke_delay;
                for (id, _) in zombies {
                    let r = (self.rounds.rng.next_u32() % 1_000_000) as f32 / 1_000_000.0;
                    t += p.nuke_gap.0 + (p.nuke_gap.1 - p.nuke_gap.0) * r;
                    self.rounds.powerups.nuke.push((id, t));
                }
                for player in self.rounds.players.clone() {
                    self.pay_points(host, player, p.nuke_points);
                }
                self.rounds.powerups.nuke_frame = self.frame.max(1);
            }
            "insta_kill" => self.rounds.powerups.insta_kill = p.duration,
            "double_points" => self.rounds.powerups.double_points = p.duration,
            "full_ammo" => {
                for idx in 0..self.players.len() {
                    let Some(mut s) = self.players[idx].take() else { continue };
                    if s.survivor && !self.is_downed(s.general) {
                        max_ammo(&mut s.arsenal);
                    }
                    self.players[idx] = Some(s);
                }
                self.rounds.powerups.max_ammo_frame = self.frame.max(1);
            }
            _ => {}
        }
        self.emit_all(EventKind::ZombiePowerUp { kind });
    }

    /// Whether a player's hit on a zombie kills it outright (insta-kill on).
    pub fn insta_kill_on(&self) -> bool {
        self.rounds.powerups.insta_kill > 0.0
    }
}

/// `givemaxammo` on every weapon and grenade (`full_ammo_powerup`).
pub fn max_ammo(a: &mut crate::weapons::Arsenal) {
    for w in &mut a.weapons {
        let main_up = std::ptr::eq(w.def, w.main);
        let alt_max = w.alt_mode.filter(|m| m.alt_own_ammo).map(|m| m.max_reserve);
        if main_up {
            w.reserve = w.main.max_reserve;
            if let Some(m) = alt_max {
                w.alt_reserve = m;
            }
        } else {
            w.alt_reserve = w.main.max_reserve;
            if let Some(m) = alt_max {
                w.reserve = m;
            }
        }
    }
    for o in [&mut a.lethal, &mut a.tactical] {
        if !o.def.id.is_empty() {
            o.count = o.def.max_reserve.max(o.def.start_reserve).max(o.count).max(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shuffled_order_skips_what_never_drops_and_reshuffles_round() {
        let mut s = PowerUpState { order: (0..PowerUps::get().kinds.len() as u8).collect(), ..Default::default() };
        let mut rng = crate::weapons::Rng::new(7);
        s.shuffle(&mut rng);
        let p = PowerUps::get();
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..40 {
            let k = s.next_valid(&mut rng).unwrap();
            assert_eq!(p.kinds[k as usize].drops, Drops::Always);
            seen.insert(p.kinds[k as usize].name.clone());
        }
        assert_eq!(seen.len(), 4, "every regular one comes up");
    }

    fn survivor() -> (Sim, crate::sim::fakehost::FakeHost) {
        use crate::sim::fakehost::{general, FakeHost};
        let host = FakeHost::default();
        host.spawn_objects.set(true);
        host.add(ObjInfo { flags: objflags::INFANTRY | objflags::GENERAL | objflags::SURVIVOR, ..general(1, 0) });
        let mut sim = Sim::new();
        sim.rounds.zombies_map = true;
        sim.take_control(&host, 0, 1);
        sim.rounds.players = vec![0];
        // Rounds running (a long wait before the next), with no spawn points to start one.
        sim.rounds.phase = crate::sim::zombies::Phase::Between { left: 1e6 };
        sim.powerups_start();
        (sim, host)
    }

    #[test]
    fn a_due_drop_lands_over_the_kill_and_a_survivor_walking_over_it_takes_it() {
        let (mut sim, host) = survivor();
        assert!((sim.rounds.powerups.score_to_drop - (500.0 + 2000.0)).abs() < 1e-3, "one player: 500 + 2000");
        sim.rounds.powerups.drop_item = true;
        sim.powerup_drop(&host, Vec3::ZERO);
        let d = &sim.rounds.powerups.dropped[0];
        assert!((d.pos.z - crate::units::inches(40.0)).abs() < 1e-4, "40 inches up");
        assert!(host.spawned.borrow().iter().any(|(_, t)| t.starts_with("ZHBO_ZM_PowerUp_")));
        assert!(!sim.rounds.powerups.drop_item, "the due drop is spent");
        let f = sim.frame + 1;
        sim.step(&host, f);
        assert!(sim.rounds.powerups.dropped.is_empty(), "he stands on it: taken");
        assert!(sim.events.iter().any(|e| matches!(e.kind, EventKind::ZombiePowerUp { .. })));
    }

    #[test]
    fn four_drops_a_round_at_most_and_one_left_lying_times_out() {
        let (mut sim, host) = survivor();
        let far = Vec3::new(500.0, 0.0, 0.0);
        for _ in 0..6 {
            sim.rounds.powerups.drop_item = true;
            sim.powerup_drop(&host, far);
        }
        assert_eq!(sim.rounds.powerups.dropped.len(), 4);
        for _ in 0..((15.0 + 11.5) / LOGIC_DT) as u32 + 2 {
            let f = sim.frame + 1;
            sim.step(&host, f);
        }
        assert!(sim.rounds.powerups.dropped.is_empty(), "26.5 s and gone");
        assert_eq!(host.destroyed.borrow().len(), 4);
    }

    #[test]
    fn the_nuke_kills_the_round_and_pays_400_and_double_points_doubles_it() {
        let (mut sim, host) = survivor();
        for id in [90, 91] {
            host.add(ObjInfo { id, owner_player: 7, flags: objflags::INFANTRY | objflags::ZOMBIE, pos: Vec3::new(50.0, 0.0, 0.0), ..Default::default() });
            sim.rounds.alive.push(id);
        }
        let nuke = PowerUps::get().index("nuke").unwrap() as u8;
        sim.rounds.powerups.double_points = 10.0;
        sim.grab_powerup(&host, nuke, Vec3::ZERO, 1);
        assert_eq!(*host.cash.borrow(), vec![(0, 800)], "400, doubled");
        for _ in 0..(2.0 / LOGIC_DT) as u32 {
            let f = sim.frame + 1;
            sim.step(&host, f);
        }
        assert_eq!(*host.killed.borrow(), vec![90, 91]);
    }

    #[test]
    fn max_ammo_fills_every_gun() {
        let mut a = crate::weapons::Arsenal::with_loadout(crate::weapons::Loadout { primary: "an94_mp".into(), secondary: "fnp45_mp".into(), held_knife: false, ..crate::weapons::Loadout::unarmed() });
        for w in &mut a.weapons {
            w.reserve = 0;
        }
        max_ammo(&mut a);
        assert!(a.weapons.iter().all(|w| w.reserve == w.main.max_reserve));
    }

    #[test]
    fn points_round_up_to_five_and_double() {
        assert_eq!(round_up_to_five(10), 10);
        assert_eq!(round_up_to_five(11), 15);
        let mut s = PowerUpState::default();
        assert_eq!(s.multiplier(), 1);
        s.double_points = 1.0;
        assert_eq!(s.multiplier(), 2);
    }
}
