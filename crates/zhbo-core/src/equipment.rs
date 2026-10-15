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
//! BO2 sets these ranges and delays in script; the values here are the commonly documented ones.

use crate::hitscan::{ray_vs_target, Shape, Target};
use crate::host::{objflags, LogicHost, ObjInfo};
use crate::math::{basis_xy, Vec3};
use crate::projectiles::blast;
use crate::units::{inches, LOGIC_DT};
use crate::weapons::WeaponDef;

/// Seconds after it is down before a piece of equipment does anything.
pub const ARM_TIME: f32 = 1.0;
/// Claymore: an enemy inside this range and within 70 degrees of its front sets it off.
pub const CLAYMORE_RANGE: f32 = inches(144.0);
pub const CLAYMORE_COS: f32 = 0.342;
/// Bouncing Betty: an enemy inside this range sets it off; it hops up and goes off after the delay.
pub const BETTY_RANGE: f32 = inches(160.0);
pub const BETTY_DELAY: f32 = 0.6;
pub const BETTY_HOP: f32 = inches(40.0);
/// Shock Charge: an enemy inside this range sets it off.
pub const SHOCK_RANGE: f32 = inches(100.0);
/// Trophy System: shoots down enemy projectiles inside this range, this many times.
pub const TROPHY_RANGE: f32 = inches(384.0);
pub const TROPHY_CHARGES: u8 = 2;
/// Sensor Grenade: enemies inside this range show on its owner's minimap.
pub const SENSOR_RANGE: f32 = inches(800.0);
/// Engineer: triggered explosives wait this much longer for an Engineer.
pub const ENGINEER_DELAY: f32 = 0.5;
/// How big equipment is to bullets.
pub const HIT_RADIUS: f32 = inches(8.0);
/// Black Hat: range, aim cone and hack time for equipment and for Zero Hour drones.
pub const HACK_RANGE: f32 = inches(1200.0);
pub const HACK_COS: f32 = 0.96;
pub const HACK_TIME: f32 = 1.5;
pub const HACK_DRONE_TIME: f32 = 3.0;
/// BO2's effects for a piece of equipment that is shot, and for a Trophy System kill, as the
/// converter names them (`zhbo_assets::fx::fxlist_name`); the converter converts [`BO2_EFFECTS`].
pub const BREAK_FX: &str = "ZHBO_BO2FX_explosions_fx_exp_equipment";
pub const TROPHY_FX: &str = "ZHBO_BO2FX_explosions_fx_exp_equipment_lg";
pub const BO2_EFFECTS: &[&str] = &["explosions/fx_exp_equipment", "explosions/fx_exp_equipment_lg"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Claymore,
    Betty,
    C4,
    Shock,
    Trophy,
    Insertion,
    Sensor,
    /// Not equipment that stays down.
    Other,
}

impl Kind {
    pub fn of(def: &WeaponDef) -> Kind {
        match def.id.split('+').next().unwrap_or("") {
            "claymore_mp" => Kind::Claymore,
            "bouncingbetty_mp" => Kind::Betty,
            "satchel_charge_mp" => Kind::C4,
            "proximity_grenade_mp" => Kind::Shock,
            "trophy_system_mp" => Kind::Trophy,
            "tactical_insertion_mp" => Kind::Insertion,
            "sensor_grenade_mp" => Kind::Sensor,
            _ => Kind::Other,
        }
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
}

/// What happened to equipment this frame, for hit markers and sounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EquipEvent {
    Exploded { owner: usize, at: Vec3, hit_enemy: bool, kill: bool },
    /// Shot or blown up by an enemy.
    Destroyed { owner: usize, by: Option<usize>, at: Vec3 },
    /// A Trophy System shot a projectile down.
    Intercepted { owner: usize, at: Vec3 },
}

impl Placed {
    pub fn new(def: &'static WeaponDef, owner: usize, general: u32, object: u32, pos: Vec3, yaw: f32, stuck_to: Option<u32>) -> Placed {
        let kind = Kind::of(def);
        Placed { kind, def, owner, general, object, pos, yaw, age: 0.0, triggered: None, charges: TROPHY_CHARGES, stuck_to }
    }

