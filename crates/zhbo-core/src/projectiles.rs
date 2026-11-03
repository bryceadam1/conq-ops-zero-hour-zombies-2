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

/// Gravity on thrown and launched things: the tuning pack's `bg_gravity` (G2; the same value the
/// player's movement falls by), Zero Hour units/s^2.
pub fn gravity() -> f32 {
    inches(crate::tuning::get().bg_gravity)
}

/// What a projectile flies by, from the tuning pack, fixed at launch (the sim is lockstep: the
/// pack is the same on every client).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flight {
    /// `bg_gravity`, Zero Hour units/s^2.
    pub gravity: f32,
    /// On a bounce: share of the speed into the surface and along it kept; below `rest_speed` it
    /// comes to rest.
    pub bounce_up: f32,
    pub bounce_along: f32,
    pub rest_speed: f32,
}

crate::statehash::state_hash!(struct Flight { gravity, bounce_up, bounce_along, rest_speed });

impl Flight {
    pub fn from_tuning(t: &crate::tuning::Tuning) -> Self {
        Flight { gravity: inches(t.bg_gravity), bounce_up: t.projectile_bounce, bounce_along: t.projectile_bounce_along, rest_speed: t.projectile_rest_speed }
    }
}
/// How close a flying projectile must pass an object to hit it.
const HIT_RADIUS: f32 = 2.5;
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
    /// The thrower's own share of this blast's damage (1: BO2's perks leave a player's own blast
    /// alone, `Arsenal::self_harm_scales`; others' Flak Jacket is `sim::guard`).
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
    /// Struck an enemy for its impact damage this step and flew on (stuck or bounced): whether
    /// that killed (the client's hit marker). Cleared every step.
    pub struck: Option<bool>,
    /// The object it last struck and bounced off: not struck again until it strikes something
    /// else (a bounce leaves it inside the object's cylinder for a step).
    pub last_struck: Option<u32>,
    /// Gravity and bounce, from the tuning pack at launch.
    pub flight: Flight,
}

crate::statehash::state_hash!(struct Projectile {
    def, owner, pos, vel, fuse, age, object, stuck_to, resting, self_blast_scale, self_stun_scale, lands, travelled,
    bounced, struck, last_struck, flight,
});

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
    /// Struck something and becomes the def's spawned grenade ([`WeaponDef::spawns`]) there,
    /// stuck to `stuck_to` (none: the ground or a shield); the direct hit already took the impact
    /// damage.
    Spawned { at: Vec3, stuck_to: Option<u32>, hit_enemy: bool, kill: bool },
}

