//! Does a blast reach and hurt Zero Hour's objects by BO2's radius damage? Sets `def` off beside
//! one object of each class the mod's battlefield holds (a soldier, a light and a heavy vehicle, a
//! hovering aircraft, a structure) at several distances from its solid, and measures each damage
//! against what BO2's data and engine imply:
//!
//! * the blast measures each entity to its bounds, not its origin (t6mp `FUN_005f2130` calls
//!   `FUN_005f1fd0`: the distance to the entity's box, clamped per axis), so the object is reached
//!   whenever its solid is inside the radius, however far its centre is (the host's query returns
//!   every object whose bounding sphere comes within it, docs/CONTRACT.md);
//! * the damage falls off from the inner damage at the solid to the outer at the radius
//!   (`(1 - d / r) * (inner - outer) + outer`), and nothing past the radius is touched;
//! * an impact-fuse direct hit deals the struck object its impact damage (`iDamage`) and skips it in
//!   the radius damage (`FUN_00608c10`), a soldier all of it and anything else BO2's
//!   `getvehicleprojectilescalar` share (`_globallogic_vehicle.gsc`, from the tuning pack), while
//!   its neighbours take the blast by their own distance;
//! * a stun (the concussion grenade) lasts `2 + 4 * (1 - d / r)` seconds over the distance from the
//!   blast to the soldier's origin (`_weapons.gsc:1366-1372`);
//! * every hit carries BO2's means of death (`FUN_00608c10`, the missile handlers): the radius its
//!   handler's splash means, a direct hit its direct means, and on anything but a soldier BO2's
//!   vehicle rule for that means (`_globallogic_vehicle.gsc`: C4's blast x30 by
//!   `getvehicleunderneathsplashscalar`, a direct hit by `getvehicleprojectilescalar`);
//! * a sticky grenade stuck to a vehicle strikes it for its inner damage by its direct means and
//!   spares it the radius (`FUN_0060a480`); stuck to a soldier, the radius takes him as any other;
//! * every strike in flight ([`check_strikes`]: launched straight at each class, short of an impact
//!   fuse's arming distance and past it) deals `iDamage` by its means — `MOD_IMPACT` for a dud or a
//!   def that does not go off on impact, else the direct means — under the vehicle rule for it.
//!
//! All in BO2's damage times `BO2_TO_ZH_DAMAGE`; Zero Hour's armour then scales it in the engine by
//! the damage type the tuning pack's `[MeansOfDeath]` table names for the means (docs/CONTRACT.md,
//! "Damage"). Used by zhbo-assets' install test over every explosive and every projectile the
//! player's BO2 install converts to, and by this crate's tests on synthetic defs.

use std::cell::RefCell;

use crate::damage::MeansOfDeath;
use crate::host::{geom, objflags, LogicHost, ObjInfo};
use crate::math::Vec3;
use crate::projectiles::{Hit, Outcome, Projectile};
use crate::units::{BO2_TO_ZH_DAMAGE, LOGIC_DT};
use crate::weapons::{Effect, WeapType, WeaponDef};

/// The thrower, far behind every blast.
const OWNER: u32 = 1;
/// The object measured, and its neighbour in the direct-hit group.
const TARGET: u32 = 5;
const NEIGHBOUR: u32 = 6;
/// The shares of the radius the object's solid stands at.
pub const SHARES: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 0.95];

/// One kind of Zero Hour object a blast meets: its geometry (stock Zero Hour values of a typical
/// unit of the class) and how high above the ground it is.
#[derive(Clone, Copy, Debug)]
pub struct TargetClass {
    pub name: &'static str,
    pub flags: u32,
    pub geom_type: i32,
    pub major_radius: f32,
    pub minor_radius: f32,
    pub height: f32,
    /// Its position's height above the ground (an aircraft hovers).
    pub altitude: f32,
}

/// The classes swept: Zero Hour's infantry (a Ranger's cylinder) and a first-person General (the
/// mod's General, infantry too: BO2's player), a Humvee and an Overlord, a Comanche hovering, and
/// a War Factory-sized structure.
pub const CLASSES: [TargetClass; 6] = [
    TargetClass { name: "infantry", flags: objflags::INFANTRY, geom_type: geom::CYLINDER, major_radius: 7.0, minor_radius: 7.0, height: 13.0, altitude: 0.0 },
    TargetClass { name: "General", flags: objflags::INFANTRY, geom_type: geom::CYLINDER, major_radius: 10.0, minor_radius: 10.0, height: 12.0, altitude: 0.0 },
    TargetClass { name: "light vehicle", flags: objflags::VEHICLE, geom_type: geom::BOX, major_radius: 14.0, minor_radius: 7.0, height: 11.5, altitude: 0.0 },
    TargetClass { name: "heavy vehicle", flags: objflags::VEHICLE, geom_type: geom::BOX, major_radius: 26.0, minor_radius: 13.0, height: 15.0, altitude: 0.0 },
    TargetClass { name: "aircraft", flags: objflags::VEHICLE | objflags::AIRCRAFT | objflags::AIRBORNE, geom_type: geom::BOX, major_radius: 20.0, minor_radius: 6.0, height: 10.0, altitude: 40.0 },
    TargetClass { name: "structure", flags: objflags::STRUCTURE, geom_type: geom::BOX, major_radius: 60.0, minor_radius: 45.0, height: 40.0, altitude: 0.0 },
];