    /// The object relationships are asked through: its own, or its owner General's.
    fn viewer(&self) -> u32 {
        if self.object != 0 {
            self.object
        } else {
            self.general
        }
    }

    pub fn is_armed(&self) -> bool {
        self.age >= ARM_TIME
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

    /// Blow it up where it is.
    fn explode(&mut self, host: &dyn LogicHost) -> EquipEvent {
        self.remove(host);
        let (hit_enemy, kill) = blast(host, self.def, self.general, self.pos, None, false, false);
        EquipEvent::Exploded { owner: self.owner, at: self.pos, hit_enemy, kill }
    }
}

/// Everything that is down, stepped once per logic frame.
#[derive(Clone, Debug, Default)]
pub struct Equipment {
    pub placed: Vec<Placed>,
}

impl Equipment {
    pub fn add(&mut self, p: Placed) {
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
            if let Some(t) = p.triggered.as_mut() {
                *t -= LOGIC_DT;
                if p.kind == Kind::Betty {
                    // The hop.
                    let ground = host.ground_height(p.pos.x, p.pos.y);
                    let up = ((BETTY_DELAY - *t) / BETTY_DELAY).clamp(0.0, 1.0);
                    if p.object != 0 {
                        host.set_pose(p.object, Vec3::new(p.pos.x, p.pos.y, ground + BETTY_HOP * up), p.yaw, false);
                    }
                }
                if *t <= 0.0 {
                    if p.kind == Kind::Betty {
                        p.pos.z = host.ground_height(p.pos.x, p.pos.y) + BETTY_HOP;
                    }
                    events.push(p.explode(host));
                    gone.push(i);
                }
                continue;
            }
            if !p.is_armed() {
                continue;
            }
            let delay = |o: &ObjInfo| if engineer(o.id) { ENGINEER_DELAY } else { 0.0 };
            match p.kind {
                Kind::Claymore => {
                    let (fwd, _) = basis_xy(p.yaw);
                    let tripped = p.enemies_near(host, CLAYMORE_RANGE).into_iter().find(|o| {
                        let to = o.pos - p.pos;
                        let d = to.length_xy().max(1e-3);
                        (to.x * fwd.x + to.y * fwd.y) / d >= CLAYMORE_COS
                    });
                    if let Some(o) = tripped {
                        p.triggered = Some(delay(&o));
                    }
                }
                Kind::Betty => {
                    if let Some(o) = p.enemies_near(host, BETTY_RANGE).first() {
                        p.triggered = Some(BETTY_DELAY + delay(o));
                    }
                }
                Kind::Shock => {
                    if let Some(o) = p.enemies_near(host, SHOCK_RANGE).first() {
                        p.triggered = Some(delay(o));
                    }
                }
                Kind::Trophy => {
                    // Enemy BO2 projectiles in flight.
                    let mut k = 0;
                    while k < projectiles.len() && p.charges > 0 {
                        let (_, proj) = &projectiles[k];
                        let hostile = host.object(proj.owner, p.viewer()).is_some_and(|o| o.has(objflags::ENEMY));
                        if hostile && (proj.pos - p.pos).length() <= TROPHY_RANGE && !proj.resting {
                            let (_, mut proj) = projectiles.remove(k);
                            proj.intercept(host);
                            host.fx(TROPHY_FX, proj.pos);
                            p.charges -= 1;
                            events.push(EquipEvent::Intercepted { owner: p.owner, at: proj.pos });
                            continue;
                        }
                        k += 1;
                    }
                    // Zero Hour's missiles and shells.
                    let mut objs = Vec::new();
                    host.query(p.pos, TROPHY_RANGE, p.viewer(), &mut objs);
                    objs.sort_by_key(|o| o.id);
                    for o in objs.iter().filter(|o| o.has(objflags::PROJECTILE) && o.has(objflags::ENEMY)) {
                        if p.charges == 0 {
                            break;
                        }
                        host.destroy(o.id);
                        host.fx(TROPHY_FX, o.pos);
                        p.charges -= 1;
                        events.push(EquipEvent::Intercepted { owner: p.owner, at: o.pos });
                    }
                    if p.charges == 0 {
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
    }

    /// The owner's detonator: every one of their C4 goes off.
    pub fn detonate(&mut self, host: &dyn LogicHost, owner: usize, events: &mut Vec<EquipEvent>) {
        let mut k = 0;
        while k < self.placed.len() {
            if self.placed[k].owner == owner && self.placed[k].kind == Kind::C4 {
                let mut p = self.placed.remove(k);
                events.push(p.explode(host));
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

    /// Enemy equipment caught in a blast at `at` breaks.
    pub fn caught_in_blast(&mut self, host: &dyn LogicHost, by: usize, general: u32, at: Vec3, radius: f32, events: &mut Vec<EquipEvent>) {
        let mut k = 0;
        while k < self.placed.len() {
            let p = &self.placed[k];
            if (p.pos - at).length() <= radius && p.hostile_to(host, general) {
                events.push(self.destroy(host, k, Some(by)));
            } else {
                k += 1;
            }
        }
    }

    /// Break the piece at `index` (shot, blown up).
    pub fn destroy(&mut self, host: &dyn LogicHost, index: usize, by: Option<usize>) -> EquipEvent {
        let mut p = self.placed.remove(index);
        host.fx(BREAK_FX, p.pos);
        p.remove(host);
        EquipEvent::Destroyed { owner: p.owner, by, at: p.pos }
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
                (d <= HACK_RANGE && d > 1e-3 && to.dot(dir) / d >= HACK_COS).then_some((i, d))
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

    /// Whether one of `owner`'s Sensor Grenades sees `pos`.
    pub fn sensed(&self, owner: usize, pos: Vec3) -> bool {
        self.placed.iter().any(|p| p.owner == owner && p.kind == Kind::Sensor && p.is_armed() && (p.pos - pos).length_xy() <= SENSOR_RANGE)
    }

    /// Remove everything a player put down (their General left the game).
    pub fn clear_owner(&mut self, host: &dyn LogicHost, owner: usize) {
        self.placed.retain_mut(|p| {
            if p.owner == owner && p.kind != Kind::Insertion {
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
        // One walks in front.
        host.objects.borrow_mut().iter_mut().find(|o| o.id == 7).unwrap().pos = Vec3::new(15.0, 0.0, 0.0);
        step_n(&mut eq, &host, 2, &mut events);
        assert!(matches!(events[0], EquipEvent::Exploded { hit_enemy: true, .. }), "{events:?}");
        assert!(eq.placed.is_empty());
        assert!(host.destroyed.borrow().contains(&50));
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
    use crate::sim::tests::{general, FakeHost};
    use crate::sim::{EventKind, Sim};
    use crate::weapons::{Class, Effect, Offhand, Place};

    fn equipment(id: &str, place: Place) -> &'static WeaponDef {
        Box::leak(Box::new(WeaponDef {
            id: id.into(),
            class: Class::Equipment,
            effect: Effect::Explode,
            place,
            start_reserve: 2,
            explosion_radius: 30.0,
            explosion_inner_damage: 200.0,
            explosion_outer_damage: 100.0,
            projectile_speed: 60.0,
            sticky: true,
            ..Default::default()
        }))
    }

    fn ready(sim: &mut Sim, host: &FakeHost) {
        sim.take_control(host, 0, 1);
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
        let black_hat = equipment("pda_hack_mp", Place::Hack);
        sim.players[0].as_mut().unwrap().arsenal.tactical = Offhand { def: black_hat, count: 1 };
        // Aim at it (eye height 10, it sits on the ground) and hold the Black Hat up.
        let pitch = -(10.0f32 / 40.0).atan();
        run(&mut sim, &host, FpsInput { held: buttons::TACTICAL, pressed: buttons::TACTICAL, pitch, ..Default::default() }, 1);
        let frames = (HACK_TIME / crate::units::LOGIC_DT) as u32 + 2;
        let mut hacked = false;
        for _ in 0..frames {
            run(&mut sim, &host, FpsInput { held: buttons::TACTICAL, pitch, ..Default::default() }, 1);
            hacked |= sim.events.iter().any(|e| e.kind == EventKind::Hacked);
        }
        assert!(hacked, "hacked within {frames} frames");
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
        assert_eq!(eq.placed[0].charges, TROPHY_CHARGES - 1);
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
}
