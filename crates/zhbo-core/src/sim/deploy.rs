/*
** Conquer Ops - Zero Hour gameplay core: the deployed Assault Shield.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! The Assault Shield planted as cover (`_riotshield.gsc` `watchriotshielddeploy` and the engine's
//! `canplaceriotshield`): the trigger sets it down where it was held, standing on the ground with
//! BO2's deployed model (`t6_wpn_shield_carry_world`, the converter's `ZHBO_Proj_riotshield_mp`);
//! it blocks every shot, melee and projectile that meets it, takes damage scaled by how it came
//! (`watchdeployedriotshielddamage`: bullets x0.025, melee x1, explosives x1) until
//! `riotshield_deployed_health` is used up. The engine's `deployriotshield` (`FUN_009672e0`)
//! takes the weapon from the player and leaves it as the shield's retrieve item: the use key next
//! to it gives it back; destroyed (or its owner dead) the weapon is gone with it. Every number is
//! the tuning pack's.

use super::*;
use crate::shield::Bearer;
use crate::units::inches;
use crate::weapons::{ShieldBox, WeaponDef, WeaponState};

/// How damage reached a deployed shield (the script's means-of-death buckets).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShieldDamage {
    Bullet,
    Melee,
    Explosive,
}

/// One deployed shield.
#[derive(Clone, Debug, PartialEq)]
pub struct Deployed {
    pub owner: usize,
    /// The owner's General.
    pub general: u32,
    pub def: &'static WeaponDef,
    /// The host object showing it (0 if none).
    pub object: u32,
    /// Its box in the world (`general` 0: no body behind it).
    pub bearer: Bearer,
    /// Scaled damage taken so far.
    pub damage_taken: f32,
    /// Seconds the destroyed model has left; `None` while it stands.
    pub destroyed: Option<f32>,
    /// The weapon taken from the owner (its slot and state), and whether `knife_held_mp` was given
    /// for it: what the retrieve item gives back.
    pub weapon: (usize, WeaponState, bool),
}

impl Deployed {
    pub fn standing(&self) -> bool {
        self.destroyed.is_none()
    }
}

impl Sim {
    /// The standing deployed shields as blockers (appended to the frame's [`Sim::shields`]).
    pub(super) fn deployed_bearers(&self) -> impl Iterator<Item = Bearer> + '_ {
        self.deployed.iter().filter(|d| d.standing()).map(|d| d.bearer)
    }

    /// The trigger: plant `def` where it is held (its held box's tag over the ground), if the
    /// ground there passes BO2's checks; else the round comes back and the hint shows.
    pub(super) fn deploy_shield(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, def: &'static WeaponDef) {
        let t = crate::tuning::get();
        match self.shield_placement(host, p, def, &t) {
            Some(bearer) => {
                let Some(weapon) = p.arsenal.take_shield() else { return };
                let origin = bearer.pos + Vec3::new(0.0, 0.0, inches(t.riotshield_placement_zoffset));
                let object = if def.projectile_object.is_empty() { 0 } else { host.spawn(&def.projectile_object, p.general, origin, p.yaw) };
                self.deployed.push(Deployed { owner: idx, general: p.general, def, object, bearer, damage_taken: 0.0, destroyed: None, weapon });
                self.emit(idx, EventKind::ShieldDeployed { at: bearer.pos });
            }
            None => {
                if let Some(w) = p.arsenal.weapons.iter_mut().find(|w| w.def.id == def.id) {
                    w.ammo = def.clip;
                }
                self.emit(idx, EventKind::CantPlant { lethal: false });
            }
        }
    }

    /// Where the shield would stand: on the ground under the held shield, within
    /// `riotshield_deploy_zdiff_max` of the feet, on ground no steeper than the pitch/roll limits,
    /// on walkable ground, and `riotshield_deploy_limit_radius` from every other standing shield.
    fn shield_placement(&self, host: &dyn LogicHost, p: &PlayerSim, def: &WeaponDef, t: &crate::tuning::Tuning) -> Option<Bearer> {
        let g = def.shield?;
        let (fwd, right) = basis_xy(p.yaw);
        let at = p.body.pos + fwd * inches(g.held.pos[0]) - right * inches(g.held.pos[1]);
        let ground = host.ground_height(at.x, at.y);
        if !host.is_walkable(at.x, at.y) || (ground - p.body.pos.z).abs() > inches(t.riotshield_deploy_zdiff_max) {
            return None;
        }
        // The ground's slope across the shield's footprint: pitch along the facing, roll across.
        let depth = inches((g.held.maxs[0] - g.held.mins[0]) * 0.5).max(1e-3);
        let width = inches((g.held.maxs[1] - g.held.mins[1]) * 0.5).max(1e-3);
        let h = |v: Vec3| host.ground_height(v.x, v.y);
        let pitch = ((h(at + fwd * depth) - h(at - fwd * depth)) / (2.0 * depth)).atan().to_degrees().abs();
        let roll = ((h(at + right * width) - h(at - right * width)) / (2.0 * width)).atan().to_degrees().abs();
        if pitch > t.riotshield_deploy_pitch_max || roll > t.riotshield_deploy_roll_max {
            return None;
        }
        let pos = Vec3::new(at.x, at.y, ground);
        let limit = inches(t.riotshield_deploy_limit_radius);
        if self.deployed.iter().any(|d| d.standing() && (d.bearer.pos - pos).length() < limit) {
            return None;
        }
        let b = ShieldBox { rot: [0.0, 0.0, 0.0, 1.0], pos: [0.0, 0.0, t.riotshield_placement_zoffset], mins: g.held.mins, maxs: g.held.maxs };
        Some(Bearer { general: 0, pos, yaw: p.yaw, held: true, b })
    }

    /// The use key next to the player's own standing shield (its retrieve item, in the building
    /// reach): it is removed, the weapon comes back with its round and up.
    pub(super) fn use_picks_up_shield(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim) -> bool {
        let reach = crate::building::REACH;
        let Some(i) = self.deployed.iter().position(|d| d.owner == idx && d.standing() && (d.bearer.pos - p.body.pos).length_xy() <= reach) else { return false };
        let d = self.deployed.remove(i);
        if d.object != 0 {
            host.destroy(d.object);
        }
        let (slot, state, gave_knife) = d.weapon;
        p.arsenal.return_shield(slot, state, gave_knife);
        self.emit(idx, EventKind::ShieldPickedUp);
        true
    }

    /// Damage to deployed shield `i` from General `by_general` (player `by`, if a player's):
    /// scaled by the means, nothing from the owner's teammates; destroyed at full health.
    pub(super) fn damage_deployed(&mut self, host: &dyn LogicHost, i: usize, by: Option<usize>, by_general: u32, bo2_damage: f32, how: ShieldDamage) {
        let t = crate::tuning::get();
        let d = &mut self.deployed[i];
        if !d.standing() {
            return;
        }
        let teammate = by_general != d.general && host.object(d.general, by_general).is_some_and(|o| o.has(objflags::ALLY));
        if teammate {
            return;
        }
        let scale = match how {
            ShieldDamage::Bullet => t.riotshield_bullet_damage_scale,
            ShieldDamage::Melee => t.riotshield_melee_damage_scale,
            ShieldDamage::Explosive => t.riotshield_explosive_damage_scale,
        };
        d.damage_taken += bo2_damage * scale;
        if d.damage_taken >= t.riotshield_deployed_health {
            d.destroyed = Some(t.riotshield_destroyed_cleanup_time);
            let owner = d.owner;
            let at = d.bearer.pos;
            self.emit(owner, EventKind::ShieldDestroyed { by: by.map(|b| b as i32), at });
            if let Some(b) = by.filter(|&b| b != owner) {
                self.emit(b, EventKind::DestroyedEquipment);
            }
        }
    }

    /// The nearest standing deployed shield a ray meets before `reach`, as (index, distance).
    pub(super) fn deployed_on_ray(&self, origin: Vec3, dir: Vec3, reach: f32) -> Option<(usize, f32)> {
        self.deployed
            .iter()
            .enumerate()
            .filter(|(_, d)| d.standing())
            .filter_map(|(i, d)| d.bearer.ray(origin, dir, reach).map(|t| (i, t)))
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
    }

    /// A blast of `def` at `at` by player `by`: every standing shield in its radius takes the
    /// explosive damage at its distance (no protection: it is the shield).
    pub(super) fn deployed_caught_in_blast(&mut self, host: &dyn LogicHost, by: usize, def: &WeaponDef, at: Vec3) {
        if def.effect != crate::weapons::Effect::Explode {
            return;
        }
        let by_general = self.players.get(by).and_then(|p| p.as_ref()).map_or(0, |p| p.general);
        for i in 0..self.deployed.len() {
            let (origin, _) = self.deployed[i].bearer.tag();
            let dist = (origin - at).length();
            if self.deployed[i].standing() && dist <= def.explosion_radius {
                self.damage_deployed(host, i, Some(by), by_general, def.explosion_damage_at(dist), ShieldDamage::Explosive);
            }
        }
    }

    /// Destroyed shields' models time out; a dead or departed owner's shields are destroyed.
    pub(super) fn step_deployed(&mut self, host: &dyn LogicHost) {
        let cleanup = crate::tuning::get().riotshield_destroyed_cleanup_time;
        for d in &mut self.deployed {
            let owner_alive = self.players.get(d.owner).and_then(|p| p.as_ref()).is_some_and(|p| p.general == d.general);
            if d.standing() && !owner_alive {
                d.destroyed = Some(cleanup);
            }
            if let Some(t) = d.destroyed.as_mut() {
                *t -= LOGIC_DT;
            }
        }
        self.deployed.retain(|d| {
            let gone = d.destroyed.is_some_and(|t| t <= 0.0);
            if gone && d.object != 0 {
                host.destroy(d.object);
            }
            !gone
        });
    }
}
