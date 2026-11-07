//! Black Ops II equipment once it is down: Claymores and Bouncing Betties that wait for an enemy,
//! C4 that waits for its owner's detonator, the Shock Charge, the Trophy System shooting down
//! enemy projectiles, the Sensor Grenade revealing enemies, and the Tactical Insertion marking
//! where its owner's next General comes in. Enemies can shoot equipment, and the Black Hat hacks
//! it to the other side.
//!
//! "Enemies" are Zero Hour's: infantry and ground vehicles of any player the owner is at war with.
//! Each piece shows as a Zero Hour object (the converted BO2 model, on the owner's team), which is
//! also how the logic asks who is an enemy of whom.
//!
//! BO2 sets the ranges and delays in its MP scripts (`_weaponobjects.gsc`, `_bouncingbetty.gsc`,
//! `_proximity_grenade.gsc`, `_trophy_system.gsc`, `_hacker_tool.gsc`); they come from the tuning
//! pack ([`EquipTuning`], each key citing its script line).

use crate::hitscan::{ray_vs_target, Shape, Target};
use crate::host::{objflags, LogicHost, ObjInfo};
use crate::math::{basis_xy, Vec3};
use crate::impacts::Surface;
use crate::projectiles::blast;
use crate::units::{inches, LOGIC_DT};
use crate::weapons::WeaponDef;


/// How big equipment is to bullets.
pub const HIT_RADIUS: f32 = inches(8.0);
/// Black Hat: the aim cone (cosine) for equipment and Zero Hour drones. BO2 tests the target's
/// lock circle on screen (`target_isincircle`, `_hacker_tool.gsc:416`, FOV 65); a cone of about
/// 16 degrees stands in for it.
pub const HACK_COS: f32 = 0.96;

/// The equipment numbers from the tuning pack (`crate::tuning`), in Zero Hour units and seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EquipTuning {
    /// Seconds after it is down before a piece does anything.
    pub arm_time: f32,
    /// Claymore: trigger radius, front cone (cosine), least distance in front, trip-to-blast delay.
    pub claymore_range: f32,
    pub claymore_cos: f32,
    pub claymore_min_dist: f32,
    pub claymore_grace: f32,
    /// Bouncing Betty: trigger radius, trip-to-jump delay, jump height and time.
    pub betty_range: f32,
    pub betty_grace: f32,
    pub betty_hop: f32,
    pub betty_jump_time: f32,
    /// Shock Charge: trigger radius and trip-to-shock delay.
    pub shock_range: f32,
    pub shock_grace: f32,
    /// Trophy System: interception range and charges.
    pub trophy_range: f32,
    pub trophy_charges: u8,
    /// Trophy System: the blast where it shoots something down (radius, centre and edge damage
    /// on BO2's 100-HP scale).
    pub trophy_blast: (f32, f32, f32),
    /// `delayexplosive` (Engineer): extra seconds a tripped explosive waits.
    pub delay_explosive: f32,
    /// Black Hat: range; the lock on equipment and on a drone (seconds of sight); the script's
    /// delay from the pull-back to the lock starting.
    pub hack_range: f32,
    pub hack_lock_equipment: f32,
    pub hack_lock_drone: f32,
    pub hack_lock_start: f32,
}

impl EquipTuning {
    pub fn from_tuning(t: &crate::tuning::Tuning) -> Self {
        EquipTuning {
            arm_time: t.equipment_arm_time,
            claymore_range: inches(t.scr_weaponobject_radius),
            claymore_cos: crate::math::det::cos(t.scr_weaponobject_coneangle.to_radians()),
            claymore_min_dist: inches(t.scr_weaponobject_mindist),
            claymore_grace: t.scr_weaponobject_graceperiod,
            betty_range: inches(t.betty_radius),
            betty_grace: t.betty_grace_period,
            betty_hop: inches(t.betty_jump_height),
            betty_jump_time: t.betty_jump_time.max(LOGIC_DT),
            shock_range: inches(t.proximity_grenade_detection_radius),
            shock_grace: t.proximity_grenade_grace_period,
            trophy_range: inches(t.trophy_system_range),
            trophy_charges: t.trophy_system_ammo.clamp(0.0, 255.0) as u8,
            trophy_blast: (inches(t.trophy_intercept_radius), t.trophy_intercept_max_damage, t.trophy_intercept_min_damage),
            delay_explosive: t.perk_delay_explosive_time,
            hack_range: inches(t.hacker_tool_max_equipment_distance),
            hack_lock_equipment: t.equipment_hacker_tool_time_ms / 1000.0,
            hack_lock_drone: t.vehicle_hacker_tool_time_ms / 1000.0,
            hack_lock_start: t.hacker_tool_lock_start,
        }
    }

    /// The trigger radius of a piece: its script radius, capped by the weapon's own explosion
    /// radius (`shouldaffectweaponobject`, `_weaponobjects.gsc:1564-1569`).
    fn trigger_range(range: f32, def: &WeaponDef) -> f32 {
        if def.explosion_radius > 0.0 { range.min(def.explosion_radius) } else { range }
    }
}

impl Default for EquipTuning {
    fn default() -> Self {
        EquipTuning::from_tuning(&crate::tuning::Tuning::default())
    }
}

/// BO2's effects for equipment that breaks or does its work, by BO2's names (the converter
/// converts each of [`BO2_EFFECTS`]; [`play`] plays one by its converted name):
/// `level._equipment_explode_fx` and `_lg` (`_weaponobjects.gsc:60-61`; the Sensor Grenade's and
/// the Trophy System's destruction), `level._equipment_emp_destroy_fx` (`:59`, anything an EMP
/// takes out), the Betty's `bettydestroyedfx` (`_bouncingbetty.gsc:13`), the Tactical Insertion's
/// `tacticalInsertionFizzle` (`_tacticalinsertion.gsc:20`) and the Trophy System's
/// `trophydetonationfx` where it shoots something down (`_trophy_system.gsc:20`).
pub const EQUIPMENT_FX: &str = "explosions/fx_exp_equipment";
pub const EQUIPMENT_LG_FX: &str = "explosions/fx_exp_equipment_lg";
pub const EMP_DESTROY_FX: &str = "weapon/emp/fx_emp_explosion_equip";
pub const BETTY_DESTROYED_FX: &str = "weapon/bouncing_betty/fx_betty_destroyed";
pub const TAC_INSERT_FIZZLE_FX: &str = "misc/fx_equip_tac_insert_exp";
pub const TROPHY_INTERCEPT_FX: &str = "weapon/trophy_system/fx_trophy_radius_detonation";
pub const BO2_EFFECTS: &[&str] = &[EQUIPMENT_FX, EQUIPMENT_LG_FX, EMP_DESTROY_FX, BETTY_DESTROYED_FX, TAC_INSERT_FIZZLE_FX, TROPHY_INTERCEPT_FX];

/// Play the BO2 effect `effect` (one of [`BO2_EFFECTS`]) at `at`.
pub fn play(host: &dyn LogicHost, effect: &str, at: Vec3) {
    host.fx(&crate::names::bo2_fx(effect), at);
}

/// What a piece does when an enemy breaks it (shot, or caught in an explosion): BO2's watcher for
/// it. C4 (`satcheldetonate`), the Claymore (`claymoredetonate`, `_weaponobjects.gsc:347-363`) and
/// the Shock Charge (`proximitydetonate`) all go to `weapondetonate`: they go off, the breaker
/// their attacker. The Betty is destroyed without its jump (`bouncingbettydetonate` with a weapon:
/// `bouncingbettydestroyed`), the Trophy System and the Sensor Grenade explode as equipment
/// (`trophysystemdetonate`, `sensorgrenadedestroyed`), the Tactical Insertion fizzles (`fizzle`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Breaks {
    Detonates,
    Destroyed(&'static str),
}

