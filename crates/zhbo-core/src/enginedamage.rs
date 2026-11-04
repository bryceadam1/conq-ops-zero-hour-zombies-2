/*
** Conquer Ops - Zero Hour gameplay core: Zero Hour's own damage to the Generals, as BO2 blows.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! A General hurt or killed by Zero Hour's own weapons (a tank's shell, a Ranger's rifle, a
//! missile) is shown as BO2 shows a blow: his flinch turns by its side and his body falls by the
//! script's `DEATH` for its kind and side (`crate::death`). The engine's damage hook
//! (`Object::attemptDamage`, host patch 0017, the bridge's `Zhbo_ShieldDamage`) reports every
//! damage it applies to [`zhbo_logic_engine_damage`]: the victim, the Zero Hour damage type (the
//! ABI's `ZHBO_DAMAGE_*` code, [`crate::damage::ZH_DAMAGE_TYPES`]) and the way it travelled (BO2's
//! `vDir`: the blast's shock-wave vector when Zero Hour has one, else from the source unit to the
//! victim). The report is queued lock-free (the hook runs inside Zero Hour's object updates,
//! possibly inside a core callback) and handed to the sim before its next step, in the engine's
//! order, so every machine reads the same blows ([`crate::sim::Sim::engine_blow`]).
//!
//! The sim's own damage (`LogicHost::damage` through the FFI host) also passes the hook; it is
//! not reported ([`dealing`]): the sim reports its own blows itself, with BO2's weapon and means.
//! Presentation only: the blows give events their side and a death its report.

use crate::math::Vec3;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

const SLOTS: usize = 64;
/// victim, kind, dir x, y, z (bits)
static QUEUE: [[AtomicU32; 5]; SLOTS] = [const { [const { AtomicU32::new(0) }; 5] }; SLOTS];
static COUNT: AtomicU32 = AtomicU32::new(0);
/// The sim is dealing damage through the engine now (its own blow: not reported).
static DEALING: AtomicBool = AtomicBool::new(false);

/// Run `f` (the FFI host's damage call into the engine) with the hook's reports muted.
pub fn dealing<R>(f: impl FnOnce() -> R) -> R {
    let was = DEALING.swap(true, Ordering::Relaxed);
    let r = f();
    DEALING.store(was, Ordering::Relaxed);
    r
}

/// Queue one engine damage report (dropped past the ring's end, as the shield's are).
pub fn report(victim: u32, kind: i32, dir: Vec3) {
    if DEALING.load(Ordering::Relaxed) || victim == 0 {
        return;
    }
    let i = COUNT.load(Ordering::Relaxed) as usize;
    if i >= SLOTS {
        return;
    }
    for (slot, v) in QUEUE[i].iter().zip([victim, kind as u32, dir.x.to_bits(), dir.y.to_bits(), dir.z.to_bits()]) {
        slot.store(v, Ordering::Relaxed);
    }
    COUNT.store(i as u32 + 1, Ordering::Relaxed);
}

/// Hand the queued reports to the sim, in the engine's order, and empty the queue.
pub fn drain(sim: &mut crate::sim::Sim) {
    let n = (COUNT.swap(0, Ordering::Relaxed) as usize).min(SLOTS);
    for e in &QUEUE[..n] {
        let w: [u32; 5] = std::array::from_fn(|k| e[k].load(Ordering::Relaxed));
        sim.engine_blow(w[0], w[1] as i32, Vec3::new(f32::from_bits(w[2]), f32::from_bits(w[3]), f32::from_bits(w[4])));
    }
}

/// Zero Hour applied damage of `ZHBO_DAMAGE_*` kind `kind` to object `victim`, travelling along
/// `(dir_x, dir_y, dir_z)` (zero: unknown). Lock-free; safe inside any core callback. ABI 33.
#[no_mangle]
pub extern "C" fn zhbo_logic_engine_damage(victim: u32, kind: i32, dir_x: f32, dir_y: f32, dir_z: f32) {
    report(victim, kind, Vec3::new(dir_x, dir_y, dir_z));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reports reach the sim in order and once; the sim's own damage is not reported.
    #[test]
    fn the_engines_blows_reach_the_sim_in_order_and_the_sims_own_do_not() {
        let _g = crate::ffi::state_lock();
        COUNT.store(0, Ordering::Relaxed);
        let mut sim = crate::sim::Sim::new();
        let mut p = crate::sim::PlayerSim::spawn_for_tests();
        p.general = 7;
        sim.players[0] = Some(p);
        let tank = crate::damage::zh_code("ARMOR_PIERCING").unwrap() as i32;
        let rifle = crate::damage::zh_code("SMALL_ARMS").unwrap() as i32;
        zhbo_logic_engine_damage(7, rifle, 1.0, 0.0, 0.0);
        dealing(|| report(7, rifle, Vec3::new(0.0, 1.0, 0.0)));
        zhbo_logic_engine_damage(9, rifle, 1.0, 0.0, 0.0); // not a General: dropped by the sim
        zhbo_logic_engine_damage(7, tank, 0.0, -1.0, 0.0);
        drain(&mut sim);
        let got: Vec<(u32, crate::death::DmgType, Vec3, bool)> = sim.blows.iter().map(|b| (b.victim, b.dmg, b.dir, b.engine)).collect();
        use crate::death::DmgType;
        assert_eq!(got, [(7, DmgType::Normal, Vec3::new(1.0, 0.0, 0.0), true), (7, DmgType::Explosive, Vec3::new(0.0, -1.0, 0.0), true)]);
        drain(&mut sim);
        assert_eq!(sim.blows.len(), 2, "drained once");
    }
}
