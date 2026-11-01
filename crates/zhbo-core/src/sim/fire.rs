/*
** Conquer Ops - Zero Hour gameplay core: the shot: weapon actions, targets and throw arcs.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::*;
use crate::impacts::Surface;
use crate::weapons::FireMode;

impl Sim {
    pub(super) fn apply_weapon_action(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, action: WeaponAction) {
        match action {
            WeaponAction::Fire { spread_deg, jitter, view_kick, gun_kick, pellet, last, left } => {
                let def = p.arsenal.current().def;
                // The shot sets the view kick's velocity (`FUN_008e3a00`); this frame's pellets fly
                // along the aim the kick had already moved.
                if pellet == 0 {
                    p.view_kick.kick(view_kick);
                    // The fire start lets a held breath go (`FUN_008dea30`).
                    let extra = p.arsenal.has_specialty(Specialty::HoldBreath);
                    p.idle.breath.fired(def, p.arsenal.ads, &p.arsenal.tuning, extra);
                }
                let eye = p.body.eye();
                // Random point in the spread cone (polar mapping of two uniform samples).
                let angle = jitter.0 * std::f32::consts::PI;
                let radius = spread_deg.to_radians() * jitter.1.abs().sqrt();
                let dir = dir_from_angles(p.fire_yaw + radius * angle.cos(), p.fire_pitch + radius * angle.sin());

                let range = def.bullet_range(self.bullet_range);
                let mut targets = gather_targets(host, eye, range, p.general);
                // A deployed shield's object is the sim's to hit (its box, below), not a target.
                targets.retain(|t| !self.deployed.iter().any(|d| d.object != 0 && d.object == t.id));
                let ground = |x: f32, y: f32| host.ground_height(x, y);
                let hit = hitscan::trace(eye, dir, range, &targets, p.general, &ground);
                // One flash and sound per shot, not per pellet.
                let fx = if pellet == 0 { Some(def.fx_weapon.as_str()).filter(|f| !f.is_empty()) } else { None };
                // What others see of the shot besides the flash: the brass once per shot, a tracer
                // on every `tracer_interval`-th bullet (each pellet is a bullet).
                if pellet == 0 {
                    let brass = if last { &def.world_last_shot_shell_fx_weapon } else { &def.world_shell_fx_weapon };
                    if !brass.is_empty() {
                        host.fire_fx(p.general, brass, 0, eye + dir * 10.0);
                    }
                }
                let tracer = !def.tracer_fx_weapon.is_empty() && tracer_due(&mut p.tracer_count, def.tracer_interval);
                // Enemy equipment in the way takes the shot.
                let reach = match hit {
                    HitResult::Object { distance, .. } => distance,
                    other => (other.point() - eye).length(),
                };
                if let Some((i, t)) = self.equipment.shot(host, p.general, eye, dir, reach) {
                    if let Some(fx) = fx {
                        host.fire_fx(p.general, fx, 0, eye + dir * t);
                    }
                    if tracer {
                        self.tracers.push(ShotTracer { shooter: p.general, def, target: 0, end: eye + dir * t, left });
                    }
                    let e = self.equipment.destroy(host, i, Some(idx));
                    self.equipment_events(host, vec![e]);
                    // BO2's equipment is metal: a spark at the round, never blood.
                    self.emit(idx, EventKind::Impact { at: eye + dir * t, dir, surface: Surface::Metal, head: false, fatal: false, impact_type: def.impact_type });
                    if pellet == 0 {
                        self.emit(idx, EventKind::Shot { origin: eye, end: eye + dir * t, left });
                        self.emit(idx, EventKind::Recoil { gun_kick });
                    }
                    return;
                }
                // A deployed shield on the ray takes the round (bullet-scaled damage).
                if let Some((i, t)) = self.deployed_on_ray(eye, dir, reach) {
                    let at = eye + dir * t;
                    if let Some(fx) = fx {
                        host.fire_fx(p.general, fx, 0, at);
                    }
                    self.damage_deployed(host, i, Some(idx), p.general, def.damage_at(t), deploy::ShieldDamage::Bullet);
                    self.emit(idx, EventKind::ShieldHit { at, dir, knife: false, impact_type: def.impact_type });
                    if tracer {
                        self.tracers.push(ShotTracer { shooter: p.general, def, target: 0, end: at, left });
                    }
                    if pellet == 0 {
                        self.emit(idx, EventKind::Shot { origin: eye, end: at, left });
                        self.emit(idx, EventKind::Recoil { gun_kick });
                    }
                    return;
                }
                // An enemy's Assault Shield on the ray: location `riotshield`, which outranks the
                // body (rank 101) and takes no damage (`_globallogic_player.gsc:840-888`).
                let hit_id = if let HitResult::Object { id, .. } = hit { Some(id) } else { None };
                let enemy = |b: &crate::shield::Bearer| b.general != p.general && targets.iter().any(|t| t.id == b.general);
                if let Some((_, t)) = crate::shield::blocking(&self.shields, &enemy, eye, dir, reach, range, hit_id) {
                    let at = eye + dir * t;
                    if let Some(fx) = fx {
                        host.fire_fx(p.general, fx, 0, at);
                    }
                    self.emit(idx, EventKind::ShieldHit { at, dir, knife: false, impact_type: def.impact_type });
                    if tracer {
                        self.tracers.push(ShotTracer { shooter: p.general, def, target: 0, end: at, left });
                    }
                    if pellet == 0 {
                        self.emit(idx, EventKind::Shot { origin: eye, end: at, left });
                        self.emit(idx, EventKind::Recoil { gun_kick });
                    }
                    return;
                }
                match hit {
                    HitResult::Object { id, point, distance, headshot } => {
                        if let Some(fx) = fx {
                            host.fire_fx(p.general, fx, id, point);
                        }
                        if tracer {
                            self.tracers.push(ShotTracer { shooter: p.general, def, target: id, end: point, left });
                        }
                        let mult = if headshot { def.head_multiplier } else { 1.0 };
                        let victim = host.object(id, p.general);
                        let infantry = victim.is_some_and(|v| v.has(objflags::INFANTRY));
                        let fmj = if def.fmj && !infantry { FMJ_VEHICLE_DAMAGE } else { 1.0 };
                        let amount = def.damage_at(distance) * mult * fmj * BO2_TO_ZH_DAMAGE;
                        let mut kill = host.damage(p.general, id, amount, damage_kind::SMALL_ARMS);
                        // FMJ: on through the first soldier into whoever is behind.
                        if def.fmj && infantry {
                            targets.retain(|t| t.id != id);
                            if let HitResult::Object { id: next, distance, headshot, .. } = hitscan::trace(point + dir * 0.1, dir, range - distance, &targets, p.general, &ground) {
                                let mult = if headshot { def.head_multiplier } else { 1.0 };
                                kill |= host.damage(p.general, next, def.damage_at(distance) * mult * PENETRATION_DAMAGE * BO2_TO_ZH_DAMAGE, damage_kind::SMALL_ARMS);
                            }
                        }
                        // BO2's blood is picked by the hit region and its lethality (FxFleshType);
                        // every other surface comes from what was hit. One impact per pellet, at
                        // its entry point (an FMJ round's exit impact is a separate feature).
                        self.emit(idx, EventKind::Impact { at: point, dir, surface: Surface::of_object(victim.as_ref()), head: headshot, fatal: kill, impact_type: def.impact_type });
                        self.emit(idx, EventKind::HitMarker { headshot, kill });
                        if kill && p.arsenal.has_specialty(Specialty::Scavenger) {
                            p.arsenal.scavenge();
                        }
                    }
                    other => {
                        if let Some(fx) = fx {
                            host.fire_fx(p.general, fx, 0, other.point());
                        }
                        if tracer {
                            self.tracers.push(ShotTracer { shooter: p.general, def, target: 0, end: other.point(), left });
                        }
                        // The ground: Zero Hour's terrain class at the point (the host's query).
                        // A shot that flew out to its full range hit nothing, so it raises nothing.
                        if matches!(other, HitResult::Terrain { .. }) {
                            let at = other.point();
                            let surface = host.surface_at(at.x, at.y);
                            self.emit(idx, EventKind::Impact { at, dir, surface, head: false, fatal: false, impact_type: def.impact_type });
                        }
                    }
                }
                if pellet == 0 {
                    self.emit(idx, EventKind::Shot { origin: eye, end: hit.point(), left });
                    self.emit(idx, EventKind::Recoil { gun_kick });
                }
            }
            WeaponAction::Launch { def, pellet } => {
                let eye = p.body.eye();
                // A stack's bolts fan across `stackFireSpread` degrees of yaw: the bolt fired with
                // k rounds loaded of an n-round stack at spread × ((k - 1) / (n - 1) - 0.5)
                // (`FUN_0066e320`; `pellet` is k - 1).
                let n = def.rounds_per_shot();
                let spread = if def.fire == FireMode::Stacked && n > 1 { def.traits.stack_fire.0.to_radians() * (pellet as f32 / (n - 1) as f32 - 0.5) } else { 0.0 };
                let dir = dir_from_angles(p.fire_yaw + spread, p.fire_pitch);
                if !def.fx_weapon.is_empty() {
                    host.fire_fx(p.general, &def.fx_weapon, 0, eye + dir * 10.0);
                }
                let proj = Projectile::launch(def, p.general, eye + dir * 2.0, dir, def.fuse_time, host, p.yaw);
                self.projectiles.push((idx, proj));
                // One launch cue per pull: the stack's last bolt, everything else's only one.
                if pellet == 0 {
                    self.emit(idx, EventKind::Launched);
                }
            }
            WeaponAction::Throw { def, fuse_left } => {
                let eye = p.body.eye();
                // BO2 throws a little above the crosshair.
                let dir = dir_from_angles(p.yaw, (p.pitch + 0.12).min(1.4));
                let mut proj = Projectile::launch(def, p.general, eye + dir * 1.5, dir, fuse_left, host, p.yaw);
                proj.vel += p.body.vel * 0.5;
                (proj.self_blast_scale, proj.self_stun_scale) = p.arsenal.self_harm_scales();
                self.projectiles.push((idx, proj));
                self.emit(idx, EventKind::Thrown { lethal: def.class == Class::Lethal });
            }
            WeaponAction::CookedOff { def } => {
                let mut proj = Projectile::launch(def, p.general, p.body.eye(), Vec3::ZERO, 0.0, host, p.yaw);
                proj.vel = Vec3::ZERO;
                proj.fuse = 0.0;
                (proj.self_blast_scale, proj.self_stun_scale) = p.arsenal.self_harm_scales();
                self.projectiles.push((idx, proj));
            }
            WeaponAction::OffhandReady { lethal } => self.emit(idx, EventKind::OffhandReady { lethal }),
            WeaponAction::CantPlant { lethal } => self.emit(idx, EventKind::CantPlant { lethal }),
            WeaponAction::MeleeSwing => self.emit(idx, EventKind::Melee),
            WeaponAction::Melee { damage } => self.melee_strike(host, idx, p, damage, MeleeReach::Body),
            WeaponAction::Plant { def } => {
                // Where BO2's launch sets it down, not a fixed distance in front: a lobbed item
                // flies, bounces and is placed where it rests (the projectiles' `Landed`).
                let Some(at) = plant_point(def, &p.body, p.yaw, p.pitch, host) else {
                    let proj = plant_launch(def, &p.body, p.yaw, p.pitch, p.general, host);
                    self.projectiles.push((idx, proj));
                    return;
                };
                let object = if def.projectile_object.is_empty() { 0 } else { host.spawn(&def.projectile_object, p.general, at, p.yaw) };
                let placed = Placed::new(def, idx, p.general, object, at, p.yaw, None);
                self.emit(idx, EventKind::Planted { kind: placed.kind });
                self.equipment.add(placed);
            }
            WeaponAction::Detonate => {
                let mut ev = Vec::new();
                self.equipment.detonate(host, idx, &mut ev);
                if !ev.is_empty() {
                    self.emit(idx, EventKind::Detonated);
                }
                // Points for what the C4 kills (the owner's own state is out of the table now).
                for e in &ev {
                    if let EquipEvent::Exploded { at, hit_enemy, kill, .. } = *e {
                        self.emit(idx, EventKind::Exploded { at });
                        if hit_enemy {
                            self.emit(idx, EventKind::HitMarker { headshot: false, kill });
                        }
                        if kill {
                        }
                    }
                }
            }
            WeaponAction::AltToggled => self.emit(idx, EventKind::AltToggled),
            WeaponAction::ReloadStarted => self.emit(idx, EventKind::ReloadStarted),
            WeaponAction::Swapped => self.emit(idx, EventKind::Swapped),
            WeaponAction::DeployShield { def } => self.deploy_shield(host, idx, p, def),
            WeaponAction::DryFire | WeaponAction::ReloadFinished => {}
        }
    }
}

/// FMJ: damage to the second soldier a round passes through.
pub const PENETRATION_DAMAGE: f32 = 0.75;

/// How far a knife reaches. In first person the General is BO2's player body, so the knife reaches
/// [`MELEE_RANGE`] past its hit radius. Under RTS control Zero Hour walked him up to the victim with
/// its own footprints (attack range is measured edge to edge, `Weapon::getAttackDistance`), so the
/// reach counts both footprints the same way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeleeReach {
    Body,
    Footprint(f32),
}

impl MeleeReach {
    fn reaches(self, from: Vec3, o: &ObjInfo) -> bool {
        let d = (o.pos - from).length_xy();
        match self {
            MeleeReach::Body => d <= MELEE_RANGE + reach_radius(o),
            MeleeReach::Footprint(own) => d <= MELEE_RANGE + own + o.major_radius,
        }
    }
}

impl Sim {
    /// BO2's knife: the nearest enemy in front of the General and in reach takes the hit.
    /// The swing's event (`EventKind::Melee`, the swipe sound) is the caller's: first person emits
    /// it on the press ([`WeaponAction::MeleeSwing`]), the RTS strike with the blow.
    pub(super) fn melee_strike(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, damage: f32, reach: MeleeReach) {
        let eye = p.body.eye();
        let (fwd, _) = basis_xy(p.yaw);
        let extra = match reach {
            MeleeReach::Body => 12.0,
            MeleeReach::Footprint(own) => own + 12.0,
        };
        let mut near = Vec::new();
        host.query(eye, MELEE_RANGE + extra, p.general, &mut near);
        let victim = near
            .iter()
            .filter(|o| o.id != p.general && !o.has(objflags::DEAD) && !o.has(objflags::ALLY))
            .filter(|o| !self.deployed.iter().any(|d| d.object != 0 && d.object == o.id))
            .filter(|o| !o.has(objflags::NO_COLLIDE) && !o.has(objflags::STRUCTURE))
            .filter(|o| {
                let to = o.pos - p.body.pos;
                let d = to.length_xy();
                reach.reaches(p.body.pos, o) && (d < 1e-3 || (to.x * fwd.x + to.y * fwd.y) / d > 0.6)
            })
            .min_by(|a, b| {
                let da = (a.pos - p.body.pos).length_xy();
                let db = (b.pos - p.body.pos).length_xy();
                da.total_cmp(&db).then(a.id.cmp(&b.id))
            });
        // A deployed shield in front, before the victim, takes the strike (melee-scaled damage).
        let victim_at = victim.map_or(MELEE_RANGE + extra, |v| (v.pos - p.body.pos).length_xy());
        if let Some((i, t)) = self.deployed_on_ray(eye, fwd, (MELEE_RANGE + extra).min(victim_at)) {
            self.damage_deployed(host, i, Some(idx), p.general, damage, deploy::ShieldDamage::Melee);
            self.emit(idx, EventKind::ShieldHit { at: eye + fwd * t, dir: fwd, knife: true, impact_type: 0 });
            return;
        }
        if let Some(v) = victim {
            // The strike at a shield bearer's body crosses the shield first: no damage
            // (location `riotshield`, `_globallogic_player.gsc:840`).
            let chest = Vec3::new(v.pos.x, v.pos.y, v.pos.z + v.height * 0.6);
            let to = chest - eye;
            let mine = |b: &crate::shield::Bearer| b.general == v.id;
            if let Some((_, t)) = crate::shield::blocking(&self.shields, &mine, eye, to.normalized_or_zero(), to.length(), to.length(), Some(v.id)) {
                let at = eye + to.normalized_or_zero() * t;
                self.emit(idx, EventKind::ShieldHit { at, dir: to.normalized_or_zero(), knife: true, impact_type: 0 });
                return;
            }
            host.fire_fx(p.general, MELEE_FX_WEAPON, v.id, v.pos);
            let kill = host.damage(p.general, v.id, damage * BO2_TO_ZH_DAMAGE, damage_kind::MELEE);
            self.emit(idx, EventKind::HitMarker { headshot: false, kill });
            if kill && p.arsenal.has_specialty(Specialty::Scavenger) {
                p.arsenal.scavenge();
            }
        }
    }
}

fn reach_radius(o: &ObjInfo) -> f32 {
    if o.has(objflags::INFANTRY) {
        INFANTRY_HIT_RADIUS
    } else {
        o.major_radius
    }
}

/// Convert nearby objects into hitscan targets. Friendlies are skipped so bullets pass through them
/// (BO2 has no friendly fire in core modes); dead and non-colliding objects are skipped.
pub fn gather_targets(host: &dyn LogicHost, eye: Vec3, range: f32, shooter: u32) -> Vec<Target> {
    let mut objs = Vec::new();
    host.query(eye, range, shooter, &mut objs);
    let mut targets: Vec<Target> = objs
        .iter()
        .filter(|o| o.id != shooter && !o.has(objflags::DEAD) && !o.has(objflags::NO_COLLIDE) && !o.has(objflags::ALLY))
        .map(target_from)
        .collect();
    // Deterministic order regardless of partition iteration order.
    targets.sort_by_key(|t| t.id);
    targets
}

pub fn target_from(o: &ObjInfo) -> Target {
    if o.has(objflags::INFANTRY) {
        return Target {
            id: o.id,
            pos: o.pos,
            shape: Shape::Cylinder { radius: INFANTRY_HIT_RADIUS.min(o.major_radius.max(1.0)), height: o.height.max(6.0) },
            head_fraction: INFANTRY_HEAD_FRACTION,
        };
    }
    let shape = match o.geom_type {
        geom::BOX => Shape::Box { half_x: o.major_radius, half_y: o.minor_radius, height: o.height.max(1.0), yaw: o.yaw },
        geom::SPHERE => Shape::Sphere { radius: o.major_radius },
        _ => Shape::Cylinder { radius: o.major_radius, height: o.height.max(1.0) },
    };
    Target { id: o.id, pos: o.pos, shape, head_fraction: 0.0 }
}

/// Launch velocity for a grenade of throw speed `speed` from `from` to land at `to`: the low arc
/// out of reach, the farthest throw, at 45 degrees).
pub fn throw_velocity(from: Vec3, to: Vec3, speed: f32) -> Vec3 {
    let g = crate::projectiles::GRAVITY;
    let flat = Vec3::new(to.x - from.x, to.y - from.y, 0.0);
    let range = flat.length_xy().max(1.0);
    let rise = to.z - from.z;
    let v2 = speed * speed;
    let disc = v2 * v2 - g * (g * range * range + 2.0 * rise * v2);
    let tan = if disc >= 0.0 { (v2 - disc.sqrt()) / (g * range) } else { 1.0 };
    let angle = tan.atan();
    let along = flat.normalized_or_zero();
    along * (speed * angle.cos()) + Vec3::new(0.0, 0.0, speed * angle.sin())
}

/// BO2's tracer cadence (`FUN_007c65c0`): a per-shooter byte countdown, clamped down to the
/// current tracer's interval (or started from it), decremented per bullet; the bullet that brings
/// it to zero draws a tracer and restarts it. Interval `n` draws the `n`-th, `2n`-th, … bullet;
/// 1 draws every bullet; 0 draws none. The game reads `drawInterval` as a byte.
pub(crate) fn tracer_due(count: &mut u8, interval: u32) -> bool {
    let n = interval as u8;
    if n < *count || *count == 0 {
        *count = n;
    }
    *count = count.wrapping_sub(1);
    if *count == 0 {
        *count = n;
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classmenu::{Draft, Wildcard};
    use crate::sim::tests::{general, FakeHost};

    /// A General (id 1, player 0) in first person with the custom class `draft` builds, ready.
    fn with_class(host: &FakeHost, draft: &Draft) -> Sim {
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(host, 0, 1);
        sim.choose_class(0, 1, 0, &draft.to_custom().to_wire_words());
        let ready = sim.player(0).unwrap().arsenal.raise_left;
        step(&mut sim, host, FpsInput::default(), frames(ready));
        sim
    }

    fn step(sim: &mut Sim, host: &FakeHost, input: FpsInput, n: u32) {
        for _ in 0..n {
            sim.input(0, 1, input);
            let f = sim.frame + 1;
            sim.step(host, f);
        }
    }

    fn frames(seconds: f32) -> u32 {
        (seconds / LOGIC_DT).ceil() as u32 + 1
    }

    /// Tap the lethal button and let the throw finish; the ids of what left the hand.
    fn throw_lethal(sim: &mut Sim, host: &FakeHost) -> Vec<String> {
        let mut thrown = Vec::new();
        for i in 0..frames(2.0) {
            let input = if i == 0 { FpsInput { held: buttons::LETHAL, pressed: buttons::LETHAL, ..Default::default() } } else { FpsInput::default() };
            let before = sim.projectiles.len();
            step(sim, host, input, 1);
            thrown.extend(sim.projectiles.iter().skip(before).map(|(_, p)| p.def.id.clone()));
        }
        thrown
    }

    /// Danger Close with its second box filled: the class carries two lethals (`primarygrenadecount`
    /// 2, `_class.gsc:718-739`),
    /// both thrown with the lethal button, and Scavenger refills to two (`_weapons.gsc:1840-1850`).
    #[test]
    fn danger_close_gives_a_second_lethal() {
        let host = FakeHost::default();
        let draft = Draft { wildcards: vec![Wildcard::DangerClose], second_offhand: [true, false], ..Draft::default() };
        let mut sim = with_class(&host, &draft);
        let a = &sim.player(0).unwrap().arsenal;
        assert_eq!((a.lethal.def.id.as_str(), a.lethal.count), ("frag_grenade_mp", 2));
        assert_eq!(throw_lethal(&mut sim, &host), vec!["frag_grenade_mp".to_string()]);
        assert_eq!(throw_lethal(&mut sim, &host), vec!["frag_grenade_mp".to_string()], "the second lethal");
        assert_eq!(sim.player(0).unwrap().arsenal.lethal.count, 0);
        let a = &mut sim.players[0].as_mut().unwrap().arsenal;
        a.scavenge();
        a.scavenge();
        a.scavenge();
        assert_eq!(a.lethal.count, 2, "scavenged back to the class's two, not past it");
        // Without the card: one.
        let plain = with_class(&FakeHost::default(), &Draft::default());
        assert_eq!(plain.player(0).unwrap().arsenal.lethal.count, 1);
    }

    /// Tactician: the lethal slot holds a tactical (`_class.gsc:704-739` gives whatever the slot
    /// holds), thrown with the lethal button beside the tactical slot's own.
    #[test]
    fn tactician_throws_a_tactical_from_the_lethal_slot() {
        let host = FakeHost::default();
        let draft = Draft { wildcards: vec![Wildcard::Tactician], lethal: "concussion_grenade_mp".into(), ..Draft::default() };
        let mut sim = with_class(&host, &draft);
        let a = &sim.player(0).unwrap().arsenal;
        assert_eq!(a.lethal.def.id, "concussion_grenade_mp");
        // The class's counts (`primarygrenadecount`/`specialgrenadecount`): one box each.
        assert_eq!((a.lethal.count, a.tactical.count), (1, 1), "the class's counts, not the item's start count");
        assert_eq!(a.tactical.def.id, "concussion_grenade_mp");
        assert_eq!(throw_lethal(&mut sim, &host), vec!["concussion_grenade_mp".to_string()]);
    }

    /// A second tactical (BO2's second box, `specialgrenadecount` 2) is given and thrown: the
    /// tactical slot's and, under Tactician, the lethal slot's; each costs a point in the draft.
    #[test]
    fn a_second_tactical_is_given_with_the_class() {
        let host = FakeHost::default();
        let draft = Draft { wildcards: vec![Wildcard::Tactician], lethal: "concussion_grenade_mp".into(), second_offhand: [true, true], ..Draft::default() };
        assert_eq!(draft.spent(), Draft { second_offhand: [false; 2], ..draft.clone() }.spent() + 2);
        let mut sim = with_class(&host, &draft);
        let a = &sim.player(0).unwrap().arsenal;
        assert_eq!((a.lethal.count, a.tactical.count), (2, 2));
        assert_eq!(throw_lethal(&mut sim, &host).len(), 1);
        assert_eq!(sim.player(0).unwrap().arsenal.lethal.count, 1);
    }
}
