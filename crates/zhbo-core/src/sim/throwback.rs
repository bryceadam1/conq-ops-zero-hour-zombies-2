/*
** Conquer Ops - Zero Hour gameplay core: picking up a live grenade to throw it back.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! BO2's grenade throwback. A live grenade near a player is in his use list (t6mp `FUN_00676d50`,
//! the missile case), and the use key picks it up (`FUN_005d2140`) to throw it as his own
//! (`FUN_00607730`), its fuse running on from what was left ([`crate::weapons::Arsenal::throw_back`]).
//!
//! The grenade qualifies when its weapon has `bThrowBack`, it is a live missile (not stuck to
//! anything, not equipment that lands, its fuse timed), its owner is an enemy or the player himself,
//! it moves no faster than `bg_maxGrenadeIndicatorSpeed`, it lies within `ThrowbackHeight` of his
//! feet, horizontally within `player_throwbackInnerRadius` of them — `player_throwbackOuterRadius`
//! once it is already his use target — within the outer radius of his eye, and he can see it. Of
//! those, the lowest use-list score: its distance, plus `UseListDotScale` × `1 − (dot + 1) / 2` off
//! his view, less `UseListGrenadeBonus` (so a grenade outranks any dropped weapon).
//!
//! The use target is kept on the player ([`PlayerSim::use_grenade`], the grenade's object), as BO2
//! keeps the cursor-hint entity: it decides the outer radius and the HUD's hint.

use super::*;

impl Sim {
    /// The live grenade `p` could throw back now, by its index in [`Sim::projectiles`].
    pub(super) fn throwback_target(&self, host: &dyn LogicHost, p: &PlayerSim) -> Option<usize> {
        let t = &p.arsenal.tuning;
        let eye = p.body.eye();
        let dir = dir_from_angles(p.yaw, p.pitch);
        let inch = inches(1.0);
        let outer = inches(t.player_throwback_outer_radius);
        let cover = crate::cover::Blockers::near(host, eye, outer * 2.0, p.general, &[]);
        let mut best: Option<(usize, f32)> = None;
        for (i, (_, g)) in self.projectiles.iter().enumerate() {
            if !g.def.throw_back || g.lands || g.stuck_to.is_some() || !g.fuse.is_finite() || g.object == 0 {
                continue;
            }
            let own = g.owner == p.general;
            if !own && host.object(g.owner, p.general).is_none_or(|o| o.has(objflags::ALLY)) {
                continue;
            }
            if g.vel.length() > inches(t.bg_max_grenade_indicator_speed) || (g.pos.z - p.body.pos.z).abs() > inches(t.throwback_height) {
                continue;
            }
            let flat = (g.pos - p.body.pos).length_xy();
            let reach = if p.use_grenade == g.object { outer } else { inches(t.player_throwback_inner_radius) };
            if flat > reach {
                continue;
            }
            let to = g.pos - eye;
            let dist = to.length();
            if dist > outer || !cover.clear(host, eye, g.pos, g.object) {
                continue;
            }
            let dot = if dist > 0.0 { dir.dot(to) / dist } else { 1.0 };
            let score = dist / inch + t.use_list_dot_scale * (1.0 - (dot + 1.0) * 0.5) - t.use_list_grenade_bonus;
            if best.is_none_or(|(_, b)| score < b) {
                best = Some((i, score));
            }
        }
        best.map(|(i, _)| i)
    }

    /// The use key with a grenade targeted: picked up into his hand, its fuse running on. True
    /// when he took it (his offhand free, `Arsenal::throw_back`).
    pub(super) fn use_grenade(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim) -> bool {
        let Some(i) = self.projectiles.iter().position(|(_, g)| g.object != 0 && g.object == p.use_grenade) else { return false };
        let (def, fuse) = (self.projectiles[i].1.def, self.projectiles[i].1.fuse);
        let mut actions = Vec::new();
        if !p.arsenal.throw_back(def, fuse, &mut actions) {
            return false;
        }
        let (_, mut g) = self.projectiles.remove(i);
        g.intercept(host);
        p.use_grenade = 0;
        p.charge = None;
        for a in actions {
            self.apply_weapon_action(host, idx, p, a);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::tests::{general, FakeHost};
    use crate::weapons::{weapon, WeaponDef};

    /// A frag with BO2's `bThrowBack` (the built-in table's has the converted values' shape).
    fn frag() -> &'static WeaponDef {
        Box::leak(Box::new(WeaponDef { throw_back: true, fuse_time: 3.5, cook: true, projectile_object: "Frag".into(), ..weapon("frag_grenade_mp").clone() }))
    }

    /// Two Generals in first person: player 0's General 1 at the origin facing +x, an enemy (2).
    fn duel() -> (FakeHost, Sim) {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(ObjInfo { pos: Vec3::new(500.0, 0.0, 0.0), ..general(2, 1) });
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        sim.take_control(&host, 1, 2);
        let p = sim.players[0].as_mut().unwrap();
        p.class = Some("x".into());
        p.arsenal = crate::weapons::Arsenal::with_loadout(crate::weapons::Loadout::test_kit());
        (host, sim)
    }

    /// A grenade of `owner`'s lying at `at` with `fuse` seconds left.
    fn lay(sim: &mut Sim, host: &FakeHost, def: &'static WeaponDef, owner: u32, at: Vec3, fuse: f32) {
        let mut g = Projectile::launch(def, owner, at, Vec3::ZERO, fuse, host, 0.0);
        g.vel = Vec3::ZERO;
        g.resting = true;
        sim.projectiles.push((1, g));
    }

    /// BO2's rules: an enemy's live frag inside the inner radius is a use target; outside it, or
    /// too high above him, or moving too fast, or a weapon without `bThrowBack`, it is not; once it
    /// is his target it stays one out to the outer radius.
    #[test]
    fn who_can_throw_back_what() {
        let t = crate::tuning::get();
        let inner = inches(t.player_throwback_inner_radius);
        let outer = inches(t.player_throwback_outer_radius);
        let (host, mut sim) = duel();
        lay(&mut sim, &host, frag(), 2, Vec3::new(inner * 0.8, 0.0, 0.0), 2.0);
        let p = sim.player(0).unwrap().clone();
        assert_eq!(sim.throwback_target(&host, &p), Some(0), "an enemy's frag within the inner radius");
        // Past the inner radius: only once it is his target.
        sim.projectiles[0].1.pos.x = (inner + outer) * 0.5;
        assert_eq!(sim.throwback_target(&host, &p), None, "first seen past the inner radius");
        let mut tracked = p.clone();
        tracked.use_grenade = sim.projectiles[0].1.object;
        assert_eq!(sim.throwback_target(&host, &tracked), Some(0), "his target already: the outer radius");
        sim.projectiles[0].1.pos = Vec3::new(inner * 0.5, 0.0, inches(t.throwback_height) * 1.2);
        assert_eq!(sim.throwback_target(&host, &p), None, "too far above his feet");
        sim.projectiles[0].1.pos = Vec3::new(inner * 0.5, 0.0, 0.0);
        sim.projectiles[0].1.vel = Vec3::new(inches(t.bg_max_grenade_indicator_speed) * 1.5, 0.0, 0.0);
        assert_eq!(sim.throwback_target(&host, &p), None, "still flying fast");
        sim.projectiles[0].1.vel = Vec3::ZERO;
        let plain: &'static WeaponDef = Box::leak(Box::new(WeaponDef { throw_back: false, ..frag().clone() }));
        sim.projectiles[0].1.def = plain;
        assert_eq!(sim.throwback_target(&host, &p), None, "no bThrowBack");
        // His own grenade: yes; an ally's: no.
        sim.projectiles[0].1.def = frag();
        sim.projectiles[0].1.owner = 1;
        assert_eq!(sim.throwback_target(&host, &p), Some(0), "his own");
        host.teams.borrow_mut().extend([(0, 0), (1, 0)]);
        sim.projectiles[0].1.owner = 2;
        assert_eq!(sim.throwback_target(&host, &p), None, "an ally's");
    }

    /// Without Fast Hands (whose `pin_back` bumps the fuse, [`fast_hands_resets_a_thrown_back_fuse`]).
    fn no_fast_hands(sim: &mut Sim) {
        let a = &mut sim.players[0].as_mut().unwrap().arsenal;
        a.loadout.perks.retain(|&p| Some(p) != crate::weapons::Perk::by_id("FastHands"));
        assert!(!a.has_specialty(crate::weapons::specialty::Specialty::PinBack));
    }

    /// Picking it up keeps the cooked fuse: thrown back at once (no button held), it leaves his
    /// hand as his, with what was left less the time in his hand; the slot keeps its count. Held
    /// past its fuse (the lethal button down), it goes off in his hand.
    #[test]
    fn a_picked_up_grenade_keeps_its_fuse() {
        let (host, mut sim) = duel();
        no_fast_hands(&mut sim);
        let def = frag();
        lay(&mut sim, &host, def, 2, Vec3::new(inches(40.0), 0.0, 0.0), 2.4);
        let count = sim.player(0).unwrap().arsenal.lethal.count;
        let use_key = FpsInput { held: buttons::FIELD_COMPUTER, pressed: buttons::FIELD_COMPUTER, ..Default::default() };
        crate::sim::tests::run(&mut sim, &host, 0, 1, FpsInput::default(), 1);
        assert_ne!(sim.player(0).unwrap().use_grenade, 0, "the frag is his use target");
        crate::sim::tests::run(&mut sim, &host, 0, 1, use_key, 1);
        assert!(sim.projectiles.is_empty(), "picked up");
        assert_eq!(sim.player(0).unwrap().arsenal.throwback.map(|d| d.id.as_str()), Some(def.id.as_str()));
        let mut frames = 0;
        while sim.projectiles.is_empty() && frames < 200 {
            crate::sim::tests::run(&mut sim, &host, 0, 1, FpsInput::default(), 1);
            frames += 1;
        }
        let (_, g) = &sim.projectiles[0];
        assert_eq!(g.owner, 1, "thrown back as his");
        let held = frames as f32 * LOGIC_DT;
        assert!(g.fuse < 2.4 - held + 0.1 && g.fuse > 0.0, "the fuse ran on in his hand: {} after {held}s", g.fuse);
        assert_eq!(sim.player(0).unwrap().arsenal.lethal.count, count, "the slot keeps its count");
        // Cooked off in hand: picked up with almost nothing left and held.
        let (host, mut sim) = duel();
        no_fast_hands(&mut sim);
        lay(&mut sim, &host, def, 2, Vec3::new(inches(40.0), 0.0, 0.0), 0.2);
        crate::sim::tests::run(&mut sim, &host, 0, 1, FpsInput::default(), 1);
        crate::sim::tests::run(&mut sim, &host, 0, 1, use_key, 1);
        let hold = FpsInput { held: buttons::LETHAL, ..Default::default() };
        let mut cooked = false;
        for _ in 0..90 {
            crate::sim::tests::run(&mut sim, &host, 0, 1, hold, 1);
            // Gone off at his own eye: he takes his own blast.
            if host.damage_log.borrow().iter().any(|d| d.1 == 1) {
                cooked = true;
                break;
            }
        }
        assert!(cooked, "it went off in his hand");
    }

    /// Fast Hands (`pin_back`) bumps a picked-up grenade's fuse to `perk_grenadeTossBackTimer`
    /// (t6mp `FUN_005d2140`): picked up with 0.2 s left, it is thrown back with about 2.5 s.
    #[test]
    fn fast_hands_resets_a_thrown_back_fuse() {
        let (host, mut sim) = duel();
        let def = frag();
        let p = sim.players[0].as_mut().unwrap();
        p.arsenal.loadout.perks.push(crate::weapons::Perk::by_id("FastHands").expect("Fast Hands"));
        assert!(p.arsenal.has_specialty(crate::weapons::specialty::Specialty::PinBack));
        lay(&mut sim, &host, def, 2, Vec3::new(inches(40.0), 0.0, 0.0), 0.2);
        let use_key = FpsInput { held: buttons::FIELD_COMPUTER, pressed: buttons::FIELD_COMPUTER, ..Default::default() };
        crate::sim::tests::run(&mut sim, &host, 0, 1, FpsInput::default(), 1);
        crate::sim::tests::run(&mut sim, &host, 0, 1, use_key, 1);
        assert!(sim.projectiles.is_empty(), "picked up");
        let mut frames = 0;
        while sim.projectiles.is_empty() && frames < 200 {
            crate::sim::tests::run(&mut sim, &host, 0, 1, FpsInput::default(), 1);
            frames += 1;
        }
        assert!(host.damage_log.borrow().iter().all(|d| d.1 != 1), "it did not go off in his hand");
        let (_, g) = &sim.projectiles[0];
        let want = (crate::tuning::get().perk_grenade_toss_back_timer).min(def.fuse_time) - frames as f32 * LOGIC_DT;
        assert!((g.fuse - want).abs() < 0.1, "bumped to the perk's timer: {} want {want}", g.fuse);
    }
}
