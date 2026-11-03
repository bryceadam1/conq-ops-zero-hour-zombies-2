//! Does a projectile hurt by its BO2 definition? Fires the real [`Projectile`] at a BO2 soldier (100
//! BO2 health) on a flat range at several distances — straight at him, and into the ground beside
//! him — and measures each damage against what BO2's data implies (t6mp `FUN_00608c10`, the missile
//! impact):
//!
//! * every strike of the soldier deals the def's `iDamage` (`damage[0]`) first, whatever happens
//!   next (a rocket's blast, a bolt's spawned grenade, a Semtex sticking, a frag bouncing off);
//! * an impact-fuse projectile (`bProjImpactExplode`) that has flown its `iProjectileActivateDist`
//!   goes off where it strikes, and one that has not is a dud (no blast);
//! * a `bolt` (`projExplosion`) becomes its `szSpawnedGrenadeWeaponName` where it strikes, stuck
//!   there, which goes off on that grenade's own fuse with that grenade's blast;
//! * a sticky grenade sticks and goes off on its fuse;
//! * a blast hurts by BO2's falloff, inner damage at the centre to outer at the radius, over the
//!   distance to his bounds (t6mp `FUN_005f1fd0`: the nearest point of the entity's box, not its
//!   origin or middle).
//!
//! Used by zhbo-assets' install test over every projectile weapon the player's BO2 install
//! converts to, and by this crate's tests on synthetic defs.

use std::cell::{Cell, RefCell};

use crate::host::{objflags, LogicHost, ObjInfo};
use crate::math::Vec3;
use crate::projectiles::{Outcome, Projectile};
use crate::units::{inches, BO2_TO_ZH_DAMAGE, LOGIC_DT};
use crate::weapons::{Effect, WeaponDef};

/// The distances (inches) the soldier stands at.
pub const RANGES: [f32; 3] = [150.0, 600.0, 1200.0];
/// The shooter and the soldier.
const SHOOTER: u32 = 1;
const TARGET: u32 = 5;
/// The soldier's BO2 health, in Zero Hour's damage.
const HEALTH: f32 = 100.0 * BO2_TO_ZH_DAMAGE;
/// The soldier's cylinder (a General's) and the launch height (his middle).
const HEIGHT: f32 = 12.0;
const MUZZLE_Z: f32 = HEIGHT * 0.5;
/// The longest a shot is followed.
const MAX_SECONDS: f32 = 15.0;

/// Which defs [`check`] measures: a gun whose pull launches a projectile, and every thrown offhand
/// that is not set down as equipment.
pub fn is_projectile(def: &WeaponDef) -> bool {
    let thrown = def.class.is_offhand() && def.projectile_speed > 0.0 && crate::equipment::Kind::of(def) == crate::equipment::Kind::Other && !def.plantable;
    (def.class.is_gun() && def.launches()) || thrown
}

/// A flat range: the ground at 0, the shooter and (maybe) the soldier.
struct Range {
    objects: RefCell<Vec<ObjInfo>>,
    frame: Cell<u32>,
    /// (frame, victim, amount).
    hits: RefCell<Vec<(u32, u32, f32)>>,
}

impl Range {
    fn new(target_at: Option<f32>) -> Range {
        let body = |id: u32, player: i32, x: f32| ObjInfo {
            id,
            owner_player: player,
            flags: objflags::INFANTRY,
            geom_type: crate::host::geom::CYLINDER,
            major_radius: 10.0,
            minor_radius: 10.0,
            height: HEIGHT,
            health: HEALTH,
            max_health: HEALTH,
            pos: Vec3::new(x, 0.0, 0.0),
            ..Default::default()
        };
        let mut objects = vec![body(SHOOTER, 0, 0.0)];
        objects.extend(target_at.map(|x| body(TARGET, 1, x)));
        Range { objects: RefCell::new(objects), frame: Cell::new(0), hits: RefCell::new(Vec::new()) }
    }

    fn target(&self) -> Option<ObjInfo> {
        self.objects.borrow().iter().find(|o| o.id == TARGET).copied()
    }
}

