/*
** Conquer Ops - Zero Hour gameplay core: the overhead General: RTS weapon tracking, ammo and offhands.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::*;

impl Sim {
    pub fn rts_update(&mut self, host: &dyn LogicHost, player: i32, general: u32, current_slot: i32, shots: [u32; 3], target: Option<Vec3>) -> RtsWeapons {
        let Ok(idx) = usize::try_from(player) else { return RtsWeapons::default() };
        if idx >= MAX_PLAYERS {
            return RtsWeapons::default();
        }
        if self.players[idx].as_ref().is_none_or(|p| p.general != general && !p.is_first_person()) {
            let Some(info) = host.object(general, general) else { return RtsWeapons::default() };
            let frame = self.frame;
            self.players[idx] = Some(PlayerSim {
                general,
                mode: Mode::Rts,
                body: Body::new(info.pos, &self.tuning),
                yaw: info.yaw,
                pitch: 0.0,
                arsenal: Arsenal::new(),
                rng: Rng::new(general.wrapping_mul(2_654_435_761) ^ frame),
                input: FpsInput { yaw: info.yaw, ..Default::default() },
                pressed_accum: 0,
                health: info.health,
                max_health: info.max_health,
                regen_wait: 0.0,
                hack: None,
                rts_lock: -1,
                rts_tactical: false,
                rts_gun: 0,
            hammer: None,
            site: None,
            building: false,
            });
        }
        let Some(p) = self.players[idx].as_mut().filter(|p| p.general == general) else { return RtsWeapons::default() };
        if p.mode != Mode::Rts {
            return rts_weapons(&p.arsenal, -1, p.rts_tactical);
        }
        let a = &mut p.arsenal;
        for (slot, &n) in shots.iter().enumerate().take(2) {
            for _ in 0..n {
                a.rts_shot(slot);
            }
        }
        // The third slot only stands in for the throw: the grenade itself is BO2's, thrown here as
        // first person throws it (its fuse, bounces and blast), at what Zero Hour aimed at.
        let mut throws = Vec::new();
        for _ in 0..shots[2] {
            let off = if p.rts_tactical { &mut a.tactical } else { &mut a.lethal };
            if off.count > 0 {
                off.count -= 1;
                throws.push(off.def);
            }
        }
        if (0..2).contains(&current_slot) {
            p.rts_gun = current_slot;
        }
        if (0..a.weapons.len() as i32).contains(&current_slot) {
            a.active = current_slot as usize;
        }
        let mut lock = std::mem::replace(&mut p.rts_lock, -1);
        if !throws.is_empty() {
            lock = p.rts_gun; // back to the gun after the throw
        }
        let a = &p.arsenal;
        let current = if lock >= 0 { lock } else { current_slot };
        let offhand_left = if p.rts_tactical { a.tactical.count } else { a.lethal.count };
        let empty = |s: i32| if s == 2 { offhand_left == 0 } else { a.rts_ammo(s as usize).0 + a.rts_ammo(s as usize).1 == 0 };
        if (0..3).contains(&current) && empty(current) {
            // Out of ammo: the first gun that has some (the grenades only by choice).
            lock = (0..a.weapons.len()).find(|&s| a.rts_ammo(s).0 + a.rts_ammo(s).1 > 0).map_or(-1, |s| s as i32);
        }
        let out = rts_weapons(a, lock, p.rts_tactical);
        if let Some(from) = host.object(general, general).map(|o| o.pos + Vec3::new(0.0, 0.0, p.body.eye_height * 0.9)) {
            let yaw = p.yaw;
            for def in throws {
                let to = target.unwrap_or(from);
                let speed = if def.projectile_speed > 0.0 { def.projectile_speed } else { crate::units::inches(800.0) };
                let vel = throw_velocity(from, to, speed);
                let mut proj = Projectile::launch(def, general, from, vel.normalized_or_zero(), def.fuse_time, host, yaw);
                proj.vel = vel;
                self.projectiles.push((idx, proj));
            }
        }
        out
    }

    /// Under RTS control: make the third slot throw the tactical (`true`) or the lethal.
    pub fn rts_offhand(&mut self, player: i32, general: u32, tactical: bool) {
        let Some(p) = usize::try_from(player).ok().and_then(|i| self.players.get_mut(i)).and_then(|p| p.as_mut()) else { return };
        if p.general == general {
            p.rts_tactical = tactical;
        }
    }

    /// The ammo a General's weapons have, for the control bar (the loadout's when the General has
    /// no arsenal yet).
    pub fn rts_ammo(&self, general: u32) -> RtsWeapons {
        match self.players.iter().flatten().find(|p| p.general == general) {
            Some(p) => rts_weapons(&p.arsenal, -1, p.rts_tactical),
            None => rts_weapons(&Arsenal::new(), -1, false),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RtsWeapons {
    pub ammo: [i32; 3],
    pub reserve: [i32; 3],
    pub lock: i32,
    /// Tactical grenades left, and whether the third slot throws them (else the lethal).
    pub tactical: i32,
    pub tactical_selected: i32,
}

fn rts_weapons(a: &Arsenal, lock: i32, tactical_selected: bool) -> RtsWeapons {
    let mut out = RtsWeapons { lock, tactical: a.tactical.count as i32, tactical_selected: tactical_selected as i32, ..Default::default() };
    for s in 0..3 {
        let (ammo, reserve) = a.rts_ammo(s);
        out.ammo[s] = ammo as i32;
        out.reserve[s] = reserve as i32;
    }
    out
}
