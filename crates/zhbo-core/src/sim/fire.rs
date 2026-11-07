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
use crate::math::det;
use crate::penetration;
use crate::damage::MeansOfDeath;

impl Sim {
    pub(super) fn apply_weapon_action(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, action: WeaponAction) {
        match action {
            WeaponAction::Fire { spread_deg, jitter, view_kick, gun_kick, pellet, last, left } => {
                self.fire_round(host, idx, p, FireArgs { spread_deg, jitter, view_kick, gun_kick, pellet, last, left });
            }
            WeaponAction::Launch { def, pellet, spread_deg, jitter } => {
                let eye = p.body.eye();
                // A stack's bolts fan across `stackFireSpread` degrees of yaw: the bolt fired with
                // k rounds loaded of an n-round stack at spread × ((k - 1) / (n - 1) - 0.5)
                // (`FUN_0066e320`; `pellet` is k - 1).
                let n = def.rounds_per_shot();
                let spread = if def.fire == FireMode::Stacked && n > 1 { def.traits.stack_fire.0.to_radians() * (pellet as f32 / (n - 1) as f32 - 0.5) } else { 0.0 };
                // The rocket flies in the shot's spread cone as a bullet does (`FUN_0066e590` passes
                // the spread to `FUN_0066d710`, whose disc `FUN_0066ce90` is `Bullet_Fire`'s), from
                // the muzzle, the eye (`FUN_0066e320` -> `FUN_005e0560`).
                let (yaw, pitch) = (p.fire_yaw + spread, p.fire_pitch);
                let dir = crate::weapons::aim_spread::cone_dir(yaw, pitch, spread_deg, jitter.0, jitter.1);
                if !def.fx_weapon.is_empty() {
                    host.fire_fx(p.general, &def.fx_weapon, 0, eye + dir * 10.0);
                }
                let mut proj = Projectile::launch(def, p.general, eye, dir, def.fuse_time, host, p.yaw);
                // Flak Jacket covers a launcher's own blast (and a bolt's grenade) as a thrown one.
                (proj.self_blast_scale, proj.self_stun_scale) = p.arsenal.self_harm_scales();
                self.projectiles.push((idx, proj));
                // BO2 pushes the one who fires it back along his view, never up (`FUN_0066d710`:
                // `RocketPushSpeed` against the forward, its z clamped at 0), for any fire type but
                // full auto.
                if def.fire != FireMode::Auto {
                    let f = dir_from_angles(yaw, pitch);
                    p.body.vel += Vec3::new(-f.x, -f.y, (-f.z).min(0.0)) * inches(p.arsenal.tuning.rocket_push_speed);
                }
                // One launch cue per pull: the stack's last bolt, everything else's only one.
                if pellet == 0 {
                    self.emit(idx, EventKind::Launched);
                }
            }
            WeaponAction::Throw { def, fuse_left, lethal } => {
                // BO2's throw (`FUN_0066cf00`) from the muzzle, the eye.
                let eye = p.body.eye();
                let vel = grenade_throw(def, p.yaw, p.pitch, p.body.vel);
                let mut proj = Projectile::launch(def, p.general, eye, vel.normalized_or_zero(), fuse_left, host, p.yaw);
                proj.vel = vel;
                (proj.self_blast_scale, proj.self_stun_scale) = p.arsenal.self_harm_scales();
                self.projectiles.push((idx, proj));
                // The slot it left (the client plays that slot's item's throw).
                self.emit(idx, EventKind::Thrown { lethal });
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
            WeaponAction::MeleeSwing => {
                self.emit(idx, EventKind::Melee);
                self.start_melee_charge(host, p);
            }
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
                    if let EquipEvent::Exploded { at, hit_enemy, kill, sound, impact_type, surface, .. } = *e {
                        self.emit(idx, EventKind::Exploded { at, sound, impact_type, surface });
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

/// A bullet striking object `id` at `point`, `distance` from the muzzle, at hit `location`, with
/// BO2's damage multiplier `mult`; `fx`: it plays its impact.
struct Strike {
    id: u32,
    point: Vec3,
    distance: f32,
    location: u8,
    mult: f32,
    fx: bool,
}

/// What a bullet's hit on an object left for its exit: the object, its surface, and the head and
/// lethal bits the exit's blood uses.
struct BulletHit {
    id: u32,
    surface: Surface,
    head: bool,
    kill: bool,
}

impl Sim {
    /// The soldier's hit boxes the bullets trace: the test override, else the converted table's.
    fn hit_body(&self) -> Option<&'static crate::hitloc::Poses> {
        self.hit_body.or_else(|| crate::weapons::catalog().hit_body())
    }

    /// How far along a pick ray (`origin`, unit `dir`, at most `max_t`; world units) it strikes
    /// General `general` (a seat's General, else `None`) standing at `feet` facing `yaw`: his hit
    /// boxes in the pose the sim traces him in, at his drawn size — the boxes the bullets strike and
    /// the native body is drawn over — or, with no converted table, the soldier's cylinder up to
    /// `height`. A click picks the General this way while the native pass draws him in place of
    /// the W3D body (whose hidden mesh Zero Hour's own pick skips). Deterministic: the sim's state
    /// and the logic object's place; it reads nothing and steps nothing.
    #[allow(clippy::too_many_arguments)]
    pub fn pick_general(&self, general: u32, feet: Vec3, yaw: f32, height: f32, origin: Vec3, dir: Vec3, max_t: f32) -> Option<f32> {
        let states = self.hit_states();
        if !states.iter().any(|(g, _)| *g == general) {
            return None;
        }
        let bodies = HitBodies { poses: self.hit_body(), states: &states };
        let o = ObjInfo { id: general, pos: feet, yaw, flags: objflags::INFANTRY, geom_type: crate::host::geom::CYLINDER, major_radius: INFANTRY_HIT_RADIUS, height, ..Default::default() };
        crate::hitscan::ray_vs_target(origin, dir, max_t, &target_from(&o, &bodies, false))
    }

    /// A bullet strikes (`FUN_005cfc60`: the range curve's damage times the multiplier; a
    /// penetrated round's next surface plays its impact only past `bullet_penetrationMinFxDist`).
    fn bullet_hit(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, dir: Vec3, s: Strike) -> BulletHit {
        let Strike { id, point, distance, location, mult, fx } = s;
        let def = p.arsenal.current().def;
        let headshot = crate::hitloc::is_head(location);
        let victim = host.object(id, p.general);
        let infantry = victim.is_some_and(|v| v.has(objflags::INFANTRY));
        // A soldier's damage is multiplied by the weapon's for the location struck (BO2's player
        // damage, `FUN_005f05c0`); anything else takes it unscaled.
        let at = if infantry { def.location_multipliers.get(location as usize).copied().unwrap_or(1.0) } else { 1.0 };
        // `specialty_armorpiercing` (FMJ grants it): a Zero Hour vehicle or structure takes BO2's
        // scorestreak bonus, `damage + damage * level.cac_armorpiercing_data`
        // (`_class.gsc:859`, `killstreaks/_ai_tank.gsc:292-293`).
        let pierce = if !infantry && p.arsenal.has_specialty(Specialty::ArmorPiercing) { 1.0 + p.arsenal.tuning.perk_armorpiercing / 100.0 } else { 1.0 };
        // A pistol's or a rifle's round, a headshot on a soldier (`MOD_HEAD_SHOT`); on anything but
        // a soldier BO2's vehicle rule for a bullet (`level.vehicledamagescalar` alone).
        let how = crate::damage::bullet(def, infantry, location);
        let rule = victim.map_or(1.0, |v| crate::damage::scale_on(&p.arsenal.tuning, &def.id, how, &v));
        let amount = def.damage_at(distance) * mult * at * pierce * rule * BO2_TO_ZH_DAMAGE;
        let kill = host.damage(p.general, id, amount, crate::damage::Blow::new(how, def, dir));
        // BO2's blood is picked by the hit region and its lethality (FxFleshType); every other
        // surface comes from what was hit.
        let surface = Surface::of_object(victim.as_ref());
        if fx {
            self.emit(idx, EventKind::Impact { at: point, dir, surface, head: headshot, fatal: kill, impact_type: def.impact_type, exit: false });
        }
        self.emit(idx, EventKind::HitMarker { headshot, kill });
        BulletHit { id, surface, head: headshot, kill }
    }
}

/// How a melee finds its victim. In first person the General is BO2's player: BO2's melee trace
/// ([`Sim::melee_trace`]). Under RTS control Zero Hour walked him up to the victim with its own
/// footprints (attack range is measured edge to edge, `Weapon::getAttackDistance`), so the blow
/// reaches the knife's range past his own footprint to the victim's, the way he faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeleeReach {
    Body,
    Footprint(f32),
}

/// How much wider than the reach the melee's object query is (each ray's trace then decides);
/// Zero Hour units.
const MELEE_QUERY_SLACK: f32 = 12.0;

/// BO2's melee trace pattern (t6mp `FUN_0066f740`, the table at `0x00b606a8`): each ray's end
/// offset across the view (times `player_meleeWidth`) and up it (times `player_meleeHeight`), traced
/// in this order. With no width and no height BO2 traces the first alone.
const MELEE_PATTERN: [(f32, f32); 9] = [(0.0, 0.0), (0.5, 0.0), (-0.5, 0.0), (0.0, 0.5), (0.0, -0.5), (1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)];

/// What BO2's melee trace struck: a soldier (BO2's player) or another object (a vehicle), a
/// deployed shield (its index and how far along the ray), or nothing it can hurt (the ground, a
/// structure, nothing in reach).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeleeHit {
    Victim(ObjInfo),
    Shield { index: usize, at: Vec3, dir: Vec3 },
    Nothing,
}

/// BO2's melee charge (the knife's lunge): the target and the seconds left of it. The client's
/// auto melee picks the target and its distance ([`Sim::lunge_target`]); a distance past the reach
/// (`player_meleeRange`) makes the melee a charge (`FUN_008c3300`, `FUN_008dc770`: charge flag
/// 0x20000 and a charge distance beyond the reach pick the charge times); the move runs
/// `sqrt(2 x distance / friction)` seconds (`FUN_008c3770`: speed `sqrt(distance x 2 x friction)`,
/// time speed / friction).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeleeCharge {
    pub target: u32,
    pub left: f32,
}

crate::statehash::state_hash!(struct MeleeCharge { target, left });

/// The middle of an object's bounds: where a melee aimed at it points.
fn middle(o: &ObjInfo) -> Vec3 {
    let (lo, hi) = crate::cover::bounds(&crate::hitscan::solid(o));
    (lo + hi) * 0.5
}

impl Sim {
    /// The client's auto melee target (t6mp `FUN_0048e8f0`, `FUN_0048cd20`, `FUN_0048d790`) and his
    /// charge distance (`dist - AutoMeleeChargeOffset`, inches capped at 255, as the command's byte
    /// carries it), in Zero Hour units. A candidate is an enemy within the range (`FUN_0048b390`: a
    /// held melee weapon's `meleeChargeRange`, else `aim_automelee_range`), whose bounds as he sees
    /// them overlap the auto melee region (`aim_automelee_region_width`/`height` of the 640×480
    /// virtual screen, centred) and whom he can see (`FUN_0048fe80`); of those, the one whose
    /// outline's middle is nearest the crosshair (the aim targets' order, `aim_target_closest_first`
    /// 0, `FUN_0048c710`). Past `aim_automelee_move_limit_range` he must be moving at
    /// `aim_automelee_move_limit` of his run speed, within `acos(aim_automelee_move_limit_angle)` of
    /// the way to him (`FUN_0048d790`); else, or with no distance left past the offset, none.
    pub(super) fn lunge_target(&self, host: &dyn LogicHost, p: &PlayerSim) -> Option<(ObjInfo, f32)> {
        let t = &p.arsenal.tuning;
        let held = p.arsenal.current().def;
        let range = if held.traits.weap_type == crate::weapons::WeapType::Melee && held.melee_charge_range > 0.0 { held.melee_charge_range } else { inches(t.aim_automelee_range) };
        let eye = p.body.eye();
        let fwd = dir_from_angles(p.yaw, p.pitch);
        let (_, right) = basis_xy(p.yaw);
        let up = right.cross(fwd);
        // BO2's view: `cg_fov` across a 4:3 screen (`FUN_0048c8f0`: tan(fov / 2) x 0.75 up).
        let tan_x = crate::math::det::tan((t.cg_fov * 0.5).to_radians());
        let tan_y = tan_x * 0.75;
        let (hx, hy) = (t.aim_automelee_region_width / 640.0, t.aim_automelee_region_height / 480.0);
        let mut near = Vec::new();
        host.query(eye, range + MELEE_QUERY_SLACK, p.general, &mut near);
        let cover = crate::cover::Blockers::near(host, eye, range + MELEE_QUERY_SLACK, p.general, &[]);
        let mut best: Option<(f32, u32, ObjInfo, f32)> = None;
        for o in near.iter().filter(|o| o.id != p.general && !o.has(objflags::DEAD) && !o.has(objflags::ALLY) && !o.has(objflags::NO_COLLIDE) && !o.has(objflags::STRUCTURE)) {
            if self.deployed.iter().any(|d| d.object != 0 && d.object == o.id) {
                continue;
            }
            let dist = (o.pos - p.body.pos).length();
            if dist > range {
                continue;
            }
            // Its bounds' corners on the screen (`FUN_0048c2e0`): clip space of those in front.
            let (lo, hi) = crate::cover::bounds(&crate::hitscan::solid(o));
            let (mut min, mut max) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
            for k in 0..8 {
                let c = Vec3::new(if k & 1 == 0 { lo.x } else { hi.x }, if k & 2 == 0 { lo.y } else { hi.y }, if k & 4 == 0 { lo.z } else { hi.z });
                let v = c - eye;
                let depth = v.dot(fwd);
                if depth <= 0.0 {
                    continue;
                }
                let (x, y) = (v.dot(right) / depth / tan_x, v.dot(up) / depth / tan_y);
                min = (min.0.min(x), min.1.min(y));
                max = (max.0.max(x), max.1.max(y));
            }
            if !(min.0 < max.0 && min.1 < max.1 && min.0 <= 1.0 && min.1 <= 1.0 && max.0 >= -1.0 && max.1 >= -1.0) {
                continue;
            }
            let (min, max) = ((min.0.clamp(-1.0, 1.0), min.1.clamp(-1.0, 1.0)), (max.0.clamp(-1.0, 1.0), max.1.clamp(-1.0, 1.0)));
            if hx < min.0 || max.0 < -hx || hy < min.1 || max.1 < -hy {
                continue;
            }
            if !cover.clear(host, eye, middle(o), o.id) {
                continue;
            }
            let (cx, cy) = ((min.0 + max.0) * 0.5, (min.1 + max.1) * 0.5);
            let screen = cx * cx + cy * cy;
            if best.is_none_or(|(s, id, _, _)| screen < s || (screen == s && o.id < id)) {
                best = Some((screen, o.id, *o, dist));
            }
        }
        let (_, _, target, dist) = best?;
        // The move test (`FUN_0048d790`).
        let to = Vec3::new(target.pos.x - p.body.pos.x, target.pos.y - p.body.pos.y, 0.0);
        if dist > inches(t.aim_automelee_move_limit_range) {
            let vel = Vec3::new(p.body.vel.x, p.body.vel.y, 0.0);
            let speed = vel.length_xy();
            if speed < t.aim_automelee_move_limit * self.tuning.run_speed(p.arsenal.move_faster()) {
                return None;
            }
            if speed > 1e-3 && to.length_xy() > 1e-3 && vel.dot(to) / (speed * to.length_xy()) < t.aim_automelee_move_limit_angle {
                return None;
            }
        }
        let charge = (dist - inches(t.automelee_charge_offset)).min(inches(255.0));
        (charge > 0.0).then_some((target, charge))
    }