impl TargetClass {
    fn soldier(&self) -> bool {
        self.flags & objflags::INFANTRY != 0
    }

    /// How far the solid reaches from the position along +x (yaw 0): a soldier's is BO2's
    /// soldier-sized cylinder ([`crate::hitscan::INFANTRY_HIT_RADIUS`]), anything else its
    /// Zero Hour geometry.
    fn half_x(&self) -> f32 {
        if self.soldier() {
            crate::hitscan::INFANTRY_HIT_RADIUS.min(self.major_radius)
        } else {
            self.major_radius
        }
    }

    /// The object of id `id` whose solid's near side is `gap` past `x` along +x, enemy to the owner.
    fn at(&self, id: u32, x: f32, gap: f32) -> ObjInfo {
        ObjInfo {
            id,
            owner_player: 1,
            flags: self.flags | objflags::ENEMY,
            geom_type: self.geom_type,
            major_radius: self.major_radius,
            minor_radius: self.minor_radius,
            height: self.height,
            health: 1.0e6,
            max_health: 1.0e6,
            pos: Vec3::new(x + gap + self.half_x(), 0.0, self.altitude),
            ..Default::default()
        }
    }

    /// How far the solid reaches from the position across, along +y.
    fn half_y(&self) -> f32 {
        if self.soldier() || self.geom_type != geom::BOX {
            self.half_x()
        } else {
            self.minor_radius
        }
    }

    /// The object as [`TargetClass::at`] (`x` 0), or turned side-on: its long side `gap` off the
    /// blast across +y, the blast level with its middle along its length.
    fn placed(&self, id: u32, gap: f32, side_on: bool) -> ObjInfo {
        let o = self.at(id, 0.0, gap);
        if side_on {
            ObjInfo { pos: Vec3::new(0.0, gap + self.half_y(), self.altitude), ..o }
        } else {
            o
        }
    }

    /// Where a blast against its near side goes off: level with its middle.
    fn blast_height(&self) -> f32 {
        self.altitude + self.height * 0.5
    }
}

/// A flat world holding the owner and the objects, recording damage (victim, amount, means of
/// death) and stuns.
struct Field {
    objects: Vec<ObjInfo>,
    hits: RefCell<Vec<(u32, f32, MeansOfDeath)>>,
    stuns: RefCell<Vec<(u32, f32)>>,
}

impl Field {
    fn new(objects: Vec<ObjInfo>) -> Field {
        let owner = ObjInfo { id: OWNER, owner_player: 0, flags: objflags::INFANTRY, geom_type: geom::CYLINDER, major_radius: 10.0, height: 12.0, pos: Vec3::new(-1.0e4, 0.0, 0.0), ..Default::default() };
        let mut all = vec![owner];
        all.extend(objects);
        Field { objects: all, hits: RefCell::new(Vec::new()), stuns: RefCell::new(Vec::new()) }
    }

    fn damage_to(&self, id: u32) -> Vec<f32> {
        self.hits.borrow().iter().filter(|h| h.0 == id).map(|h| h.1).collect()
    }

    /// The means of death of each hit on `id`, in order.
    fn means_to(&self, id: u32) -> Vec<MeansOfDeath> {
        self.hits.borrow().iter().filter(|h| h.0 == id).map(|h| h.2).collect()
    }
}

