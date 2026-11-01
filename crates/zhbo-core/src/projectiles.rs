//! Thrown and launched things: grenades (bouncing, sticking, cooking), launcher rockets, crossbow
//! bolts, ballistic knives and combat axes. Each flies in the simulation; the host shows it as a
//! Zero Hour object (the converted BO2 model) and plays its effect when it goes off.
//!
//! Runs inside the lockstep logic: no randomness, fixed step.

use crate::host::{damage_kind, disable_kind, objflags, LogicHost, ObjInfo};
use crate::impacts::Surface;
use crate::math::Vec3;
use crate::units::{inches, BO2_TO_ZH_DAMAGE, LOGIC_DT};
use crate::weapons::{Effect, WeaponDef};

/// BO2's `g_gravity` (800 in/s^2).
pub const GRAVITY: f32 = inches(800.0);
/// Bounce: vertical speed kept, horizontal speed kept, below which a grenade comes to rest.
const BOUNCE_UP: f32 = 0.3;
const BOUNCE_ALONG: f32 = 0.6;
const REST_SPEED: f32 = 3.0;
/// How close a flying projectile must pass an object to hit it.
const HIT_RADIUS: f32 = 2.5;
/// Seconds before an impact-fuse projectile arms (it can't hit its thrower).
const ARM_TIME: f32 = 0.08;
/// Stun and EMP lengths (seconds) at the blast centre and at the edge.
const STUN_SECONDS: (f32, f32) = (5.0, 2.0);
const EMP_SECONDS: f32 = 10.0;
/// A smoke grenade's screen: how long it stays up and how wide it is (BO2's Willy Pete).
pub const SMOKE_TIME: f32 = 15.0;
pub const SMOKE_RADIUS: f32 = inches(220.0);
/// Zero Hour FX for a blast when neither the impact table nor the weapon names one (an archive
/// without the converted table, or a mod-only def).
const DEFAULT_EXPLOSION_FX: &str = "FX_BoobyTrapExplosion";

#[derive(Clone, Debug, PartialEq)]
pub struct Projectile {
    pub def: &'static WeaponDef,
    pub owner: u32,
    pub pos: Vec3,
    pub vel: Vec3,
    /// Seconds until it goes off (`INFINITY` for impact fuses).
    pub fuse: f32,
    pub age: f32,
    /// Host object showing it (0 if none).
    pub object: u32,
    /// Stuck to the ground or to an object (sticky grenades, bolts).
    pub stuck_to: Option<u32>,
    pub resting: bool,
    /// The thrower's own share of this blast's damage (`flakjacket`: `perk_flakJacket` percent,
    /// `_class.gsc:1023-1032`), 1 without it.
    pub self_blast_scale: f32,
    /// The thrower's own share of this stun (`stunprotection`: x0.1, `_weapons.gsc:1375-1376`).
    pub self_stun_scale: f32,
    /// Becomes placed equipment where it comes to rest or sticks (C4, Shock Charge, Sensor
    /// Grenade) instead of going off.
    pub lands: bool,
    /// Path length flown so far (BO2's `G_RunMissile` sums each frame's move), against the def's
    /// `activate_dist`.
    pub travelled: f32,
    /// Bounced this step: where, and off what (the client's bounce sound). Cleared every step.
    pub bounced: Option<(Vec3, Surface)>,
}

/// What happened this step, for the client (sounds, hit markers).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    /// Went off on `surface` (what it hit directly, else the ground under it).
    Exploded { at: Vec3, hit_enemy: bool, kill: bool, surface: Surface },
    Hit { at: Vec3, kill: bool },
    /// Struck `surface` before arming (`WeaponDef::activate_dist`): no blast, only the direct hit's
    /// impact damage (BO2's `iDamage`).
    Dud { at: Vec3, hit_enemy: bool, kill: bool, surface: Surface },
    /// Came to rest (or stuck to `stuck_to`) as equipment; its object stays.
    Landed { at: Vec3, stuck_to: Option<u32> },
}