    /// On the melee press: the auto melee's target, when his charge distance is past the reach
    /// (`player_meleeRange`), makes this a charge melee (its charge time and delay) and starts the
    /// lunge (`FUN_008c3300` takes the command's target, `FUN_008dc770` charges).
    pub(super) fn start_melee_charge(&mut self, host: &dyn LogicHost, p: &mut PlayerSim) {
        p.charge = None;
        let Some((target, gap)) = self.lunge_target(host, p) else { return };
        if gap <= MELEE_RANGE {
            return; // in reach: the plain melee
        }
        let friction = crate::units::inches(p.arsenal.tuning.player_melee_charge_friction).max(1.0);
        p.arsenal.charge_melee();
        p.charge = Some(MeleeCharge { target: target.id, left: (2.0 * gap / friction).sqrt() });
    }

    /// This frame's lunge for the movement (`None`: none, or it ended — its time ran out, or the
    /// target died or went out of sight). It stops [`Tuning::automelee_charge_offset`] short of him
    /// (the distance the command carried is the target's less that).
    pub(super) fn melee_charge(&self, host: &dyn LogicHost, p: &mut PlayerSim) -> Option<crate::movement::Charge> {
        let c = p.charge.as_mut()?;
        c.left -= LOGIC_DT;
        let target = host.object(c.target, p.general).filter(|o| !o.has(objflags::DEAD));
        let (Some(o), true) = (target, c.left > -1e-6) else {
            p.charge = None;
            return None;
        };
        let t = &p.arsenal.tuning;
        Some(crate::movement::Charge {
            target: o.pos,
            stop: inches(t.automelee_charge_offset),
            max_speed: crate::units::inches(t.player_melee_charge_max_speed),
            friction: crate::units::inches(t.player_melee_charge_friction),
        })
    }