impl LogicHost for Field {
    fn log(&self, _msg: &str) {}
    fn ground_height(&self, _x: f32, _y: f32) -> f32 {
        0.0
    }
    fn is_walkable(&self, _x: f32, _y: f32) -> bool {
        true
    }
    fn extent(&self) -> (Vec3, Vec3) {
        (Vec3::new(-2.0e4, -2.0e4, -100.0), Vec3::new(2.0e4, 2.0e4, 1000.0))
    }
    fn object(&self, id: u32, _viewer: u32) -> Option<ObjInfo> {
        self.objects.iter().find(|o| o.id == id).copied()
    }
    fn query(&self, center: Vec3, radius: f32, _viewer: u32, out: &mut Vec<ObjInfo>) {
        out.extend(self.objects.iter().filter(|o| o.in_query(center, radius)).copied());
    }
    fn set_pose(&self, _id: u32, _pos: Vec3, _yaw: f32, _moving: bool) {}
    fn set_controlled(&self, _id: u32, _controlled: bool) {}
    fn fire_fx(&self, _shooter: u32, _fx_weapon: &str, _target: u32, _pos: Vec3) {}
    fn damage(&self, _shooter: u32, victim: u32, amount: f32, blow: crate::damage::Blow) -> bool {
        self.hits.borrow_mut().push((victim, amount, blow.how));
        false
    }
    fn heal(&self, _id: u32, _amount: f32) {}
    fn spawn(&self, _template: &str, _owner: u32, _pos: Vec3, _yaw: f32) -> u32 {
        0
    }
    fn destroy(&self, _id: u32) {}
    fn disable(&self, id: u32, _kind: i32, seconds: f32) {
        self.stuns.borrow_mut().push((id, seconds));
    }
    fn fx(&self, _name: &str, _pos: Vec3) {}
    fn capture(&self, _id: u32, _new_owner: u32) {}
    fn smoke(&self, _center: Vec3, _radius: f32) {}
}

/// Which defs [`check`] measures: everything that goes off with a radius and hurts or stuns there.
pub fn is_explosive(def: &WeaponDef) -> bool {
    def.explosion_radius > 0.0 && match def.effect {
        Effect::Explode => def.explosion_inner_damage > 0.0 || def.explosion_outer_damage > 0.0,
        Effect::Stun => true,
        _ => false,
    }
}

/// BO2's means of death for `def`'s radius damage, its direct hit and its strike when it does not
/// go off on impact (or is a dud): the missile handler's (t6mp's table at `0x00c8c0e4`: a grenade's
/// 3/4, a rocket's 5/6) and `FUN_00608c10`'s `MOD_IMPACT`. Written out from BO2's engine here, the
/// expected side for [`crate::damage`]'s.
fn bo2_means(def: &WeaponDef) -> (MeansOfDeath, MeansOfDeath) {
    if def.traits.weap_type == WeapType::Grenade {
        (MeansOfDeath::GrenadeSplash, MeansOfDeath::Grenade)
    } else {
        (MeansOfDeath::ProjectileSplash, MeansOfDeath::Projectile)
    }
}

/// BO2's vehicle rule (`_globallogic_vehicle.gsc`) for `def` hitting by `how`, read key by key
/// from the tuning pack: a splash means x `getvehicleprojectilesplashscalar` (`:224`), then
/// `MOD_PROJECTILE`/`MOD_GRENADE` x `getvehicleprojectilescalar` (`:56`), `MOD_GRENADE_SPLASH` x
/// `getvehicleunderneathsplashscalar` (`:63`), and everything x `vehicledamagescalar` (`:70`);
/// 1 on a soldier.
fn bo2_rule(def: &WeaponDef, how: MeansOfDeath, soldier: bool) -> f32 {
    use crate::tuning::{VEHICLE_PROJECTILE_SCALAR, VEHICLE_PROJECTILE_SPLASH_SCALAR, VEHICLE_UNDERNEATH_SPLASH_SCALAR};
    if soldier {
        return 1.0;
    }
    let t = crate::tuning::get();
    let key = |prefix: &str| t.per_weapon(prefix, &def.id);
    let splash = matches!(how, MeansOfDeath::ProjectileSplash | MeansOfDeath::GrenadeSplash | MeansOfDeath::Explosive);
    let mut scale = if splash { key(VEHICLE_PROJECTILE_SPLASH_SCALAR) } else { 1.0 };
    match how {
        MeansOfDeath::Projectile | MeansOfDeath::Grenade => scale *= key(VEHICLE_PROJECTILE_SCALAR),
        MeansOfDeath::GrenadeSplash => scale *= key(VEHICLE_UNDERNEATH_SPLASH_SCALAR),
        _ => {}
    }
    scale * t.vehicle_damage_scalar
}

/// BO2's radius damage at `dist` from the solid (`FUN_005f2130`), Zero Hour's damage; 0 past it.
fn falloff(def: &WeaponDef, dist: f32) -> f32 {
    let r = def.explosion_radius;
    if dist >= r {
        return 0.0;
    }
    ((1.0 - dist / r) * (def.explosion_inner_damage - def.explosion_outer_damage) + def.explosion_outer_damage) * BO2_TO_ZH_DAMAGE
}