impl Projectile {
    /// A projectile leaving `from` along `dir` (unit vector) for `owner`.
    pub fn launch(def: &'static WeaponDef, owner: u32, from: Vec3, dir: Vec3, fuse_left: f32, host: &dyn LogicHost, yaw: f32) -> Projectile {
        let speed = if def.projectile_speed > 0.0 { def.projectile_speed } else { inches(800.0) };
        let vel = dir * speed + Vec3::new(0.0, 0.0, def.projectile_speed_up);
        let object = if def.projectile_object.is_empty() { 0 } else { host.spawn(&def.projectile_object, owner, from, yaw) };
        let lands = crate::equipment::Kind::of(def) != crate::equipment::Kind::Other;
        let fuse = if lands || def.impact_explode || def.effect == Effect::Impact || def.fuse_time <= 0.0 { f32::INFINITY } else { fuse_left };
        Projectile { def, owner, pos: from, vel, fuse, age: 0.0, object, stuck_to: None, resting: false, self_blast_scale: 1.0, self_stun_scale: 1.0, lands, travelled: 0.0, bounced: None }
    }

    /// Advance one logic frame with no shields about (see [`Projectile::step_shielded`]).
    pub fn step(&mut self, host: &dyn LogicHost) -> Option<Outcome> {
        self.step_shielded(host, &[])
    }

    /// Advance one logic frame. Returns `Some` when it is finished (went off or hit). An enemy's
    /// Assault Shield in the way stops it (`_globallogic_player.gsc:840`: location `riotshield`):
    /// an explosive goes off on it (its impact damage is the script's 0, its splash the blast's,
    /// which the shield protects against), anything else falls dead.
    pub fn step_shielded(&mut self, host: &dyn LogicHost, shields: &[crate::shield::Bearer]) -> Option<Outcome> {
        let dt = LOGIC_DT;
        self.age += dt;
        self.bounced = None;
        self.fuse -= dt;
        if self.fuse <= 0.0 {
            return Some(self.go_off(host, None, shields));
        }
        if self.lands && (self.resting || self.stuck_to.is_some()) {
            return Some(Outcome::Landed { at: self.pos, stuck_to: self.stuck_to });
        }
        if let Some(id) = self.stuck_to {
            // Riding on a stuck object (Semtex on a tank).
            if let Some(o) = host.object(id, self.owner).filter(|o| !o.has(objflags::DEAD)) {
                self.pos = Vec3::new(o.pos.x, o.pos.y, o.pos.z + o.height * 0.5);
            }
        } else if !self.resting {
            if self.def.projectile_gravity {
                self.vel.z -= GRAVITY * dt;
            }
            let next = self.pos + self.vel * dt;
            // Not armed yet when this move ends short of the activation distance (BO2 compares the
            // path summed so far, this move included, `FUN_00604280`).
            self.travelled += (next - self.pos).length();
            let dud = self.def.activate_dist > 0.0 && self.travelled < self.def.activate_dist;
            // An enemy's shield along the way (what sticks or lands keeps its own path).
            if self.age > ARM_TIME && !self.def.sticky && !self.lands {
                let seg = next - self.pos;
                let len = seg.length();
                // A deployed shield (`general` 0) stops everyone's; a carried one an enemy's.
                let enemy = |b: &crate::shield::Bearer| b.general == 0 || b.general != self.owner && host.object(b.general, self.owner).is_some_and(|o| !o.has(objflags::ALLY) && !o.has(objflags::DEAD));
                if let Some((_, t)) = crate::shield::blocking(shields, &enemy, self.pos, seg.normalized_or_zero(), len, len, None) {
                    self.pos += seg.normalized_or_zero() * t;
                    if self.def.effect == Effect::Explode {
                        return Some(self.go_off(host, None, shields));
                    }
                    self.remove_object(host);
                    return Some(Outcome::Hit { at: self.pos, kill: false });
                }
            }
            // Objects along the way.
            if self.age > ARM_TIME {
                if let Some(o) = self.hit_object(host, self.pos, next) {
                    self.pos = next;
                    if self.def.sticky {
                        self.stuck_to = Some(o.id);
                    } else if self.def.impact_explode && dud {
                        return Some(self.dud(host, Some(o)));
                    } else if self.def.impact_explode {
                        return Some(self.go_off(host, Some(o), shields));
                    } else if self.def.effect == Effect::Impact {
                        return Some(self.impact(host, o));
                    } else {
                        // Grenades bounce off.
                        self.vel = Vec3::new(-self.vel.x * BOUNCE_ALONG, -self.vel.y * BOUNCE_ALONG, self.vel.z * BOUNCE_UP);
                        self.bounce(host, Surface::of_object(Some(&o)));
                    }
                }
            }
            let ground = host.ground_height(next.x, next.y);
            if next.z <= ground {
                self.pos = Vec3::new(next.x, next.y, ground);
                if self.def.impact_explode && dud {
                    return Some(self.dud(host, None));
                }
                if self.def.impact_explode {
                    return Some(self.go_off(host, None, shields));
                }
                if self.def.sticky || self.def.effect == Effect::Impact {
                    self.resting = true;
                    self.vel = Vec3::ZERO;
                    if self.def.effect == Effect::Impact {
                        return Some(Outcome::Hit { at: self.pos, kill: false });
                    }
                } else {
                    self.vel = Vec3::new(self.vel.x * BOUNCE_ALONG, self.vel.y * BOUNCE_ALONG, -self.vel.z * BOUNCE_UP);
                    self.bounce(host, host.surface_at(self.pos.x, self.pos.y));
                    if self.vel.length() < REST_SPEED {
                        self.resting = true;
                        self.vel = Vec3::ZERO;
                    }
                }
            } else {
                self.pos = next;
            }
            if self.age > 30.0 {
                return Some(Outcome::Hit { at: self.pos, kill: false });
            }
        }
        if self.object != 0 {
            let yaw = self.vel.y.atan2(self.vel.x);
            host.set_pose(self.object, self.pos, yaw, false);
        }
        None
    }