impl Projectile {
    /// A projectile leaving `from` along `dir` (unit vector) for `owner`.
    pub fn launch(def: &'static WeaponDef, owner: u32, from: Vec3, dir: Vec3, fuse_left: f32, host: &dyn LogicHost, yaw: f32) -> Projectile {
        let speed = if def.projectile_speed > 0.0 { def.projectile_speed } else { inches(800.0) };
        let vel = dir * speed + Vec3::new(0.0, 0.0, def.projectile_speed_up);
        let object = if def.projectile_object.is_empty() { 0 } else { host.spawn(&def.projectile_object, owner, from, yaw) };
        let lands = crate::equipment::Kind::of(def) != crate::equipment::Kind::Other;
        let fuse = if lands || def.impact_explode || def.effect == Effect::Impact || def.fuse_time <= 0.0 { f32::INFINITY } else { fuse_left };
        let flight = Flight::from_tuning(&crate::tuning::get());
        Projectile { def, owner, pos: from, vel, fuse, age: 0.0, object, stuck_to: None, resting: false, self_blast_scale: 1.0, self_stun_scale: 1.0, lands, travelled: 0.0, bounced: None, struck: None, last_struck: None, flight }
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
        self.struck = None;
        self.fuse -= dt;
        if self.fuse <= 0.0 && self.fuse_may_fire() {
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
                self.vel.z -= self.flight.gravity * dt;
            }
            let mut next = self.pos + self.vel * dt;
            // A move into the ground ends where it meets it: nothing beyond (or under) is struck.
            let ground = host.ground_height(next.x, next.y);
            if next.z <= ground && self.pos.z > next.z {
                let t = ((self.pos.z - ground) / (self.pos.z - next.z)).clamp(0.0, 1.0);
                next = self.pos + (next - self.pos) * t;
                next.z = next.z.max(ground);
            }
            // Not armed yet when this move ends short of the activation distance (BO2 compares the
            // path summed so far, this move included, `FUN_00604280`).
            self.travelled += (next - self.pos).length();
            let dud = self.def.activate_dist > 0.0 && self.travelled < self.def.activate_dist;
            // An enemy's shield along the way (what sticks or lands keeps its own path; a bolt that
            // spawns its grenade strikes the shield like anything else).
            let spawns = !self.def.spawns.is_empty();
            if (spawns || !self.def.sticky) && !self.lands {
                let seg = next - self.pos;
                let len = seg.length();
                // A deployed shield (`general` 0) stops everyone's; a carried one an enemy's.
                let enemy = |b: &crate::shield::Bearer| b.general == 0 || b.general != self.owner && host.object(b.general, self.owner).is_some_and(|o| !o.has(objflags::ALLY) && !o.has(objflags::DEAD));
                if let Some((_, t)) = crate::shield::blocking(shields, &enemy, self.pos, seg.normalized_or_zero(), len, len, None) {
                    self.pos += seg.normalized_or_zero() * t;
                    if spawns {
                        self.remove_object(host);
                        return Some(Outcome::Spawned { at: self.pos, stuck_to: None, hit_enemy: false, kill: false });
                    }
                    if self.def.effect == Effect::Explode {
                        return Some(self.go_off(host, None, shields));
                    }
                    self.remove_object(host);
                    return Some(Outcome::Hit { at: self.pos, kill: false });
                }
            }
            // Objects along the way (never its thrower, as BO2's missile skips its owner).
            if let Some((o, t)) = self.hit_object(host, self.pos, next) {
                // Where along this move it met him, not the move's end (a bolt covers 25 units
                // a step).
                let at = self.pos + (next - self.pos) * t;
                self.travelled -= (next - at).length();
                let dud = self.def.activate_dist > 0.0 && self.travelled < self.def.activate_dist;
                next = at;
                self.pos = at;
                if spawns {
                    return Some(self.spawn(host, Some(o)));
                } else if self.def.impact_explode && dud {
                    return Some(self.dud(host, Some(o)));
                } else if self.def.impact_explode {
                    return Some(self.go_off(host, Some(o), shields));
                } else if self.def.effect == Effect::Impact {
                    return Some(self.impact(host, o));
                }
                // BO2 strikes every entity a missile hits for its `iDamage` before it sticks or
                // bounces (t6mp `FUN_00608c10`): a Semtex stuck to a player, a frag or
                // flashbang off his head.
                self.strike(host, &o);
                if self.def.sticky {
                    self.stuck_to = Some(o.id);
                } else {
                    // Grenades bounce off.
                    self.last_struck = Some(o.id);
                    let surface = Surface::of_object(Some(&o));
                    let (along, up) = self.bounce_scales(surface);
                    self.vel = Vec3::new(-self.vel.x * along, -self.vel.y * along, self.vel.z * up);
                    self.bounce(host, surface);
                }
            }
            let ground = host.ground_height(next.x, next.y);
            if next.z <= ground {
                self.pos = Vec3::new(next.x, next.y, ground);
                self.last_struck = None;
                if spawns {
                    return Some(self.spawn(host, None));
                }
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
                    let surface = host.surface_at(self.pos.x, self.pos.y);
                    let (along, up) = self.bounce_scales(surface);
                    self.vel = Vec3::new(self.vel.x * along, self.vel.y * along, -self.vel.z * up);
                    self.bounce(host, surface);
                    if self.vel.length() < self.flight.rest_speed {
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
            let yaw = crate::math::det::atan2(self.vel.y, self.vel.x);
            host.set_pose(self.object, self.pos, yaw, false);
        }
        None
    }

    /// The first object the move `from`→`to` meets, and how far along the move (0..1) it does.
    fn hit_object(&self, host: &dyn LogicHost, from: Vec3, to: Vec3) -> Option<(ObjInfo, f32)> {
        let mid = (from + to) * 0.5;
        let reach = (to - from).length() * 0.5 + HIT_RADIUS + 12.0;
        let mut objs = Vec::new();
        host.query(mid, reach, self.owner, &mut objs);
        objs.into_iter()
            .filter(|o| o.id != self.owner && o.id != self.object && Some(o.id) != self.last_struck && !o.has(objflags::DEAD) && !o.has(objflags::NO_COLLIDE) && !o.has(objflags::ALLY))
            .filter_map(|o| {
                // Closest approach of the segment to the object's axis, within its radius and height.
                let r = if o.has(objflags::INFANTRY) { 1.5 } else { o.major_radius } + HIT_RADIUS * 0.5;
                let seg = to - from;
                let len2 = (seg.x * seg.x + seg.y * seg.y).max(1e-6);
                let t = (((o.pos.x - from.x) * seg.x + (o.pos.y - from.y) * seg.y) / len2).clamp(0.0, 1.0);
                let p = from + seg * t;
                let (dx, dy) = (p.x - o.pos.x, p.y - o.pos.y);
                (dx * dx + dy * dy <= r * r && p.z >= o.pos.z - 1.0 && p.z <= o.pos.z + o.height.max(2.0) + 1.0).then_some((o, t))
            })
            // The nearest along the move (ties by id, deterministic).
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.id.cmp(&b.0.id)))
    }

    /// The impact damage (`iDamage`) of a strike on `o` that does not end the flight; notes the
    /// hit for the client ([`Self::struck`]).
    fn strike(&mut self, host: &dyn LogicHost, o: &ObjInfo) {
        let amount = self.def.damage[0] * BO2_TO_ZH_DAMAGE;
        if amount > 0.0 {
            self.struck = Some(host.damage(self.owner, o.id, amount, damage_kind::SMALL_ARMS));
        }
    }

    /// Struck `direct` (none: the ground) as a bolt (BO2's `projExplosion` `bolt`, t6mp
    /// `FUN_00608c10`): what it hit takes the def's impact damage (`iDamage`, before the explosion
    /// type is looked at) and the bolt becomes its spawned grenade, stuck to it.
    fn spawn(&mut self, host: &dyn LogicHost, direct: Option<ObjInfo>) -> Outcome {
        self.remove_object(host);
        let (mut hit_enemy, mut kill) = (false, false);
        if let Some(o) = &direct {
            let amount = self.def.damage[0] * BO2_TO_ZH_DAMAGE;
            if amount > 0.0 {
                kill = host.damage(self.owner, o.id, amount, damage_kind::SMALL_ARMS);
                hit_enemy = true;
            }
        }
        Outcome::Spawned { at: self.pos, stuck_to: direct.map(|o| o.id), hit_enemy, kill }
    }

    /// The grenade a bolt becomes where it struck ([`Outcome::Spawned`]): `def` at `at`, stuck to
    /// `stuck_to` (else resting there), on `def`'s own fuse, with the bolt's owner and self-harm
    /// scales.
    pub fn spawned(&self, def: &'static WeaponDef, at: Vec3, stuck_to: Option<u32>, host: &dyn LogicHost) -> Projectile {
        let yaw = crate::math::det::atan2(self.vel.y, self.vel.x);
        let mut p = Projectile::launch(def, self.owner, at, Vec3::ZERO, def.fuse_time, host, yaw);
        p.vel = Vec3::ZERO;
        p.stuck_to = stuck_to;
        p.resting = stuck_to.is_none();
        p.self_blast_scale = self.self_blast_scale;
        p.self_stun_scale = self.self_stun_scale;
        p
    }

    fn impact(&mut self, host: &dyn LogicHost, o: ObjInfo) -> Outcome {
        self.remove_object(host);
        let amount = self.def.damage[0] * BO2_TO_ZH_DAMAGE;
        let kill = host.damage(self.owner, o.id, amount, damage_kind::SMALL_ARMS);
        Outcome::Hit { at: self.pos, kill }
    }

    /// A bounce off `surface`: the impact table's `grenade_bounce` cell for it, when BO2 has one
    /// (t6mp `FUN_004c1590` event 0x47), here; the def's sound by the client ([`Self::bounced`]).
    /// The speed kept bouncing off `surface` (along it, into it): the def's BO2
    /// `parallelBounce`/`perpendicularBounce` cell, else the tuning pack's.
    fn bounce_scales(&self, surface: Surface) -> (f32, f32) {
        self.def.bounce_off(surface).unwrap_or((self.flight.bounce_along, self.flight.bounce_up))
    }

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
/// Read from logic data only, like every surface (docs/CONTRACT.md).
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
    // A smoke grenade's own effect is its script's screen, which water never raises.
    if def.effect == Effect::Smoke && !crate::smoke::raised_on(surface) {
        return (table, None);
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
                // What it struck takes the impact damage (`iDamage`) and is spared the radius: t6mp
                // `FUN_00608c10` damages the struck entity first, then passes it to the radius
                // damage (`FUN_005f2130`) as the one to skip.
                let mut amount = if direct_hit { d.damage[0] } else { d.explosion_damage_at(dist) } * BO2_TO_ZH_DAMAGE;
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
                // `time = 2 + 4 * scale`, scale 1 at the centre, 0 at the edge (`_weapons.gsc:1372`);
                // the thrower's own scale runs over half the radius (`:1364-1365`).
                let thrown = crate::equipment::Kind::of(d) == crate::equipment::Kind::Other;
                let radius = d.explosion_radius.max(1.0) * if own { tuning.concussion_self_radius_scale } else { 1.0 };
                let scale = 1.0 - (dist / radius).clamp(0.0, 1.0);
                let mut secs = tuning.concussion_time_min + tuning.concussion_time_scale * scale;
                if own {
                    secs *= self_stun_scale;
                    // BO2 concusses the thrower too (every damage reaches `onweapondamage`,
                    // `_globallogic_player.gsc:1222`); placed equipment's stun is not the script's.
                    if thrown {
                        host.disable(o.id, disable_kind::PARALYZED, secs);
                    }
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
                    host.disable(o.id, disable_kind::EMP, tuning.emp_grenade_duration);
                    hit_enemy = true;
                }
            }
            Effect::Smoke | Effect::None | Effect::Impact => {}
        }
    }
    Blast { hit_enemy, kill, surface }
}