fn near(got: f32, want: f32) -> bool {
    (got - want).abs() <= 0.01 + want.abs() * 1e-3
}

/// Whether the def's radius hurts (a stun hurts only when it carries real damage: the Shock
/// Charge, [`crate::projectiles::blast`]).
fn hurts(def: &WeaponDef) -> bool {
    match def.effect {
        Effect::Explode => true,
        Effect::Stun => def.explosion_inner_damage > 10.0,
        _ => false,
    }
}

/// Every way `def`'s blast departs from BO2's radius damage against Zero Hour's objects (empty if
/// none).
pub fn check(def: &'static WeaponDef) -> Vec<String> {
    let mut fails = Vec::new();
    if !is_explosive(def) {
        return fails;
    }
    let tuning = crate::tuning::get();
    let r = def.explosion_radius;
    let thrown = crate::equipment::Kind::of(def) == crate::equipment::Kind::Other;
    for class in CLASSES {
        let mut fail = |s: String| fails.push(format!("{} vs {}: {s}", def.id, class.name));
        // Off its end and off its side at each share of the radius, and just past it.
        for (share, side_on) in SHARES.into_iter().chain([1.05]).flat_map(|s| [(s, false), (s, true)]) {
            let gap = r * share;
            let face = if side_on { "side" } else { "end" };
            let target = class.placed(TARGET, gap, side_on);
            let field = Field::new(vec![target]);
            let at = Vec3::new(0.0, 0.0, class.blast_height());
            crate::projectiles::blast(&field, def, OWNER, at, Hit::NONE, 1.0, 1.0, &[]);
            let got: f32 = field.damage_to(TARGET).iter().sum();
            let (splash, _) = bo2_means(def);
            let want = if hurts(def) { falloff(def, gap) * bo2_rule(def, splash, class.soldier()) } else { 0.0 };
            if !near(got, want) {
                fail(format!("a blast {:.0}% of its radius off its {face} dealt {got:.1}, BO2's radius damage says {want:.1} ({})", share * 100.0, splash.name()));
            }
            if let Some(m) = field.means_to(TARGET).into_iter().find(|m| *m != splash) {
                fail(format!("a blast off its {face} hit by {}, BO2's radius damage is {}", m.name(), splash.name()));
            }
            // A thrown stun, on a soldier: BO2's time over the distance to his origin (the
            // engine's paralysis for anything else is the mod's own).
            if def.effect == Effect::Stun && class.soldier() && thrown {
                let stun: Vec<f32> = field.stuns.borrow().iter().filter(|s| s.0 == TARGET).map(|s| s.1).collect();
                let origin = (target.pos - at).length();
                let want = (gap < r).then(|| tuning.concussion_time_min + tuning.concussion_time_scale * (1.0 - origin / r).clamp(0.0, 1.0));
                match (stun.first(), want) {
                    (Some(&got), Some(want)) if (got - want).abs() > 1e-3 => fail(format!("stunned {got:.2} s at {:.0}% of its radius, BO2's concussion says {want:.2} s", share * 100.0)),
                    (None, Some(want)) => fail(format!("not stunned at {:.0}% of its radius, BO2's concussion says {want:.2} s", share * 100.0)),
                    (Some(&got), None) => fail(format!("stunned {got:.2} s past its radius")),
                    _ => {}
                }
            }
        }
        // A direct hit on it in a tight group (an impact fuse): it takes its strike only, the
        // neighbour behind it the blast by its own gap.
        if def.impact_explode && def.effect == Effect::Explode {
            let gap = r * 0.25;
            let struck = class.at(TARGET, 0.0, 0.0);
            let behind = class.at(NEIGHBOUR, struck.pos.x + class.half_x(), gap);
            let field = Field::new(vec![struck, behind]);
            let at = Vec3::new(0.0, 0.0, class.blast_height());
            crate::projectiles::blast(&field, def, OWNER, at, Hit::direct(struck), 1.0, 1.0, &[]);
            let (splash, direct) = bo2_means(def);
            let scale = bo2_rule(def, direct, class.soldier());
            let want = def.damage[0] * scale * BO2_TO_ZH_DAMAGE;
            let got = field.damage_to(TARGET);
            if got.len() != 1 || !near(got[0], want) {
                fail(format!("the struck one took {got:?}, BO2's strike is {want:.1} (iDamage {} x {scale}) and no radius damage", def.damage[0]));
            }
            if field.means_to(TARGET) != [direct] {
                fail(format!("the struck one was hit by {:?}, BO2's direct hit is {}", field.means_to(TARGET), direct.name()));
            }
            let got: f32 = field.damage_to(NEIGHBOUR).iter().sum();
            let want = falloff(def, gap + 2.0 * class.half_x()) * bo2_rule(def, splash, class.soldier());
            if !near(got, want) {
                fail(format!("the one behind the struck one took {got:.1}, BO2's radius damage at its hull says {want:.1}"));
            }
        }
        // Stuck to it (a Semtex, C4, the crossbow's bolt grenade): a vehicle takes the inner
        // damage by the direct means and no radius (`FUN_0060a480`), a soldier the radius.
        if def.sticky && def.effect == Effect::Explode {
            let stuck = class.at(TARGET, 0.0, 0.0);
            let field = Field::new(vec![stuck]);
            let at = Vec3::new(0.0, 0.0, class.blast_height());
            crate::projectiles::blast(&field, def, OWNER, at, Hit { direct: None, stuck_to: Some(TARGET) }, 1.0, 1.0, &[]);
            let (splash, direct) = bo2_means(def);
            let how = if class.soldier() { splash } else { direct };
            let want = def.explosion_inner_damage * bo2_rule(def, how, class.soldier()) * BO2_TO_ZH_DAMAGE;
            let got = field.damage_to(TARGET);
            if got.len() != 1 || !near(got[0], want) || field.means_to(TARGET) != [how] {
                fail(format!("stuck to it, it dealt {got:?} by {:?}, BO2 says {want:.1} by {}", field.means_to(TARGET), how.name()));
            }
        }
    }
    fails
}