    fn hit_object(&self, host: &dyn LogicHost, from: Vec3, to: Vec3) -> Option<ObjInfo> {
        let mid = (from + to) * 0.5;
        let reach = (to - from).length() * 0.5 + HIT_RADIUS + 12.0;
        let mut objs = Vec::new();
        host.query(mid, reach, self.owner, &mut objs);
        objs.into_iter()
            .filter(|o| o.id != self.owner && o.id != self.object && !o.has(objflags::DEAD) && !o.has(objflags::NO_COLLIDE) && !o.has(objflags::ALLY))
            .filter(|o| {
                // Closest approach of the segment to the object's axis, within its radius and height.
                let r = if o.has(objflags::INFANTRY) { 1.5 } else { o.major_radius } + HIT_RADIUS * 0.5;
                let seg = to - from;
                let len2 = (seg.x * seg.x + seg.y * seg.y).max(1e-6);
                let t = (((o.pos.x - from.x) * seg.x + (o.pos.y - from.y) * seg.y) / len2).clamp(0.0, 1.0);
                let p = from + seg * t;
                let (dx, dy) = (p.x - o.pos.x, p.y - o.pos.y);
                dx * dx + dy * dy <= r * r && p.z >= o.pos.z - 1.0 && p.z <= o.pos.z + o.height.max(2.0) + 1.0
            })
            .min_by_key(|o| o.id)
    }

    fn impact(&mut self, host: &dyn LogicHost, o: ObjInfo) -> Outcome {
        self.remove_object(host);
        let amount = self.def.damage[0] * BO2_TO_ZH_DAMAGE;
        let kill = host.damage(self.owner, o.id, amount, damage_kind::SMALL_ARMS);
        Outcome::Hit { at: self.pos, kill }
    }

    /// A bounce off `surface`: the impact table's `grenade_bounce` cell for it, when BO2 has one
    /// (t6mp `FUN_004c1590` event 0x47), here; the def's sound by the client ([`Self::bounced`]).
    fn bounce(&mut self, host: &dyn LogicHost, surface: Surface) {
        if let Some(fx) = crate::impacts::fx_cell(crate::impacts::GRENADE_BOUNCE, surface) {
            host.fx(&fx, self.pos);
        }
        self.bounced = Some((self.pos, surface));
    }

