//! Thrown and launched things: grenades (bouncing, sticking, cooking), launcher rockets, crossbow
//! bolts, ballistic knives and combat axes. Each flies in the simulation; the host shows it as a
//! Zero Hour object (the converted BO2 model) and plays its effect when it goes off.
//!
//! Runs inside the lockstep logic: no randomness, fixed step.

use crate::damage::MeansOfDeath;
use crate::host::{disable_kind, objflags, LogicHost, ObjInfo};
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
/// How close a flying projectile must pass an object to hit it when its def carries no projectile
/// model bounds (a table converted before `ProjectileBounds`, or a def without a model): the
/// mod's old stand-in, kept for such tables only.
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
    /// Grenade) instead of going off; a timed one's fuse carries over to the piece.
    pub lands: bool,
    /// Path length flown so far (BO2's `G_RunMissile` sums each frame's move), against the def's
    /// `activate_dist`.
    pub travelled: f32,
    /// Bounced this step: where, and off what (the client's bounce sound). Cleared every step.
    pub bounced: Option<(Vec3, Surface)>,
    /// Struck an enemy for its impact damage this step and flew on (stuck or bounced): whether
    /// that killed (the client's hit marker). Cleared every step.
    pub struck: Option<bool>,
    /// The object it last struck and bounced off: not struck again while its path still meets it
    /// (a bounce leaves it inside the object's cylinder, and a grenade that drops against the hull
    /// rolls along it a few steps; a frag once struck an Overlord on six frames running).
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
        // A piece that lands is set down with no fuse, unless its throw armed a timed one
        // (`timedDetonation`, t6mp `FUN_00607c00`): the Sensor Grenade's runs on where it lies.
        let untimed_piece = lands && !def.timed_detonation;
        let fuse = if untimed_piece || def.impact_explode || def.effect == Effect::Impact || def.fuse_time <= 0.0 { f32::INFINITY } else { fuse_left };
        let flight = Flight::from_tuning(&crate::tuning::get());
        Projectile { def, owner, pos: from, vel, fuse, age: 0.0, object, stuck_to: None, resting: false, self_blast_scale: 1.0, self_stun_scale: 1.0, lands, travelled: 0.0, bounced: None, struck: None, last_struck: None, flight }
    }

    /// Advance one logic frame with no shields about (see [`Projectile::step_shielded`]).
    pub fn step(&mut self, host: &dyn LogicHost) -> Option<Outcome> {
        self.step_shielded(host, &[], &[])
    }

    /// Advance one logic frame. Returns `Some` when it is finished (went off or hit). An enemy's
    /// Assault Shield in the way stops it (`_globallogic_player.gsc:840`: location `riotshield`):
    /// an explosive goes off on it (its impact damage is the script's 0, its splash the blast's,
    /// which the shield protects against), anything else falls dead.
    ///
    /// `players` are the seats' Generals (BO2's players): a Shock Charge that strikes an enemy one
    /// in flight goes off at once (`watchproximitygrenadehitplayer`, `_proximity_grenade.gsc:127-
    /// 145`: on `grenade_bounce` off a player not of the owner's team and not off a shield,
    /// `proximitydetonate`), after the strike's impact damage (t6mp `FUN_00608c10`).
    pub fn step_shielded(&mut self, host: &dyn LogicHost, shields: &[crate::shield::Bearer], players: &[u32]) -> Option<Outcome> {
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
            // The ground rising across the move (a slope, a hillside) counts as much as the
            // ground it falls onto.
            let mut contact = false;
            if let Some(t) = ground_crossing(host, self.pos, next) {
                if t <= 0.0 {
                    // Lying on a slope and pressed into it: it slides down the face (the move
                    // less its part into the ground). Level ground keeps it where it is.
                    let n = ground_normal(host, self.pos.x, self.pos.y);
                    let step = next - self.pos;
                    let into = step.dot(n);
                    next = if n.z < SLOPE_NORMAL_Z && into < 0.0 { self.pos + (step - n * into) } else { self.pos };
                } else {
                    next = self.pos + (next - self.pos) * t;
                }
                next.z = next.z.max(host.ground_height(next.x, next.y));
                contact = true;
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
            let (hit, still_touching) = self.hit_object(host, self.pos, next);
            if !still_touching {
                self.last_struck = None;
            }
            if let Some((o, t)) = hit {
                // Where along this move it met him, not the move's end (a bolt covers 25 units
                // a step).
                let at = self.pos + (next - self.pos) * t;
                // Met short of where the move met the ground: it never reached the ground this
                // step (the object turns it before it can), so no ground contact follows.
                if t < 1.0 {
                    contact = false;
                }
                self.travelled -= (next - at).length();
                let dud = self.def.activate_dist > 0.0 && self.travelled < self.def.activate_dist;
                next = at;
                self.pos = at;
                if spawns {
                    return Some(self.spawn(host, Some(o), dud));
                } else if self.def.impact_explode && dud {
                    return Some(self.dud(host, Some(o)));
                } else if self.def.impact_explode {
                    return Some(self.go_off(host, Some(o), shields));
                } else if self.def.effect == Effect::Impact {
                    return Some(self.impact(host, o, dud));
                }
                // BO2 strikes every entity a missile hits for its `iDamage` before it sticks or
                // bounces (t6mp `FUN_00608c10`): a Semtex stuck to a player, a frag or
                // flashbang off his head.
                self.strike(host, &o, dud);
                if self.def.equipment_kind == crate::weapons::Kind::Shock && players.contains(&o.id) {
                    return Some(self.go_off(host, None, shields));
                }
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
            if contact || next.z <= ground {
                self.pos = Vec3::new(next.x, next.y, ground);
                if spawns {
                    return Some(self.spawn(host, None, dud));
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
                    // Off the ground's own face: the speed along it kept by `along`, into it
                    // turned back by `up` (BO2's parallel/perpendicular bounce). On level ground
                    // the face is up: the horizontal speed scales and the fall turns over.
                    let surface = host.surface_at(self.pos.x, self.pos.y);
                    let (along, up) = self.bounce_scales(surface);
                    let n = ground_normal(host, self.pos.x, self.pos.y);
                    let into = self.vel.dot(n);
                    let normal = n * into;
                    let tangent = self.vel - normal;
                    self.vel = if into < 0.0 { tangent * along - normal * up } else { tangent * along + normal };
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

    /// The first object the move `from`→`to` meets, and how far along the move (0..1) it does
    /// (never [`Self::last_struck`]); and whether the move still meets the last one struck. A
    /// soldier is met within its thin cylinder; anything else by its Zero Hour geometry (a box, a
    /// cylinder or a sphere, [`crate::hitscan::solid`]), grown by the projectile's own radius.
    fn hit_object(&self, host: &dyn LogicHost, from: Vec3, to: Vec3) -> (Option<(ObjInfo, f32)>, bool) {
        let mid = (from + to) * 0.5;
        let reach = (to - from).length() * 0.5 + HIT_RADIUS + 12.0;
        let mut objs = Vec::new();
        host.query(mid, reach, self.owner, &mut objs);
        let seg = to - from;
        let len = seg.length();
        let dir = if len > 1e-6 { seg * (1.0 / len) } else { Vec3::new(0.0, 0.0, -1.0) };
        let met: Vec<(ObjInfo, f32)> = objs
            .into_iter()
            .filter(|o| o.id != self.owner && o.id != self.object && !o.has(objflags::DEAD) && !o.has(objflags::NO_COLLIDE) && !o.has(objflags::ALLY))
            .filter_map(|o| {
                let solid = struck_solid(&o, self.def);
                let t = crate::hitscan::ray_vs_target(from, dir, len.max(1e-6), &solid)?;
                Some((o, if len > 1e-6 { t / len } else { 0.0 }))
            })
            .collect();
        let touching = met.iter().any(|(o, _)| Some(o.id) == self.last_struck);
        // The nearest along the move (ties by id, deterministic).
        let first = met.into_iter().filter(|(o, _)| Some(o.id) != self.last_struck).min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.id.cmp(&b.0.id)));
        (first, touching)
    }

    /// A blow of this projectile's by `how`: its weapon, travelling the way it flies (BO2's missile
    /// passes its velocity as `vDir`).
    fn blow(&self, how: MeansOfDeath) -> crate::damage::Blow {
        crate::damage::Blow::new(how, self.def, self.vel)
    }

    /// The impact damage (`iDamage`) of a strike on `o` that does not end the flight (BO2's
    /// `MOD_IMPACT`: the def does not go off on impact); notes the hit for the client
    /// ([`Self::struck`]).
    fn strike(&mut self, host: &dyn LogicHost, o: &ObjInfo, dud: bool) {
        let how = crate::damage::strike(self.def, dud);
        let amount = impact_damage(self.def, o, how);
        if amount > 0.0 {
            self.struck = Some(host.damage(self.owner, o.id, amount, self.blow(how)));
        }
    }

    /// Struck `direct` (none: the ground) as a bolt (BO2's `projExplosion` `bolt`, t6mp
    /// `FUN_00608c10`): what it hit takes the def's impact damage (`iDamage`, before the explosion
    /// type is looked at) and the bolt becomes its spawned grenade, stuck to it.
    fn spawn(&mut self, host: &dyn LogicHost, direct: Option<ObjInfo>, dud: bool) -> Outcome {
        self.remove_object(host);
        let (mut hit_enemy, mut kill) = (false, false);
        if let Some(o) = &direct {
            let how = crate::damage::strike(self.def, dud);
            let amount = impact_damage(self.def, o, how);
            if amount > 0.0 {
                kill = host.damage(self.owner, o.id, amount, self.blow(how));
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

    /// A thrown or launched blade striking `o` (a ballistic knife, a combat axe): its impact
    /// damage, BO2's `MOD_IMPACT` (neither goes off on impact).
    fn impact(&mut self, host: &dyn LogicHost, o: ObjInfo, dud: bool) -> Outcome {
        self.remove_object(host);
        let how = crate::damage::strike(self.def, dud);
        let kill = host.damage(self.owner, o.id, impact_damage(self.def, &o, how), self.blow(how));
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
            let how = crate::damage::strike(self.def, true);
            let amount = impact_damage(self.def, &o, how);
            if amount > 0.0 {
                kill = host.damage(self.owner, o.id, amount, self.blow(how));
                hit_enemy = true;
            }
        }
        Outcome::Dud { at: self.pos, hit_enemy, kill, surface }
    }

    /// The blast (or stun, EMP, smoke).
    fn go_off(&mut self, host: &dyn LogicHost, direct: Option<ObjInfo>, shields: &[crate::shield::Bearer]) -> Outcome {
        self.remove_object(host);
        let b = blast(host, self.def, self.owner, self.pos, Hit { direct, stuck_to: self.stuck_to }, self.self_blast_scale, self.self_stun_scale, shields);
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

/// The solid a flying projectile of `def` strikes `o` by. BO2 traces the missile's box — its
/// `projectileModel`'s bounds (t6mp `FUN_00607cb0` -> `FUN_007825b0`), axis-aligned, offset from
/// its origin — against each entity's bounds: a player's box (15" either side of his origin, the
/// movement radius, and his height) and anything else's own. Here the object's solid (a soldier
/// as BO2's player box, anything else its Zero Hour geometry, [`crate::hitscan::solid`]) is moved
/// by the box's offset and grown by its half extents (horizontally by the larger, as the shapes
/// are round or turned), so the projectile's origin meets it where the two boxes touch. A def
/// without bounds keeps the old stand-in: half of [`HIT_RADIUS`] all round.
fn struck_solid(o: &ObjInfo, def: &WeaponDef) -> crate::hitscan::Target {
    use crate::hitscan::{Shape, Target};
    let (centre, half) = def.projectile_box().unwrap_or((Vec3::ZERO, Vec3::new(HIT_RADIUS * 0.5, HIT_RADIUS * 0.5, 1.0)));
    let r = half.x.max(half.y);
    let base = crate::hitscan::solid(o);
    let low = Vec3::new(o.pos.x - centre.x, o.pos.y - centre.y, o.pos.z - centre.z - half.z);
    let grow = 2.0 * half.z;
    let shape = match base.shape {
        _ if o.has(objflags::INFANTRY) => {
            let player = crate::movement::MoveTuning::default().radius;
            Shape::Box { half_x: player + r, half_y: player + r, height: o.height.max(2.0) + grow, yaw: 0.0 }
        }
        Shape::Box { half_x, half_y, height, yaw } => Shape::Box { half_x: half_x + r, half_y: half_y + r, height: height + grow, yaw },
        Shape::Cylinder { radius, height } => Shape::Cylinder { radius: radius + r, height: height + grow },
        // The sphere stands on the position (centred a radius up): grown, it stands `r` lower.
        Shape::Sphere { radius } => return Target { pos: Vec3::new(o.pos.x - centre.x, o.pos.y - centre.y, o.pos.z - centre.z - r), shape: Shape::Sphere { radius: radius + r }, ..base },
        other => other,
    };
    Target { pos: low, shape, ..base }
}

/// What a missile's strike by means `how` ([`crate::damage::strike`]) deals the object it struck
/// (t6mp `FUN_00608c10`): the def's impact damage (`iDamage`), in Zero Hour's damage. A soldier (a
/// General or Zero Hour infantry: BO2's players) takes all of it; anything else (a vehicle, an
/// aircraft, a structure: BO2's vehicles and killstreak gear) takes BO2's vehicle rule for the
/// means ([`crate::damage::vehicle_scale`]): a direct hit (`MOD_PROJECTILE`) the weapon's
/// `getvehicleprojectilescalar` share (a fifth for the SMAW and FHJ-18, whose 1000 is there to kill
/// any player outright), an impact (`MOD_IMPACT`: a dud, a frag or a bolt striking) all of it.
pub fn impact_damage(def: &WeaponDef, o: &ObjInfo, how: MeansOfDeath) -> f32 {
    def.damage[0] * crate::damage::scale_on(&crate::tuning::get(), &def.id, how, o) * BO2_TO_ZH_DAMAGE
}

/// How far a blast at `at` is from `o`, for its damage: to the object's solid (its Zero Hour
/// geometry; BO2's soldier-sized cylinder for infantry, [`crate::hitscan::solid`]), as BO2's radius
/// damage measures each entity to its bounds (t6mp `FUN_005f1fd0`). A tank beside the one struck is
/// as far as the gap between their hulls, not between their centres.
pub fn blast_distance(o: &ObjInfo, at: Vec3) -> f32 {
    crate::hitscan::distance_to(&crate::hitscan::solid(o), at)
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
/// (`FUN_004c0ac0`), then a smoke grenade's screen (`screen_fx`, its script's cloud, which water
/// never raises). A laser sensor (the Sensor Grenade) plays no table cell: the client's smoke-type
/// explosion skips it for `isLaserSensor` (`FUN_004c1590`, case 0x4c). None of the first two ->
/// the stand-in (no converted table, or a mod-only def).
pub fn blast_fx(def: &WeaponDef, surface: Surface) -> (Option<String>, Option<&str>, Option<&str>) {
    let table = if def.laser_sensor { None } else { crate::impacts::fx_name(def.impact_type, surface, false, false) };
    let own = (!def.explosion_fx.is_empty()).then_some(def.explosion_fx.as_str());
    let screen = (def.effect == Effect::Smoke && !def.screen_fx.is_empty() && crate::smoke::raised_on(surface)).then_some(def.screen_fx.as_str());
    if table.is_none() && own.is_none() && (def.effect != Effect::Smoke || def.screen_fx.is_empty()) {
        return (None, Some(DEFAULT_EXPLOSION_FX), None);
    }
    (table, own, screen)
}

/// The effects BO2 plays for `def` striking `surface` as a dud (event 0x51): the impact table's
/// `projectile_dud` cell for the surface (exact, when set), then the def's own `projDudEffect`.
pub fn dud_fx(def: &WeaponDef, surface: Surface) -> [Option<String>; 2] {
    let table = crate::impacts::fx_cell(crate::impacts::PROJECTILE_DUD, surface);
    [table, (!def.dud_fx.is_empty()).then(|| def.dud_fx.clone())]
}

/// What a blast went off against: the object it struck directly (an impact fuse's), and the one it
/// was stuck to (a Semtex, C4 or the crossbow's bolt grenade on a tank).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hit {
    pub direct: Option<ObjInfo>,
    pub stuck_to: Option<u32>,
}

impl Hit {
    /// Nothing struck, nothing stuck to (a grenade on the ground, placed equipment).
    pub const NONE: Hit = Hit { direct: None, stuck_to: None };

    /// Struck `o` directly.
    pub fn direct(o: ObjInfo) -> Hit {
        Hit { direct: Some(o), stuck_to: None }
    }
}

/// `def` going off at `at` for `owner` (a General's id): explosion damage, stun, EMP or smoke over
/// its radius, and its effects. `hit.direct` took a direct hit; a sticky def stuck to a vehicle
/// (`hit.stuck_to`) strikes it for its inner damage instead of the radius (t6mp `FUN_0060a480`). A
/// General with a shield keeps `1 − protection` of the damage
/// ([`crate::shield::Bearer::blast_protection`]).
#[allow(clippy::too_many_arguments)]
pub fn blast(host: &dyn LogicHost, def: &'static WeaponDef, owner: u32, at: Vec3, hit: Hit, self_blast_scale: f32, self_stun_scale: f32, shields: &[crate::shield::Bearer]) -> Blast {
    let tuning = crate::tuning::get();
    let d = def;
    let direct = hit.direct;
    let surface = blast_surface(host, at, direct.as_ref());
    // A def with no blast of its own plays its effects only when it names one (the Sensor
    // Grenade's scan): the stand-in is for explosives.
    if d.effect != Effect::None || !d.explosion_fx.is_empty() {
        let (table, own, screen) = blast_fx(d, surface);
        for fx in table.as_deref().into_iter().chain(own).chain(screen) {
            host.fx(fx, at);
        }
    }
    // Everything whose solid the blast may reach: the host returns each object whose bounding
    // sphere comes within the radius (docs/CONTRACT.md), so a structure or an Overlord whose centre
    // is farther than the radius is still found; the reach is then measured to its solid. What
    // was struck is always in it (it takes its strike even if the host would not list it).
    let mut objs = Vec::new();
    host.query(at, d.explosion_radius.max(1.0), owner, &mut objs);
    if let Some(o) = direct.filter(|x| !objs.iter().any(|o| o.id == x.id)) {
        objs.push(o);
    }
    objs.sort_by_key(|o| o.id);
    // What may hide a target from it: the terrain and the solids about (BO2's radius damage scales
    // each entity's damage by how much of it the blast sees, `FUN_005f0f00`; [`crate::cover`]).
    // The object it struck or is stuck to is where it starts: never in the way.
    let ignore: Vec<u32> = direct.map(|o| o.id).into_iter().chain(hit.stuck_to).collect();
    let cover = crate::cover::Blockers::near(host, at, d.explosion_radius.max(1.0) * 2.0, owner, &ignore);
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
        let dist = blast_distance(o, at);
        let direct_hit = direct.is_some_and(|x| x.id == o.id);
        // A sticky grenade stuck to a vehicle (BO2: `stickiness` all or all_orient and its ground
        // entity a vehicle, `FUN_0060a480`) strikes it for its inner damage by its handler's direct
        // means and spares it the radius; a soldier it is stuck to takes the radius as any other.
        let stuck_hit = d.sticky && hit.stuck_to == Some(o.id) && !o.has(objflags::INFANTRY) && !direct_hit;
        if dist > d.explosion_radius && !direct_hit && !stuck_hit {
            continue;
        }
        let own = o.id == owner;
        // How much of it the blast sees: a direct strike and a stuck grenade's strike are not the
        // radius damage. The flashbang's whiteout is the engine's own sight line from his eye
        // instead (`FUN_005f1c60`, one trace, mask `0x807823`).
        let seen = if direct_hit || stuck_hit {
            1.0
        } else if d.effect == Effect::Stun && d.id == crate::sim::FLASH_WEAPON {
            let eye = if o.has(objflags::INFANTRY) { o.pos + Vec3::new(0.0, 0.0, crate::cover::soldier_eye(host, &tuning, o)) } else { o.pos + Vec3::new(0.0, 0.0, o.height * 0.5) };
            if cover.clear(host, at, eye, o.id) { 1.0 } else { 0.0 }
        } else {
            cover.fraction(host, &tuning, at, o)
        };
        if seen <= 0.0 {
            continue;
        }
        match d.effect {
            // A def with no other effect still deals its radius damage when it has one: t6mp
            // `FUN_0060a480` runs the radius damage (`FUN_005f2130`) for any def whose
            // `iExplosionInnerDamage` is not 0 (the Sensor Grenade's 15).
            Effect::Explode | Effect::None if d.effect == Effect::Explode || d.explosion_inner_damage > 0.0 => {
                // What it struck takes the impact damage (`iDamage`) and is spared the radius: t6mp
                // `FUN_00608c10` damages the struck entity first, then passes it to the radius
                // damage (`FUN_005f2130`) as the one to skip. Each by its means of death, under
                // BO2's vehicle rule for it on anything but a soldier.
                let (how, mut amount) = if direct_hit {
                    let how = crate::damage::strike(d, false);
                    (how, impact_damage(d, o, how))
                } else if stuck_hit {
                    let how = crate::damage::direct(d);
                    (how, d.explosion_inner_damage * crate::damage::scale_on(&tuning, &d.id, how, o) * BO2_TO_ZH_DAMAGE)
                } else {
                    let how = crate::damage::splash(d);
                    (how, d.explosion_damage_at(dist) * seen * crate::damage::scale_on(&tuning, &d.id, how, o) * BO2_TO_ZH_DAMAGE)
                };
                if own {
                    amount *= self_blast_scale;
                }
                if let Some(b) = shields.iter().find(|b| b.general == o.id) {
                    amount *= 1.0 - b.blast_protection(at, &tuning);
                }
                if amount > 0.0 {
                    let k = host.damage(owner, o.id, amount, crate::damage::Blow::from_to(how, d, at, o.pos));
                    if !own {
                        hit_enemy = true;
                        kill |= k;
                    }
                }
            }
            Effect::Stun => {
                // `time = 2 + 4 * scale`, scale 1 at the centre, 0 at the edge (`_weapons.gsc:1372`),
                // over the distance from the blast to his origin (`:1366`, `self.origin`: his feet,
                // not his bounds); the thrower's own scale runs over half the radius (`:1364-1365`).
                let thrown = crate::equipment::Kind::of(d) == crate::equipment::Kind::Other;
                let radius = d.explosion_radius.max(1.0) * if own { tuning.concussion_self_radius_scale } else { 1.0 };
                let scale = 1.0 - ((o.pos - at).length() / radius).clamp(0.0, 1.0);
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
                    // Its radius damage too: t6mp's explode think runs the radius damage for any def
                    // whose `iExplosionInnerDamage` is not 0 (`FUN_0060a480` -> `FUN_005f2130`; the
                    // Shock Charge's 1 over 200"). Its zap's ticks are the sim's (`shock::Taser`).
                    let how = crate::damage::splash(d);
                    let amount = d.explosion_damage_at(dist) * seen * crate::damage::scale_on(&tuning, &d.id, how, o) * BO2_TO_ZH_DAMAGE;
                    if amount > 0.0 {
                        kill |= host.damage(owner, o.id, amount, crate::damage::Blow::from_to(how, d, at, o.pos));
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
            Effect::Explode | Effect::Smoke | Effect::None | Effect::Impact => {}
        }
    }
    Blast { hit_enemy, kill, surface }
}

/// Where the move `from` → `to` first meets the ground, as a share of the move (0..=1), if it does:
/// the move is walked in steps of at most [`GROUND_STEP`] across the height map, and the crossing
/// is placed where the height above the ground, taken as linear between the two samples that
/// bracket it, reaches zero (exact on level ground). Ground that rises across the move — a slope
/// or a hillside the grenade flies into — is met as surely as ground it falls onto. Before, only a
/// fall was tested, against the ground at the move's end: a grenade flying into rising ground was
/// put back where it was every frame, hanging in the air until it timed out without going off.
fn ground_crossing(host: &dyn LogicHost, from: Vec3, to: Vec3) -> Option<f32> {
    let above = |t: f32| {
        let p = from + (to - from) * t;
        p.z - host.ground_height(p.x, p.y)
    };
    let steps = ((to - from).length_xy() / GROUND_STEP).ceil().clamp(1.0, MAX_GROUND_STEPS) as u32;
    let (mut t0, mut h0) = (0.0f32, above(0.0));
    for i in 1..=steps {
        let t1 = i as f32 / steps as f32;
        let h1 = above(t1);
        if h1 <= 0.0 {
            // Already in the ground at the start of the step: the contact is there.
            return Some(if h0 <= 0.0 { t0 } else { t0 + (t1 - t0) * h0 / (h0 - h1) });
        }
        (t0, h0) = (t1, h1);
    }
    None
}

/// The ground's upward normal at (`x`, `y`), from the height map's slope across
/// [`GROUND_STEP`] (straight up on level ground).
fn ground_normal(host: &dyn LogicHost, x: f32, y: f32) -> Vec3 {
    let d = GROUND_STEP * 0.5;
    let dx = (host.ground_height(x + d, y) - host.ground_height(x - d, y)) / (2.0 * d);
    let dy = (host.ground_height(x, y + d) - host.ground_height(x, y - d)) / (2.0 * d);
    Vec3::new(-dx, -dy, 1.0).normalized_or_zero()
}

/// The ground test's step along a projectile's move (Zero Hour units), and the most steps a move
/// takes (a fast rocket's move is walked more coarsely).
const GROUND_STEP: f32 = 2.0;
/// Ground whose normal leans further than this from straight up (about 2.5°) is a slope a lying
/// grenade slides down.
const SLOPE_NORMAL_Z: f32 = 0.999;
const MAX_GROUND_STEPS: f32 = 32.0;

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

    /// The missile's box is its `projectileModel`'s bounds (t6mp `FUN_00607cb0` -> `FUN_007825b0`)
    /// and a soldier's is BO2's player box (15" either side): a grenade passing 17" beside him
    /// strikes him when its model reaches 3" out, and misses with a 1" model; a def without bounds
    /// keeps the old stand-in.
    #[test]
    fn a_projectile_strikes_by_its_models_box_against_the_players() {
        let o = soldier(7, 1, 0.0);
        let at = |d: f32| Vec3::new(o.pos.x - 50.0, o.pos.y + inches(d), o.pos.z + inches(30.0));
        let hits = |bounds: [f32; 6], side: f32| {
            let def = WeaponDef { projectile_bounds: bounds, ..Default::default() };
            let t = struck_solid(&o, &def);
            crate::hitscan::ray_vs_target(at(side), Vec3::new(1.0, 0.0, 0.0), 100.0, &t).is_some()
        };
        assert!(hits([-3.0, -3.0, -1.0, 3.0, 3.0, 1.0], 17.0), "15 + 3 reaches 17");
        assert!(!hits([-1.0, -1.0, -1.0, 1.0, 1.0, 1.0], 17.0), "15 + 1 does not");
        assert!(hits([-1.0, -1.0, -1.0, 1.0, 1.0, 1.0], 15.5));
        // An off-centre model is offset with it: 18" off on -y, a box reaching 4" towards +y
        // touches him, one reaching 4" further out does not.
        assert!(hits([-1.0, 0.0, -1.0, 1.0, 4.0, 1.0], -18.0) && !hits([-1.0, -4.0, -1.0, 1.0, 0.0, 1.0], -18.0));
        // No bounds: the old stand-in (half of 2.5 units all round).
        let r = HIT_RADIUS * 0.5 / inches(1.0);
        assert!(hits([0.0; 6], 15.0 + r - 0.5) && !hits([0.0; 6], 15.0 + r + 0.5));
    }

    /// A frag thrown into a tank's hull strikes it once (its `iDamage`), however long it lies
    /// against the hull before its fuse: in game one struck an Overlord on six frames running,
    /// because touching the ground forgot which object it had bounced off.
    #[test]
    fn a_grenade_against_a_hull_strikes_it_once() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let tank = crate::host::ObjInfo {
            id: 5,
            owner_player: 1,
            flags: objflags::VEHICLE,
            geom_type: crate::host::geom::BOX,
            major_radius: 26.0,
            minor_radius: 13.0,
            height: 15.0,
            health: 1100.0,
            max_health: 1100.0,
            pos: Vec3::new(50.0, 0.0, 0.0),
            ..Default::default()
        };
        host.add(tank);
        let mut damage = [0.0; 6];
        damage[0] = 15.0;
        let frag: &'static WeaponDef = Box::leak(Box::new(WeaponDef { damage, ..weapon("frag_grenade_mp").clone() }));
        // Thrown as the General throws (the def's speed), at pitches from level to a lob, at the
        // hull from 60 to 120 units off.
        for i in 0..=8 {
            for d in [60.0, 84.0, 120.0] {
                host.damage_log.borrow_mut().clear();
                host.objects.borrow_mut()[1].pos.x = d + 26.0;
                let pitch = -0.1 + 0.1 * i as f32;
                let mut p = Projectile::launch(frag, 1, Vec3::new(0.0, 0.0, 11.0), Vec3::new(pitch.cos(), 0.0, pitch.sin()), frag.fuse_time, &host, 0.0);
                let mut steps = 0;
                while p.step(&host).is_none() {
                    steps += 1;
                    assert!(steps < 600);
                }
                let strikes = host.damage_log.borrow().iter().filter(|h| h.1 == 5 && (h.2 - 15.0 * BO2_TO_ZH_DAMAGE).abs() < 1e-3).count();
                assert!(strikes <= 1, "thrown at pitch {pitch} from {d}: {:?}", host.damage_log.borrow());
            }
        }
        // Dropping in close to the hull, low and slow (a frag that landed short and skips on).
        host.objects.borrow_mut()[1].pos.x = 50.0;
        let mut struck = 0;
        for start in [16.0, 19.0, 21.0, 22.0] {
            for z in [0.5, 1.5, 3.0] {
                for speed in [20.0, 40.0, 80.0, 160.0] {
                    host.damage_log.borrow_mut().clear();
                    let mut p = Projectile::launch(frag, 1, Vec3::new(start, 0.0, z), Vec3::new(1.0, 0.0, 0.0), frag.fuse_time, &host, 0.0);
                    p.vel = Vec3::new(speed, 0.0, 0.0);
                    let mut steps = 0;
                    while p.step(&host).is_none() {
                        steps += 1;
                        assert!(steps < 600);
                    }
                    let strikes = host.damage_log.borrow().iter().filter(|h| h.1 == 5 && (h.2 - 15.0 * BO2_TO_ZH_DAMAGE).abs() < 1e-3).count();
                    assert!(strikes <= 1, "from {start} at {z} high, {speed}: {:?}", host.damage_log.borrow());
                    struck += strikes;
                }
            }
        }
        assert!(struck > 0, "some reached the hull");
    }

    /// Every projectile the catalog throws or fires, aimed into a hillside (a 1:1 slope rising
    /// 10 units ahead): it meets the slope and comes to its end there — a blast, a dud, a landing,
    /// a bolt's grenade — never hanging in the air until the flight's 30 s limit drops it without
    /// a sound. (Before, a move into ground that rose across it was put back where it started, so a
    /// concussion or a rocket thrown at a hill hung in place; the in-game multiplayer test's
    /// concussions never went off.) On level ground nothing changed.
    #[test]
    fn a_projectile_flying_into_a_hillside_meets_it() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.hill.set(Some((10.0, 1.0)));
        let defs: Vec<_> = crate::weapons::catalog().all().iter().copied().filter(|d| d.class == crate::weapons::Class::Launcher || (d.class.is_offhand() && d.place == crate::weapons::Place::Throw))
            // Ones with an end of their own (a stand-in launcher without projectile data flies and
            // lies forever).
            .filter(|d| d.fuse_time > 0.0 || d.impact_explode || d.effect == Effect::Impact || !d.spawns.is_empty() || crate::equipment::Kind::of(d) != crate::equipment::Kind::Other)
            .collect();
        assert!(defs.len() >= 3, "{}", defs.len());
        for def in defs {
            let mut p = Projectile::launch(def, 1, Vec3::new(0.0, 0.0, 8.0), Vec3::new(1.0, 0.0, 0.05).normalized_or_zero(), def.fuse_time, &host, 0.0);
            let mut outcome = None;
            for _ in 0..(40.0 / LOGIC_DT) as u32 {
                if let Some(o) = p.step(&host) {
                    outcome = Some(o);
                    break;
                }
                assert!(p.pos.z >= host.ground_height(p.pos.x, p.pos.y) - 1e-3, "{}: under the hill at {:?}", def.id, p.pos);
            }
            let outcome = outcome.unwrap_or_else(|| panic!("{} never finished", def.id));
            assert!(p.age < 30.0, "{}: hung until the flight limit ({outcome:?} at {:?})", def.id, p.pos);
            assert!(p.pos.x > 9.0, "{}: it reached the hill: {:?}", def.id, p.pos);
        }
    }

    /// A grenade whose move would end in the ground but meets a hull first (it passes beside the
    /// tank, low) bounces off the hull where it met it and is not also put on the ground and
    /// bounced off it in the same step (the ground crossing is the move's, the strike comes first).
    #[test]
    fn a_hull_met_before_the_ground_turns_it_in_the_air() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(crate::host::ObjInfo {
            id: 5,
            owner_player: 1,
            flags: objflags::VEHICLE,
            geom_type: crate::host::geom::BOX,
            major_radius: 26.0,
            minor_radius: 13.0,
            height: 15.0,
            health: 1100.0,
            max_health: 1100.0,
            // Its rear face (grown by the grenade's half radius) across the path at x = 22.
            pos: Vec3::new(22.0 + 26.0 + HIT_RADIUS * 0.5, 0.0, 0.0),
            ..Default::default()
        });
        let def = weapon("frag_grenade_mp");
        let mut p = Projectile::launch(def, 1, Vec3::new(20.0, 0.0, 1.0), Vec3::ZERO, 10.0, &host, 0.0);
        // 10 units on and 2 down this step: the ground is met halfway, the hull at a fifth.
        p.vel = Vec3::new(10.0, 0.0, -2.0) * (1.0 / LOGIC_DT);
        p.step(&host);
        assert_eq!(p.last_struck, Some(5));
        assert!((p.pos.x - 22.0).abs() < 1e-3 && p.pos.z > 0.5, "where it met the hull, in the air: {:?}", p.pos);
        assert!(p.vel.x < 0.0 && p.vel.z < 0.0, "turned by the hull only: {:?}", p.vel);
    }

    /// On level ground the bounce is what it always was: the horizontal speed kept by `along`, the
    /// fall turned over by `up`.
    #[test]
    fn a_bounce_on_level_ground_keeps_its_old_shape() {
        let host = FakeHost::default();
        let def = weapon("frag_grenade_mp");
        let mut p = Projectile::launch(def, 1, Vec3::new(0.0, 0.0, 0.5), Vec3::ZERO, 10.0, &host, 0.0);
        p.vel = Vec3::new(30.0, 0.0, -30.0);
        let (along, up) = p.bounce_scales(Surface::Dirt);
        while p.bounced.is_none() {
            p.step(&host);
        }
        assert!((p.vel.x - 30.0 * along).abs() < 1e-3, "{:?}", p.vel);
        assert!(p.vel.z > 0.0 && p.pos.z == 0.0, "{:?} {:?}", p.vel, p.pos);
        let _ = up;
    }

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
    /// A piece that lands keeps a timed throw's fuse (`timedDetonation`, t6mp `FUN_00607c00`: the
    /// Sensor Grenade) and lands with the rest of it; an untimed one (C4, the Claymore) has none.
    #[test]
    fn a_landing_piece_keeps_only_a_timed_fuse() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let piece = |timed: bool, kind: crate::weapons::Kind| -> &'static WeaponDef {
            let mut d = weapon("frag_grenade_mp").clone();
            (d.fuse_time, d.timed_detonation, d.equipment_kind, d.effect) = (4.5, timed, kind, crate::weapons::Effect::None);
            Box::leak(Box::new(d))
        };
        let from = Vec3::new(0.0, 0.0, 40.0);
        let dir = Vec3::new(1.0, 0.0, 0.2).normalized_or_zero();
        let mut sensor = Projectile::launch(piece(true, crate::weapons::Kind::Sensor), 1, from, dir, 4.5, &host, 0.0);
        let c4 = Projectile::launch(piece(false, crate::weapons::Kind::C4), 1, from, dir, 4.5, &host, 0.0);
        assert!(c4.fuse.is_infinite(), "an untimed piece is set down with no fuse");
        assert_eq!(sensor.fuse, 4.5);
        let outcome = (0..2000).find_map(|_| sensor.step(&host)).expect("lands");
        assert!(matches!(outcome, Outcome::Landed { .. }), "{outcome:?}");
        assert!(sensor.fuse > 0.0 && sensor.fuse < 4.5, "the rest of the fuse lands with it: {}", sensor.fuse);
    }

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