    /// BO2's melee trace (t6mp `FUN_00670330` -> `FUN_00670040` -> `FUN_0066fc30` ->
    /// `FUN_0066f740`): nine rays from `MeleeTracePullback` behind the eye to `player_meleeRange`
    /// along `fwd`, their ends spread by [`MELEE_PATTERN`] times `player_meleeWidth` across and
    /// `player_meleeHeight` up. The first ray to strike a soldier (BO2's player) strikes him;
    /// otherwise the nearest thing any ray struck: another object (a vehicle) is struck, a deployed
    /// shield takes the blow, the ground or a structure (BO2's world) ends it. Each ray stops at
    /// the first solid on it: enemies (allies pass, as bullets do), structures, deployed shields and
    /// the ground.
    pub(super) fn melee_trace(&self, host: &dyn LogicHost, p: &PlayerSim, fwd: Vec3) -> MeleeHit {
        let t = &p.arsenal.tuning;
        let eye = p.body.eye();
        let yaw = crate::math::det::atan2(fwd.y, fwd.x);
        let (_, right) = basis_xy(yaw);
        let up = right.cross(fwd);
        let (range, w, h) = (inches(t.player_melee_range), inches(t.player_melee_width), inches(t.player_melee_height));
        let start = eye - fwd * inches(t.melee_trace_pullback);
        let mut near = Vec::new();
        host.query(eye, range + w + h + MELEE_QUERY_SLACK, p.general, &mut near);
        near.sort_by_key(|o| o.id);
        let solids: Vec<(ObjInfo, Target)> = near
            .into_iter()
            // A structure stands in the way whoever owns it.
            .filter(|o| o.id != p.general && !o.has(objflags::DEAD) && !o.has(objflags::NO_COLLIDE) && (!o.has(objflags::ALLY) || o.has(objflags::STRUCTURE)))
            .filter(|o| !self.deployed.iter().any(|d| d.object != 0 && d.object == o.id))
            .map(|o| (o, crate::hitscan::solid(&o)))
            .collect();
        let ground = |x: f32, y: f32| host.ground_height(x, y);
        let rays = if w > 0.0 || h > 0.0 { MELEE_PATTERN.len() } else { 1 };
        // The nearest non-soldier strike so far: how far along its ray, and what.
        let mut best: Option<(f32, MeleeHit)> = None;
        for &(a, b) in &MELEE_PATTERN[..rays] {
            let end = eye + fwd * range + right * (w * a) + up * (h * b);
            let seg = end - start;
            let len = seg.length();
            if len < 1e-4 {
                continue;
            }
            let dir = seg * (1.0 / len);
            let mut first: Option<(f32, MeleeHit)> = crate::hitscan::trace_terrain(start, dir, len, &ground).map(|t| (t, MeleeHit::Nothing));
            for (o, solid) in &solids {
                if let Some(d) = crate::hitscan::ray_vs_target(start, dir, len, solid) {
                    if first.is_none_or(|(f, _)| d < f) {
                        let hit = if o.has(objflags::STRUCTURE) { MeleeHit::Nothing } else { MeleeHit::Victim(*o) };
                        first = Some((d, hit));
                    }
                }
            }
            if let Some((i, d)) = self.deployed_on_ray(start, dir, len) {
                if first.is_none_or(|(f, _)| d < f) {
                    first = Some((d, MeleeHit::Shield { index: i, at: start + dir * d, dir }));
                }
            }
            let Some((d, hit)) = first else { continue };
            if let MeleeHit::Victim(o) = hit {
                if o.has(objflags::INFANTRY) {
                    return hit;
                }
            }
            if best.is_none_or(|(b, _)| d <= b) {
                best = Some((d, hit));
            }
        }
        best.map_or(MeleeHit::Nothing, |(_, h)| h)
    }