impl LogicHost for Range {
    fn log(&self, _msg: &str) {}
    fn ground_height(&self, _x: f32, _y: f32) -> f32 {
        0.0
    }
    fn is_walkable(&self, _x: f32, _y: f32) -> bool {
        true
    }
    fn extent(&self) -> (Vec3, Vec3) {
        (Vec3::new(-5000.0, -5000.0, -100.0), Vec3::new(5000.0, 5000.0, 1000.0))
    }
    fn object(&self, id: u32, _viewer: u32) -> Option<ObjInfo> {
        self.objects.borrow().iter().find(|o| o.id == id).copied()
    }
    fn query(&self, center: Vec3, radius: f32, _viewer: u32, out: &mut Vec<ObjInfo>) {
        out.extend(self.objects.borrow().iter().filter(|o| o.in_query(center, radius)).copied());
    }
    fn set_pose(&self, _id: u32, _pos: Vec3, _yaw: f32, _moving: bool) {}
    fn set_controlled(&self, _id: u32, _controlled: bool) {}
    fn fire_fx(&self, _shooter: u32, _fx_weapon: &str, _target: u32, _pos: Vec3) {}
    fn damage(&self, _shooter: u32, victim: u32, amount: f32, _kind: i32) -> bool {
        self.hits.borrow_mut().push((self.frame.get(), victim, amount));
        let mut objs = self.objects.borrow_mut();
        let Some(v) = objs.iter_mut().find(|o| o.id == victim) else { return false };
        v.health -= amount;
        if v.health <= 0.0 && !v.has(objflags::DEAD) {
            v.flags |= objflags::DEAD;
            return true;
        }
        false
    }
    fn heal(&self, _id: u32, _amount: f32) {}
    fn spawn(&self, _template: &str, _owner: u32, _pos: Vec3, _yaw: f32) -> u32 {
        0
    }
    fn destroy(&self, _id: u32) {}
    fn disable(&self, _id: u32, _kind: i32, _seconds: f32) {}
    fn fx(&self, _name: &str, _pos: Vec3) {}
    fn capture(&self, _id: u32, _new_owner: u32) {}
    fn smoke(&self, _center: Vec3, _radius: f32) {}
}

fn launch(def: &'static WeaponDef, host: &Range, pitch: f32, fuse: f32) -> Projectile {
    let dir = Vec3::new(pitch.cos(), 0.0, pitch.sin());
    Projectile::launch(def, SHOOTER, Vec3::new(0.0, 0.0, MUZZLE_Z), dir, fuse, host, 0.0)
}

/// The launch pitch whose flight (on an empty range) passes `x` nearest the height `z` before it
/// first touches the ground, and the path flown and seconds taken to there; `None` when nothing
/// reaches `x`.
fn aim(def: &'static WeaponDef, x: f32, z: f32) -> Option<Aim> {
    search(def, |prev, p| {
        (p.pos.x >= x && prev.x < x).then(|| {
            let t = (x - prev.x) / (p.pos.x - prev.x).max(1e-6);
            let path = p.travelled - (p.pos - prev).length() * (1.0 - t);
            ((prev.z + (p.pos.z - prev.z) * t - z).abs(), path)
        })
    })
}

/// The launch pitch whose flight (on an empty range) first meets the ground nearest `x`, and the
/// path flown and seconds taken to there.
fn aim_ground(def: &'static WeaponDef, x: f32) -> Option<Aim> {
    search(def, |_, p| (p.pos.z <= 0.0).then(|| ((p.pos.x - x).abs(), p.travelled)))
}

/// A launch pitch, and the path flown and seconds taken to the point aimed at.
#[derive(Clone, Copy, Debug)]
struct Aim {
    pitch: f32,
    path: f32,
    time: f32,
}

/// Over launch pitches, the one whose flight `measure` scores lowest (a miss under 1 unit), and its
/// path. `measure` sees each move (from, the projectile after it) until it answers or the flight
/// ends.
fn search(def: &'static WeaponDef, measure: impl Fn(Vec3, &Projectile) -> Option<(f32, f32)>) -> Option<Aim> {
    let mut best: Option<(f32, Aim)> = None;
    for i in 0..=1800 {
        let pitch = -0.6 + i as f32 * 0.001;
        let host = Range::new(None);
        let mut p = launch(def, &host, pitch, f32::INFINITY);
        for _ in 0..(MAX_SECONDS / LOGIC_DT) as usize {
            let prev = p.pos;
            let done = p.step(&host).is_some();
            if let Some((miss, path)) = measure(prev, &p) {
                if best.is_none_or(|(m, _)| miss < m) {
                    best = Some((miss, Aim { pitch, path, time: p.age }));
                }
                break;
            }
            if done || p.pos.z <= 0.0 || p.resting {
                break;
            }
        }
    }
    best.filter(|(miss, _)| *miss < 1.0).map(|(_, aim)| aim)
}