/// BO2's grenade-on-ground rule (t6mp `FUN_0060a480`, the missile's explode think): a def with
/// `bExplodeOnGround` whose fuse runs out while the grenade is off the ground
/// (`groundEntityNum == ENTITYNUM_NONE`) thinks again next frame instead of going off, for up to
/// [`EXPLODE_ON_GROUND_WAIT`] from launch. The ground entity is set once the grenade settles
/// (`FUN_00607fa0`), so it goes off where it comes to rest: the Willy Pete (fuse 10 ms) smokes
/// where it lands, not in the hand; the concussion and EMP grenades likewise.
pub const EXPLODE_ON_GROUND_WAIT: f32 = 60.0;

impl Projectile {
    /// Whether a fuse that has run out may go off this frame ([`EXPLODE_ON_GROUND_WAIT`]).
    fn fuse_may_fire(&self) -> bool {
        !self.def.explode_on_ground || self.resting || self.stuck_to.is_some() || self.age >= EXPLODE_ON_GROUND_WAIT
    }
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

    /// A smoke grenade (`bExplodeOnGround`, fuse 10 ms) goes off where it comes to rest, not as it
    /// leaves the hand; a frag (no flag) still goes off in the air on its fuse.
    #[test]
    fn an_explode_on_ground_grenade_waits_until_it_rests() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut smoke = weapon("frag_grenade_mp").clone();
        (smoke.fuse_time, smoke.explode_on_ground, smoke.effect) = (0.01, true, crate::weapons::Effect::Smoke);
        let smoke: &'static WeaponDef = Box::leak(Box::new(smoke));
        let from = Vec3::new(0.0, 0.0, 40.0);
        let mut p = Projectile::launch(smoke, 1, from, Vec3::new(1.0, 0.0, 0.5).normalized_or_zero(), smoke.fuse_time, &host, 0.0);
        let mut steps = 0;
        let outcome = loop {
            let was_resting = p.resting;
            if let Some(o) = p.step(&host) {
                assert!(was_resting, "went off only once at rest");
                break o;
            }
            steps += 1;
            assert!(steps < 2000);
        };
        assert!(steps > 5, "not as it left the hand ({steps} steps)");
        let Outcome::Exploded { at, .. } = outcome else { panic!("{outcome:?}") };
        assert!((at - from).length_xy() > 20.0 && at.z <= host.ground_height(at.x, at.y) + 1e-3, "on the ground, away from the hand: {at:?}");
        // Without the flag the same fuse runs out in the air.
        let mut air = smoke.clone();
        air.explode_on_ground = false;
        let air: &'static WeaponDef = Box::leak(Box::new(air));
        let mut q = Projectile::launch(air, 1, from, Vec3::new(1.0, 0.0, 0.5).normalized_or_zero(), air.fuse_time, &host, 0.0);
        assert!(q.step(&host).is_some(), "a plain fuse goes off on time");
    }

    /// A bounce keeps the def's BO2 `parallelBounce`/`perpendicularBounce` for the surface struck
    /// (the dirt the fake ground is), and the tuning pack's scales where the def has none.
    #[test]
    fn a_bounce_keeps_the_defs_per_surface_share() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut frag = weapon("frag_grenade_mp").clone();
        frag.fuse_time = 10.0;
        let first_bounce = |def: &'static WeaponDef| {
            let mut p = Projectile::launch(def, 1, Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, -1.0).normalized_or_zero(), def.fuse_time, &host, 0.0);
            loop {
                let before = p.vel;
                p.step(&host);
                if p.bounced.is_some() {
                    return (before, p.vel);
                }
            }
        };
        frag.bounce = vec![("dirt".into(), 0.25, 0.5)];
        let with: &'static WeaponDef = Box::leak(Box::new(frag.clone()));
        let (before, after) = first_bounce(with);
        assert!((after.x / before.x - 0.25).abs() < 1e-3, "along: {before:?} -> {after:?}");
        assert!((after.z / -(before.z - with_gravity_step(with)) - 0.5).abs() < 0.05, "into: {before:?} -> {after:?}");
        frag.bounce.clear();
        let without: &'static WeaponDef = Box::leak(Box::new(frag));
        let (before, after) = first_bounce(without);
        let t = crate::tuning::Tuning::default();
        assert!((after.x / before.x - t.projectile_bounce_along).abs() < 1e-3, "the pack's along share");
    }

    /// One frame's fall, for comparing a bounce's vertical speed with the speed before it.
    fn with_gravity_step(def: &WeaponDef) -> f32 {
        if def.projectile_gravity { gravity() * LOGIC_DT } else { 0.0 }
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
        // At rest: a concussion grenade goes off on the ground (`bExplodeOnGround`).
        p.resting = true;
        p.step(&host);
        assert!(host.disabled.borrow().iter().any(|(id, _, secs)| *id == 5 && *secs > 1.0));
    }
}