    /// The RTS strike's victim: the nearest enemy whose Zero Hour footprint the blow reaches
    /// along his facing, the knife's range past his own footprint (`own`), as Zero Hour measures
    /// attack range edge to edge.
    fn footprint_victim(&self, host: &dyn LogicHost, p: &PlayerSim, own: f32) -> MeleeHit {
        let (fwd, _) = basis_xy(p.yaw);
        let reach = own + MELEE_RANGE;
        let mut near = Vec::new();
        host.query(p.body.pos, reach + MELEE_QUERY_SLACK, p.general, &mut near);
        near.sort_by_key(|o| o.id);
        let mut best: Option<(f32, ObjInfo)> = None;
        for o in near.iter().filter(|o| o.id != p.general && !o.has(objflags::DEAD) && !o.has(objflags::ALLY) && !o.has(objflags::NO_COLLIDE) && !o.has(objflags::STRUCTURE)) {
            if self.deployed.iter().any(|d| d.object != 0 && d.object == o.id) {
                continue;
            }
            let footprint = if o.has(objflags::INFANTRY) {
                Target { id: o.id, pos: o.pos, shape: Shape::Cylinder { radius: o.major_radius, height: o.height.max(1.0) }, head_fraction: 0.0 }
            } else {
                crate::hitscan::solid(o)
            };
            let from = Vec3::new(p.body.pos.x, p.body.pos.y, o.pos.z + o.height.max(1.0) * 0.5);
            if let Some(d) = crate::hitscan::ray_vs_target(from, fwd, reach, &footprint) {
                if best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, *o));
                }
            }
        }
        best.map_or(MeleeHit::Nothing, |(_, o)| MeleeHit::Victim(o))
    }
}

impl Sim {
    /// BO2's knife: what its trace strikes takes the hit ([`Sim::melee_trace`]; under RTS control
    /// [`MeleeReach::Footprint`]). A charge melee's view has converged on the enemy it charges at
    /// (the client's auto melee turns the view onto the locked target, `FUN_0048e8f0`), so its
    /// trace runs at him. The swing's event (`EventKind::Melee`, the swipe sound) is the caller's:
    /// first person emits it on the press ([`WeaponAction::MeleeSwing`]), the RTS strike with the
    /// blow.
    pub(super) fn melee_strike(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, damage: f32, reach: MeleeReach) {
        let eye = p.body.eye();
        let charged = p.charge.and_then(|c| host.object(c.target, p.general)).filter(|o| !o.has(objflags::DEAD));
        let fwd = match charged {
            Some(o) => (middle(&o) - eye).normalized_or_zero(),
            None => dir_from_angles(p.yaw, p.pitch),
        };
        let hit = match reach {
            MeleeReach::Body => self.melee_trace(host, p, fwd),
            MeleeReach::Footprint(own) => self.footprint_victim(host, p, own),
        };
        let v = match hit {
            MeleeHit::Nothing => return,
            // A deployed shield takes the strike (melee-scaled damage).
            MeleeHit::Shield { index, at, dir } => {
                self.damage_deployed(host, index, Some(idx), p.general, damage, deploy::ShieldDamage::Melee);
                self.emit(idx, EventKind::ShieldHit { at, dir, knife: true, impact_type: 0 });
                return;
            }
            MeleeHit::Victim(v) => v,
        };
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
        self.emit(idx, EventKind::MeleeHit);
        let melee = p.arsenal.melee_weapon();
        let rule = crate::damage::scale_on(&p.arsenal.tuning, &melee.id, MeansOfDeath::Melee, &v);
        // The blow travels the way he faces (BO2's melee passes his forward as `vDir`).
        let kill = host.damage(p.general, v.id, damage * rule * BO2_TO_ZH_DAMAGE, crate::damage::Blow::new(MeansOfDeath::Melee, melee, fwd));
        self.emit(idx, EventKind::HitMarker { headshot: false, kill });
    }
}

/// How a trace poses soldiers: the converted hit boxes per body state, and the body state of each
/// General the sim moves (`(object, state)`); every other soldier stands.
#[derive(Clone, Copy, Debug, Default)]
pub struct HitBodies<'a> {
    pub poses: Option<&'static crate::hitloc::Poses>,
    pub states: &'a [(u32, u16)],
}