pub fn breaks(kind: Kind) -> Breaks {
    match kind {
        Kind::C4 | Kind::Claymore | Kind::Shock => Breaks::Detonates,
        Kind::Betty => Breaks::Destroyed(BETTY_DESTROYED_FX),
        Kind::Trophy => Breaks::Destroyed(EQUIPMENT_LG_FX),
        Kind::Insertion => Breaks::Destroyed(TAC_INSERT_FIZZLE_FX),
        Kind::Sensor | Kind::Other => Breaks::Destroyed(EQUIPMENT_FX),
    }
}

/// Whether a blast of `effect` breaks equipment: an explosion does, an EMP does (its own way);
/// a concussion, flashbang or Shock Charge only stuns it and smoke does nothing
/// (`weaponobjectdamage`'s `continue` cases, `_weaponobjects.gsc:603-628`).
pub fn blast_breaks(effect: crate::weapons::Effect) -> bool {
    use crate::weapons::Effect;
    matches!(effect, Effect::Explode | Effect::Emp)
}

/// A Trophy System shooting something down at `at` (`projectileexplode`, `_trophy_system.gsc:
/// 228-233`): its detonation effect there, and `radiusdamage( projposition, 128, 105, 10, self )`
/// from its owner — the full damage at the centre falling linearly to the edge, on BO2's scale. The
/// script's `radiusdamage` names no means of death, so it deals the script default's
/// `MOD_EXPLOSIVE` (the explicit `"MOD_EXPLOSIVE"` the other scripts pass for the same blast):
/// Flak Jacket's `isexplosivedamage` and BO2's vehicle splash rule apply to it.
fn trophy_kill(host: &dyn LogicHost, p: &Placed, at: Vec3, tn: &EquipTuning) {
    play(host, TROPHY_INTERCEPT_FX, at);
    let (radius, max, min) = tn.trophy_blast;
    let mut objs = Vec::new();
    host.query(at, radius, p.viewer(), &mut objs);
    objs.sort_by_key(|o| o.id);
    // The script's `radiusdamage` is the engine's radius damage: scaled by what it sees.
    let cover = crate::cover::Blockers::near(host, at, radius * 2.0, p.viewer(), &[]);
    let t = crate::tuning::get();
    for o in objs.iter().filter(|o| o.has(objflags::ENEMY) && !o.has(objflags::DEAD) && !o.has(objflags::PROJECTILE)) {
        let d = crate::projectiles::blast_distance(o, at);
        if d > radius {
            continue;
        }
        let seen = cover.fraction(host, &t, at, o);
        if seen <= 0.0 {
            continue;
        }
        let bo2 = (max - (max - min) * (d / radius.max(1e-3))) * seen;
        host.damage(p.general, o.id, bo2 * crate::units::BO2_TO_ZH_DAMAGE, crate::damage::Blow::from_to(crate::damage::MeansOfDeath::Explosive, p.def, at, o.pos));
    }
}

/// What a piece of equipment is and does (`crate::weapons::Kind`, re-exported here for the
/// sim's equipment code). Lives on the weapon definition (the weapon table's `Equipment` block,
/// refactor plan 3.2): the converter writes it, the sim reads it. The hand-written id match
/// this replaces was a second copy of the table.
pub use crate::weapons::Kind;

impl Kind {
    /// The weapon's kind, straight from the weapon table (refactor plan 3.2). The old
    /// hand-written id match this replaced was a second copy of the table.
    pub fn of(def: &WeaponDef) -> Kind {
        def.equipment_kind
    }
}

/// A piece of equipment that is down.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub kind: Kind,
    pub def: &'static WeaponDef,
    /// The owning player's index, and the General who put it down (its damage comes from them).
    pub owner: usize,
    pub general: u32,
    /// Its Zero Hour object (0 if the art is missing).
    pub object: u32,
    pub pos: Vec3,
    pub yaw: f32,
    pub age: f32,
    /// Set off: seconds until it goes off.
    pub triggered: Option<f32>,
    /// Trophy System charges left.
    pub charges: u8,
    /// Stuck to a Zero Hour object (C4 or a Shock Charge on a tank).
    pub stuck_to: Option<u32>,
    /// A timed throw's fuse still running (seconds; `timedDetonation`, the Sensor Grenade): when it
    /// runs out the piece goes off where it lies and is gone (t6mp `FUN_0060a480`: the blast, then
    /// `G_FreeEntity`).
    pub fuse: Option<f32>,
}

crate::statehash::state_hash!(struct Placed { kind, def, owner, general, object, pos, yaw, age, triggered, charges, stuck_to, fuse });

/// What happened to equipment this frame, for hit markers and sounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EquipEvent {
    /// Went off: `def`'s blast sound and impact type, on `surface` (the client picks the cues).
    Exploded { owner: usize, at: Vec3, hit_enemy: bool, kill: bool, def: &'static WeaponDef, sound: &'static str, impact_type: i32, surface: Surface },
    /// Shot or blown up by an enemy.
    Destroyed { owner: usize, by: Option<usize>, at: Vec3 },
    /// A Trophy System shot a projectile down.
    Intercepted { owner: usize, at: Vec3 },
}

impl Placed {
    pub fn new(def: &'static WeaponDef, owner: usize, general: u32, object: u32, pos: Vec3, yaw: f32, stuck_to: Option<u32>) -> Placed {
        let kind = Kind::of(def);
        let charges = EquipTuning::default().trophy_charges;
        Placed { kind, def, owner, general, object, pos, yaw, age: 0.0, triggered: None, charges, stuck_to, fuse: None }
    }

    /// The object relationships are asked through: its own, or its owner General's.
    fn viewer(&self) -> u32 {
        if self.object != 0 {
            self.object
        } else {
            self.general
        }
    }

    pub fn is_armed(&self, t: &EquipTuning) -> bool {
        self.age >= t.arm_time
    }

    /// Whether `o` (as seen from this equipment) is an enemy that sets it off.
    fn trips(o: &ObjInfo) -> bool {
        o.has(objflags::ENEMY) && !o.has(objflags::DEAD) && !o.has(objflags::NO_COLLIDE) && !o.has(objflags::AIRBORNE) && (o.has(objflags::INFANTRY) || o.has(objflags::VEHICLE))
    }

    fn enemies_near(&self, host: &dyn LogicHost, range: f32) -> Vec<ObjInfo> {
        let mut objs = Vec::new();
        host.query(self.pos, range + 12.0, self.viewer(), &mut objs);
        objs.retain(|o| Self::trips(o) && (o.pos - self.pos).length_xy() <= range + if o.has(objflags::INFANTRY) { 0.0 } else { o.major_radius });
        objs.sort_by_key(|o| o.id);
        objs
    }

    /// Whether the owner of this piece is an enemy of `general`'s owner.
    pub fn hostile_to(&self, host: &dyn LogicHost, general: u32) -> bool {
        self.object != 0 && host.object(self.object, general).is_some_and(|o| o.has(objflags::ENEMY))
    }

    pub fn target(&self) -> Target {
        Target { id: self.object, pos: self.pos, shape: Shape::Sphere { radius: HIT_RADIUS }, head_fraction: 0.0 }
    }

    fn remove(&mut self, host: &dyn LogicHost) {
        if self.object != 0 {
            host.destroy(self.object);
            self.object = 0;
        }
    }

