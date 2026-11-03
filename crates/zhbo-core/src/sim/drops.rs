//! Dropped weapons: BO2's weapon drops and pickups as deterministic sim objects.
//!
//! * **A death drops the weapon in hand** (`_globallogic_player.gsc:1912-1919` →
//!   `_weapons.gsc:364` `dropweaponfordeath`): outside the grace period, unless it was a team kill
//!   or a suicide, the last droppable weapon he held (`crate::weapons::pickup`) with its rounds,
//!   attachments (the combination itself) and options (camo, reticle). The engine's `Drop_Item`
//!   (t6mp `FUN_005fae60`) tosses it from his middle, along his facing plus a random push; it falls
//!   and lies where it lands. It goes after 60 s (`deletepickupafterawhile`).
//! * **Walking over one takes its ammo** (`FUN_005fba10` → `FUN_005faa60`): a first-person
//!   General whose box touches its box and who holds the weapon or one of its base, every frame.
//! * **The use key swaps for one** (`FUN_005fb7d0`): the dropped weapon BO2's use list picks
//!   (`FUN_00676d50`: within `player_useRadius` of the eye, the lowest distance plus facing score)
//!   when he does not hold that weapon; the weapon he puts down lies where the new one was, with no
//!   timer, and he may not take it back for a second (its owner, `FUN_008bbb00`).
//! * **At most `g_maxDroppedWeapons` lie at once**: one more frees the one farthest from every
//!   General, sparing `avoidDropCleanup` weapons (`FUN_005f9230`).
//!
//! Each one is drawn as its converted Zero Hour prop (`WeaponDef::drop_object`, its BO2 world
//! model), which the sim spawns and moves; the object is presentation, the [`Dropped`] record is
//! the state (hashed in the world's part of the fingerprint). Zero Hour's own infantry drop
//! nothing: they carry Zero Hour weapons, not BO2 ones.
/*
** Conquer Ops - Zero Hour gameplay core: dropped weapons.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::*;
use crate::weapons::{Carried, WeaponOptions, WeaponState};
use std::cell::RefCell;

/// A weapon lying in the world (BO2's dropped item).
#[derive(Clone, Debug, PartialEq)]
pub struct Dropped {
    pub weapon: Carried,
    /// Where it is (its origin, on the ground once it rests) and how it moves while it falls.
    pub pos: Vec3,
    pub vel: Vec3,
    pub resting: bool,
    pub yaw: f32,
    /// The frame it goes (`deletepickupafterawhile`); `None`: it stays until the cleanup.
    pub expires: Option<u32>,
    /// The General who put it down, who may not pick it up before frame `owner_until`.
    pub owner: u32,
    pub owner_until: u32,
    /// Its Zero Hour object (0: none).
    pub object: u32,
}

crate::statehash::state_hash!(struct Dropped { weapon, pos, vel, resting, yaw, expires, owner, owner_until, object });

/// A kill the step saw: the victim and whether it was his own or a teammate's (`_globallogic_player.gsc`
/// `wassuicide`/`wasteamkill`, which keep the weapon from dropping).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kill {
    pub victim: u32,
    pub friendly: bool,
}

crate::statehash::state_hash!(struct Kill { victim, friendly });

/// The engine with every kill noted: the step runs through it, so a death knows who caused it.
pub(super) struct Witness<'a> {
    host: &'a dyn LogicHost,
    kills: RefCell<Vec<Kill>>,
}

impl<'a> Witness<'a> {
    pub(super) fn new(host: &'a dyn LogicHost) -> Self {
        Witness { host, kills: RefCell::new(Vec::new()) }
    }

    pub(super) fn take(&self) -> Vec<Kill> {
        self.kills.take()
    }
}

impl LogicHost for Witness<'_> {
    fn log(&self, msg: &str) {
        self.host.log(msg)
    }
    fn ground_height(&self, x: f32, y: f32) -> f32 {
        self.host.ground_height(x, y)
    }
    fn is_walkable(&self, x: f32, y: f32) -> bool {
        self.host.is_walkable(x, y)
    }
    fn surface_at(&self, x: f32, y: f32) -> crate::impacts::Surface {
        self.host.surface_at(x, y)
    }
    fn extent(&self) -> (Vec3, Vec3) {
        self.host.extent()
    }
    fn object(&self, id: u32, viewer: u32) -> Option<ObjInfo> {
        self.host.object(id, viewer)
    }
    fn query(&self, center: Vec3, radius: f32, viewer: u32, out: &mut Vec<ObjInfo>) {
        self.host.query(center, radius, viewer, out)
    }
    fn set_pose(&self, id: u32, pos: Vec3, yaw: f32, moving: bool) {
        self.host.set_pose(id, pos, yaw, moving)
    }
    fn set_controlled(&self, id: u32, controlled: bool) {
        self.host.set_controlled(id, controlled)
    }
    fn fire_fx(&self, shooter: u32, fx_weapon: &str, target: u32, pos: Vec3) {
        self.host.fire_fx(shooter, fx_weapon, target, pos)
    }
    fn damage(&self, shooter: u32, victim: u32, amount: f32, how: crate::damage::MeansOfDeath) -> bool {
        // The relation as the blow lands (the shooter may be gone by the time the death is read).
        let friendly = shooter == victim || self.host.object(shooter, victim).is_some_and(|o| o.has(objflags::ALLY));
        let kill = self.host.damage(shooter, victim, amount, how);
        if kill {
            self.kills.borrow_mut().push(Kill { victim, friendly });
        }
        kill
    }
    fn heal(&self, id: u32, amount: f32) {
        self.host.heal(id, amount)
    }
    fn spawn(&self, template: &str, owner: u32, pos: Vec3, yaw: f32) -> u32 {
        self.host.spawn(template, owner, pos, yaw)
    }
    fn destroy(&self, id: u32) {
        self.host.destroy(id)
    }
    fn disable(&self, id: u32, kind: i32, seconds: f32) {
        self.host.disable(id, kind, seconds)
    }
    fn fx(&self, name: &str, pos: Vec3) {
        self.host.fx(name, pos)
    }
    fn capture(&self, id: u32, new_owner: u32) {
        self.host.capture(id, new_owner)
    }
    fn smoke(&self, center: Vec3, radius: f32) {
        self.host.smoke(center, radius)
    }
    fn build_options(&self, general: u32) -> Vec<crate::building::BuildOption> {
        self.host.build_options(general)
    }
    fn can_place(&self, general: u32, template: u32, pos: Vec3, angle: f32) -> bool {
        self.host.can_place(general, template, pos, angle)
    }
    fn place(&self, general: u32, template: u32, pos: Vec3, angle: f32) -> u32 {
        self.host.place(general, template, pos, angle)
    }
    fn work_site(&self, general: u32, pos: Vec3, reach: f32) -> Option<crate::building::Site> {
        self.host.work_site(general, pos, reach)
    }
    fn work(&self, general: u32, site: u32, rate: f32) -> f32 {
        self.host.work(general, site, rate)
    }
}

impl PlayerSim {
    /// The options `w` wears: its own when it was picked up, else its class slot's (the primary's
    /// `camo`/`reticle`, the sidearm's `secondarycamo`/`secondaryreticle`, `_class.gsc`
    /// `calcweaponoptions(class, 0/1)`), with the class's reticle colour.
    pub fn options_of(&self, w: &WeaponState) -> WeaponOptions {
        if let Some(o) = &w.options {
            return o.clone();
        }
        let kit = &self.arsenal.loadout;
        let (camo, reticle) = if w.main.id == kit.secondary {
            (&self.secondary_camo, &self.secondary_reticle)
        } else if w.main.id == kit.primary {
            (&self.camo, &self.reticle)
        } else {
            return WeaponOptions::default();
        };
        WeaponOptions { camo: camo.clone(), reticle: reticle.clone(), reticle_color: self.reticle_color.clone() }
    }
}

impl Sim {
    /// The kills of a step, kept for the victims who are Generals of a seat until their death is
    /// read ([`Sim::drop_for_death`]).
    pub(super) fn note_kills(&mut self, kills: Vec<Kill>) {
        let generals: Vec<u32> = self.players.iter().flatten().map(|p| p.general).collect();
        self.kills.extend(kills.into_iter().filter(|k| generals.contains(&k.victim)));
        self.kills.retain(|k| generals.contains(&k.victim));
    }

    /// General `p` of seat `idx` died at `at` (his object's place; `None`: it is gone, and he is
    /// where his first-person body was): drop his weapon, as `dropweaponfordeath` does.
    pub(super) fn drop_for_death(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, at: Option<Vec3>) {
        let friendly = self.kills.iter().rev().find(|k| k.victim == p.general).is_some_and(|k| k.friendly);
        self.kills.retain(|k| k.victim != p.general);
        if friendly || self.in_grace_period() {
            return;
        }
        let Some(at) = at.or_else(|| p.is_first_person().then_some(p.body.pos)) else { return };
        let Some(i) = p.arsenal.death_drop() else { return };
        let w = &p.arsenal.weapons[i];
        let weapon = Carried::from_state(w, p.options_of(w));
        let t = crate::tuning::get();
        // `Drop_Item`: from the middle of his box, tossed along his facing and pushed at random.
        let from = at + Vec3::new(0.0, 0.0, inches(t.player_box_height) * 0.5);
        let rng = &mut p.rng;
        let (c, s) = (crate::math::det::cos(p.yaw), crate::math::det::sin(p.yaw));
        let forward = inches(t.drop_item_forward_speed);
        let vel = Vec3::new(
            c * forward + rng.signed() * inches(t.drop_item_random_speed),
            s * forward + rng.signed() * inches(t.drop_item_random_speed),
            inches(t.drop_item_up_speed) + rng.signed() * inches(t.drop_item_random_up_speed),
        );
        let expires = self.frame + (t.dropped_weapon_lifetime / LOGIC_DT).round() as u32;
        let def = weapon.def;
        self.put_down(host, Dropped { weapon, pos: from, vel, resting: false, yaw: p.yaw, expires: Some(expires), owner: p.general, owner_until: 0, object: 0 });
        self.emit(idx, EventKind::WeaponDropped { def, at: from });
    }

    /// A weapon goes down (`d`: where, how it moves, who put it down and when it goes; its owner's
    /// wait and its object are set here): the cleanup first when the world is full
    /// (`FUN_005f9230`), then its record and its Zero Hour object.
    fn put_down(&mut self, host: &dyn LogicHost, mut d: Dropped) {
        let t = crate::tuning::get();
        if self.drops.len() >= (t.g_max_dropped_weapons.max(1.0) as usize) {
            self.free_a_drop(host);
        }
        let def = d.weapon.def;
        d.object = if def.drop_object.is_empty() { 0 } else { host.spawn(&def.drop_object, d.owner, d.pos, d.yaw) };
        let shown = if def.drop_object.is_empty() { "(none)" } else { def.drop_object.as_str() };
        host.log(&format!("drops: {} down at ({:.0}, {:.0}) as {shown} object {}", def.id, d.pos.x, d.pos.y, d.object));
        d.owner_until = self.frame + (t.dropped_item_owner_time / LOGIC_DT).round() as u32;
        d.resting = d.vel == Vec3::ZERO;
        self.drops.push(d);
    }

    /// Free the dropped weapon farthest from every General (the nearest General's distance, the
    /// largest), sparing `avoidDropCleanup`; the first one when all are spared (`FUN_005f9230`'s
    /// "Using index zero").
    fn free_a_drop(&mut self, host: &dyn LogicHost) {
        let generals: Vec<Vec3> = self.players.iter().flatten().map(|p| host.object(p.general, p.general).map_or(p.body.pos, |o| o.pos)).collect();
        let mut best: Option<(usize, f32)> = None;
        for (i, d) in self.drops.iter().enumerate() {
            if d.weapon.def.drop.avoid_drop_cleanup {
                continue;
            }
            let near = generals.iter().map(|g| { let v = *g - d.pos; v.dot(v) }).fold(f32::MAX, f32::min);
            if best.is_none_or(|(_, b)| near > b) {
                best = Some((i, near));
            }
        }
        let i = best.map_or(0, |(i, _)| i);
        if i < self.drops.len() {
            let d = self.drops.remove(i);
            if d.object != 0 {
                host.destroy(d.object);
            }
        }
    }

    /// Each logic frame: the falling ones fall, the old ones go, and every first-person General
    /// walking over one takes its ammo.
    pub(super) fn step_drops(&mut self, host: &dyn LogicHost) {
        let g = crate::projectiles::gravity();
        let frame = self.frame;
        let mut gone = Vec::new();
        for (i, d) in self.drops.iter_mut().enumerate() {
            if d.expires.is_some_and(|e| frame >= e) {
                gone.push(i);
                continue;
            }
            if d.resting {
                continue;
            }
            d.vel.z -= g * LOGIC_DT;
            d.pos += d.vel * LOGIC_DT;
            let ground = host.ground_height(d.pos.x, d.pos.y);
            if d.pos.z <= ground {
                d.pos.z = ground;
                d.vel = Vec3::ZERO;
                d.resting = true;
            }
            if d.object != 0 {
                host.set_pose(d.object, d.pos, d.yaw, false);
            }
        }
        for i in gone.into_iter().rev() {
            let d = self.drops.remove(i);
            if d.object != 0 {
                host.destroy(d.object);
            }
        }
        self.walk_overs(host);
    }

    /// The touch (`FUN_005fba10` with the ammo test): seat by seat, each dropped weapon whose box
    /// his touches.
    fn walk_overs(&mut self, host: &dyn LogicHost) {
        let t = crate::tuning::get();
        let half = inches(t.dropped_item_half_size);
        let reach = self.tuning.radius + half;
        let top = inches(t.player_box_height) + half;
        for idx in 0..self.players.len() {
            let Some(mut p) = self.players[idx].take() else { continue };
            if matches!(p.mode, Mode::Fps) && !p.awaiting_class() {
                let mut i = 0;
                while i < self.drops.len() {
                    let d = &mut self.drops[i];
                    let rel = d.pos - p.body.pos;
                    let touches = rel.x.abs() <= reach && rel.y.abs() <= reach && rel.z >= -half && rel.z <= top;
                    if !touches || !may_take(d, &p, self.frame) {
                        i += 1;
                        continue;
                    }
                    let taken = p.arsenal.take_ammo(&mut d.weapon);
                    let (def, at) = (d.weapon.def, d.pos);
                    if taken.any {
                        self.emit(idx, EventKind::AmmoPickedUp { def, at });
                    }
                    if taken.remove {
                        let d = self.drops.remove(i);
                        if d.object != 0 {
                            host.destroy(d.object);
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            self.players[idx] = Some(p);
        }
    }

    /// The dropped weapon `p`'s use key would take now (BO2's use list, `FUN_00676d50`): within
    /// `player_useRadius` of his eye, one he may take and does not hold, the lowest score of its
    /// distance plus how far it is off his view. The HUD's hint asks the same.
    pub fn use_target(&self, p: &PlayerSim) -> Option<usize> {
        let t = crate::tuning::get();
        let eye = p.body.eye();
        let dir = dir_from_angles(p.yaw, p.pitch);
        let inch = inches(1.0);
        let lift = Vec3::new(0.0, 0.0, inches(t.dropped_item_half_size));
        let mut best: Option<(usize, f32)> = None;
        for (i, d) in self.drops.iter().enumerate() {
            if !may_take(d, p, self.frame) || p.arsenal.holds_exact(d.weapon.def) {
                continue;
            }
            let to = d.pos + lift - eye;
            let dist = to.length();
            if dist / inch > t.player_use_radius {
                continue;
            }
            let dot = if dist > 0.0 { dir.dot(to) / dist } else { 1.0 };
            let score = dist / inch + t.use_list_dot_scale * (1.0 - (dot + 1.0) * 0.5);
            if best.is_none_or(|(_, b)| score < b) {
                best = Some((i, score));
            }
        }
        best.map(|(i, _)| i)
    }

    /// The use key with a dropped weapon targeted: the swap. True when it took it.
    pub(super) fn use_drop(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim) -> bool {
        let Some(i) = self.use_target(p) else { return false };
        let item = self.drops[i].weapon.clone();
        // The options each held weapon wears, taken before the arsenal changes.
        let class_options: Vec<WeaponOptions> = p.arsenal.weapons.iter().map(|w| p.options_of(w)).collect();
        let before: Vec<String> = p.arsenal.weapons.iter().map(|w| w.main.id.clone()).collect();
        let picked = p.arsenal.pick_up(item, |w| before.iter().position(|id| *id == w.main.id).map_or_else(|| w.options.clone().unwrap_or_default(), |k| class_options[k].clone()));
        let Ok(put) = picked else { return false };
        let d = self.drops.remove(i);
        if d.object != 0 {
            host.destroy(d.object);
        }
        let def = d.weapon.def;
        self.emit(idx, EventKind::WeaponPickedUp { def, at: d.pos });
        if let Some(put) = put {
            // Where the picked one lay (`FUN_005fb450` traces it there), at rest, no timer.
            self.put_down(host, Dropped { weapon: put, pos: d.pos, vel: Vec3::ZERO, resting: true, yaw: d.yaw, expires: None, owner: p.general, owner_until: 0, object: 0 });
        }
        true
    }

    /// Every dropped weapon's world model now: its object, where it lies and the weapon (the
    /// third person and tests read it; presentation).
    pub fn dropped_weapons(&self) -> impl Iterator<Item = (&'static crate::weapons::WeaponDef, Vec3)> + '_ {
        self.drops.iter().map(|d| (d.weapon.def, d.pos))
    }
}

/// Whether `p` may take `d` at all now: not its owner within his second (`FUN_008bbb00`), not
/// lying prone for a `blocksProne` weapon, not with a carried killstreak gun up unless `d` is of
/// its base (`FUN_005fb900`).
fn may_take(d: &Dropped, p: &PlayerSim, frame: u32) -> bool {
    let owner_wait = d.owner == p.general && frame < d.owner_until;
    let prone = d.weapon.def.drop.blocks_prone && p.body.stance == Stance::Prone;
    let up = p.arsenal.current().main;
    let killstreak = up.drop.carried_killstreak && crate::weapons::base_of(&up.id) != crate::weapons::base_of(&d.weapon.def.id);
    !owner_wait && !prone && !killstreak
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::tests::{general, run, spawn, FakeHost};
    use crate::weapons::{weapon, Arsenal, Loadout, WeaponDef};

    /// After the grace period (`ClassChangeGracePeriod`), when a death drops its weapon.
    const LATE: u32 = 1000;

    /// General `id` of seat `seat` in first person at `x`, facing +x, with `spec`'s kit up and the
    /// class camo `camo_gold` on his primary.
    fn armed(sim: &mut Sim, host: &FakeHost, seat: i32, id: u32, x: f32, spec: &str) {
        let mut g = general(id, seat);
        g.pos = Vec3::new(x, 0.0, 0.0);
        host.add(g);
        spawn(sim, host, seat, id);
        let p = sim.players[seat as usize].as_mut().unwrap();
        p.body.pos = Vec3::new(x, 0.0, 0.0);
        p.arsenal = Arsenal::with_loadout(Loadout::unarmed().with_spec(spec));
        p.arsenal.raise_left = 0.0;
        p.camo = "camo_gold".into();
    }

    fn kill(host: &FakeHost, id: u32) {
        for o in host.objects.borrow_mut().iter_mut().filter(|o| o.id == id) {
            o.flags |= objflags::DEAD;
        }
    }

    fn step(sim: &mut Sim, host: &FakeHost, frames: u32) {
        for _ in 0..frames {
            let f = sim.frame + 1;
            sim.step(host, f);
        }
    }

    fn lying(id: &str, at: Vec3) -> Dropped {
        let weapon = Carried { def: weapon(id), clip: 5, stock: 10, alt_clip: 0, alt_stock: 0, options: WeaponOptions { camo: "camo_dot".into(), ..Default::default() } };
        Dropped { weapon, pos: at, vel: Vec3::ZERO, resting: true, yaw: 0.0, expires: None, owner: 0, owner_until: 0, object: 0 }
    }

    #[test]
    fn a_death_drops_the_weapon_in_hand_with_its_rounds_and_camo_and_it_falls_to_the_ground() {
        let host = FakeHost::default();
        let mut sim = Sim::new();
        sim.frame = LATE;
        armed(&mut sim, &host, 0, 1, 0.0, "primary=an94_mp+reflex;secondary=fnp45_mp");
        sim.players[0].as_mut().unwrap().arsenal.weapons[0].ammo = 17;
        kill(&host, 1);
        step(&mut sim, &host, 1);
        assert!(sim.players[0].is_none(), "his record is gone");
        assert_eq!(sim.drops.len(), 1);
        let d = &sim.drops[0];
        assert_eq!(d.weapon.def.id, "an94_mp+reflex", "the combination itself: its attachments");
        assert_eq!(d.weapon.clip, 17);
        assert_eq!(d.weapon.options.camo, "camo_gold");
        assert!(d.pos.z > 0.0 && !d.resting, "tossed from his middle");
        assert!(sim.events.iter().any(|e| matches!(e.kind, EventKind::WeaponDropped { .. })));
        step(&mut sim, &host, 30);
        assert!(sim.drops[0].resting && sim.drops[0].pos.z == 0.0, "it lies on the ground");
    }

    #[test]
    fn no_drop_in_the_grace_period_for_a_suicide_or_a_team_kill_or_without_rounds() {
        // The grace period.
        let host = FakeHost::default();
        let mut sim = Sim::new();
        armed(&mut sim, &host, 0, 1, 0.0, "primary=an94_mp;secondary=fnp45_mp");
        kill(&host, 1);
        step(&mut sim, &host, 1);
        assert!(sim.drops.is_empty(), "in the grace period");

        // A suicide (his own blast through the step's witness).
        let host = FakeHost::default();
        let mut sim = Sim::new();
        sim.frame = LATE;
        armed(&mut sim, &host, 0, 1, 0.0, "primary=an94_mp;secondary=fnp45_mp");
        let w = Witness::new(&host);
        assert!(w.damage(1, 1, 1000.0, crate::damage::MeansOfDeath::GrenadeSplash));
        sim.note_kills(w.take());
        assert_eq!(sim.kills, [Kill { victim: 1, friendly: true }]);
        step(&mut sim, &host, 1);
        assert!(sim.drops.is_empty() && sim.kills.is_empty(), "a suicide drops nothing");

        // An enemy's kill drops it.
        let host = FakeHost::default();
        let mut sim = Sim::new();
        sim.frame = LATE;
        armed(&mut sim, &host, 0, 1, 0.0, "primary=an94_mp;secondary=fnp45_mp");
        host.add(general(9, 1));
        let w = Witness::new(&host);
        assert!(w.damage(9, 1, 1000.0, crate::damage::MeansOfDeath::RifleBullet));
        sim.note_kills(w.take());
        step(&mut sim, &host, 1);
        assert_eq!(sim.drops.len(), 1, "an enemy's kill drops the weapon");

        // Nothing in it.
        let host = FakeHost::default();
        let mut sim = Sim::new();
        sim.frame = LATE;
        armed(&mut sim, &host, 0, 1, 0.0, "primary=an94_mp;secondary=fnp45_mp");
        let w = &mut sim.players[0].as_mut().unwrap().arsenal.weapons[0];
        (w.ammo, w.reserve) = (0, 0);
        kill(&host, 1);
        step(&mut sim, &host, 1);
        assert!(sim.drops.is_empty(), "no rounds, no drop");
    }

    #[test]
    fn walking_over_the_same_weapon_takes_its_ammo_and_the_item() {
        let host = FakeHost::default();
        let mut sim = Sim::new();
        sim.frame = LATE;
        armed(&mut sim, &host, 0, 1, 0.0, "primary=an94_mp;secondary=fnp45_mp");
        sim.players[0].as_mut().unwrap().arsenal.weapons[0].reserve = 0;
        // Out of reach first.
        sim.drops.push(lying("an94_mp", Vec3::new(inches(40.0), 0.0, 0.0)));
        step(&mut sim, &host, 1);
        assert_eq!(sim.drops.len(), 1);
        sim.drops[0].pos = Vec3::new(inches(10.0), inches(-12.0), 0.0);
        step(&mut sim, &host, 1);
        assert!(sim.drops.is_empty(), "the exact weapon: taken");
        assert_eq!(sim.player(0).unwrap().arsenal.weapons[0].reserve, 15, "its stock and its magazine");
        assert!(sim.events.iter().any(|e| matches!(e.kind, EventKind::AmmoPickedUp { .. })));
    }

    #[test]
    fn the_use_key_swaps_for_a_dropped_weapon_and_puts_the_one_up_down_in_its_place() {
        let host = FakeHost::default();
        let mut sim = Sim::new();
        sim.frame = LATE;
        armed(&mut sim, &host, 0, 1, 0.0, "primary=an94_mp;secondary=fnp45_mp");
        let at = Vec3::new(inches(60.0), 0.0, 0.0);
        sim.drops.push(lying("svu_mp", at));
        assert_eq!(sim.use_target(sim.player(0).unwrap()), Some(0));
        let press = FpsInput { pressed: buttons::FIELD_COMPUTER, ..Default::default() };
        run(&mut sim, &host, 0, 1, press, 1);
        let p = sim.player(0).unwrap();
        assert!(matches!(p.mode, Mode::Fps), "the field computer stays shut");
        assert_eq!(p.arsenal.current().main.id, "svu_mp");
        assert_eq!((p.arsenal.current().ammo, p.arsenal.current().reserve), (5, 10));
        assert_eq!(p.options_of(p.arsenal.current()).camo, "camo_dot", "it wears the camo it was dropped with");
        assert_eq!(sim.drops.len(), 1);
        let d = &sim.drops[0];
        assert_eq!((d.weapon.def.id.as_str(), d.pos, d.expires), ("an94_mp", at, None));
        assert_eq!(d.weapon.options.camo, "camo_gold", "the class weapon leaves with the class camo");
        assert!(sim.events.iter().any(|e| matches!(e.kind, EventKind::WeaponPickedUp { .. })));
        // He may not take it straight back.
        assert_eq!(sim.use_target(sim.player(0).unwrap()), None);
        run(&mut sim, &host, 0, 1, FpsInput::default(), 31);
        assert_eq!(sim.use_target(sim.player(0).unwrap()), Some(0), "after a second he may");

        // Nothing to use: the key opens the field computer as ever.
        sim.drops.clear();
        run(&mut sim, &host, 0, 1, press, 1);
        assert!(matches!(sim.player(0).unwrap().mode, Mode::FieldComputer { .. }));
    }

    #[test]
    fn a_dropped_weapon_goes_after_its_minute() {
        let host = FakeHost::default();
        let mut sim = Sim::new();
        sim.frame = LATE;
        let def: &'static WeaponDef = Box::leak(Box::new(WeaponDef { drop_object: "ZHBO_Drop_an94_mp".into(), ..weapon("an94_mp").clone() }));
        let weapon = Carried { def, clip: 1, stock: 0, alt_clip: 0, alt_stock: 0, options: WeaponOptions::default() };
        sim.put_down(&host, Dropped { weapon, pos: Vec3::ZERO, vel: Vec3::ZERO, resting: true, yaw: 0.0, expires: Some(LATE + 1800), owner: 7, owner_until: 0, object: 0 });
        let object = sim.drops[0].object;
        assert_ne!(object, 0, "its prop object");
        assert_eq!(host.spawned.borrow()[0].1, "ZHBO_Drop_an94_mp");
        step(&mut sim, &host, 1799);
        assert_eq!(sim.drops.len(), 1);
        step(&mut sim, &host, 1);
        assert!(sim.drops.is_empty() && host.destroyed.borrow().contains(&object), "60 s: gone with its object");
    }

    #[test]
    fn a_full_world_frees_the_drop_farthest_from_every_general_but_spares_the_shield() {
        let host = FakeHost::default();
        let mut sim = Sim::new();
        sim.frame = LATE;
        armed(&mut sim, &host, 0, 1, 0.0, "primary=an94_mp;secondary=fnp45_mp");
        let cap = crate::tuning::get().g_max_dropped_weapons as usize;
        let shield: &'static WeaponDef = Box::leak(Box::new(WeaponDef { drop: crate::weapons::DropRules { avoid_drop_cleanup: true, ..Default::default() }, ..weapon("riotshield_mp").clone() }));
        for i in 0..cap {
            let mut d = lying("svu_mp", Vec3::new(100.0 + i as f32 * 10.0, 0.0, 0.0));
            if i == cap - 1 {
                d.weapon.def = shield;
            }
            sim.drops.push(d);
        }
        let weapon = Carried { def: weapon("an94_mp"), clip: 1, stock: 0, alt_clip: 0, alt_stock: 0, options: WeaponOptions::default() };
        sim.put_down(&host, Dropped { weapon, pos: Vec3::ZERO, vel: Vec3::ZERO, resting: true, yaw: 0.0, expires: None, owner: 0, owner_until: 0, object: 0 });
        assert_eq!(sim.drops.len(), cap);
        assert!(sim.drops.iter().any(|d| d.weapon.def.id == "riotshield_mp"), "avoidDropCleanup spares the farthest");
        assert!(!sim.drops.iter().any(|d| d.pos.x == 100.0 + (cap - 2) as f32 * 10.0), "the farthest other one went");
    }
}