impl HitBodies<'_> {
    /// The boxes and their scale for soldier `id`: a General in his body state's pose at his
    /// drawn size ([`crate::units::GENERAL_SCALE`]), Zero Hour's infantry standing at BO2's.
    fn body(&self, id: u32) -> Option<(&'static crate::hitloc::Body, f32)> {
        let poses = self.poses?;
        match self.states.iter().find(|(g, _)| *g == id) {
            Some(&(_, s)) => poses.get(s).map(|b| (b, crate::units::GENERAL_SCALE)),
            None => poses.stand().map(|b| (b, 1.0)),
        }
    }
}

/// Convert nearby objects into hitscan targets. Friendlies are skipped so bullets pass through them
/// (BO2 has no friendly fire in core modes); dead and non-colliding objects are skipped. Infantry
/// are traced as BO2's hit boxes (`bodies`) when there are any, ranked as a `rifle` bullet ranks
/// them.
pub fn gather_targets(host: &dyn LogicHost, eye: Vec3, range: f32, shooter: u32, bodies: &HitBodies, rifle: bool) -> Vec<Target> {
    let mut objs = Vec::new();
    host.query(eye, range, shooter, &mut objs);
    targets_of(&objs, shooter, bodies, rifle)
}

/// [`gather_targets`] over objects already queried (and maybe rewound, [`antilag::rewind`]).
fn targets_of(objs: &[ObjInfo], shooter: u32, bodies: &HitBodies, rifle: bool) -> Vec<Target> {
    let mut targets: Vec<Target> = objs
        .iter()
        .filter(|o| o.id != shooter && !o.has(objflags::DEAD) && !o.has(objflags::NO_COLLIDE) && !o.has(objflags::ALLY))
        .map(|o| target_from(o, bodies, rifle))
        .collect();
    // Deterministic order regardless of partition iteration order.
    targets.sort_by_key(|t| t.id);
    targets
}

pub fn target_from(o: &ObjInfo, bodies: &HitBodies, rifle: bool) -> Target {
    if let Some((body, scale)) = o.has(objflags::INFANTRY).then(|| bodies.body(o.id)).flatten() {
        return Target { id: o.id, pos: o.pos, shape: Shape::Body { body, yaw: o.yaw, scale, rifle }, head_fraction: 0.0 };
    }
    crate::hitscan::solid(o)
}

/// BO2's grenade throw velocity (t6mp `FUN_0066cf00`) for a view `yaw`/`pitch` and the thrower's
/// velocity `mover`: `forward × iProjectileSpeed + up × iProjectileSpeedRelativeUp + z ×
/// iProjectileSpeedUp`, plus the view's horizontal forward × `iProjectileSpeedForward` when it has
/// one; then the thrower's velocity along that throw is added (`vel += n × (mover · n)`, `n` the
/// throw's direction), nothing across it.
pub fn grenade_throw(def: &crate::weapons::WeaponDef, yaw: f32, pitch: f32, mover: Vec3) -> Vec3 {
    let fwd = dir_from_angles(yaw, pitch);
    let (_, right) = basis_xy(yaw);
    let up = right.cross(fwd);
    let mut vel = fwd * def.projectile_speed + up * def.projectile_speed_relative_up + Vec3::new(0.0, 0.0, def.projectile_speed_up);
    if def.projectile_speed_forward != 0.0 {
        vel += Vec3::new(fwd.x, fwd.y, 0.0).normalized_or_zero() * def.projectile_speed_forward;
    }
    let n = vel.normalized_or_zero();
    vel + n * mover.dot(n)
}

/// Launch velocity for a grenade of throw speed `speed` from `from` to land at `to`: the low arc
/// out of reach, the farthest throw, at 45 degrees).
pub fn throw_velocity(from: Vec3, to: Vec3, speed: f32) -> Vec3 {
    let g = crate::projectiles::gravity();
    let flat = Vec3::new(to.x - from.x, to.y - from.y, 0.0);
    let range = flat.length_xy().max(1.0);
    let rise = to.z - from.z;
    let v2 = speed * speed;
    let disc = v2 * v2 - g * (g * range * range + 2.0 * rise * v2);
    let tan = if disc >= 0.0 { (v2 - disc.sqrt()) / (g * range) } else { 1.0 };
    let angle = det::atan(tan);
    let along = flat.normalized_or_zero();
    along * (speed * det::cos(angle)) + Vec3::new(0.0, 0.0, speed * det::sin(angle))
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


/// One bullet of a [`WeaponAction::Fire`] (its fields, minus the weapon's own state).
pub(super) struct FireArgs {
    pub spread_deg: f32,
    pub jitter: (f32, f32),
    pub view_kick: (f32, f32, f32),
    pub gun_kick: (f32, f32),
    pub pellet: u16,
    pub last: bool,
    pub left: bool,
}

/// What every way a round can end shares: its flash and tracer, and the shot/recoil cues the
/// first pellet of a shot carries.
struct Round<'a> {
    shooter: u32,
    def: &'static crate::weapons::WeaponDef,
    fx: Option<&'a str>,
    tracer: bool,
    pellet: u16,
    left: bool,
    gun_kick: (f32, f32),
    eye: Vec3,
    spread: f32,
}