    /// Blow it up where it is; its damage reaches each General through his guard, and what it does
    /// to a first-person General goes into `hits` (the sim's shellshocks).
    fn explode(&mut self, host: &dyn LogicHost, shields: &[crate::shield::Bearer], guards: &[crate::sim::guard::Guard], hits: &std::cell::RefCell<Vec<crate::sim::guard::ShockHit>>) -> EquipEvent {
        let (owner, general) = (self.owner, self.general);
        self.explode_for(host, owner, general, shields, guards, hits)
    }

    /// [`Placed::explode`] with another attacker: a piece an enemy broke goes off as his
    /// (`weapondetonate`: `self detonate( attacker )`), its damage and credit his.
    fn explode_for(&mut self, host: &dyn LogicHost, seat: usize, general: u32, shields: &[crate::shield::Bearer], guards: &[crate::sim::guard::Guard], hits: &std::cell::RefCell<Vec<crate::sim::guard::ShockHit>>) -> EquipEvent {
        self.remove(host);
        let guarded = crate::sim::guard::Guarded { host, guards, def: self.def, stuck_to: self.stuck_to, thrown: false, owner: general, at: Some(self.pos), hits };
        let b = blast(&guarded, self.def, general, self.pos, crate::projectiles::Hit { direct: None, stuck_to: self.stuck_to }, 1.0, 1.0, shields);
        let d = self.def;
        EquipEvent::Exploded { owner: seat, at: self.pos, hit_enemy: b.hit_enemy, kill: b.kill, def: d, sound: d.explode_sound.as_str(), impact_type: d.impact_type, surface: b.surface }
    }
}

/// Everything that is down, stepped once per logic frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Equipment {
    pub placed: Vec<Placed>,
    /// The frame's shield bearers (the sim's [`crate::sim::Sim::shields`]): blasts test them.
    pub shields: Vec<crate::shield::Bearer>,
    /// The frame's Generals' perks against others' blasts (the sim's, refreshed with the shields):
    /// every blast — equipment's and projectiles' — reaches the host through them
    /// ([`crate::sim::guard`]).
    pub guards: Vec<crate::sim::guard::Guard>,
    /// What this frame's blasts did to first-person Generals, for the sim's shellshocks
    /// ([`crate::sim::guard::ShockHit`]); the sim drains it.
    pub shock_hits: std::cell::RefCell<Vec<crate::sim::guard::ShockHit>>,
    /// The tuning pack's equipment numbers.
    pub tuning: EquipTuning,
    /// The enemies the Sensor Grenades put on their throwers' compasses ([`crate::sensor`]).
    pub marks: crate::sensor::Marks,
    /// The tuning pack's sensor numbers.
    pub sensor: crate::sensor::SensorTuning,
}

// `shields` and `guards` are the frame's snapshots of the hashed players and deployed shields,
// rebuilt at the start of every step; `shock_hits` is filled and drained inside the step;
// `tuning` and `sensor` are the tuning pack (the content CRC covers it).
crate::statehash::state_hash!(struct Equipment { placed, marks } skip { shields, guards, shock_hits, tuning, sensor });

impl Equipment {
    pub fn add(&mut self, mut p: Placed) {
        p.charges = self.tuning.trophy_charges;
        self.placed.push(p);
    }

    /// Advance one logic frame. `engineer(id)` says whether an object is a General with Engineer
    /// (it delays explosives it trips). Projectiles a Trophy System shoots down are removed from
    /// `projectiles`.
    pub fn step(
        &mut self,
        host: &dyn LogicHost,
        projectiles: &mut Vec<(usize, crate::projectiles::Projectile)>,
        engineer: &dyn Fn(u32) -> bool,
        events: &mut Vec<EquipEvent>,
    ) {
        let mut gone = Vec::new();
        // What went off this step, for the chain: (seat, General, where, radius, effect).
        let mut blasts: Vec<(usize, u32, Vec3, f32, crate::weapons::Effect)> = Vec::new();
        let shields = &self.shields;
        let guards = &self.guards;
        let hits = &self.shock_hits;
        let tn = self.tuning;
        let st = self.sensor;
        self.marks.step(&st);
        let marks = &mut self.marks;
        for (i, p) in self.placed.iter_mut().enumerate() {
            p.age += LOGIC_DT;
            // Riding on what it stuck to; gone with it.
            if let Some(id) = p.stuck_to {
                match host.object(id, p.viewer()).filter(|o| !o.has(objflags::DEAD)) {
                    Some(o) => {
                        p.pos = Vec3::new(o.pos.x, o.pos.y, o.pos.z + o.height * 0.5);
                        if p.object != 0 {
                            host.set_pose(p.object, p.pos, p.yaw, false);
                        }
                    }
                    None => p.stuck_to = None,
                }
            }
            // A timed throw's fuse (the Sensor Grenade's): it goes off where it lies, its own
            // explosion (t6mp `FUN_0060a480`), and is gone.
            if let Some(f) = p.fuse.as_mut() {
                *f -= LOGIC_DT;
                if *f <= 0.0 {
                    // A laser sensor's detonation pings the enemies in its blast on its thrower's
                    // compass (`G_SensorGrenadeDetonate`, event 0x4e).
                    if p.def.laser_sensor {
                        marks.detonated(host, p.viewer(), p.owner, p.pos, p.def.explosion_radius, &st);
                    }
                    events.push(p.explode(host, shields, guards, hits));
                    blasts.push((p.owner, p.general, p.pos, p.def.explosion_radius, p.def.effect));
                    gone.push(i);
                    continue;
                }
            }
            // The Sensor Grenade's scan runs from the throw (its fuse tells how long ago).
            if p.kind == Kind::Sensor {
                let since = p.fuse.map_or(p.age, |f| p.def.fuse_time - f);
                marks.scan(host, p.viewer(), p.owner, p.pos, since, &st);
            }
            if let Some(t) = p.triggered.as_mut() {
                *t -= LOGIC_DT;
                if p.kind == Kind::Betty {
                    // The jump: the last `bettyjumptime` seconds, up to `bettyjumpheight`, where it
                    // goes off (`bouncingbettyjumpandexplode`).
                    let ground = host.ground_height(p.pos.x, p.pos.y);
                    let up = (1.0 - *t / tn.betty_jump_time).clamp(0.0, 1.0);
                    if p.object != 0 {
                        host.set_pose(p.object, Vec3::new(p.pos.x, p.pos.y, ground + tn.betty_hop * up), p.yaw, false);
                    }
                }
                if *t <= 0.0 {
                    if p.kind == Kind::Betty {
                        p.pos.z = host.ground_height(p.pos.x, p.pos.y) + tn.betty_hop;
                    }
                    events.push(p.explode(host, shields, guards, hits));
                    blasts.push((p.owner, p.general, p.pos, p.def.explosion_radius, p.def.effect));
                    gone.push(i);
                }
                continue;
            }
            if !p.is_armed(&tn) {
                continue;
            }
            // The trip, then the grace period, then Engineer's extra wait (`_weaponobjects.gsc`
            // `proximityweaponobjectdetonation`, `:1547-1551`).
            let delay = |o: &ObjInfo| if engineer(o.id) { tn.delay_explosive } else { 0.0 };
            match p.kind {
                Kind::Claymore => {
                    let (fwd, _) = basis_xy(p.yaw);
                    let range = EquipTuning::trigger_range(tn.claymore_range, p.def);
                    let tripped = p.enemies_near(host, range).into_iter().find(|o| {
                        let to = o.pos - p.pos;
                        let ahead = to.x * fwd.x + to.y * fwd.y;
                        ahead >= tn.claymore_min_dist && ahead / to.length_xy().max(1e-3) > tn.claymore_cos
                    });
                    if let Some(o) = tripped {
                        p.triggered = Some(tn.claymore_grace + delay(&o));
                    }
                }
                Kind::Betty => {
                    if let Some(o) = p.enemies_near(host, EquipTuning::trigger_range(tn.betty_range, p.def)).first() {
                        p.triggered = Some(tn.betty_grace + delay(o) + tn.betty_jump_time);
                    }
                }
                Kind::Shock => {
                    if let Some(o) = p.enemies_near(host, EquipTuning::trigger_range(tn.shock_range, p.def)).first() {
                        p.triggered = Some(tn.shock_grace + delay(o));
                    }
                }
                Kind::Trophy => {
                    // Enemy BO2 projectiles in flight.
                    let mut k = 0;
                    while k < projectiles.len() && p.charges > 0 {
                        let (_, proj) = &projectiles[k];
                        let hostile = host.object(proj.owner, p.viewer()).is_some_and(|o| o.has(objflags::ENEMY));
                        if hostile && (proj.pos - p.pos).length() <= tn.trophy_range && !proj.resting {
                            let (_, mut proj) = projectiles.remove(k);
                            proj.intercept(host);
                            trophy_kill(host, p, proj.pos, &tn);
                            p.charges -= 1;
                            events.push(EquipEvent::Intercepted { owner: p.owner, at: proj.pos });
                            continue;
                        }
                        k += 1;
                    }
                    // Zero Hour's missiles and shells.
                    let mut objs = Vec::new();
                    host.query(p.pos, tn.trophy_range, p.viewer(), &mut objs);
                    objs.sort_by_key(|o| o.id);
                    for o in objs.iter().filter(|o| o.has(objflags::PROJECTILE) && o.has(objflags::ENEMY) && o.origin_within(p.pos, tn.trophy_range)) {
                        if p.charges == 0 {
                            break;
                        }
                        host.destroy(o.id);
                        trophy_kill(host, p, o.pos, &tn);
                        p.charges -= 1;
                        events.push(EquipEvent::Intercepted { owner: p.owner, at: o.pos });
                    }
                    // Spent, it blows itself apart (`trophyactive` -> `trophysystemdetonate`).
                    if p.charges == 0 {
                        play(host, EQUIPMENT_LG_FX, p.pos);
                        p.remove(host);
                        gone.push(i);
                    }
                }
                Kind::Insertion | Kind::Sensor | Kind::C4 | Kind::Other => {}
            }
        }
        for i in gone.into_iter().rev() {
            self.placed.remove(i);
        }
        // An explosion breaks the enemy equipment it reaches, which may go off in turn.
        for (seat, general, at, radius, effect) in blasts {
            self.caught_in_blast(host, seat, general, at, radius, effect, events);
        }
    }