/// Which defs [`check_strikes`] flies: anything launched or thrown that deals impact damage.
pub fn strikes(def: &WeaponDef) -> bool {
    def.damage[0] > 0.0 && def.projectile_speed > 0.0 && (def.launches() || def.class.is_offhand())
}

/// One flight of `def` straight at a `class` object whose near side is `gap` ahead of the muzzle:
/// every hit on it, in order (the bolt's spawned grenade, `resolve`, carries on where it struck).
fn fly_at(def: &'static WeaponDef, class: TargetClass, gap: f32, resolve: &dyn Fn(&str) -> Option<&'static WeaponDef>) -> Field {
    let target = class.at(TARGET, 0.0, gap);
    let field = Field::new(vec![target]);
    let from = Vec3::new(0.0, 0.0, class.blast_height());
    let mut live = vec![Projectile::launch(def, OWNER, from, Vec3::new(1.0, 0.0, 0.0), def.fuse_time, &field, 0.0)];
    for _ in 0..(15.0 / LOGIC_DT) as u32 {
        let mut next = Vec::new();
        for mut p in live {
            match p.step(&field) {
                None => next.push(p),
                Some(Outcome::Spawned { at, stuck_to, .. }) => {
                    if let Some(g) = resolve(&p.def.spawns) {
                        next.push(p.spawned(g, at, stuck_to, &field));
                    }
                }
                Some(_) => {}
            }
        }
        live = next;
        if live.is_empty() {
            break;
        }
    }
    field
}