impl Sim {
    /// A bullet: aim it in the spread cone, trace it, and land it on equipment, a deployed or held
    /// Assault Shield, a body (then on through BO2's penetration walk) or the ground.
    fn fire_round(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, a: FireArgs) {
        let FireArgs { spread_deg, jitter, view_kick, gun_kick, pellet, last, left } = a;
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
        // The pellet's place in the cone (`Bullet_Fire`: the angle, then a linear radius, on the
        // view's tangent plane).
        let dir = crate::weapons::aim_spread::cone_dir(p.fire_yaw, p.fire_pitch, spread_deg, jitter.0, jitter.1);

        let range = def.bullet_range(self.bullet_range);
        // The players as the shooter saw them (BO2's antilag rewind, `sim::antilag`).
        let then = self.rewind_frame(p).cloned();
        let mut states = self.hit_states();
        let mut objs = Vec::new();
        host.query(eye, range, p.general, &mut objs);
        if let Some(then) = &then {
            antilag::rewind(&mut objs, &mut states, then);
        }
        let bodies = HitBodies { poses: self.hit_body(), states: &states };
        let mut targets = targets_of(&objs, p.general, &bodies, def.traits.rifle_bullet);
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
        let round = Round { shooter: p.general, def, fx, tracer, pellet, left, gun_kick, eye, spread: spread_deg };
        // Enemy equipment in the way takes the shot.
        let reach = match hit {
            HitResult::Object { distance, .. } => distance,
            other => (other.point() - eye).length(),
        };
        if let Some((i, t)) = self.equipment.shot(host, p.general, eye, dir, reach) {
            let at = eye + dir * t;
            self.round_effects(host, &round, 0, at);
            // A round breaks it as BO2's script says (`equipment::breaks`): C4, a Claymore or a
            // Shock Charge goes off as the shooter's.
            let mut ev = Vec::new();
            self.equipment.destroy(host, i, Some((idx, p.general)), crate::weapons::Effect::None, &mut ev);
            self.equipment_events(host, ev);
            // BO2's equipment is metal: a spark at the round, never blood.
            self.emit(idx, EventKind::Impact { at, dir, surface: Surface::Metal, head: false, fatal: false, impact_type: def.impact_type, exit: false });
            self.round_done(idx, &round, at);
            return;
        }
        // A deployed shield on the ray takes the round (bullet-scaled damage).
        if let Some((i, t)) = self.deployed_on_ray(eye, dir, reach) {
            let at = eye + dir * t;
            self.round_effects(host, &round, 0, at);
            self.damage_deployed(host, i, Some(idx), p.general, def.damage_at(t), deploy::ShieldDamage::Bullet);
            self.emit(idx, EventKind::ShieldHit { at, dir, knife: false, impact_type: def.impact_type });
            self.round_done(idx, &round, at);
            return;
        }
        // An enemy's Assault Shield on the ray: location `riotshield`, which outranks every
        // body box (rank 101) and takes no damage (`_globallogic_player.gsc:840-888`); the
        // body it is tested against is the hit-location box the trace struck.
        let hit_id = if let HitResult::Object { id, .. } = hit { Some(id) } else { None };
        let enemy = |b: &crate::shield::Bearer| b.general != p.general && targets.iter().any(|t| t.id == b.general);
        let shields = then.as_ref().map_or(&self.shields, |f| &f.shields);
        if let Some((_, t)) = crate::shield::blocking(shields, &enemy, eye, dir, reach, range, hit_id) {
            let at = eye + dir * t;
            self.round_effects(host, &round, 0, at);
            self.emit(idx, EventKind::ShieldHit { at, dir, knife: false, impact_type: def.impact_type });
            self.round_done(idx, &round, at);
            return;
        }
        match hit {
            HitResult::Object { id, point, distance, location } => {
                if let Some(seen) = then.as_ref().filter(|f| f.pose(id).is_some()).map(|f| f.frame) {
                    self.antilag_hits.push((p.general, id, seen));
                }
                self.round_effects(host, &round, id, point);
                let general = p.general;
                let last = self.bullet_hit(host, idx, p, dir, Strike { id, point, distance, location, mult: 1.0, fx: true });
                // BO2's penetration loop (`FUN_005d0350`): on through what it hit, the
                // multiplier falling with each solid's thickness over its depth.
                let surface_of = |id: u32| Surface::of_object(host.object(id, general).as_ref());
                let shot = penetration::Shot {
                    pen: def.traits.penetrate,
                    perk: p.arsenal.has_specialty(Specialty::BulletPenetration),
                    origin: eye,
                    dir,
                    range,
                    targets: &targets,
                    ignore: general,
                    ground: &ground,
                    surface_of: &surface_of,
                };
                let mut steps = Vec::new();
                penetration::walk(&self.penetration, &shot, (id, distance, last.surface), &mut steps);
                self.penetrated(host, idx, p, dir, steps, last);
            }
            other => {
                self.round_effects(host, &round, 0, other.point());
                // The ground: Zero Hour's terrain class at the point (the host's query).
                // A shot that flew out to its full range hit nothing, so it raises nothing.
                if matches!(other, HitResult::Terrain { .. }) {
                    let at = other.point();
                    let surface = host.surface_at(at.x, at.y);
                    self.emit(idx, EventKind::Impact { at, dir, surface, head: false, fatal: false, impact_type: def.impact_type, exit: false });
                }
            }
        }
        self.round_done(idx, &round, hit.point());
    }

    /// Land the steps of BO2's penetration walk past the first hit (`last`): each solid entered
    /// takes the round at its falling multiplier, and each exit plays its effect.
    fn penetrated(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, dir: Vec3, steps: Vec<penetration::Step>, mut last: BulletHit) {
        let def = p.arsenal.current().def;
        for step in steps {
            match step {
                // The exit plays the type's exit row and family (BO2's flag 4), with the hit's own
                // head/fatal bits for blood.
                penetration::Step::Exit { id, at, surface, fx: true } => {
                    let (head, fatal) = if id == last.id { (last.head, last.kill) } else { (false, false) };
                    self.emit(idx, EventKind::Impact { at, dir, surface, head, fatal, impact_type: def.impact_type, exit: true });
                }
                penetration::Step::Exit { .. } => {}
                penetration::Step::Enter { hit: HitResult::Object { id, point, distance, location }, mult, fx } => {
                    last = self.bullet_hit(host, idx, p, dir, Strike { id, point, distance, location, mult, fx });
                }
                penetration::Step::Enter { hit, fx, .. } => {
                    if fx && matches!(hit, HitResult::Terrain { .. }) {
                        let at = hit.point();
                        let surface = host.surface_at(at.x, at.y);
                        self.emit(idx, EventKind::Impact { at, dir, surface, head: false, fatal: false, impact_type: def.impact_type, exit: false });
                    }
                }
            }
        }
    }

    /// The round's flash at `at` (on `target`, 0 for none) and its tracer, when it has them.
    fn round_effects(&mut self, host: &dyn LogicHost, r: &Round, target: u32, at: Vec3) {
        if let Some(fx) = r.fx {
            host.fire_fx(r.shooter, fx, target, at);
        }
        if r.tracer {
            self.tracers.push(ShotTracer { shooter: r.shooter, def: r.def, target, end: at, left: r.left });
        }
    }

    /// The shot and recoil cues, once per shot (the first pellet), ending at `end`.
    fn round_done(&mut self, idx: usize, r: &Round, end: Vec3) {
        if r.pellet == 0 {
            self.emit(idx, EventKind::Shot { origin: r.eye, end, left: r.left, spread: r.spread });
            self.emit(idx, EventKind::Recoil { gun_kick: r.gun_kick });
        }
    }
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