    /// The owner's detonator: every one of their C4 goes off.
    pub fn detonate(&mut self, host: &dyn LogicHost, owner: usize, events: &mut Vec<EquipEvent>) {
        let mut k = 0;
        while k < self.placed.len() {
            if self.placed[k].owner == owner && self.placed[k].kind == Kind::C4 {
                let mut p = self.placed.remove(k);
                events.push(p.explode(host, &self.shields, &self.guards, &self.shock_hits));
                self.caught_in_blast(host, p.owner, p.general, p.pos, p.def.explosion_radius, p.def.effect, events);
            } else {
                k += 1;
            }
        }
    }

    /// The nearest enemy equipment a shot from `origin` along `dir` hits before `max_t`, by index.
    pub fn shot(&self, host: &dyn LogicHost, shooter: u32, origin: Vec3, dir: Vec3, max_t: f32) -> Option<(usize, f32)> {
        self.placed
            .iter()
            .enumerate()
            .filter(|(_, p)| p.object != 0 && p.hostile_to(host, shooter))
            .filter_map(|(i, p)| ray_vs_target(origin, dir, max_t, &p.target()).map(|t| (i, t)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// Enemy equipment caught in a blast of `effect` at `at` (General `general`'s, player `by`'s)
    /// breaks ([`blast_breaks`], [`Equipment::destroy`]); what goes off breaks more in turn.
    #[allow(clippy::too_many_arguments)]
    pub fn caught_in_blast(&mut self, host: &dyn LogicHost, by: usize, general: u32, at: Vec3, radius: f32, effect: crate::weapons::Effect, events: &mut Vec<EquipEvent>) {
        if !blast_breaks(effect) {
            return;
        }
        // Each break may set off more (and take them out of the list): look again from the top.
        // A piece the blast does not see is spared (the radius damage's visibility, `FUN_005f0f00`;
        // the piece is a point at its place).
        let cover = crate::cover::Blockers::near(host, at, radius.max(1.0) * 2.0, general, &[]);
        while let Some(k) = self.placed.iter().position(|p| (p.pos - at).length() <= radius && p.hostile_to(host, general) && cover.sees_point(host, at, p.pos + Vec3::new(0.0, 0.0, 0.05), p.object)) {
            self.destroy(host, k, Some((by, general)), effect, events);
        }
    }

    /// Break the piece at `index`, by player and General `by` (shot, or in a blast of `cause`):
    /// an EMP takes it out with its own effect (`waitanddetonate`'s `_equipment_emp_destroy_fx`;
    /// the detonations then only delete it); anything else does what [`breaks`] says, a piece that
    /// goes off doing so as the breaker's.
    pub fn destroy(&mut self, host: &dyn LogicHost, index: usize, by: Option<(usize, u32)>, cause: crate::weapons::Effect, events: &mut Vec<EquipEvent>) {
        let mut p = self.placed.remove(index);
        let breaker = by.map(|(seat, _)| seat);
        events.push(EquipEvent::Destroyed { owner: p.owner, by: breaker, at: p.pos });
        if cause == crate::weapons::Effect::Emp {
            play(host, EMP_DESTROY_FX, p.pos);
            p.remove(host);
            return;
        }
        match breaks(p.kind) {
            Breaks::Detonates => {
                let (seat, general) = by.unwrap_or((p.owner, p.general));
                events.push(p.explode_for(host, seat, general, &self.shields, &self.guards, &self.shock_hits));
                self.caught_in_blast(host, seat, general, p.pos, p.def.explosion_radius, p.def.effect, events);
            }
            Breaks::Destroyed(fx) => {
                play(host, fx, p.pos);
                p.remove(host);
            }
        }
    }

    /// The enemy equipment `general` is aiming at from `eye` along `dir` (Black Hat), by index.
    pub fn aimed_at(&self, host: &dyn LogicHost, general: u32, eye: Vec3, dir: Vec3) -> Option<usize> {
        self.placed
            .iter()
            .enumerate()
            .filter(|(_, p)| p.object != 0 && p.hostile_to(host, general))
            .filter_map(|(i, p)| {
                let to = p.pos - eye;
                let d = to.length();
                (d <= self.tuning.hack_range && d > 1e-3 && to.dot(dir) / d >= HACK_COS).then_some((i, d))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// The Black Hat turned the piece at `index` to `owner`'s side.
    pub fn hacked(&mut self, host: &dyn LogicHost, index: usize, owner: usize, general: u32) {
        let p = &mut self.placed[index];
        p.owner = owner;
        p.general = general;
        if p.object != 0 {
            host.capture(p.object, general);
        }
        p.triggered = None;
        p.age = 0.0;
        p.charges = p.charges.max(1);
    }

    /// The player's Tactical Insertion, if one is down (it is used up by the next General).
    pub fn take_insertion(&mut self, host: &dyn LogicHost, owner: usize) -> Option<(Vec3, f32)> {
        let i = self.placed.iter().position(|p| p.owner == owner && p.kind == Kind::Insertion)?;
        let mut p = self.placed.remove(i);
        p.remove(host);
        Some((p.pos, p.yaw))
    }


    /// Remove everything a player put down but his Tactical Insertion (his General died: the next
    /// one comes in at it).
    pub fn clear_owner(&mut self, host: &dyn LogicHost, owner: usize) {
        self.clear(host, owner, true);
    }

    /// Remove everything a player put down, his Tactical Insertion too: he left the game (BO2's
    /// `deleteweaponobjectson` on `disconnect` deletes every weapon object of his, and
    /// `_tacticalinsertion.gsc` `watchdisconnect` destroys his insertion).
    pub fn clear_leaver(&mut self, host: &dyn LogicHost, owner: usize) {
        self.clear(host, owner, false);
        self.marks.clear_owner(owner);
    }

    fn clear(&mut self, host: &dyn LogicHost, owner: usize, keep_insertion: bool) {
        self.placed.retain_mut(|p| {
            if p.owner == owner && !(keep_insertion && p.kind == Kind::Insertion) {
                p.remove(host);
                false
            } else {
                true
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::tests::{general, soldier, FakeHost};
    use crate::weapons::{Class, Effect, Place};

    fn def(id: &str, effect: Effect) -> &'static WeaponDef {
        Box::leak(Box::new(WeaponDef {
            id: id.into(),
            class: Class::Equipment,
            effect,
            place: Place::Plant,
            equipment_kind: match id {
                "claymore_mp" => Kind::Claymore,
                "bouncingbetty_mp" => Kind::Betty,
                "satchel_charge_mp" => Kind::C4,
                "proximity_grenade_mp" => Kind::Shock,
                "trophy_system_mp" => Kind::Trophy,
                "tactical_insertion_mp" => Kind::Insertion,
                "sensor_grenade_mp" => Kind::Sensor,
                _ => Kind::Other,
            },
            explosion_radius: 30.0,
            explosion_inner_damage: 200.0,
            explosion_outer_damage: 100.0,
            ..Default::default()
        }))
    }

    fn step_n(eq: &mut Equipment, host: &FakeHost, n: usize, events: &mut Vec<EquipEvent>) {
        let mut projectiles = Vec::new();
        for _ in 0..n {
            eq.step(host, &mut projectiles, &|_| false, events);
        }
    }

    #[test]
    fn a_claymore_waits_to_arm_then_fires_at_enemies_in_front_only() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut claymore = general(50, 0);
        claymore.flags = objflags::NO_COLLIDE;
        host.add(claymore);
        let mut eq = Equipment::default();
        eq.add(Placed::new(def("claymore_mp", Effect::Explode), 0, 1, 50, Vec3::ZERO, 0.0, None));
        // An enemy behind it does nothing.
        host.add(soldier(7, 1, -15.0));
        let mut events = Vec::new();
        step_n(&mut eq, &host, 60, &mut events);
        assert!(events.is_empty(), "{events:?}");
        // One closer in front than `scr_weaponobject_mindist` does nothing either.
        let t = EquipTuning::default();
        host.objects.borrow_mut().iter_mut().find(|o| o.id == 7).unwrap().pos = Vec3::new(t.claymore_min_dist * 0.5, 0.0, 0.0);
        step_n(&mut eq, &host, 10, &mut events);
        assert!(events.is_empty() && eq.placed[0].triggered.is_none(), "{events:?}");
        // One walks in front: the trip, then BO2's grace period, then the blast.
        host.objects.borrow_mut().iter_mut().find(|o| o.id == 7).unwrap().pos = Vec3::new(15.0, 0.0, 0.0);
        let grace = (t.claymore_grace / LOGIC_DT).round() as usize;
        step_n(&mut eq, &host, grace - 1, &mut events);
        assert!(events.is_empty() && eq.placed[0].triggered.is_some(), "tripped, still in its grace period: {events:?}");
        step_n(&mut eq, &host, 3, &mut events);
        assert!(matches!(events[0], EquipEvent::Exploded { hit_enemy: true, .. }), "{events:?}");
        assert!(eq.placed.is_empty());
        assert!(host.destroyed.borrow().contains(&50));
    }

    /// A Sensor Grenade's timed fuse runs on where it lies (`timedDetonation`, t6mp
    /// `FUN_00607c00`): when it runs out it goes off as BO2's explode think does (`FUN_0060a480`):
    /// its own `projExplosionEffect` and no impact-table cell (`isLaserSensor`, `FUN_004c1590` case
    /// 0x4c), its radius damage, the blast's event, and the piece is gone (`G_FreeEntity`). A piece
    /// whose throw armed no fuse stays down.
    #[test]
    fn a_timed_piece_goes_off_where_it_lies_when_its_fuse_runs_out() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut piece = general(50, 0);
        piece.flags = objflags::NO_COLLIDE;
        host.add(piece);
        host.add(soldier(7, 1, 20.0));
        let sensor: &'static WeaponDef = Box::leak(Box::new(WeaponDef {
            timed_detonation: true,
            laser_sensor: true,
            explosion_fx: "ZHBO_BO2FX_sensor_scan".into(),
            explode_sound: "wpn_sensor_grenade_explode".into(),
            impact_type: 7,
            explosion_inner_damage: 15.0,
            explosion_outer_damage: 15.0,
            ..def("sensor_grenade_mp", Effect::None).clone()
        }));
        let mut eq = Equipment::default();
        let mut p = Placed::new(sensor, 0, 1, 50, Vec3::ZERO, 0.0, None);
        p.fuse = Some(0.5);
        eq.add(p);
        let mut untimed = Placed::new(def("tactical_insertion_mp", Effect::None), 0, 1, 0, Vec3::new(500.0, 0.0, 0.0), 0.0, None);
        untimed.fuse = None;
        eq.add(untimed);
        let mut events = Vec::new();
        let before = (0.5 / LOGIC_DT).round() as usize - 1;
        step_n(&mut eq, &host, before, &mut events);
        assert!(events.is_empty() && eq.placed.len() == 2, "still ticking: {events:?}");
        step_n(&mut eq, &host, 2, &mut events);
        assert!(
            matches!(events.as_slice(), [EquipEvent::Exploded { owner: 0, sound: "wpn_sensor_grenade_explode", impact_type: 7, hit_enemy: true, .. }]),
            "{events:?}"
        );
        assert_eq!(*host.played_fx.borrow(), vec!["ZHBO_BO2FX_sensor_scan".to_string()], "its own effect, no table cell");
        assert!(host.destroyed.borrow().contains(&50), "the piece is gone");
        assert_eq!(eq.placed.len(), 1, "the untimed piece stays down");
        assert!(host.damage_log.borrow().iter().any(|d| d.1 == 7), "its radius damage: {:?}", host.damage_log.borrow());
        // Its detonation put the enemy in its blast on the thrower's compass (event 0x4e).
        assert!(eq.marks.of(0).any(|m| m.live && m.target == 7), "{:?}", eq.marks);
        step_n(&mut eq, &host, 300, &mut events);
        assert_eq!(events.len(), 1, "it went off once");
    }

    /// One piece of `id` of player 0 (General 1) at the origin, its object 50, a soldier of player
    /// 1 (General 7) 20 units off, and General 2 of player 1 who breaks it.
    fn broken_scene(id: &str, effect: Effect) -> (FakeHost, Equipment) {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(general(2, 1));
        let mut piece = general(50, 0);
        piece.flags = objflags::NO_COLLIDE;
        host.add(piece);
        host.add(soldier(7, 1, 20.0));
        let mut eq = Equipment::default();
        eq.add(Placed::new(def(id, effect), 0, 1, 50, Vec3::ZERO, 0.0, None));
        (host, eq)
    }

    /// Every item, shot by an enemy, does what BO2's watcher for it does: C4, the Claymore and the
    /// Shock Charge go off as the shooter's (`weapondetonate`), the others are destroyed with their
    /// own effect (`bouncingbettydestroyed`, `trophysystemdetonate`, `sensorgrenadedestroyed`,
    /// `fizzle`). The mod used to break them all with one effect, and nothing went off.
    #[test]
    fn every_item_breaks_as_its_script_says() {
        for (id, effect, want) in [
            ("satchel_charge_mp", Effect::Explode, None),
            ("claymore_mp", Effect::Explode, None),
            ("proximity_grenade_mp", Effect::Stun, None),
            ("bouncingbetty_mp", Effect::Explode, Some(BETTY_DESTROYED_FX)),
            ("trophy_system_mp", Effect::None, Some(EQUIPMENT_LG_FX)),
            ("sensor_grenade_mp", Effect::None, Some(EQUIPMENT_FX)),
            ("tactical_insertion_mp", Effect::None, Some(TAC_INSERT_FIZZLE_FX)),
        ] {
            let (host, mut eq) = broken_scene(id, effect);
            let mut events = Vec::new();
            eq.destroy(&host, 0, Some((1, 2)), Effect::None, &mut events);
            assert!(eq.placed.is_empty() && host.destroyed.borrow().contains(&50), "{id}: gone");
            assert!(events.iter().any(|e| matches!(e, EquipEvent::Destroyed { owner: 0, by: Some(1), .. })), "{id}: the shooter destroyed it");
            let went_off = events.iter().find(|e| matches!(e, EquipEvent::Exploded { .. }));
            match want {
                None => {
                    assert!(matches!(went_off, Some(EquipEvent::Exploded { owner: 1, .. })), "{id}: goes off, the shooter's: {events:?}");
                    // Its blast is the shooter's: General 2 is the attacker, so the owner's side is hit.
                    if effect == Effect::Explode {
                        assert!(host.damage_log.borrow().iter().any(|d| d.0 == 2), "{id}: {:?}", host.damage_log.borrow());
                    }
                }
                Some(fx) => {
                    assert!(went_off.is_none(), "{id}: no blast");
                    assert_eq!(*host.played_fx.borrow(), vec![crate::names::bo2_fx(fx)], "{id}");
                }
            }
        }
    }

    /// An explosion breaks the enemy equipment it reaches, and a Claymore broken that way goes off
    /// in turn and breaks the next (the script's chained detonation, `waschained`); a concussion
    /// only stuns equipment and smoke leaves it, an EMP takes it out with its own effect.
    #[test]
    fn blasts_break_chain_and_spare_by_their_effect() {
        let (host, mut eq) = broken_scene("claymore_mp", Effect::Explode);
        let mut second = general(51, 0);
        second.flags = objflags::NO_COLLIDE;
        host.add(second);
        eq.add(Placed::new(def("claymore_mp", Effect::Explode), 0, 1, 51, Vec3::new(25.0, 0.0, 0.0), 0.0, None));
        let mut events = Vec::new();
        for spare in [Effect::Stun, Effect::Smoke] {
            eq.caught_in_blast(&host, 1, 2, Vec3::new(-5.0, 0.0, 0.0), 10.0, spare, &mut events);
            assert_eq!(eq.placed.len(), 2, "{spare:?} breaks nothing");
        }
        // A frag 5 units off the first: it goes off, its 30-unit blast takes the second 25 off.
        eq.caught_in_blast(&host, 1, 2, Vec3::new(-5.0, 0.0, 0.0), 10.0, Effect::Explode, &mut events);
        assert!(eq.placed.is_empty(), "the chain took both");
        assert_eq!(events.iter().filter(|e| matches!(e, EquipEvent::Exploded { .. })).count(), 2);
        // An EMP: its own effect, no blast.
        let (host, mut eq) = broken_scene("claymore_mp", Effect::Explode);
        let mut events = Vec::new();
        eq.caught_in_blast(&host, 1, 2, Vec3::ZERO, 10.0, Effect::Emp, &mut events);
        assert!(eq.placed.is_empty() && !events.iter().any(|e| matches!(e, EquipEvent::Exploded { .. })));
        assert_eq!(*host.played_fx.borrow(), vec![crate::names::bo2_fx(EMP_DESTROY_FX)]);
    }

    #[test]
    fn a_betty_waits_its_grace_then_jumps_and_goes_off_at_the_top() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut betty = general(50, 0);
        betty.flags = objflags::NO_COLLIDE;
        host.add(betty);
        let mut eq = Equipment::default();
        eq.add(Placed::new(def("bouncingbetty_mp", Effect::Explode), 0, 1, 50, Vec3::ZERO, 0.0, None));
        let mut events = Vec::new();
        step_n(&mut eq, &host, 40, &mut events);
        // An enemy behind it trips it too (`ignoredirection`), inside the weapon's blast radius.
        host.add(soldier(7, 1, -15.0));
        let t = EquipTuning::default();
        let frames = |s: f32| (s / LOGIC_DT).round() as usize;
        step_n(&mut eq, &host, frames(t.betty_grace) - 1, &mut events);
        assert!(events.is_empty() && eq.placed[0].triggered.is_some(), "{events:?}");
        step_n(&mut eq, &host, frames(t.betty_jump_time) - 1, &mut events);
        assert!(events.is_empty(), "still jumping: {events:?}");
        step_n(&mut eq, &host, 4, &mut events);
        match events.first() {
            Some(EquipEvent::Exploded { at, .. }) => assert!((at.z - t.betty_hop).abs() < 1e-3, "at the top of the jump: {at:?}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_delayexplosive_victim_waits_out_the_perk_time_too() {
        // `_weaponobjects.gsc:1548-1551`: the grace period, then `perk_delayExplosiveTime` for a
        // victim with `specialty_delayexplosive` (Engineer).
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut eq = Equipment::default();
        eq.add(Placed::new(def("proximity_grenade_mp", Effect::Stun), 0, 1, 0, Vec3::ZERO, 0.0, None));
        let mut projectiles = Vec::new();
        let mut events = Vec::new();
        for _ in 0..40 {
            eq.step(&host, &mut projectiles, &|_| true, &mut events);
        }
        host.add(soldier(7, 1, 5.0));
        eq.step(&host, &mut projectiles, &|id| id == 7, &mut events);
        let t = EquipTuning::default();
        let left = eq.placed[0].triggered.expect("tripped");
        assert!((left - (t.shock_grace + t.delay_explosive)).abs() < 1e-4, "{left}");
        assert_eq!(t.delay_explosive, 0.9);
    }

    #[test]
    fn the_trigger_radius_is_the_scripts_capped_by_the_blast() {
        let mut d = WeaponDef::default();
        let t = EquipTuning::default();
        assert_eq!(EquipTuning::trigger_range(t.betty_range, &d), t.betty_range, "no blast radius: the script's");
        d.explosion_radius = t.betty_range * 0.5;
        assert_eq!(EquipTuning::trigger_range(t.betty_range, &d), t.betty_range * 0.5);
        assert_eq!((t.betty_range, t.shock_range), (inches(192.0), inches(150.0)));
    }

    #[test]
    fn c4_waits_for_its_owner() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(soldier(7, 1, 5.0));
        let mut eq = Equipment::default();
        eq.add(Placed::new(def("satchel_charge_mp", Effect::Explode), 0, 1, 0, Vec3::ZERO, 0.0, None));
        let mut events = Vec::new();
        step_n(&mut eq, &host, 90, &mut events);
        assert!(events.is_empty());
        eq.detonate(&host, 1, &mut events);
        assert!(events.is_empty(), "someone else's detonator");
        eq.detonate(&host, 0, &mut events);
        assert!(matches!(events[0], EquipEvent::Exploded { kill: true, .. }), "{events:?}");
    }
}

#[cfg(test)]
mod sim_tests {
    use super::*;
    use crate::input::{buttons, FpsInput};
    use crate::sim::tests::{general, spawn, FakeHost};
    use crate::sim::{EventKind, Sim};
    use crate::weapons::{Class, Effect, Offhand, OffhandUse, Place};

    fn equipment(id: &str, place: Place) -> &'static WeaponDef {
        Box::leak(Box::new(WeaponDef {
            id: id.into(),
            class: Class::Equipment,
            effect: Effect::Explode,
            place,
            equipment_kind: match id {
                "claymore_mp" => Kind::Claymore,
                "bouncingbetty_mp" => Kind::Betty,
                "satchel_charge_mp" => Kind::C4,
                "proximity_grenade_mp" => Kind::Shock,
                "trophy_system_mp" => Kind::Trophy,
                "tactical_insertion_mp" => Kind::Insertion,
                "sensor_grenade_mp" => Kind::Sensor,
                _ => Kind::Other,
            },
            start_reserve: 2,
            explosion_radius: 30.0,
            explosion_inner_damage: 200.0,
            explosion_outer_damage: 100.0,
            projectile_speed: 60.0,
            sticky: true,
            // BO2's flags for these (`hasDetonator` on C4, `isHoldUseGrenade` on the Black Hat).
            has_detonator: id == "satchel_charge_mp",
            hold_use: place == Place::Hack,
            ..Default::default()
        }))
    }

    fn ready(sim: &mut Sim, host: &FakeHost) {
        spawn(sim, host, 0, 1);
        let wait = (sim.player(0).unwrap().arsenal.raise_left / crate::units::LOGIC_DT) as u32 + 2;
        for f in 0..wait {
            sim.input(0, 1, FpsInput::default());
            sim.step(host, f + 1);
        }
    }

    fn run(sim: &mut Sim, host: &FakeHost, input: FpsInput, n: u32) {
        for _ in 0..n {
            sim.input(0, 1, input);
            let f = sim.frame + 1;
            sim.step(host, f);
        }
    }

    #[test]
    fn the_black_hat_turns_enemy_equipment() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut claymore = general(60, 1);
        claymore.pos = Vec3::new(40.0, 0.0, 0.0);
        claymore.flags = objflags::NO_COLLIDE;
        host.add(claymore);
        let mut sim = Sim::new();
        ready(&mut sim, &host);
        sim.equipment.add(Placed::new(equipment("claymore_mp", Place::Plant), 1, 99, 60, Vec3::new(40.0, 0.0, 0.0), 0.0, None));
        // The install's `pda_hack_mp`: `iHoldFireTime` 250 ms, `iFireDelay` 130 ms.
        let mut def = equipment("pda_hack_mp", Place::Hack).clone();
        (def.timing.hold_fire_time, def.timing.fire_delay) = (0.25, 0.13);
        let black_hat: &'static WeaponDef = Box::leak(Box::new(def));
        sim.players[0].as_mut().unwrap().arsenal.tactical = Offhand { def: black_hat, count: 1 };
        // Aim at it (eye height 10, it sits on the ground) and hold the Black Hat up.
        let pitch = -(10.0f32 / 40.0).atan();
        run(&mut sim, &host, FpsInput { held: buttons::TACTICAL, pressed: buttons::TACTICAL, pitch, ..Default::default() }, 1);
        // BO2's hack on equipment: the pull-back (250 ms) outlasts the script's lock (0.1 s start +
        // 50 ms), then the tool's fire delay (130 ms): about 0.38 s from the pull-back.
        let t = EquipTuning::default();
        assert!(t.hack_lock_start + t.hack_lock_equipment < black_hat.timing.hold_fire_time);
        let expected = black_hat.timing.hold_fire_time + black_hat.timing.fire_delay;
        let mut hacked_at = None;
        for f in 0..40u32 {
            run(&mut sim, &host, FpsInput { held: buttons::TACTICAL, pitch, ..Default::default() }, 1);
            if hacked_at.is_none() && sim.events.iter().any(|e| e.kind == EventKind::Hacked) {
                hacked_at = Some(f);
            }
        }
        let at = hacked_at.expect("hacked") as f32 * crate::units::LOGIC_DT;
        assert!((at - expected).abs() <= 3.0 * crate::units::LOGIC_DT, "hacked {at} s after the pull-back, BO2 ~{expected} s");
        assert_eq!(sim.equipment.placed[0].owner, 0);
        assert_eq!(host.get(60).owner_player, 0, "its object is ours now");
        assert_eq!(sim.player(0).unwrap().arsenal.tactical.count, 0);
    }

    #[test]
    fn a_trophy_system_shoots_down_an_enemy_grenade() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(general(2, 1));
        let mut trophy = general(61, 0);
        trophy.flags = objflags::NO_COLLIDE;
        host.add(trophy);
        let mut eq = Equipment::default();
        eq.add(Placed::new(equipment("trophy_system_mp", Place::Plant), 0, 1, 61, Vec3::ZERO, 0.0, None));
        let frag = crate::weapons::weapon("frag_grenade_mp");
        let enemy_grenade = crate::projectiles::Projectile::launch(frag, 2, Vec3::new(30.0, 0.0, 5.0), Vec3::new(-1.0, 0.0, 0.0), 3.0, &host, 0.0);
        let mut projectiles = vec![(1usize, enemy_grenade)];
        let mut events = Vec::new();
        for _ in 0..40 {
            eq.step(&host, &mut projectiles, &|_| false, &mut events);
        }
        assert!(projectiles.is_empty(), "grenade shot down");
        assert!(events.iter().any(|e| matches!(e, EquipEvent::Intercepted { owner: 0, .. })));
        assert_eq!(eq.placed[0].charges, EquipTuning::default().trophy_charges - 1);
        // BO2's `projectileexplode`: its detonation effect where it shot, and a 128" blast there
        // from the trophy's owner (105 at the centre, 10 at the edge): General 2, the thrower,
        // stands outside it; a soldier of his beside the grenade takes it.
        assert!(host.played_fx.borrow().contains(&crate::names::bo2_fx(TROPHY_INTERCEPT_FX)), "{:?}", host.played_fx.borrow());
        let (radius, max, min) = EquipTuning::default().trophy_blast;
        assert!((radius - crate::units::inches(128.0)).abs() < 1e-4 && max == 105.0 && min == 10.0);
        // A second grenade with a soldier of player 1 beside where it is shot down.
        host.add(crate::sim::tests::soldier(8, 1, 28.0));
        let second = crate::projectiles::Projectile::launch(frag, 2, Vec3::new(30.0, 0.0, 5.0), Vec3::new(-1.0, 0.0, 0.0), 3.0, &host, 0.0);
        let mut projectiles = vec![(1usize, second)];
        for _ in 0..40 {
            eq.step(&host, &mut projectiles, &|_| false, &mut events);
        }
        assert!(host.damage_log.borrow().iter().any(|d| d.0 == 1 && d.1 == 8), "the trophy's blast is its owner's: {:?}", host.damage_log.borrow());
        // Spent: it blows itself apart (`trophysystemdetonate`).
        assert!(eq.placed.is_empty() && host.played_fx.borrow().contains(&crate::names::bo2_fx(EQUIPMENT_LG_FX)));
    }

    #[test]
    fn double_tapping_reload_sets_off_thrown_c4() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        ready(&mut sim, &host);
        let c4 = equipment("satchel_charge_mp", Place::Throw);
        sim.players[0].as_mut().unwrap().arsenal.lethal = Offhand { def: c4, count: 1 };
        run(&mut sim, &host, FpsInput { held: buttons::LETHAL, pressed: buttons::LETHAL, ..Default::default() }, 1);
        run(&mut sim, &host, FpsInput { held: buttons::LETHAL, ..Default::default() }, 12);
        run(&mut sim, &host, FpsInput::default(), 60);
        assert_eq!(sim.equipment.placed.len(), 1, "the C4 landed and stays");
        run(&mut sim, &host, FpsInput { pressed: buttons::RELOAD, ..Default::default() }, 1);
        run(&mut sim, &host, FpsInput::default(), 3);
        run(&mut sim, &host, FpsInput { pressed: buttons::RELOAD, ..Default::default() }, 1);
        assert!(sim.equipment.placed.is_empty(), "detonated");
        assert!(sim.events.iter().any(|e| e.kind == EventKind::Detonated));
    }

