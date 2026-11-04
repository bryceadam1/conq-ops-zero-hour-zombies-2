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
                let mut proj = Projectile::launch(def, p.general, eye + dir * 2.0, dir, def.fuse_time, host, p.yaw);
                // Flak Jacket covers a launcher's own blast (and a bolt's grenade) as a thrown one.
                (proj.self_blast_scale, proj.self_stun_scale) = p.arsenal.self_harm_scales();
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
        let kill = host.damage(p.general, id, amount, how);
        // BO2's blood is picked by the hit region and its lethality (FxFleshType); every other
        // surface comes from what was hit.
        let surface = Surface::of_object(victim.as_ref());
        if fx {
            self.emit(idx, EventKind::Impact { at: point, dir, surface, head: headshot, fatal: kill, impact_type: def.impact_type, exit: false });
        }
        self.emit(idx, EventKind::HitMarker { headshot, kill });
        if kill && p.arsenal.has_specialty(Specialty::Scavenger) {
            p.arsenal.scavenge();
        }
        BulletHit { id, surface, head: headshot, kill }
    }
}

/// How far a knife reaches. In first person the General is BO2's player body, so the knife reaches
/// [`MELEE_RANGE`] past its hit radius. Under RTS control Zero Hour walked him up to the victim with
/// its own footprints (attack range is measured edge to edge, `Weapon::getAttackDistance`), so the
/// reach counts both footprints the same way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeleeReach {
    Body,
    Footprint(f32),
}

/// How much wider than the reach the melee's object query is (the reach test then decides);
/// Zero Hour units.
const MELEE_QUERY_SLACK: f32 = 12.0;
/// The knife's facing cone (cosine, about 53 degrees): the mod's stand-in for BO2's melee trace
/// box (`player_meleeWidth`/`player_meleeHeight` 10 in) plus its lunge's aim assist.
const MELEE_FACING_COS: f32 = 0.6;

impl MeleeReach {
    /// Whether a blow from `from` (the General's feet) reaches `o`: a soldier within the reach of
    /// his BO2-sized body, anything else within the reach of its Zero Hour footprint (a box's
    /// side, not a circle of its length: BO2's melee trace meets the entity's bounds).
    fn reaches(self, from: Vec3, o: &ObjInfo) -> bool {
        let d = (o.pos - from).length_xy();
        match self {
            MeleeReach::Body if o.has(objflags::INFANTRY) => d <= MELEE_RANGE + reach_radius(o),
            MeleeReach::Body => crate::hitscan::distance_to(&crate::hitscan::solid(o), Vec3::new(from.x, from.y, o.pos.z)) <= MELEE_RANGE,
            MeleeReach::Footprint(own) => d <= MELEE_RANGE + own + o.major_radius,
        }
    }
}

/// BO2's melee charge (the knife's lunge): the target and the seconds left of it. A melee at an
/// enemy in front, past the reach (`player_meleeRange`) but inside the melee weapon's
/// `meleeChargeRange`, charges at him (`FUN_008dc770`: charge flag 0x20000 and a charge distance
/// beyond the reach pick the charge times); the move runs `sqrt(2 x gap / friction)` seconds
/// (`FUN_008c3770`: speed `sqrt(gap x 2 x friction)`, time speed / friction).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeleeCharge {
    pub target: u32,
    pub left: f32,
}

crate::statehash::state_hash!(struct MeleeCharge { target, left });

impl Sim {
    /// The enemy a melee can reach within `range` (past the victim's hit radius), nearest first:
    /// in front of the General, alive, not an ally, not a structure or a deployed shield.
    fn melee_target(&self, host: &dyn LogicHost, p: &PlayerSim, range: f32) -> Option<ObjInfo> {
        let (fwd, _) = basis_xy(p.yaw);
        let mut near = Vec::new();
        host.query(p.body.eye(), range + MELEE_QUERY_SLACK, p.general, &mut near);
        near.into_iter()
            .filter(|o| o.id != p.general && !o.has(objflags::DEAD) && !o.has(objflags::ALLY))
            .filter(|o| !self.deployed.iter().any(|d| d.object != 0 && d.object == o.id))
            .filter(|o| !o.has(objflags::NO_COLLIDE) && !o.has(objflags::STRUCTURE))
            .filter(|o| {
                let to = o.pos - p.body.pos;
                let d = to.length_xy();
                d <= range + reach_radius(o) && (d < 1e-3 || (to.x * fwd.x + to.y * fwd.y) / d > MELEE_FACING_COS)
            })
            .min_by(|a, b| (a.pos - p.body.pos).length_xy().total_cmp(&(b.pos - p.body.pos).length_xy()).then(a.id.cmp(&b.id)))
    }