    /// Struck before arming (BO2's `G_RunMissile` turns the explosion into `dud`, `FUN_00608c10`):
    /// the direct hit takes the impact damage (`iDamage`), nothing goes off, and the event plays
    /// the `projectile_dud` row's cell for the surface and the def's `projDudEffect` (event 0x51).
    fn dud(&mut self, host: &dyn LogicHost, direct: Option<ObjInfo>) -> Outcome {
        self.remove_object(host);
        let surface = blast_surface(host, self.pos, direct.as_ref());
        for fx in dud_fx(self.def, surface).iter().flatten() {
            host.fx(fx, self.pos);
        }
        let (mut hit_enemy, mut kill) = (false, false);
        if let Some(o) = direct {
            let amount = self.def.damage[0] * BO2_TO_ZH_DAMAGE;
            if amount > 0.0 {
                kill = host.damage(self.owner, o.id, amount, damage_kind::SMALL_ARMS);
                hit_enemy = true;
            }
        }
        Outcome::Dud { at: self.pos, hit_enemy, kill, surface }
    }

    /// The blast (or stun, EMP, smoke).
    fn go_off(&mut self, host: &dyn LogicHost, direct: Option<ObjInfo>, shields: &[crate::shield::Bearer]) -> Outcome {
        self.remove_object(host);
        let b = blast(host, self.def, self.owner, self.pos, direct, self.self_blast_scale, self.self_stun_scale, shields);
        Outcome::Exploded { at: self.pos, hit_enemy: b.hit_enemy, kill: b.kill, surface: b.surface }
    }

    /// Stop it where it is (a Trophy System shot it down).
    pub fn intercept(&mut self, host: &dyn LogicHost) {
        self.remove_object(host);
    }

    fn remove_object(&mut self, host: &dyn LogicHost) {
        if self.object != 0 {
            host.destroy(self.object);
            self.object = 0;
        }
    }
}

/// What a blast did: whether it hit or killed an enemy, and the surface it went off on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blast {
    pub hit_enemy: bool,
    pub kill: bool,
    pub surface: Surface,
}

/// The surface a blast at `at` goes off on: what it hit directly, else the ground there (BO2 passes
/// the projectile's hit surface to its impact routine, `FUN_004c0ac0` -> `FUN_00533210`).
/// **Presentation only**, like every surface (docs/CONTRACT.md).
pub fn blast_surface(host: &dyn LogicHost, at: Vec3, direct: Option<&ObjInfo>) -> Surface {
    match direct {
        Some(o) => Surface::of_object(Some(o)),
        None => host.surface_at(at.x, at.y),
    }
}

/// The effects BO2 plays for `def` going off on `surface`: the impact table's cell for the def's
/// `impactType` and the surface, then the def's own `projExplosionEffect` on top
/// (`FUN_004c0ac0`). Neither -> the stand-in (no converted table, or a mod-only def).
pub fn blast_fx(def: &WeaponDef, surface: Surface) -> (Option<String>, Option<&str>) {
    let table = crate::impacts::fx_name(def.impact_type, surface, false, false);
    let own = (!def.explosion_fx.is_empty()).then_some(def.explosion_fx.as_str());
    if table.is_none() && own.is_none() {
        return (None, Some(DEFAULT_EXPLOSION_FX));
    }
    (table, own)
}

/// The effects BO2 plays for `def` striking `surface` as a dud (event 0x51): the impact table's
/// `projectile_dud` cell for the surface (exact, when set), then the def's own `projDudEffect`.
pub fn dud_fx(def: &WeaponDef, surface: Surface) -> [Option<String>; 2] {
    let table = crate::impacts::fx_cell(crate::impacts::PROJECTILE_DUD, surface);
    [table, (!def.dud_fx.is_empty()).then(|| def.dud_fx.clone())]
}