    /// A class built in the editor with a claymore and a Trophy System (BO2 lists both by their
    /// statstable slot) keeps them through the profile repair, spawns with both and plants both.
    #[test]
    fn an_editor_class_with_placed_equipment_spawns_with_it_and_plants_it() {
        let d = crate::classmenu::Draft { lethal: "claymore_mp".into(), tactical: "trophy_system_mp".into(), ..Default::default() };
        let mut c = d.to_custom();
        assert!(crate::classrepair::repair(&mut c).is_empty(), "the repair keeps them");
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        spawn(&mut sim, &host, 0, 1);
        sim.players[0].as_mut().unwrap().arsenal = crate::weapons::Arsenal::with_loadout(c.loadout());
        ready(&mut sim, &host);
        let a = &sim.player(0).unwrap().arsenal;
        assert_eq!((a.lethal.def.id.as_str(), a.tactical.def.id.as_str()), ("claymore_mp", "trophy_system_mp"));
        for button in [buttons::LETHAL, buttons::TACTICAL] {
            run(&mut sim, &host, FpsInput { held: button, pressed: button, ..Default::default() }, 1);
            run(&mut sim, &host, FpsInput::default(), 120);
        }
        let planted: Vec<Kind> = sim.events.iter().filter_map(|e| if let EventKind::Planted { kind } = e.kind { Some(kind) } else { None }).collect();
        assert_eq!(planted, vec![Kind::Claymore, Kind::Trophy]);
        assert_eq!(sim.equipment.placed.len(), 2);
    }

