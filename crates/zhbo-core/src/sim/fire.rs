/*
** Conquer Ops - Zero Hour gameplay core: the shot: weapon actions, targets and throw arcs.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::*;

impl Sim {
    pub(super) fn apply_weapon_action(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, action: WeaponAction) {
        match action {
            WeaponAction::Fire { spread_deg, jitter, recoil, pellet } => {
                let def = p.arsenal.current().def;
                let eye = p.body.eye();
                // Random point in the spread cone (polar mapping of two uniform samples).
                let angle = jitter.0 * std::f32::consts::PI;
                let radius = spread_deg.to_radians() * jitter.1.abs().sqrt();
                let dir = dir_from_angles(p.yaw + radius * angle.cos(), p.pitch + radius * angle.sin());

                let mut targets = gather_targets(host, eye, def.max_range, p.general);
                let ground = |x: f32, y: f32| host.ground_height(x, y);
                let hit = hitscan::trace(eye, dir, def.max_range, &targets, p.general, &ground);
                // One tracer and sound per shot, not per pellet.
                let fx = if pellet == 0 { Some(def.fx_weapon.as_str()).filter(|f| !f.is_empty()) } else { None };
                // Enemy equipment in the way takes the shot.
                let reach = match hit {
                    HitResult::Object { distance, .. } => distance,
                    other => (other.point() - eye).length(),
                };
                if let Some((i, t)) = self.equipment.shot(host, p.general, eye, dir, reach) {
                    if let Some(fx) = fx {
                        host.fire_fx(p.general, fx, 0, eye + dir * t);
                    }
                    let e = self.equipment.destroy(host, i, Some(idx));
                    self.equipment_events(vec![e]);
                    if pellet == 0 {
                        self.emit(idx, EventKind::Shot { origin: eye, end: eye + dir * t });
                        self.emit(idx, EventKind::Recoil { pitch_deg: recoil.0, yaw_deg: recoil.1 });
                    }
                    return;
                }
                match hit {
                    HitResult::Object { id, point, distance, headshot } => {
                        if let Some(fx) = fx {
                            host.fire_fx(p.general, fx, id, point);
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
                            if let HitResult::Object { id: next, distance, headshot, .. } = hitscan::trace(point + dir * 0.1, dir, def.max_range - distance, &targets, p.general, &ground) {
                                let mult = if headshot { def.head_multiplier } else { 1.0 };
                                kill |= host.damage(p.general, next, def.damage_at(distance) * mult * PENETRATION_DAMAGE * BO2_TO_ZH_DAMAGE, damage_kind::SMALL_ARMS);
                            }
                        }
                        self.emit(idx, EventKind::HitMarker { headshot, kill });
                        if kill {
                            if p.arsenal.has(Perk::Scavenger) {
                                p.arsenal.scavenge();
                            }
                        }
                    }
                    other => {
                        if let Some(fx) = fx {
                            host.fire_fx(p.general, fx, 0, other.point());
                        }
                    }
                }
                if pellet == 0 {
                    self.emit(idx, EventKind::Shot { origin: eye, end: hit.point() });
                    self.emit(idx, EventKind::Recoil { pitch_deg: recoil.0, yaw_deg: recoil.1 });
                }
            }
            WeaponAction::Launch { def, pellet } => {
                let eye = p.body.eye();
                // The tri-bolt's bolts fan out a little.
                let spread = (pellet as f32 - (def.pellets.max(1) - 1) as f32 * 0.5) * 0.04;
                let dir = dir_from_angles(p.yaw + spread, p.pitch);
                if !def.fx_weapon.is_empty() {
                    host.fire_fx(p.general, &def.fx_weapon, 0, eye + dir * 10.0);
                }
                let proj = Projectile::launch(def, p.general, eye + dir * 2.0, dir, def.fuse_time, host, p.yaw);
                self.projectiles.push((idx, proj));
                if pellet == 0 {
                    self.emit(idx, EventKind::Launched);
                }
            }
            WeaponAction::Throw { def, fuse_left } => {
                let eye = p.body.eye();
                // BO2 throws a little above the crosshair.
                let dir = dir_from_angles(p.yaw, (p.pitch + 0.12).min(1.4));
                let mut proj = Projectile::launch(def, p.general, eye + dir * 1.5, dir, fuse_left, host, p.yaw);
                proj.vel = proj.vel + p.body.vel * 0.5;
                proj.flak_jacket = p.arsenal.has(Perk::FlakJacket);
                proj.tactical_mask = p.arsenal.has(Perk::TacticalMask);
                self.projectiles.push((idx, proj));
                self.emit(idx, EventKind::Thrown { lethal: def.class == Class::Lethal });
            }
            WeaponAction::CookedOff { def } => {
                let mut proj = Projectile::launch(def, p.general, p.body.eye(), Vec3::ZERO, 0.0, host, p.yaw);
                proj.vel = Vec3::ZERO;
                proj.fuse = 0.0;
                proj.flak_jacket = p.arsenal.has(Perk::FlakJacket);
                self.projectiles.push((idx, proj));
            }
            WeaponAction::OffhandReady { lethal } => self.emit(idx, EventKind::OffhandReady { lethal }),
            WeaponAction::Melee { damage } => {
                self.emit(idx, EventKind::Melee);
                let eye = p.body.eye();
                let (fwd, _) = basis_xy(p.yaw);
                let mut near = Vec::new();
                host.query(eye, MELEE_RANGE + 12.0, p.general, &mut near);
                let victim = near
                    .iter()
                    .filter(|o| o.id != p.general && !o.has(objflags::DEAD) && !o.has(objflags::ALLY))
                    .filter(|o| !o.has(objflags::NO_COLLIDE) && !o.has(objflags::STRUCTURE))
                    .filter(|o| {
                        let to = o.pos - p.body.pos;
                        let d = to.length_xy();
                        d <= MELEE_RANGE + reach_radius(o) && (d < 1e-3 || (to.x * fwd.x + to.y * fwd.y) / d > 0.6)
                    })
                    .min_by(|a, b| {
                        let da = (a.pos - p.body.pos).length_xy();
                        let db = (b.pos - p.body.pos).length_xy();
                        da.total_cmp(&db).then(a.id.cmp(&b.id))
                    });
                if let Some(v) = victim {
                    host.fire_fx(p.general, MELEE_FX_WEAPON, v.id, v.pos);
                    let kill = host.damage(p.general, v.id, damage * BO2_TO_ZH_DAMAGE, damage_kind::MELEE);
                    self.emit(idx, EventKind::HitMarker { headshot: false, kill });
                    if kill {
                        if p.arsenal.has(Perk::Scavenger) {
                            p.arsenal.scavenge();
                        }
                    }
                }
            }
            WeaponAction::Plant { def } => {
                let (fwd, _) = basis_xy(p.yaw);
                let at = p.body.pos + fwd * PLANT_DISTANCE;
                let at = Vec3::new(at.x, at.y, host.ground_height(at.x, at.y));
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
            WeaponAction::DryFire | WeaponAction::ReloadFinished => {}
        }
    }
}

pub const PENETRATION_DAMAGE: f32 = 0.75;

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

/// [`World`] backed by the engine: terrain from `TheTerrainLogic`, solid footprints from structures
/// and ground vehicles.
pub struct HostWorld<'a> {
    pub host: &'a dyn LogicHost,
    pub viewer: u32,
}

impl World for HostWorld<'_> {
    fn ground_height(&self, x: f32, y: f32) -> f32 {
        self.host.ground_height(x, y)
    }

    fn is_walkable(&self, x: f32, y: f32) -> bool {
        self.host.is_walkable(x, y)
    }

    fn extent(&self) -> (Vec3, Vec3) {
        self.host.extent()
    }

    fn blockers(&self, pos: Vec3, radius: f32, out: &mut Vec<Blocker>) {
        let mut objs = Vec::new();
        self.host.query(pos, radius, self.viewer, &mut objs);
        objs.sort_by_key(|o| o.id);
        for o in objs.iter().filter(|o| o.id != self.viewer) {
            if o.has(objflags::DEAD) || o.has(objflags::NO_COLLIDE) || o.has(objflags::AIRBORNE) {
                continue;
            }
            if !(o.has(objflags::STRUCTURE) || o.has(objflags::VEHICLE)) {
                continue;
            }
            let top = o.pos.z + o.height.max(1.0);
            out.push(match o.geom_type {
                geom::BOX => Blocker::Box { center: o.pos, half_x: o.major_radius, half_y: o.minor_radius, yaw: o.yaw, top },
                _ => Blocker::Circle { center: o.pos, radius: o.major_radius, top },
            });
        }
    }
}

/// Where the hammer puts a structure with a footprint of `radius`, and which way it faces: its
/// front towards him, turned by `turn`.
pub fn placement(host: &dyn LogicHost, p: &PlayerSim, radius: f32, turn: f32) -> (Vec3, f32) {
    let pos = building::place_point(p.body.eye(), dir_from_angles(p.yaw, p.pitch), radius, |x, y| host.ground_height(x, y));
    (pos, crate::math::wrap_angle(p.yaw + std::f32::consts::PI + turn))
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