/// `def` going off at `at` for `owner` (a General's id): explosion damage, stun, EMP or smoke over
/// its radius, and its effects. `direct` took a direct hit. A General with a shield keeps
/// `1 − protection` of the damage ([`crate::shield::Bearer::blast_protection`]).
#[allow(clippy::too_many_arguments)]
pub fn blast(host: &dyn LogicHost, def: &WeaponDef, owner: u32, at: Vec3, direct: Option<ObjInfo>, self_blast_scale: f32, self_stun_scale: f32, shields: &[crate::shield::Bearer]) -> Blast {
    let tuning = crate::tuning::get();
    let d = def;
    let surface = blast_surface(host, at, direct.as_ref());
    if d.effect != Effect::None {
        let (table, own) = blast_fx(d, surface);
        for fx in table.as_deref().into_iter().chain(own) {
            host.fx(fx, at);
        }
    }
    let mut objs = Vec::new();
    host.query(at, d.explosion_radius.max(1.0) + 12.0, owner, &mut objs);
    objs.sort_by_key(|o| o.id);
    let (mut hit_enemy, mut kill) = (false, false);
    for o in objs.iter().filter(|o| !o.has(objflags::DEAD) && !o.has(objflags::NO_COLLIDE)) {
        let ally = o.has(objflags::ALLY) && o.id != owner;
        if ally {
            continue; // no friendly fire, as in BO2's core modes
        }
        // A deployed shield's object: the sim wears the shield down itself
        // (`Sim::deployed_caught_in_blast`).
        if shields.iter().any(|b| b.object != 0 && b.object == o.id) {
            continue;
        }
        let centre = Vec3::new(o.pos.x, o.pos.y, o.pos.z + o.height * 0.5);
        let dist = ((centre - at).length() - if o.has(objflags::INFANTRY) { 0.0 } else { o.major_radius }).max(0.0);
        let direct_hit = direct.is_some_and(|x| x.id == o.id);
        if dist > d.explosion_radius && !direct_hit {
            continue;
        }
        let own = o.id == owner;
        match d.effect {
            Effect::Explode => {
                let mut amount = d.explosion_damage_at(dist) * BO2_TO_ZH_DAMAGE;
                if direct_hit {
                    amount = amount.max(d.damage[0] * BO2_TO_ZH_DAMAGE);
                }
                if own {
                    amount *= self_blast_scale;
                }
                if let Some(b) = shields.iter().find(|b| b.general == o.id) {
                    amount *= 1.0 - b.blast_protection(at, &tuning);
                }
                if amount > 0.0 {
                    let k = host.damage(owner, o.id, amount, damage_kind::EXPLOSION);
                    if !own {
                        hit_enemy = true;
                        kill |= k;
                    }
                }
            }
            Effect::Stun => {
                let f = (dist / d.explosion_radius.max(1.0)).clamp(0.0, 1.0);
                let mut secs = STUN_SECONDS.0 + (STUN_SECONDS.1 - STUN_SECONDS.0) * f;
                if own {
                    secs *= self_stun_scale;
                }
                if !own {
                    host.disable(o.id, disable_kind::PARALYZED, secs);
                    // The Shock Charge also hurts.
                    let amount = d.explosion_damage_at(dist) * BO2_TO_ZH_DAMAGE;
                    if amount > 0.0 && d.explosion_inner_damage > 10.0 {
                        kill |= host.damage(owner, o.id, amount, damage_kind::EXPLOSION);
                    }
                    hit_enemy = true;
                }
            }
            Effect::Emp => {
                if !own && !o.has(objflags::INFANTRY) {
                    host.disable(o.id, disable_kind::EMP, EMP_SECONDS);
                    hit_enemy = true;
                }
            }
            Effect::Smoke | Effect::None | Effect::Impact => {}
        }
    }
    Blast { hit_enemy, kill, surface }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::tests::{general, soldier, FakeHost};
    use crate::weapons::weapon;

    #[test]
    fn a_frag_flies_bounces_and_explodes_on_its_fuse() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(soldier(5, 1, 30.0));
        let frag = weapon("frag_grenade_mp");
        let mut p = Projectile::launch(frag, 1, Vec3::new(0.0, 0.0, 10.0), Vec3::new(1.0, 0.0, 0.2).normalized_or_zero(), frag.fuse_time, &host, 0.0);
        let mut t = 0.0;
        let outcome = loop {
            if let Some(o) = p.step(&host) {
                break o;
            }
            t += LOGIC_DT;
            assert!(t < 10.0);
        };
        assert!((t - frag.fuse_time).abs() < 0.1, "went off at {t}");
        assert!(matches!(outcome, Outcome::Exploded { .. }));
        assert!(p.pos.x > 5.0, "it was thrown forward: {:?}", p.pos);
    }

    #[test]
    fn concussion_stuns_enemies_in_the_radius() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(soldier(5, 1, 3.0));
        let conc = weapon("concussion_grenade_mp");
        let mut p = Projectile::launch(conc, 1, Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0), 0.01, &host, 0.0);
        p.vel = Vec3::ZERO;
        p.fuse = 0.0;
        p.step(&host);
        assert!(host.disabled.borrow().iter().any(|(id, _, secs)| *id == 5 && *secs > 1.0));
    }
}
