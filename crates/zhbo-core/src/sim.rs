//! The deterministic, lockstep part of the mod: one [`PlayerSim`] per player whose General is under
//! first-person control. Driven by `zhbo_logic_*` calls from `GameLogic`; must produce identical
//! results on every client given identical inputs.

use crate::host::{geom, objflags, LogicHost, ObjInfo};
use crate::hitscan::{self, HitResult, Shape, Target, INFANTRY_HEAD_FRACTION, INFANTRY_HIT_RADIUS};
use crate::input::{buttons, FpsInput};
use crate::math::{basis_xy, dir_from_angles, Vec3};
use crate::movement::{self, Blocker, Body, MoveModifiers, MoveTuning, World};
use crate::units::{BO2_TO_ZH_DAMAGE, LOGIC_DT};
use crate::weapons::{Arsenal, Rng, WeaponAction, MELEE_DAMAGE, MELEE_RANGE};

pub const MAX_PLAYERS: usize = 16;

/// How long the field computer takes to come up before control returns to the RTS view (BO2's
/// laptop raise for scorestreaks is about this long).
pub const FIELD_COMPUTER_RAISE: f32 = 0.7;
/// BO2 health regeneration: wait this long after the last hit, then regenerate.
pub const REGEN_DELAY: f32 = 5.0;
pub const REGEN_FULL_TIME: f32 = 3.0;
/// Hard cap on queued client events so a headless or paused client cannot grow it forever.
const MAX_EVENTS: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Fps,
    /// Field computer coming up; `t` counts seconds since it was opened.
    FieldComputer { t: f32 },
    /// The General is back under normal RTS control (player is using the field computer view).
    Rts,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EventKind {
    EnteredFps { general: u32 },
    FieldComputerOpened,
    ReturnedToRts { general: u32 },
    Shot { origin: Vec3, end: Vec3 },
    HitMarker { headshot: bool, kill: bool },
    Recoil { pitch_deg: f32, yaw_deg: f32 },
    Damaged { amount: f32 },
    ReloadStarted,
    Swapped,
    GeneralDied { general: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub player: i32,
    pub kind: EventKind,
}

#[derive(Clone, Debug)]
pub struct PlayerSim {
    pub general: u32,
    pub mode: Mode,
    pub body: Body,
    pub yaw: f32,
    pub pitch: f32,
    pub arsenal: Arsenal,
    pub rng: Rng,
    pub input: FpsInput,
    pressed_accum: u32,
    pub health: f32,
    pub max_health: f32,
    pub regen_wait: f32,
}

impl PlayerSim {
    pub fn is_first_person(&self) -> bool {
        matches!(self.mode, Mode::Fps | Mode::FieldComputer { .. })
    }
}

pub struct Sim {
    pub players: Vec<Option<PlayerSim>>,
    pub tuning: MoveTuning,
    pub frame: u32,
    pub events: Vec<Event>,
}

impl Default for Sim {
    fn default() -> Self {
        Self::new()
    }
}

impl Sim {
    pub fn new() -> Self {
        Self { players: vec![None; MAX_PLAYERS], tuning: MoveTuning::default(), frame: 0, events: Vec::new() }
    }

    pub fn reset(&mut self) {
        *self = Sim::new();
    }

    fn emit(&mut self, player: usize, kind: EventKind) {
        if self.events.len() >= MAX_EVENTS {
            self.events.remove(0);
        }
        self.events.push(Event { player: player as i32, kind });
    }

    pub fn player(&self, player: i32) -> Option<&PlayerSim> {
        self.players.get(usize::try_from(player).ok()?)?.as_ref()
    }

    /// Ids of Generals currently driven by first-person input (their AI and physics are suspended).
    pub fn controlled_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.players.iter().flatten().filter(|p| p.is_first_person()).map(|p| p.general)
    }

    /// `MSG_ZHBO_TAKE_CONTROL`: the player clicked "Take Control" on their General.
    pub fn take_control(&mut self, host: &dyn LogicHost, player: i32, general: u32) {
        let Ok(idx) = usize::try_from(player) else { return };
        if idx >= MAX_PLAYERS {
            return;
        }
        let Some(info) = host.object(general, general) else { return };
        if info.has(objflags::DEAD) || info.owner_player != player {
            return;
        }
        // One General per player in first person; release a previous one.
        if let Some(prev) = self.players[idx].as_ref() {
            if prev.general != general && prev.is_first_person() {
                host.set_controlled(prev.general, false);
            }
        }
        let tuning = self.tuning;
        let frame = self.frame;
        let state = match self.players[idx].take() {
            Some(mut p) if p.general == general => {
                p.body = Body::new(info.pos, &tuning);
                p.mode = Mode::Fps;
                p.yaw = info.yaw;
                p.pitch = 0.0;
                p.input = FpsInput { yaw: info.yaw, ..Default::default() };
                p
            }
            _ => PlayerSim {
                general,
                mode: Mode::Fps,
                body: Body::new(info.pos, &tuning),
                yaw: info.yaw,
                pitch: 0.0,
                arsenal: Arsenal::new(),
                rng: Rng::new(general.wrapping_mul(2_654_435_761) ^ frame),
                input: FpsInput { yaw: info.yaw, ..Default::default() },
                pressed_accum: 0,
                health: info.health,
                max_health: info.max_health,
                regen_wait: 0.0,
            },
        };
        self.players[idx] = Some(state);
        host.set_controlled(general, true);
        self.emit(idx, EventKind::EnteredFps { general });
    }

    /// `MSG_ZHBO_FPS_INPUT`: latest input for a player's General.
    pub fn input(&mut self, player: i32, general: u32, input: FpsInput) {
        let Ok(idx) = usize::try_from(player) else { return };
        if let Some(Some(p)) = self.players.get_mut(idx) {
            if p.general == general {
                let input = input.sanitized();
                p.pressed_accum |= input.pressed;
                p.input = input;
            }
        }
    }

    /// Once per logic frame, after the frame's messages were dispatched.
    pub fn step(&mut self, host: &dyn LogicHost, frame: u32) {
        self.frame = frame;
        for idx in 0..self.players.len() {
            let Some(mut p) = self.players[idx].take() else { continue };
            let keep = self.step_player(host, idx, &mut p);
            if keep {
                self.players[idx] = Some(p);
            }
        }
    }

    fn step_player(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim) -> bool {
        let Some(info) = host.object(p.general, p.general) else {
            self.emit(idx, EventKind::GeneralDied { general: p.general });
            return false;
        };
        if info.has(objflags::DEAD) {
            host.set_controlled(p.general, false);
            self.emit(idx, EventKind::GeneralDied { general: p.general });
            return false;
        }
        if !p.is_first_person() {
            return true;
        }

        // Something else moved the General (teleport, crate, script): follow it.
        if (info.pos - p.body.pos).length_xy() > 8.0 {
            p.body.pos = info.pos;
            p.body.vel = Vec3::ZERO;
        }

        self.track_health(host, idx, p, &info);

        let mut input = p.input;
        input.pressed = p.pressed_accum;
        p.pressed_accum = 0;
        p.yaw = input.yaw;
        p.pitch = input.pitch;

        let world = HostWorld { host, viewer: p.general };
        match p.mode {
            Mode::Fps => {
                if input.pressed & buttons::FIELD_COMPUTER != 0 {
                    p.mode = Mode::FieldComputer { t: 0.0 };
                    p.arsenal.ads = 0.0;
                    self.emit(idx, EventKind::FieldComputerOpened);
                } else {
                    let mods = MoveModifiers {
                        speed_scale: p.arsenal.move_scale(),
                        block_sprint: p.arsenal.ads > 0.0,
                    };
                    movement::step(&mut p.body, &input, mods, &self.tuning, &world, LOGIC_DT);
                    let mut actions = Vec::new();
                    p.arsenal.step(input.held, input.pressed, p.body.sprinting, &mut p.rng, &mut actions);
                    for a in actions {
                        self.apply_weapon_action(host, idx, p, a);
                    }
                }
            }
            Mode::FieldComputer { t } => {
                // Stand still while the laptop comes up; gravity still applies.
                let idle = FpsInput { yaw: input.yaw, pitch: input.pitch, ..Default::default() };
                movement::step(&mut p.body, &idle, MoveModifiers::default(), &self.tuning, &world, LOGIC_DT);
                let t = t + LOGIC_DT;
                if t >= FIELD_COMPUTER_RAISE {
                    p.mode = Mode::Rts;
                    host.set_pose(p.general, p.body.pos, p.yaw, false);
                    host.set_controlled(p.general, false);
                    self.emit(idx, EventKind::ReturnedToRts { general: p.general });
                    return true;
                }
                p.mode = Mode::FieldComputer { t };
            }
            Mode::Rts => return true,
        }

        let moving = p.body.horizontal_speed() > 1.0;
        host.set_pose(p.general, p.body.pos, p.yaw, moving);
        true
    }

    fn track_health(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, info: &ObjInfo) {
        p.max_health = info.max_health;
        if info.health < p.health - 1e-3 {
            p.regen_wait = REGEN_DELAY;
            self.emit(idx, EventKind::Damaged { amount: p.health - info.health });
        } else {
            p.regen_wait = (p.regen_wait - LOGIC_DT).max(0.0);
            if p.regen_wait <= 0.0 && info.health < info.max_health {
                let amount = (info.max_health / REGEN_FULL_TIME * LOGIC_DT).min(info.max_health - info.health);
                host.heal(p.general, amount);
                p.health = info.health + amount;
                return;
            }
        }
        p.health = info.health;
    }

    fn apply_weapon_action(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, action: WeaponAction) {
        match action {
            WeaponAction::Fire { spread_deg, jitter, recoil } => {
                let def = p.arsenal.current().def;
                let eye = p.body.eye();
                // Random point in the spread cone (polar mapping of two uniform samples).
                let angle = jitter.0 * std::f32::consts::PI;
                let radius = spread_deg.to_radians() * jitter.1.abs().sqrt();
                let dir = dir_from_angles(p.yaw + radius * angle.cos(), p.pitch + radius * angle.sin());

                let targets = gather_targets(host, eye, def.max_range, p.general);
                let ground = |x: f32, y: f32| host.ground_height(x, y);
                let hit = hitscan::trace(eye, dir, def.max_range, &targets, p.general, &ground);
                match hit {
                    HitResult::Object { id, point, distance, headshot } => {
                        host.fire_fx(p.general, def.zh_fx_weapon, id, point);
                        let mult = if headshot { def.head_multiplier } else { 1.0 };
                        let amount = def.damage_at(distance) * mult * BO2_TO_ZH_DAMAGE;
                        let kill = host.damage(p.general, id, amount, false);
                        self.emit(idx, EventKind::HitMarker { headshot, kill });
                    }
                    other => host.fire_fx(p.general, def.zh_fx_weapon, 0, other.point()),
                }
                self.emit(idx, EventKind::Shot { origin: eye, end: hit.point() });
                self.emit(idx, EventKind::Recoil { pitch_deg: recoil.0, yaw_deg: recoil.1 });
            }
            WeaponAction::Melee => {
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
                    let kill = host.damage(p.general, v.id, MELEE_DAMAGE * BO2_TO_ZH_DAMAGE, true);
                    self.emit(idx, EventKind::HitMarker { headshot: false, kill });
                }
            }
            WeaponAction::ReloadStarted => self.emit(idx, EventKind::ReloadStarted),
            WeaponAction::Swapped => self.emit(idx, EventKind::Swapped),
            WeaponAction::DryFire | WeaponAction::ReloadFinished => {}
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::input::pack_move;
    use std::cell::RefCell;

    /// A flat world with a set of objects; records engine calls.
    #[derive(Default)]
    pub struct FakeHost {
        pub objects: RefCell<Vec<ObjInfo>>,
        pub controlled: RefCell<Vec<u32>>,
        pub fx: RefCell<Vec<(u32, String, u32)>>,
        pub damage_log: RefCell<Vec<(u32, u32, f32)>>,
    }

    impl FakeHost {
        pub fn add(&self, info: ObjInfo) {
            self.objects.borrow_mut().push(info);
        }

        pub fn get(&self, id: u32) -> ObjInfo {
            *self.objects.borrow().iter().find(|o| o.id == id).unwrap()
        }

        fn rel(&self, mut o: ObjInfo, viewer: u32) -> ObjInfo {
            let owner = self.objects.borrow().iter().find(|v| v.id == viewer).map(|v| v.owner_player);
            o.flags &= !(objflags::ALLY | objflags::ENEMY);
            if let Some(owner) = owner {
                o.flags |= if o.owner_player == owner { objflags::ALLY } else { objflags::ENEMY };
            }
            o
        }
    }

    impl LogicHost for FakeHost {
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
        fn object(&self, id: u32, viewer: u32) -> Option<ObjInfo> {
            self.objects.borrow().iter().find(|o| o.id == id).map(|o| self.rel(*o, viewer))
        }
        fn query(&self, center: Vec3, radius: f32, viewer: u32, out: &mut Vec<ObjInfo>) {
            for o in self.objects.borrow().iter() {
                if (o.pos - center).length() <= radius {
                    out.push(self.rel(*o, viewer));
                }
            }
        }
        fn set_pose(&self, id: u32, pos: Vec3, yaw: f32, _moving: bool) {
            for o in self.objects.borrow_mut().iter_mut().filter(|o| o.id == id) {
                o.pos = pos;
                o.yaw = yaw;
            }
        }
        fn set_controlled(&self, id: u32, controlled: bool) {
            let mut c = self.controlled.borrow_mut();
            c.retain(|&x| x != id);
            if controlled {
                c.push(id);
            }
        }
        fn fire_fx(&self, shooter: u32, fx_weapon: &str, target: u32, _pos: Vec3) {
            self.fx.borrow_mut().push((shooter, fx_weapon.to_string(), target));
        }
        fn damage(&self, shooter: u32, victim: u32, amount: f32, _melee: bool) -> bool {
            self.damage_log.borrow_mut().push((shooter, victim, amount));
            let mut objs = self.objects.borrow_mut();
            let v = objs.iter_mut().find(|o| o.id == victim).unwrap();
            v.health -= amount;
            if v.health <= 0.0 {
                v.flags |= objflags::DEAD;
                return true;
            }
            false
        }
        fn heal(&self, id: u32, amount: f32) {
            for o in self.objects.borrow_mut().iter_mut().filter(|o| o.id == id) {
                o.health = (o.health + amount).min(o.max_health);
            }
        }
    }

    pub fn general(id: u32, player: i32) -> ObjInfo {
        ObjInfo {
            id,
            owner_player: player,
            flags: objflags::INFANTRY,
            geom_type: geom::CYLINDER,
            major_radius: 10.0,
            minor_radius: 10.0,
            height: 12.0,
            health: 300.0,
            max_health: 300.0,
            ..Default::default()
        }
    }

    pub fn soldier(id: u32, player: i32, x: f32) -> ObjInfo {
        ObjInfo { pos: Vec3::new(x, 0.0, 0.0), health: 120.0, max_health: 120.0, ..general(id, player) }
    }

    fn run(sim: &mut Sim, host: &FakeHost, player: i32, general: u32, input: FpsInput, frames: u32) {
        for _ in 0..frames {
            sim.input(player, general, input);
            let f = sim.frame + 1;
            sim.step(host, f);
        }
    }

    fn events(sim: &Sim) -> Vec<EventKind> {
        sim.events.iter().map(|e| e.kind).collect()
    }

    #[test]
    fn take_control_suspends_ai_and_moves_general() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        assert_eq!(*host.controlled.borrow(), vec![1]);
        run(&mut sim, &host, 0, 1, FpsInput { move_packed: pack_move(1.0, 0.0), ..Default::default() }, 30);
        assert!(host.get(1).pos.x > 20.0, "walked forward: {:?}", host.get(1).pos);
        assert_eq!(sim.controlled_ids().collect::<Vec<_>>(), vec![1]);
    }

    #[test]
    fn cannot_control_someone_elses_general() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 1, 1);
        assert!(sim.player(1).is_none());
        assert!(host.controlled.borrow().is_empty());
    }

    #[test]
    fn shooting_kills_an_enemy_and_reports_hitmarkers() {
        let host = FakeHost::default();
        let mut g = general(1, 0);
        g.pos = Vec3::new(0.0, 0.0, 0.0);
        host.add(g);
        host.add(soldier(2, 1, 60.0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        // Let the weapon raise, then aim (ADS for accuracy) and fire. Eye is at 10 units; soldier
        // body centre is at 6, so aim slightly down.
        let pitch = -(4.0f32 / 60.0).atan();
        let aim = FpsInput { held: buttons::ADS, pitch, ..Default::default() };
        run(&mut sim, &host, 0, 1, aim, 20);
        let fire = FpsInput { held: buttons::ADS | buttons::FIRE, pitch, ..Default::default() };
        run(&mut sim, &host, 0, 1, fire, 20);
        assert!(host.get(2).has(objflags::DEAD), "enemy killed; damage {:?}", host.damage_log.borrow());
        let ev = events(&sim);
        assert!(ev.iter().any(|e| matches!(e, EventKind::HitMarker { kill: true, .. })));
        assert!(host.fx.borrow().iter().all(|(s, w, _)| *s == 1 && w == "ZHBO_MTAR_FX"));
    }

    #[test]
    fn bullets_pass_through_friendlies() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(soldier(3, 0, 30.0));
        host.add(soldier(2, 1, 60.0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        let pitch = -(4.0f32 / 60.0).atan();
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS, pitch, ..Default::default() }, 20);
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS | buttons::FIRE, pitch, ..Default::default() }, 10);
        assert_eq!(host.get(3).health, 120.0, "friendly untouched");
        assert!(host.get(2).health < 120.0, "enemy behind was hit");
    }

    #[test]
    fn field_computer_returns_control_to_rts_and_back() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::FIELD_COMPUTER, ..Default::default() }, 1);
        assert!(matches!(sim.player(0).unwrap().mode, Mode::FieldComputer { .. }));
        assert_eq!(*host.controlled.borrow(), vec![1], "still controlled while the laptop comes up");
        run(&mut sim, &host, 0, 1, FpsInput::default(), 25);
        assert_eq!(sim.player(0).unwrap().mode, Mode::Rts);
        assert!(host.controlled.borrow().is_empty(), "AI resumed");
        assert!(events(&sim).contains(&EventKind::ReturnedToRts { general: 1 }));
        sim.take_control(&host, 0, 1);
        assert_eq!(sim.player(0).unwrap().mode, Mode::Fps);
    }

    #[test]
    fn health_regenerates_after_delay_and_death_releases() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        run(&mut sim, &host, 0, 1, FpsInput::default(), 1);
        host.objects.borrow_mut()[0].health = 150.0;
        run(&mut sim, &host, 0, 1, FpsInput::default(), 30 * 4);
        assert!(host.get(1).health < 151.0, "no regen during the delay");
        run(&mut sim, &host, 0, 1, FpsInput::default(), 30 * 5);
        assert_eq!(host.get(1).health, 300.0, "regenerated to full");
        host.objects.borrow_mut()[0].flags |= objflags::DEAD;
        run(&mut sim, &host, 0, 1, FpsInput::default(), 1);
        assert!(sim.player(0).is_none());
        assert!(events(&sim).contains(&EventKind::GeneralDied { general: 1 }));
    }

    #[test]
    fn identical_inputs_give_identical_results() {
        let make = || {
            let host = FakeHost::default();
            host.add(general(1, 0));
            host.add(soldier(2, 1, 80.0));
            host.add(soldier(4, 1, 90.0));
            let mut sim = Sim::new();
            sim.take_control(&host, 0, 1);
            for i in 0..200u32 {
                let input = FpsInput {
                    held: if i % 7 < 4 { buttons::FIRE } else { buttons::SPRINT },
                    move_packed: pack_move(((i % 11) as f32 / 5.0) - 1.0, ((i % 5) as f32 / 2.0) - 1.0),
                    yaw: (i as f32 * 0.01).sin(),
                    pitch: -0.05,
                    ..Default::default()
                };
                sim.input(0, 1, input);
                sim.step(&host, i + 1);
            }
            let hits = host.damage_log.borrow().len();
            (host.get(1).pos, host.get(2).health, host.get(4).health, hits)
        };
        assert_eq!(make(), make());
    }
}
