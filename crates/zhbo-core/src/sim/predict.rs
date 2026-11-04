//! What the client's prediction runs: the sim's own step for one player, on a copy of the sim,
//! against the engine through a [`Sandbox`] that answers every query and swallows every change.
//!
//! Zero Hour's lockstep runs a first-person input its run-ahead after it was sent, so the local
//! player's machine predicts its own General over the inputs still in flight
//! (`client::predict`): it copies the sim ([`Sim::scratch`]) and steps that player alone over
//! each of them ([`Sim::predict_step`]), with the same `step_player` the logic step runs, so a
//! prediction is what the sim will compute when the world it reads has not changed in between.
//! Nothing here reaches the real sim: the copy is thrown away, the sandbox never forwards a
//! change to the engine (no pose, damage, spawn, FX or log), and the events the copy emits are
//! presentation for the predicting machine only.
/*
** Conquer Ops - Zero Hour gameplay core: the local player's predicted step.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::*;
use std::cell::Cell;

/// The engine as a prediction sees it: every read forwarded, every write dropped. The predicted
/// General's own pose and health are the prediction's (the engine still has him where the last
/// real step left him, which the step would take for a teleport and follow).
pub struct Sandbox<'a> {
    host: &'a dyn LogicHost,
    general: u32,
    pos: Cell<Vec3>,
    yaw: Cell<f32>,
    health: Cell<f32>,
}

impl<'a> Sandbox<'a> {
    pub fn new(host: &'a dyn LogicHost, p: &PlayerSim) -> Self {
        Sandbox { host, general: p.general, pos: Cell::new(p.body.pos), yaw: Cell::new(p.yaw), health: Cell::new(p.health) }
    }

    fn own(&self, mut o: ObjInfo) -> ObjInfo {
        if o.id == self.general {
            o.pos = self.pos.get();
            o.yaw = self.yaw.get();
            o.health = self.health.get().min(o.max_health);
        }
        o
    }
}

impl LogicHost for Sandbox<'_> {
    fn log(&self, _msg: &str) {}
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
        self.host.object(id, viewer).map(|o| self.own(o))
    }
    fn query(&self, center: Vec3, radius: f32, viewer: u32, out: &mut Vec<ObjInfo>) {
        let from = out.len();
        self.host.query(center, radius, viewer, out);
        for o in &mut out[from..] {
            *o = self.own(*o);
        }
    }
    fn set_pose(&self, id: u32, pos: Vec3, yaw: f32, _moving: bool) {
        if id == self.general {
            self.pos.set(pos);
            self.yaw.set(yaw);
        }
    }
    fn set_controlled(&self, _id: u32, _controlled: bool) {}
    fn fire_fx(&self, _shooter: u32, _fx_weapon: &str, _target: u32, _pos: Vec3) {}
    /// Nobody is hurt by a prediction (and so it never knows of a kill: the sim's hit marker says so).
    fn damage(&self, _shooter: u32, _victim: u32, _amount: f32, _how: crate::damage::MeansOfDeath) -> bool {
        false
    }
    fn heal(&self, id: u32, amount: f32) {
        if id == self.general {
            self.health.set(self.health.get() + amount);
        }
    }
    fn spawn(&self, _template: &str, _owner: u32, _pos: Vec3, _yaw: f32) -> u32 {
        0
    }
    fn destroy(&self, _id: u32) {}
    fn disable(&self, _id: u32, _kind: i32, _seconds: f32) {}
    fn fx(&self, _name: &str, _pos: Vec3) {}
    fn capture(&self, _id: u32, _new_owner: u32) {}
    fn smoke(&self, _center: Vec3, _radius: f32) {}
    fn build_options(&self, general: u32) -> Vec<crate::building::BuildOption> {
        self.host.build_options(general)
    }
    fn can_place(&self, general: u32, template: u32, pos: Vec3, angle: f32) -> bool {
        self.host.can_place(general, template, pos, angle)
    }
    fn place(&self, _general: u32, _template: u32, _pos: Vec3, _angle: f32) -> u32 {
        0
    }
    fn work_site(&self, general: u32, pos: Vec3, reach: f32) -> Option<crate::building::Site> {
        self.host.work_site(general, pos, reach)
    }
    /// Still working (the site's real progress is the sim's).
    fn work(&self, _general: u32, _site: u32, _rate: f32) -> f32 {
        0.0
    }
}

impl Sim {
    /// A copy of everything a player's step reads, to predict on: the players, the equipment,
    /// smoke and shields, the history, the frame and the tuning; no projectiles in flight (a
    /// player's step does not move them) and no presentation buffers.
    pub fn scratch(&self) -> Sim {
        Sim {
            players: self.players.clone(),
            projectiles: Vec::new(),
            equipment: self.equipment.clone(),
            smokes: self.smokes.clone(),
            smoke: self.smoke,
            tuning: self.tuning,
            bullet_range: self.bullet_range,
            penetration: self.penetration.clone(),
            hit_body: self.hit_body,
            frame: self.frame,
            chosen: self.chosen.clone(),
            events: Vec::new(),
            tracers: Vec::new(),
            shields: self.shields.clone(),
            deployed: self.deployed.clone(),
            drops: self.drops.clone(),
            kills: self.kills.clone(),
            combat_seen: 0,
            emitted: 0,
            antilag_hits: Vec::new(),
            // A player's step deals damage only through the sandbox, which kills nobody.
            score: super::score::Score::default(),
            history: self.history.clone(),
            antilag_frames: self.antilag_frames,
        }
    }

    /// One logic frame of player `idx` alone on a [`Sim::scratch`] copy: `input` arrives (sent
    /// while frame `seen_frame` showed) and his step runs as [`Sim::step`] runs it, through a
    /// [`Sandbox`] over `host`. Returns false when he has no record after it (died, or none).
    pub fn predict_step(&mut self, host: &dyn LogicHost, idx: usize, input: FpsInput, seen_frame: u32) -> bool {
        let Some(general) = self.players.get(idx).and_then(|p| p.as_ref()).map(|p| p.general) else { return false };
        self.frame += 1;
        self.input_seen(idx as i32, general, input, seen_frame);
        // As `step` begins: the shields and the perks against blasts, as the frame starts.
        self.shields = self.shield_bearers();
        self.equipment.shields.clone_from(&self.shields);
        self.equipment.guards.clear();
        self.equipment.guards.extend(self.players.iter().flatten().map(|p| guard::Guard::of(p.general, &p.arsenal, p.is_first_person())));
        let Some(mut p) = self.players[idx].take() else { return false };
        let sandbox = Sandbox::new(host, &p);
        self.combat_seen = 0;
        let keep = self.step_player(&sandbox, idx, &mut p);
        if self.combat_seen & (1 << idx) != 0 {
            p.combat = true;
        }
        if keep {
            self.players[idx] = Some(p);
        }
        keep
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::tests::{general, soldier, spawn, FakeHost};

    /// The prediction is the sim: the same inputs stepped on a scratch copy leave the player's
    /// record exactly as the real steps do, and the engine never hears of it.
    #[test]
    fn a_predicted_step_is_the_sims_step_and_touches_nothing() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(soldier(2, 1, 60.0));
        let mut sim = Sim::new();
        spawn(&mut sim, &host, 0, 1);
        let p = sim.players[0].as_mut().unwrap();
        p.arsenal = Arsenal::with_loadout(crate::weapons::Loadout::test_kit());
        p.arsenal.raise_left = 0.0;
        let inputs: Vec<FpsInput> = (0..40)
            .map(|i| FpsInput {
                held: if (10..25).contains(&i) { buttons::FIRE | buttons::ADS } else { 0 },
                pressed: if i == 10 { buttons::FIRE | buttons::ADS } else if i == 30 { buttons::RELOAD } else { 0 },
                move_packed: crate::input::pack_move(if i >= 26 { 1.0 } else { 0.0 }, if i >= 32 { 0.5 } else { 0.0 }),
                yaw: if i >= 26 { 0.02 * i as f32 } else { 0.0 },
                pitch: -(4.0f32 / 60.0).atan(),
            })
            .collect();
        let damage_before = host.damage_log.borrow().len();
        let fx_before = host.fx.borrow().len();
        let mut scratch = sim.scratch();
        let mut predicted_events = Vec::new();
        for (i, input) in inputs.iter().enumerate() {
            assert!(scratch.predict_step(&host, 0, *input, 100 + i as u32));
            predicted_events.extend(scratch.events.drain(..).map(|e| e.kind));
        }
        assert_eq!(host.damage_log.borrow().len(), damage_before, "the prediction hurt nobody");
        assert_eq!(host.fx.borrow().len(), fx_before, "and played nothing in the engine");
        assert!(predicted_events.iter().any(|k| matches!(k, EventKind::Shot { .. })), "it fired");
        assert!(predicted_events.iter().any(|k| matches!(k, EventKind::HitMarker { kill: false, .. })), "and predicted a hit");

        // The real thing: a soldier who moves nothing, so the world it reads is the same.
        let host2 = FakeHost::default();
        host2.add(general(1, 0));
        host2.add(soldier(2, 1, 60.0));
        let mut real = Sim::new();
        spawn(&mut real, &host2, 0, 1);
        let p = real.players[0].as_mut().unwrap();
        p.arsenal = Arsenal::with_loadout(crate::weapons::Loadout::test_kit());
        p.arsenal.raise_left = 0.0;
        for (i, input) in inputs.iter().enumerate() {
            real.input_seen(0, 1, *input, 100 + i as u32);
            real.step(&host2, real.frame + 1);
            // The soldier soaks the damage the prediction could not deal: heal him back so the
            // world stays the one the prediction read.
            for o in host2.objects.borrow_mut().iter_mut().filter(|o| o.id == 2) {
                o.health = o.max_health;
                o.flags &= !objflags::DEAD;
            }
        }
        let a = real.player(0).unwrap();
        let b = scratch.player(0).unwrap();
        assert_eq!(format!("{:?}", a.body), format!("{:?}", b.body));
        assert_eq!((a.yaw, a.pitch, a.seen_frame), (b.yaw, b.pitch, b.seen_frame));
        assert_eq!(format!("{:?}", a.view_kick), format!("{:?}", b.view_kick));
        assert_eq!(a.arsenal.current().ammo, b.arsenal.current().ammo);
        assert_eq!(a.arsenal.ads, b.arsenal.ads);
        assert_eq!(a.rng, b.rng);
    }
}