    /// The thrown event names the slot the item left, whatever its class: the C4 (BO2's
    /// `weapClass` equipment, in the lethal slot) is a lethal throw, so the client plays the lethal
    /// slot's throw sound for it (it played the tactical's: the slot was read off the class).
    #[test]
    fn a_thrown_c4_is_the_lethal_slots_throw() {
        let host = FakeHost::default();
        let draft = Draft { lethal: "satchel_charge_mp".into(), ..Draft::test_kit() };
        let mut sim = with_class(&host, &draft);
        assert_eq!(sim.player(0).unwrap().arsenal.lethal.def.class, crate::weapons::Class::Equipment);
        sim.events.clear();
        assert_eq!(throw_lethal(&mut sim, &host), vec!["satchel_charge_mp".to_string()]);
        let thrown: Vec<bool> = sim.events.iter().filter_map(|e| if let EventKind::Thrown { lethal } = e.kind { Some(lethal) } else { None }).collect();
        assert_eq!(thrown, [true]);
    }

    /// Danger Close with its second box filled: the class carries two lethals (`primarygrenadecount`
    /// 2, `_class.gsc:718-739`),
    /// both thrown with the lethal button, and Scavenger refills to two (`_weapons.gsc:1840-1850`).
    #[test]
    fn danger_close_gives_a_second_lethal() {
        let host = FakeHost::default();
        let draft = Draft { wildcards: vec![Wildcard::DangerClose], second_offhand: [true, false], ..Draft::test_kit() };
        let mut sim = with_class(&host, &draft);
        let a = &sim.player(0).unwrap().arsenal;
        assert_eq!((a.lethal.def.id.as_str(), a.lethal.count), ("frag_grenade_mp", 2));
        assert_eq!(throw_lethal(&mut sim, &host), vec!["frag_grenade_mp".to_string()]);
        assert_eq!(throw_lethal(&mut sim, &host), vec!["frag_grenade_mp".to_string()], "the second lethal");
        assert_eq!(sim.player(0).unwrap().arsenal.lethal.count, 0);
        let a = &mut sim.players[0].as_mut().unwrap().arsenal;
        a.scavenge(false);
        a.scavenge(false);
        a.scavenge(false);
        assert_eq!(a.lethal.count, 2, "scavenged back to the class's two, not past it");
        // Without the card: one.
        let plain = with_class(&FakeHost::default(), &Draft::test_kit());
        assert_eq!(plain.player(0).unwrap().arsenal.lethal.count, 1);
    }

    /// Tactician: the lethal slot holds a tactical (`_class.gsc:704-739` gives whatever the slot
    /// holds), thrown with the lethal button beside the tactical slot's own.
    #[test]
    fn tactician_throws_a_tactical_from_the_lethal_slot() {
        let host = FakeHost::default();
        let draft = Draft { wildcards: vec![Wildcard::Tactician], lethal: "concussion_grenade_mp".into(), ..Draft::test_kit() };
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
        let draft = Draft { wildcards: vec![Wildcard::Tactician], lethal: "concussion_grenade_mp".into(), second_offhand: [true, true], ..Draft::test_kit() };
        assert_eq!(draft.spent(), Draft { second_offhand: [false; 2], ..draft.clone() }.spent() + 2);
        let mut sim = with_class(&host, &draft);
        let a = &sim.player(0).unwrap().arsenal;
        assert_eq!((a.lethal.count, a.tactical.count), (2, 2));
        assert_eq!(throw_lethal(&mut sim, &host).len(), 1);
        assert_eq!(sim.player(0).unwrap().arsenal.lethal.count, 1);
    }

    /// Player 0's General 1 at the origin, in first person with the test kit, facing +x.
    fn kitted(host: &FakeHost) -> Sim {
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(host, 0, 1);
        let p = sim.players[0].as_mut().unwrap();
        p.arsenal = crate::weapons::Arsenal::with_loadout(crate::weapons::Loadout::test_kit());
        sim
    }

    fn act(sim: &mut Sim, host: &FakeHost, action: WeaponAction) {
        let mut p = sim.players[0].take().unwrap();
        sim.apply_weapon_action(host, 0, &mut p, action);
        sim.players[0] = Some(p);
    }

    /// BO2's throw (`FUN_0066cf00`): the plain view at `iProjectileSpeed`, the view's up at
    /// `iProjectileSpeedRelativeUp`, world up at `iProjectileSpeedUp`, the flat forward at
    /// `iProjectileSpeedForward`; then only the thrower's velocity along the throw. No lift above
    /// the crosshair, no half of his velocity across it.
    #[test]
    fn a_throw_is_bo2s() {
        let def = crate::weapons::WeaponDef { projectile_speed: 600.0, projectile_speed_up: 30.0, projectile_speed_relative_up: 50.0, projectile_speed_forward: 40.0, ..Default::default() };
        let v = grenade_throw(&def, 0.0, 0.0, Vec3::ZERO);
        assert!((v - Vec3::new(640.0, 0.0, 80.0)).length() < 1e-3, "{v:?}");
        let (yaw, pitch) = (0.7f32, 0.4f32);
        let f = dir_from_angles(yaw, pitch);
        let up = basis_xy(yaw).1.cross(f);
        let flat = Vec3::new(f.x, f.y, 0.0).normalized_or_zero();
        let want = f * 600.0 + up * 50.0 + Vec3::new(0.0, 0.0, 30.0) + flat * 40.0;
        assert!((grenade_throw(&def, yaw, pitch, Vec3::ZERO) - want).length() < 1e-3);
        // Moving across the throw adds nothing; along it, all of it.
        let n = want.normalized_or_zero();
        let across = Vec3::new(-n.y, n.x, 0.0).normalized_or_zero() * 100.0;
        assert!((grenade_throw(&def, yaw, pitch, across) - want).length() < 1e-2);
        assert!((grenade_throw(&def, yaw, pitch, n * 100.0) - (want + n * 100.0)).length() < 1e-2);
    }