    /// BO2's offhand states for equipment (Ghidra `FUN_008e2710`..`FUN_008dcbe0`): the claymore's
    /// own viewmodel pulls back for its `hold_fire_time`, is held while the button is, and on the
    /// release is set down `fire_delay` later; after its `fire_time` the gun comes back up in its
    /// quick raise. The same input replays to the same frames.
    #[test]
    fn a_claymore_pulls_back_is_set_down_on_the_release_and_the_gun_comes_back() {
        let trace = || {
            let host = FakeHost::default();
            host.add(general(1, 0));
            let mut sim = Sim::new();
            ready(&mut sim, &host);
            let claymore = crate::weapons::weapon("claymore_mp");
            sim.players[0].as_mut().unwrap().arsenal.lethal = Offhand { def: claymore, count: 1 };
            let mut log = Vec::new();
            for f in 0..90 {
                let input = if f < 20 { FpsInput { held: buttons::LETHAL, pressed: if f == 0 { buttons::LETHAL } else { 0 }, ..Default::default() } } else { FpsInput::default() };
                run(&mut sim, &host, input, 1);
                let a = &sim.player(0).unwrap().arsenal;
                let planted = sim.events.iter().any(|e| matches!(e.kind, EventKind::Planted { .. }));
                log.push((a.offhand, planted, sim.equipment.placed.len()));
            }
            (claymore, log)
        };
        let (claymore, log) = trace();
        assert_eq!(log, trace().1, "deterministic");
        // Held up (its own viewmodel's hold) the whole time the button is down.
        assert!(log[..20].iter().all(|(u, planted, _)| matches!(u, Some(OffhandUse::Holding { .. })) && !planted));
        // Set down `fire_delay` after the release.
        let at = log.iter().position(|(_, planted, _)| *planted).expect("planted");
        let delay = (claymore.timing.fire_delay / crate::units::LOGIC_DT).ceil() as usize;
        assert!((at as i64 - (20 + delay) as i64).abs() <= 1, "planted at frame {at}, want {}", 20 + delay);
        assert_eq!(log[at].2, 1);
        // Then the gun's quick raise, then nothing in hand.
        assert!(log.iter().any(|(u, ..)| matches!(u, Some(OffhandUse::Raising { .. }))));
        assert!(log.last().unwrap().0.is_none(), "back to the gun");
        assert_eq!(sim_count(&log), 1);
    }