/// Fly `def` at `pitch` on `host` until everything it leaves is spent, as the sim does: a bolt's
/// spawned grenade (`resolve`) carries on where it struck.
fn fly(def: &'static WeaponDef, host: &Range, pitch: f32, resolve: &dyn Fn(&str) -> Option<&'static WeaponDef>) -> Result<(), String> {
    let mut live = vec![launch(def, host, pitch, def.fuse_time)];
    for frame in 1..=(MAX_SECONDS / LOGIC_DT) as u32 {
        host.frame.set(frame);
        let mut next = Vec::new();
        for mut p in live {
            match p.step(host) {
                None => next.push(p),
                Some(Outcome::Spawned { at, stuck_to, .. }) => {
                    let grenade = resolve(&p.def.spawns).ok_or_else(|| format!("its spawned grenade {:?} is not in the table", p.def.spawns))?;
                    next.push(p.spawned(grenade, at, stuck_to, host));
                }
                Some(_) => {}
            }
        }
        live = next;
        if live.is_empty() {
            break;
        }
    }
    Ok(())
}

/// The blast `def` (or the grenade it spawns) sets off, if any.
fn blast_def(def: &'static WeaponDef, resolve: &dyn Fn(&str) -> Option<&'static WeaponDef>) -> Option<&'static WeaponDef> {
    let d = if def.spawns.is_empty() { def } else { resolve(&def.spawns)? };
    let explodes = d.impact_explode || d.sticky || !def.spawns.is_empty();
    (explodes && d.effect == Effect::Explode && d.explosion_radius > 0.0).then_some(d)
}

fn near(got: f32, want: f32) -> bool {
    (got - want).abs() <= 0.5 + want.abs() * 0.05
}