    /// Every thrown offhand of the roster leaves from the eye at BO2's velocity.
    #[test]
    fn every_throw_of_the_roster_leaves_the_eye_at_bo2s_velocity() {
        for def in crate::weapons::catalog().all().iter().copied().filter(|d| matches!(d.traits.weap_type, crate::weapons::WeapType::Grenade | crate::weapons::WeapType::Mine)) {
            let host = FakeHost::default();
            let mut sim = kitted(&host);
            let p = sim.players[0].as_mut().unwrap();
            (p.yaw, p.pitch, p.body.vel) = (0.3, -0.2, Vec3::new(20.0, 5.0, 0.0));
            let (eye, want) = (p.body.eye(), grenade_throw(def, 0.3, -0.2, p.body.vel));
            act(&mut sim, &host, WeaponAction::Throw { def, fuse_left: def.fuse_time, lethal: true });
            let (_, g) = sim.projectiles.last().unwrap();
            assert!((g.pos - eye).length() < 1e-4 && (g.vel - want).length() < 1e-3, "{}: {:?} {:?}", def.id, g.pos, g.vel);
        }
    }

    /// A rocket flies in the shot's spread cone (`FUN_0066d710`, `Bullet_Fire`'s disc) and pushes
    /// the shooter back along his view, never up, `RocketPushSpeed` (none for full auto); every
    /// launcher of the roster.
    #[test]
    fn a_rocket_spreads_and_pushes_its_shooter_back() {
        let t = crate::tuning::get();
        let launchers: Vec<_> = crate::weapons::catalog().all().iter().copied().filter(|d| d.launches()).collect();
        assert!(!launchers.is_empty());
        for def in launchers {
            let host = FakeHost::default();
            let mut sim = kitted(&host);
            let p = sim.players[0].as_mut().unwrap();
            (p.fire_yaw, p.fire_pitch, p.body.vel) = (0.0, 0.3, Vec3::ZERO);
            act(&mut sim, &host, WeaponAction::Launch { def, pellet: 0, spread_deg: 4.0, jitter: (0.25, 1.0) });
            let (_, g) = sim.projectiles.last().unwrap();
            let dir = g.vel.normalized_or_zero();
            let view = dir_from_angles(0.0, 0.3);
            if def.fire == FireMode::Stacked && def.rounds_per_shot() > 1 {
                continue; // its bolt fans by the stack too
            }
            let off = dir.dot(view).clamp(-1.0, 1.0).acos().to_degrees();
            if def.projectile_speed_up == 0.0 {
                assert!((off - 4.0).abs() < 0.05, "{}: {off} degrees off the view", def.id);
            }
            let vel = sim.player(0).unwrap().body.vel;
            if def.fire == FireMode::Auto {
                assert_eq!(vel, Vec3::ZERO, "{}: full auto has no push", def.id);
            } else {
                // Aimed up, the push's z is down (`-forward.z`, clamped at 0 from above).
                let push = Vec3::new(-view.x, -view.y, (-view.z).min(0.0)) * inches(t.rocket_push_speed);
                assert!((vel - push).length() < 1e-4, "{}: {vel:?} vs {push:?}", def.id);
                assert!(vel.z <= 0.0, "never up");
            }
        }
    }

    /// BO2's melee trace: straight ahead in reach he is struck; 40 degrees off the view he is not
    /// (the old 53-degree cone struck him); looking up over him misses; a wall between stops it.
    #[test]
    fn the_knife_traces_bo2s_rays() {
        let inch = inches(1.0);
        let strike = |at: Vec3, yaw: f32, pitch: f32, wall: bool| {
            let host = FakeHost::default();
            let mut sim = kitted(&host);
            host.add(ObjInfo { pos: at, ..general(2, 1) });
            if wall {
                host.add(ObjInfo { id: 9, flags: objflags::STRUCTURE, pos: Vec3::new(at.x * 0.5, at.y * 0.5, 0.0), geom_type: crate::host::geom::BOX, major_radius: 0.3, minor_radius: 20.0, height: 30.0, yaw, ..Default::default() });
            }
            let p = sim.players[0].as_mut().unwrap();
            (p.yaw, p.pitch) = (yaw, pitch);
            let p = sim.player(0).unwrap().clone();
            matches!(sim.melee_trace(&host, &p, dir_from_angles(yaw, pitch)), MeleeHit::Victim(o) if o.id == 2)
        };
        let ahead = Vec3::new(45.0 * inch, 0.0, 0.0);
        assert!(strike(ahead, 0.0, 0.0, false), "in reach, ahead");
        let a = 40f32.to_radians();
        assert!(!strike(Vec3::new(a.cos(), a.sin(), 0.0) * (45.0 * inch), 0.0, 0.0, false), "40 degrees off the view");
        assert!(!strike(ahead, 0.0, 1.2, false), "looking up over him");
        assert!(!strike(ahead, 0.0, 0.0, true), "a wall between");
        assert!(!strike(Vec3::new(75.0 * inch, 0.0, 0.0), 0.0, 0.0, false), "past the reach");
    }

    /// Explosion cover (`FUN_005f2130` x `FUN_005f0f00`): a structure between the blast and a
    /// General spares him every explosive of the roster's blast, and a concussion's stun; in the
    /// open he takes it.
    #[test]
    fn a_wall_covers_a_general_from_every_blast() {
        for def in crate::weapons::catalog().all().iter().copied().filter(|d| matches!(d.effect, crate::weapons::Effect::Explode | crate::weapons::Effect::Stun) && d.explosion_radius > 2.0) {
            let taken = |wall: bool| {
                let host = FakeHost::default();
                host.add(general(1, 0));
                let at = (def.explosion_radius * 0.5).min(30.0);
                host.add(ObjInfo { pos: Vec3::new(at, 0.0, 0.0), ..general(2, 1) });
                if wall {
                    host.add(ObjInfo { id: 9, flags: objflags::STRUCTURE, pos: Vec3::new(at * 0.5, 0.0, 0.0), geom_type: crate::host::geom::BOX, major_radius: 0.3, minor_radius: 40.0, height: 40.0, ..Default::default() });
                }
                let mut sim = Sim::new();
                sim.take_control(&host, 0, 1);
                let mut g = Projectile::launch(def, 1, Vec3::new(0.0, 0.0, 0.5), Vec3::new(0.0, 0.0, -1.0), 0.0, &host, 0.0);
                (g.fuse, g.resting, g.lands) = (0.0, true, false);
                sim.projectiles.push((0, g));
                crate::sim::tests::run(&mut sim, &host, 0, 1, FpsInput::default(), 1);
                let hurt = host.damage_log.borrow().iter().any(|d| d.1 == 2);
                let stunned = host.disabled.borrow().iter().any(|d| d.0 == 2);
                hurt || stunned
            };
            assert!(taken(false), "{} reaches him in the open", def.id);
            assert!(!taken(true), "{} is stopped by the wall", def.id);
        }
    }
}