    /// On the melee press: a target past the reach but inside the melee weapon's charge range
    /// makes this a charge melee (its charge time and delay) and starts the lunge.
    pub(super) fn start_melee_charge(&mut self, host: &dyn LogicHost, p: &mut PlayerSim) {
        p.charge = None;
        let knife = p.arsenal.melee_weapon();
        if knife.melee_charge_range <= MELEE_RANGE {
            return;
        }
        let Some(target) = self.melee_target(host, p, knife.melee_charge_range) else { return };
        let gap = (target.pos - p.body.pos).length_xy() - reach_radius(&target) - MELEE_RANGE;
        if gap <= 0.0 {
            return; // in reach: the plain melee
        }
        let friction = crate::units::inches(p.arsenal.tuning.player_melee_charge_friction).max(1.0);
        p.arsenal.charge_melee();
        p.charge = Some(MeleeCharge { target: target.id, left: (2.0 * gap / friction).sqrt() });
    }

    /// This frame's lunge for the movement (`None`: none, or it ended — its time ran out, or the
    /// target died or went out of sight).
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
            stop: MELEE_RANGE + reach_radius(&o),
            max_speed: crate::units::inches(t.player_melee_charge_max_speed),
            friction: crate::units::inches(t.player_melee_charge_friction),
        })
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
            MeleeReach::Body => MELEE_QUERY_SLACK,
            MeleeReach::Footprint(own) => own + MELEE_QUERY_SLACK,
        };
        // A charge melee strikes the enemy it charges at (BO2 locks the charge target, ps +0x530),
        // wherever the lunge has got to when the charge delay runs out.
        let charged = p.charge.and_then(|c| host.object(c.target, p.general)).filter(|o| !o.has(objflags::DEAD));
        let mut near = Vec::new();
        host.query(eye, MELEE_RANGE + extra, p.general, &mut near);
        let victim = charged.or_else(|| near
            .iter()
            .filter(|o| o.id != p.general && !o.has(objflags::DEAD) && !o.has(objflags::ALLY))
            .filter(|o| !self.deployed.iter().any(|d| d.object != 0 && d.object == o.id))
            .filter(|o| !o.has(objflags::NO_COLLIDE) && !o.has(objflags::STRUCTURE))
            .filter(|o| {
                let to = o.pos - p.body.pos;
                let d = to.length_xy();
                reach.reaches(p.body.pos, o) && (d < 1e-3 || (to.x * fwd.x + to.y * fwd.y) / d > MELEE_FACING_COS)
            })
            .min_by(|a, b| {
                let da = (a.pos - p.body.pos).length_xy();
                let db = (b.pos - p.body.pos).length_xy();
                da.total_cmp(&db).then(a.id.cmp(&b.id))
            })
            .cloned());
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
            self.emit(idx, EventKind::MeleeHit);
            let rule = crate::damage::scale_on(&p.arsenal.tuning, &p.arsenal.melee_weapon().id, MeansOfDeath::Melee, &v);
            let kill = host.damage(p.general, v.id, damage * rule * BO2_TO_ZH_DAMAGE, MeansOfDeath::Melee);
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
        // Random point in the spread cone (polar mapping of two uniform samples).
        let angle = jitter.0 * std::f32::consts::PI;
        let radius = spread_deg.to_radians() * jitter.1.abs().sqrt();
        let dir = dir_from_angles(p.fire_yaw + radius * det::cos(angle), p.fire_pitch + radius * det::sin(angle));

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
        let round = Round { shooter: p.general, def, fx, tracer, pellet, left, gun_kick, eye };
        // Enemy equipment in the way takes the shot.
        let reach = match hit {
            HitResult::Object { distance, .. } => distance,
            other => (other.point() - eye).length(),
        };
        if let Some((i, t)) = self.equipment.shot(host, p.general, eye, dir, reach) {
            let at = eye + dir * t;
            self.round_effects(host, &round, 0, at);
            let e = self.equipment.destroy(host, i, Some(idx));
            self.equipment_events(host, vec![e]);
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
            self.emit(idx, EventKind::Shot { origin: r.eye, end, left: r.left });
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
        a.scavenge();
        a.scavenge();
        a.scavenge();
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
}