/// Every way `def`'s strikes in flight depart from BO2's against each class of Zero Hour object
/// (empty if none): launched straight at it from just short of it (short of an impact fuse's
/// arming distance, a dud) and from past that distance, the first hit is `iDamage` by
/// `FUN_00608c10`'s means (`MOD_IMPACT` for a dud or a def that does not go off on impact, else the
/// handler's direct means) under BO2's vehicle rule for it; a sticky grenade it leaves on a
/// vehicle then strikes it by its direct means, on a soldier by its radius.
pub fn check_strikes(def: &'static WeaponDef, resolve: &dyn Fn(&str) -> Option<&'static WeaponDef>) -> Vec<String> {
    let mut fails = Vec::new();
    if !strikes(def) {
        return fails;
    }
    let arming = def.activate_dist;
    let mut gaps = vec![(arming * 0.5).clamp(1.0, 15.0)];
    if def.impact_explode && arming > 0.0 {
        gaps.push(arming + 10.0);
    }
    for class in CLASSES {
        for &gap in &gaps {
            let mut fail = |s: String| fails.push(format!("{} vs {} from {gap:.0} away: {s}", def.id, class.name));
            let field = fly_at(def, class, gap, resolve);
            let dud = def.impact_explode && arming > 0.0 && gap < arming;
            let (_, direct) = bo2_means(def);
            let how = if dud || !def.impact_explode { MeansOfDeath::Impact } else { direct };
            let want = def.damage[0] * bo2_rule(def, how, class.soldier()) * BO2_TO_ZH_DAMAGE;
            let hits = field.hits.borrow().iter().filter(|h| h.0 == TARGET).map(|h| (h.1, h.2)).collect::<Vec<_>>();
            let Some(&(got, got_how)) = hits.first() else {
                fail(format!("it never struck (BO2's strike: {want:.1} by {})", how.name()));
                continue;
            };
            if got_how != how || !near(got, want) {
                fail(format!("it struck for {got:.1} by {}, BO2's strike is {want:.1} by {} (iDamage {})", got_how.name(), how.name(), def.damage[0]));
            }
            // What it leaves stuck in a vehicle strikes by its direct means (placed equipment, C4,
            // waits for its owner: `equipment`'s blast, [`check`]'s stuck case).
            let stuck = if def.spawns.is_empty() { def.sticky.then_some(def) } else { resolve(&def.spawns).filter(|g| g.sticky) };
            let stuck = stuck.filter(|g| crate::equipment::Kind::of(g) == crate::equipment::Kind::Other);
            if let Some(g) = stuck.filter(|g| g.effect == Effect::Explode && !class.soldier() && !(def.impact_explode && !dud)) {
                let (_, g_direct) = bo2_means(g);
                let want = g.explosion_inner_damage * bo2_rule(g, g_direct, false) * BO2_TO_ZH_DAMAGE;
                match hits.get(1) {
                    Some(&(got, h)) if h == g_direct && near(got, want) => {}
                    other => fail(format!("the stuck {} dealt {other:?}, BO2 strikes the vehicle it is stuck to for {want:.1} by {}", g.id, g_direct.name())),
                }
            }
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

    /// BO2's SMAW (`smaw_mp`: iDamage 1000, impact fuse, radius 225 in, 150 to 70).
    fn smaw() -> &'static WeaponDef {
        leak(WeaponDef {
            id: "smaw_mp".into(),
            effect: Effect::Explode,
            impact_explode: true,
            damage: damage(1000.0),
            explosion_radius: crate::units::inches(225.0),
            explosion_inner_damage: 150.0,
            explosion_outer_damage: 70.0,
            ..Default::default()
        })
    }

    fn typed(weap_type: WeapType, d: WeaponDef) -> &'static WeaponDef {
        let mut d = d;
        d.traits.weap_type = weap_type;
        leak(d)
    }

    /// BO2's C4 (`satchel_charge_mp`: radius 256 in, 200 to 65; `stickiness` all_orient).
    fn c4() -> &'static WeaponDef {
        typed(WeapType::Grenade, WeaponDef {
            id: "satchel_charge_mp".into(),
            class: crate::weapons::Class::Equipment,
            effect: Effect::Explode,
            sticky: true,
            damage: damage(15.0),
            explosion_radius: crate::units::inches(256.0),
            explosion_inner_damage: 200.0,
            explosion_outer_damage: 65.0,
            ..Default::default()
        })
    }

    /// BO2's Semtex (`sticky_grenade_mp`: iDamage 15, sticks, fuse 2.5 s, radius 256 in, 185 to 50).
    fn semtex() -> &'static WeaponDef {
        typed(WeapType::Grenade, WeaponDef {
            id: "sticky_grenade_mp".into(),
            class: crate::weapons::Class::Lethal,
            effect: Effect::Explode,
            sticky: true,
            fuse_time: 2.5,
            damage: damage(15.0),
            projectile_speed: crate::units::inches(920.0),
            explosion_radius: crate::units::inches(256.0),
            explosion_inner_damage: 185.0,
            explosion_outer_damage: 50.0,
            ..Default::default()
        })
    }

    /// An under-barrel grenade (`gl_an94_mp`: iDamage 135, impact fuse armed after 300 in).
    fn launcher_grenade() -> &'static WeaponDef {
        typed(WeapType::Projectile, WeaponDef {
            id: "gl_an94_mp".into(),
            class: crate::weapons::Class::Launcher,
            effect: Effect::Explode,
            impact_explode: true,
            activate_dist: crate::units::inches(300.0),
            damage: damage(135.0),
            projectile_speed: crate::units::inches(1800.0),
            projectile_speed_up: crate::units::inches(10.0),
            projectile_gravity: true,
            explosion_radius: crate::units::inches(256.0),
            explosion_inner_damage: 135.0,
            explosion_outer_damage: 50.0,
            ..Default::default()
        })
    }

    /// The combat axe (`hatchet_mp`: iDamage 125, a grenade that strikes and falls).
    fn axe() -> &'static WeaponDef {
        typed(WeapType::Grenade, WeaponDef {
            id: "hatchet_mp".into(),
            class: crate::weapons::Class::Lethal,
            effect: Effect::Impact,
            damage: damage(125.0),
            projectile_speed: crate::units::inches(1600.0),
            projectile_gravity: true,
            ..Default::default()
        })
    }

    /// A frag (iDamage 15, bounces, fuse 3.5 s).
    fn frag() -> &'static WeaponDef {
        typed(WeapType::Grenade, WeaponDef {
            id: "frag_grenade_mp".into(),
            class: crate::weapons::Class::Lethal,
            effect: Effect::Explode,
            fuse_time: 3.5,
            damage: damage(15.0),
            projectile_speed: crate::units::inches(920.0),
            projectile_gravity: true,
            explosion_radius: crate::units::inches(256.0),
            explosion_inner_damage: 200.0,
            explosion_outer_damage: 75.0,
            ..Default::default()
        })
    }

    fn none(_: &str) -> Option<&'static WeaponDef> {
        None
    }

    #[test]
    fn every_strike_hits_every_class_by_bo2s_means_of_death() {
        for d in [smaw(), launcher_grenade(), semtex(), axe(), frag(), c4()] {
            let fails = check(d);
            assert!(fails.is_empty(), "{fails:#?}");
            let fails = check_strikes(d, &none);
            assert!(fails.is_empty(), "{fails:#?}");
        }
        let all = crate::weapons::catalog().all();
        let resolve = |id: &str| crate::weapons::catalog().get(id);
        let flown: Vec<&str> = all.iter().filter(|d| strikes(d)).map(|d| d.id.as_str()).collect();
        assert!(flown.contains(&"frag_grenade_mp") && flown.contains(&"gl_an94_mp"), "{flown:?}");
        let fails: Vec<String> = all.iter().flat_map(|d| check_strikes(d, &resolve)).collect();
        assert!(fails.is_empty(), "{fails:#?}");
    }

    /// The report's cases on a tank (the heavy vehicle): a frag, an axe, a dud launcher grenade
    /// strike it by `MOD_IMPACT`, which BO2's vehicle callback takes whole (and the table sends as
    /// a blast, not as Zero Hour's small arms); an armed one by `MOD_PROJECTILE`.
    #[test]
    fn strikes_on_a_tank_are_impacts_not_small_arms() {
        let tank = CLASSES[3];
        let table = crate::tuning::get().means_of_death;
        for (d, gap, how, want) in [
            (frag(), 10.0, MeansOfDeath::Impact, 15.0),
            (axe(), 10.0, MeansOfDeath::Impact, 125.0),
            (launcher_grenade(), 10.0, MeansOfDeath::Impact, 135.0),
            (launcher_grenade(), crate::units::inches(300.0) + 10.0, MeansOfDeath::Projectile, 135.0),
            (smaw(), 10.0, MeansOfDeath::Projectile, 1000.0 * 0.2),
        ] {
            let field = fly_at(d, tank, gap, &none);
            let first = field.hits.borrow().iter().find(|h| h.0 == TARGET).copied();
            assert_eq!(first.map(|h| h.2), Some(how), "{}", d.id);
            assert!(near(first.unwrap().1, want * BO2_TO_ZH_DAMAGE), "{}: {first:?}", d.id);
            assert_eq!(table.zh_name(how), "EXPLOSION", "{} reaches TankArmor as a blast", how.name());
        }
    }

    /// C4's blast on a vehicle is BO2's x30 (`getvehicleunderneathsplashscalar`), wherever it lies;
    /// stuck to the vehicle it strikes for its inner damage by `MOD_GRENADE` instead (`FUN_0060a480`),
    /// and a soldier takes the plain blast.
    #[test]
    fn c4_wrecks_a_vehicle_by_bo2s_thirty_fold_splash() {
        let d = c4();
        let tank = CLASSES[3];
        let gap = d.explosion_radius * 0.5;
        let field = Field::new(vec![tank.at(TARGET, 0.0, gap)]);
        crate::projectiles::blast(&field, d, OWNER, Vec3::new(0.0, 0.0, 7.0), Hit::NONE, 1.0, 1.0, &[]);
        assert_eq!(field.means_to(TARGET), [MeansOfDeath::GrenadeSplash]);
        assert!(near(field.damage_to(TARGET)[0], falloff(d, gap) * 30.0), "{:?}", field.damage_to(TARGET));
        let field = Field::new(vec![tank.at(TARGET, 0.0, 0.0)]);
        crate::projectiles::blast(&field, d, OWNER, Vec3::new(0.0, 0.0, 7.0), Hit { direct: None, stuck_to: Some(TARGET) }, 1.0, 1.0, &[]);
        assert_eq!(field.means_to(TARGET), [MeansOfDeath::Grenade]);
        assert!(near(field.damage_to(TARGET)[0], 200.0 * BO2_TO_ZH_DAMAGE));
        let soldier = CLASSES[0];
        let field = Field::new(vec![soldier.at(TARGET, 0.0, gap)]);
        crate::projectiles::blast(&field, d, OWNER, Vec3::new(0.0, 0.0, 6.0), Hit::NONE, 1.0, 1.0, &[]);
        assert!(near(field.damage_to(TARGET)[0], falloff(d, gap)), "{:?}", field.damage_to(TARGET));
    }

    /// A rocket flying past a tank's side strikes its box, not a cylinder of its length: 5 units off
    /// the side it flies on, at the side it strikes (Zero Hour's box geometry, `hitscan::solid`).
    #[test]
    fn a_rocket_strikes_a_tank_by_its_box() {
        let d = leak(WeaponDef { projectile_speed: crate::units::inches(2500.0), projectile_gravity: false, ..smaw().clone() });
        let tank = CLASSES[3];
        for (side, hits) in [(tank.minor_radius + 5.0, false), (tank.minor_radius - 1.0, true)] {
            let target = ObjInfo { pos: Vec3::new(80.0, side, 0.0), ..tank.at(TARGET, 0.0, 0.0) };
            let field = Field::new(vec![target]);
            let mut p = Projectile::launch(d, OWNER, Vec3::new(0.0, 0.0, 7.0), Vec3::new(1.0, 0.0, 0.0), 0.0, &field, 0.0);
            let mut struck = false;
            for _ in 0..60 {
                if let Some(Outcome::Exploded { .. }) = p.step(&field) {
                    struck = !field.damage_to(TARGET).is_empty() && field.means_to(TARGET)[0] == MeansOfDeath::Projectile;
                    break;
                }
            }
            assert_eq!(struck, hits, "the rocket {} units off the axis: {:?} {:?} at {:?}", side, field.damage_to(TARGET), field.means_to(TARGET), p.pos);
        }
    }

    #[test]
    fn the_built_in_explosives_reach_every_class_by_bo2s_radius_damage() {
        let defs: Vec<_> = crate::weapons::catalog().all().iter().copied().filter(|d| is_explosive(d)).collect();
        assert!(defs.iter().any(|d| d.effect == Effect::Explode) && defs.iter().any(|d| d.effect == Effect::Stun), "{:?}", defs.iter().map(|d| &d.id).collect::<Vec<_>>());
        let fails: Vec<String> = defs.iter().flat_map(|d| check(d)).collect();
        assert!(fails.is_empty(), "{fails:#?}");
    }

    #[test]
    fn a_smaw_into_a_group_hurts_every_class_by_bo2s_rules() {
        let fails = check(smaw());
        assert!(fails.is_empty(), "{fails:#?}");
    }

    /// The report's case: a SMAW into a tank group. A tank beside the struck one, its hull 10 units
    /// away, takes the blast; so does a structure whose centre is well past the radius.
    #[test]
    fn a_tank_beside_the_struck_one_and_a_structure_take_the_blast() {
        let d = smaw();
        let tank = CLASSES[3];
        let struck = tank.at(TARGET, 0.0, 0.0);
        let beside = ObjInfo { id: NEIGHBOUR, pos: Vec3::new(struck.pos.x, 2.0 * tank.minor_radius + 10.0, 0.0), ..struck };
        let building = CLASSES[5].at(7, 0.0, 5.0);
        assert!(building.pos.x > d.explosion_radius + 12.0, "its centre is beyond the old query's reach");
        let field = Field::new(vec![struck, beside, building]);
        crate::projectiles::blast(&field, d, OWNER, Vec3::new(0.0, 0.0, 7.0), Hit::direct(struck), 1.0, 1.0, &[]);
        // The blast at the struck tank's near end (x 0): the one beside runs along it, its side 10
        // units off the struck one's, so its nearest point is across the struck one's half width
        // and the gap.
        let want_beside = falloff(d, tank.minor_radius + 10.0);
        assert!(near(field.damage_to(NEIGHBOUR).iter().sum(), want_beside), "{:?} vs {want_beside}", field.damage_to(NEIGHBOUR));
        assert!(want_beside > 0.0);
        assert!(near(field.damage_to(7).iter().sum(), falloff(d, 5.0)), "{:?}", field.damage_to(7));
        // The struck one: a fifth of 1000, BO2's vehicle rule for the SMAW.
        assert_eq!(field.damage_to(TARGET), vec![1000.0 * 0.2 * BO2_TO_ZH_DAMAGE]);
    }
}