/// Every way `def`'s projectile hurts differently from its BO2 definition (empty if none).
/// `resolve` finds a spawned grenade's def by id (the table the def came from).
pub fn check(def: &'static WeaponDef, resolve: &dyn Fn(&str) -> Option<&'static WeaponDef>) -> Vec<String> {
    let mut fails = Vec::new();
    if !is_projectile(def) {
        return fails;
    }
    let mut fail = |s: String| fails.push(format!("{}: {s}", def.id));
    let impact = def.damage[0] * BO2_TO_ZH_DAMAGE;
    let spawned = (!def.spawns.is_empty()).then(|| resolve(&def.spawns));
    if let Some(None) = spawned {
        fail(format!("its spawned grenade {:?} is not in the table", def.spawns));
        return fails;
    }
    let spawned = spawned.flatten();
    let blast = blast_def(def, resolve);
    // What a strike leads to, by BO2's data.
    let bounces = def.spawns.is_empty() && !def.impact_explode && !def.sticky && def.effect != Effect::Impact;
    for range in RANGES {
        let x = inches(range);
        // Straight at him.
        let Some(Aim { pitch, path, time }) = aim(def, x, MUZZLE_Z) else { continue };
        // A fuse that runs out on the way (the smoke grenade's 10 ms) goes off before any strike.
        let fuse_first = def.fuse_time > 0.0 && !def.impact_explode && def.effect != Effect::Impact && def.fuse_time < time;
        let host = Range::new(Some(x));
        if let Err(e) = fly(def, &host, pitch, resolve) {
            fail(e);
            return fails;
        }
        let hits: Vec<(u32, f32)> = host.hits.borrow().iter().filter(|h| h.1 == TARGET).map(|h| (h.0, h.2)).collect();
        let at = format!("at {range} in");
        let Some(&(strike, first)) = hits.first() else {
            if (impact > 0.0 || blast.is_some()) && !fuse_first {
                fail(format!("{at}: a direct hit did no damage (BO2's impact damage is {})", def.damage[0]));
            }
            continue;
        };
        let armed = def.activate_dist <= 0.0 || path >= def.activate_dist;
        if def.impact_explode && armed {
            // The blast goes off on him: at least the impact damage (BO2 applies `iDamage` to what
            // it struck and spares it the radius).
            if first + 0.5 < impact {
                fail(format!("{at}: the direct hit dealt {first:.1}, BO2's impact damage is {impact:.1}"));
            }
        } else if !near(first, impact) {
            fail(format!("{at}: the strike dealt {first:.1}, BO2's impact damage is {impact:.1}"));
        }
        // The grenade a bolt leaves in him, or a Semtex stuck on him, goes off on its fuse.
        let fused = match spawned {
            Some(g) => Some((g, strike as f32 * LOGIC_DT + g.fuse_time)),
            None if def.sticky && def.effect == Effect::Explode => Some((def, def.fuse_time)),
            None => None,
        };
        if let Some((g, _)) = fused.filter(|(g, _)| g.fuse_time <= 0.0) {
            fail(format!("{at}: {} sticks and has no fuse: it never goes off", g.id));
            continue;
        }
        let mut bo2_total = def.damage[0];
        if let Some((g, when)) = fused {
            let want = g.explosion_damage_at(0.0) * BO2_TO_ZH_DAMAGE;
            bo2_total += g.explosion_damage_at(0.0);
            let dead = hits.iter().map(|h| h.1).sum::<f32>() >= HEALTH;
            let blast = hits.iter().skip(1).find(|h| h.1 > 0.0);
            match blast {
                Some(&(f, amount)) => {
                    if (f as f32 * LOGIC_DT - when).abs() > 2.0 * LOGIC_DT {
                        fail(format!("{at}: the stuck {} went off at {:.2} s, BO2's fuse says {when:.2} s", g.id, f as f32 * LOGIC_DT));
                    }
                    if !near(amount, want) {
                        fail(format!("{at}: the stuck {} dealt {amount:.1}, BO2's blast at its centre is {want:.1}", g.id));
                    }
                }
                None if !dead || first < HEALTH => fail(format!("{at}: the stuck {} never went off", g.id)),
                None => {}
            }
        }
        // BO2's verdict on a 100-health player (a bouncing grenade's own blast lands wherever it
        // rolls, so only its strike is measured).
        if !bounces {
            let killed = host.target().is_some_and(|t| t.has(objflags::DEAD));
            if killed != (bo2_total >= 100.0) {
                fail(format!("{at}: killed {killed}, BO2's {bo2_total:.0} damage on a 100-health player says {}", bo2_total >= 100.0));
            }
        }
        // Into the ground half the blast radius short of him: BO2's falloff over the distance to
        // his bounds (`FUN_005f1fd0`), the side of his soldier-sized cylinder a blast on the
        // ground beside him meets.
        let Some(b) = blast else { continue };
        let short = b.explosion_radius * 0.5;
        if x - short < 1.0 {
            continue;
        }
        let Some(Aim { pitch, path, .. }) = aim_ground(def, x - short) else { continue };
        let host = Range::new(Some(x));
        if let Err(e) = fly(def, &host, pitch, resolve) {
            fail(e);
            return fails;
        }
        let dist = (short - crate::hitscan::INFANTRY_HIT_RADIUS).max(0.0);
        let dud = def.impact_explode && def.activate_dist > 0.0 && path < def.activate_dist;
        let want = if dud { 0.0 } else { b.explosion_damage_at(dist) * BO2_TO_ZH_DAMAGE };
        let got: f32 = host.hits.borrow().iter().filter(|h| h.1 == TARGET).map(|h| h.2).sum();
        if !near(got, want) {
            fail(format!("{at}: a blast {:.0} in from him dealt {got:.1}, BO2's falloff says {want:.1}{}", dist / inches(1.0), if dud { " (a dud)" } else { "" }));
        }
    }
    fails
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leak(d: WeaponDef) -> &'static WeaponDef {
        Box::leak(Box::new(d))
    }

    fn damage(d: f32) -> [f32; 6] {
        let mut out = [0.0; 6];
        out[0] = d;
        out
    }

    /// BO2's crossbow and the explosive bolt it spawns (`crossbow_mp`: iDamage 40, `projExplosion`
    /// bolt, spawns `explosive_bolt_mp`: fuse 1 s, radius 100 in, 150 to 25).
    fn crossbow() -> (&'static WeaponDef, &'static WeaponDef) {
        let bolt = leak(WeaponDef {
            id: "test_explosive_bolt_mp".into(),
            class: crate::weapons::Class::Lethal,
            effect: Effect::Explode,
            sticky: true,
            fuse_time: 1.0,
            projectile_speed: inches(900.0),
            explosion_radius: inches(100.0),
            explosion_inner_damage: 150.0,
            explosion_outer_damage: 25.0,
            damage: damage(5.0),
            ..Default::default()
        });
        let crossbow = leak(WeaponDef {
            id: "test_crossbow_mp".into(),
            class: crate::weapons::Class::Special,
            effect: Effect::Explode,
            sticky: true,
            spawns: bolt.id.clone(),
            projectile_speed: inches(4500.0),
            projectile_speed_up: inches(90.0),
            explosion_radius: inches(1.0),
            explosion_inner_damage: 1.0,
            explosion_outer_damage: 1.0,
            damage: damage(40.0),
            ..Default::default()
        });
        (crossbow, bolt)
    }

    #[test]
    fn the_crossbow_bolt_strikes_then_its_spawned_bolt_goes_off_and_kills() {
        let (crossbow, bolt) = crossbow();
        let resolve = |id: &str| (id == bolt.id).then_some(bolt);
        let fails = check(crossbow, &resolve);
        assert!(fails.is_empty(), "{fails:#?}");
        // And by hand, at 600 in: the strike's 40, then the bolt's 150 a second later.
        let x = inches(600.0);
        let pitch = aim(crossbow, x, MUZZLE_Z).unwrap().pitch;
        let host = Range::new(Some(x));
        fly(crossbow, &host, pitch, &resolve).unwrap();
        let hits = host.hits.borrow().clone();
        assert_eq!(hits.len(), 2, "{hits:?}");
        assert_eq!(hits[0].2, 40.0 * BO2_TO_ZH_DAMAGE);
        assert!(((hits[1].0 - hits[0].0) as f32 * LOGIC_DT - bolt.fuse_time).abs() <= LOGIC_DT, "{hits:?}");
        assert!(near(hits[1].2, 150.0 * BO2_TO_ZH_DAMAGE), "{hits:?}");
        assert!(host.target().unwrap().has(objflags::DEAD));
    }

    /// The bug the playtest found: a bolt that only stuck (no spawned grenade, no fuse) did nothing.
    #[test]
    fn a_crossbow_without_its_spawned_grenade_is_caught() {
        let (crossbow, bolt) = crossbow();
        let mut broken = crossbow.clone();
        broken.spawns.clear();
        let fails = check(leak(broken), &|id: &str| (id == bolt.id).then_some(bolt));
        assert!(fails.iter().any(|f| f.contains("never goes off")), "{fails:#?}");
        // Its table must carry the spawned grenade.
        let fails = check(crossbow, &|_| None);
        assert!(fails.iter().any(|f| f.contains("not in the table")), "{fails:#?}");
    }

    #[test]
    fn a_rocket_kills_on_a_direct_hit_and_a_launcher_grenade_duds_up_close() {
        let rocket = leak(WeaponDef {
            id: "test_rocket_mp".into(),
            class: crate::weapons::Class::Launcher,
            effect: Effect::Explode,
            impact_explode: true,
            projectile_gravity: false,
            projectile_speed: inches(2500.0),
            explosion_radius: inches(225.0),
            explosion_inner_damage: 150.0,
            explosion_outer_damage: 70.0,
            damage: damage(1000.0),
            ..Default::default()
        });
        let fails = check(rocket, &|_| None);
        assert!(fails.is_empty(), "{fails:#?}");
        let gl = leak(WeaponDef {
            id: "test_gl_mp".into(),
            class: crate::weapons::Class::Launcher,
            effect: Effect::Explode,
            impact_explode: true,
            activate_dist: inches(300.0),
            projectile_speed: inches(1800.0),
            projectile_speed_up: inches(10.0),
            explosion_radius: inches(256.0),
            explosion_inner_damage: 135.0,
            explosion_outer_damage: 50.0,
            damage: damage(135.0),
            ..Default::default()
        });
        let fails = check(gl, &|_| None);
        assert!(fails.is_empty(), "{fails:#?}");
    }

    #[test]
    fn a_thrown_grenade_strikes_for_its_impact_damage() {
        let frag = crate::weapons::weapon("frag_grenade_mp");
        assert!(is_projectile(frag));
        let fails = check(frag, &|_| None);
        assert!(fails.is_empty(), "{fails:#?}");
        let semtex = leak(WeaponDef { id: "test_semtex_mp".into(), sticky: true, ..frag.clone() });
        let fails = check(semtex, &|_| None);
        assert!(fails.is_empty(), "{fails:#?}");
    }

    #[test]
    fn built_in_projectiles_hurt_by_their_definitions() {
        let resolve = |id: &str| crate::weapons::catalog().get(id);
        let fails: Vec<String> = crate::weapons::catalog().all().iter().flat_map(|d| check(d, &resolve)).collect();
        assert!(fails.is_empty(), "{fails:#?}");
    }
}