    fn sim_count(log: &[(Option<OffhandUse>, bool, usize)]) -> usize {
        log.last().map_or(0, |l| l.2)
    }

    /// C4 (`hasDetonator`): with none left in hand, the lethal button fires the detonator: the
    /// charges go off `detonate_delay` after the press and the gun is simply back after
    /// `detonate_time`, no raise (`FUN_008dcd20`).
    #[test]
    fn the_c4_button_with_none_left_is_the_detonator() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        ready(&mut sim, &host);
        let c4 = crate::weapons::weapon("satchel_charge_mp");
        assert!(c4.has_detonator);
        sim.players[0].as_mut().unwrap().arsenal.lethal = Offhand { def: c4, count: 1 };
        // Throw the one in hand.
        run(&mut sim, &host, FpsInput { held: buttons::LETHAL, pressed: buttons::LETHAL, ..Default::default() }, 1);
        run(&mut sim, &host, FpsInput { held: buttons::LETHAL, ..Default::default() }, 5);
        run(&mut sim, &host, FpsInput::default(), 90);
        assert_eq!(sim.equipment.placed.len(), 1, "the C4 landed and stays");
        assert_eq!(sim.player(0).unwrap().arsenal.lethal.count, 0);
        // The button again: the detonator.
        run(&mut sim, &host, FpsInput { held: buttons::LETHAL, pressed: buttons::LETHAL, ..Default::default() }, 1);
        assert!(matches!(sim.player(0).unwrap().arsenal.offhand, Some(OffhandUse::Detonating { .. })));
        let delay = (c4.detonate_delay / crate::units::LOGIC_DT).ceil() as usize;
        let mut gone_at = None;
        for f in 1..60 {
            run(&mut sim, &host, FpsInput::default(), 1);
            if gone_at.is_none() && sim.equipment.placed.is_empty() {
                gone_at = Some(f);
            }
        }
        let gone_at = gone_at.expect("detonated");
        assert!((gone_at as i64 - delay as i64).abs() <= 1, "went off at frame {gone_at}, want {delay}");
        assert!(sim.events.iter().any(|e| e.kind == EventKind::Detonated));
        assert!(sim.player(0).unwrap().arsenal.offhand.is_none());
    }
}
