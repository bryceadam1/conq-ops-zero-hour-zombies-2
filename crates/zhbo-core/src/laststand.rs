/*
** Conquer Ops - Zero Hour gameplay core: BO2 Zombies' last stand in Zero Hour's damage path.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! A survivor's health is the engine's, so whether a blow kills him or downs him is decided where
//! the engine applies it: the bridge's damage hook (`Zhbo_ShieldDamage`, host patch 0017) asks
//! [`zhbo_logic_survivor_damage`] for every blow on a survivor on a Zombies map, with the damage
//! his body would take and the health he has. BO2's `player_damage_override` (`_zm.gsc:4239-4252`):
//! a blow that would kill him while another survivor is up leaves him at 1 health and puts him in
//! last stand instead (`playerlaststand`, `self.health = 1`); the last one up dies, and with him the
//! game. A downed survivor takes no more harm (BO2's zombies ignore him, `self.ignoreme = 1`; he
//! dies only by bleeding out, [`crate::sim::laststand`]).
//!
//! The hook runs inside Zero Hour's object updates, possibly inside a core callback, so it reads a
//! lock-free mirror of the sim's downed survivors and how many are up, written after each logic
//! step from sim state only, and queues the downs it decides for the sim's next step, in the
//! engine's order: every machine decides the same.

use crate::sim::{Sim, MAX_PLAYERS};
use std::sync::atomic::{AtomicU32, Ordering};

/// The downed survivors' Generals (0: free).
static DOWNED: [AtomicU32; MAX_PLAYERS] = [const { AtomicU32::new(0) }; MAX_PLAYERS];
/// Survivors alive and not downed.
static UP: AtomicU32 = AtomicU32::new(0);
/// The survivors with Jugger-Nog, and those whom going down alone revives (Quick Revive, solo).
static JUGG: [AtomicU32; MAX_PLAYERS] = [const { AtomicU32::new(0) }; MAX_PLAYERS];
static SELF_REVIVE: [AtomicU32; MAX_PLAYERS] = [const { AtomicU32::new(0) }; MAX_PLAYERS];
/// Jugger-Nog's share of each blow: the player's health over his (`player_max_health` / `JuggHealth`).
static JUGG_SCALE: AtomicU32 = AtomicU32::new(0);
/// The downs decided since the last step, in order.
static QUEUE: [AtomicU32; MAX_PLAYERS] = [const { AtomicU32::new(0) }; MAX_PLAYERS];
static COUNT: AtomicU32 = AtomicU32::new(0);

/// Mirror the sim's last stands for the hook (after each step, and on a reset).
pub fn publish(sim: &Sim) {
    let mut ids = sim.rounds.downed.iter().map(|d| d.general);
    for slot in &DOWNED {
        slot.store(ids.next().unwrap_or(0), Ordering::Relaxed);
    }
    UP.store(sim.rounds.up, Ordering::Relaxed);
    let (jugg, solo) = sim.perk_mirror();
    for (slots, ids) in [(&JUGG, jugg), (&SELF_REVIVE, solo)] {
        let mut ids = ids.into_iter();
        for slot in slots {
            slot.store(ids.next().unwrap_or(0), Ordering::Relaxed);
        }
    }
    let scale = crate::tuning::get().player_max_health / crate::zombies::Perks::get().jugg_health.max(1.0);
    JUGG_SCALE.store(scale.to_bits(), Ordering::Relaxed);
}

fn listed(slots: &[AtomicU32; MAX_PLAYERS], id: u32) -> bool {
    slots.iter().any(|s| s.load(Ordering::Relaxed) == id)
}

/// Hand the queued downs to the sim (before its step), in the engine's order.
pub fn drain(sim: &mut Sim, host: &dyn crate::host::LogicHost) {
    let n = (COUNT.swap(0, Ordering::Relaxed) as usize).min(MAX_PLAYERS);
    for slot in &QUEUE[..n] {
        sim.go_down(host, slot.load(Ordering::Relaxed));
    }
}

fn is_downed(id: u32) -> bool {
    DOWNED.iter().any(|s| s.load(Ordering::Relaxed) == id)
}

/// The damage a blow may do to the survivor `victim` with `health` left, of the `amount` his body
/// would take. Lock-free; safe inside any core callback.
#[no_mangle]
pub extern "C" fn zhbo_logic_survivor_damage(victim: u32, amount: f32, health: f32) -> f32 {
    if victim == 0 || is_downed(victim) {
        return if victim == 0 { amount } else { 0.0 };
    }
    // Jugger-Nog: his health is `JuggHealth` of the player's, so each blow does that share.
    let amount = if listed(&JUGG, victim) { amount * f32::from_bits(JUGG_SCALE.load(Ordering::Relaxed)) } else { amount };
    if amount < health || (UP.load(Ordering::Relaxed) <= 1 && !listed(&SELF_REVIVE, victim)) {
        return amount;
    }
    let i = COUNT.load(Ordering::Relaxed) as usize;
    let Some(free) = DOWNED.iter().find(|s| s.load(Ordering::Relaxed) == 0) else { return amount };
    if i >= MAX_PLAYERS {
        return amount;
    }
    // Down at once for the rest of this frame's blows, until the sim's next mirror.
    free.store(victim, Ordering::Relaxed);
    UP.fetch_sub(1, Ordering::Relaxed);
    QUEUE[i].store(victim, Ordering::Relaxed);
    COUNT.store(i as u32 + 1, Ordering::Relaxed);
    (health - 1.0).max(0.0)
}

#[cfg(test)]
pub(crate) fn reset_for_tests() {
    for s in DOWNED.iter().chain(&QUEUE).chain(&JUGG).chain(&SELF_REVIVE) {
        s.store(0, Ordering::Relaxed);
    }
    COUNT.store(0, Ordering::Relaxed);
    UP.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lethal blow downs a survivor while another is up and leaves him at 1; then he takes
    /// nothing; the last one up takes the whole blow.
    #[test]
    fn a_lethal_blow_downs_while_another_survivor_is_up() {
        let _g = crate::ffi::state_lock();
        reset_for_tests();
        UP.store(2, Ordering::Relaxed);
        assert_eq!(zhbo_logic_survivor_damage(7, 30.0, 100.0), 30.0, "a blow that leaves him standing passes");
        assert_eq!(zhbo_logic_survivor_damage(7, 900.0, 100.0), 99.0);
        assert_eq!(zhbo_logic_survivor_damage(7, 50.0, 1.0), 0.0, "downed: no more harm");
        assert_eq!(zhbo_logic_survivor_damage(8, 900.0, 100.0), 900.0, "the last one up dies");
        assert_eq!(COUNT.load(Ordering::Relaxed), 1);
        assert_eq!(QUEUE[0].load(Ordering::Relaxed), 7);
        reset_for_tests();
    }

    /// Jugger-Nog takes its share off each blow; alone, Quick Revive downs instead of killing.
    #[test]
    fn jugger_nog_softens_blows_and_solo_quick_revive_downs() {
        let _g = crate::ffi::state_lock();
        reset_for_tests();
        JUGG[0].store(7, Ordering::Relaxed);
        JUGG_SCALE.store((100.0f32 / 160.0).to_bits(), Ordering::Relaxed);
        UP.store(1, Ordering::Relaxed);
        assert!((zhbo_logic_survivor_damage(7, 60.0, 100.0) - 37.5).abs() < 1e-4);
        assert_eq!(zhbo_logic_survivor_damage(8, 900.0, 100.0), 900.0, "alone without Quick Revive: dead");
        SELF_REVIVE[0].store(8, Ordering::Relaxed);
        assert_eq!(zhbo_logic_survivor_damage(8, 900.0, 100.0), 99.0, "alone with it: down");
        reset_for_tests();
    }
}
