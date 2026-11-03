//! The fake engine world the sim's tests (and, behind the `harness` feature, the full-catalog
//! lockstep gate in `zhbo-assets`) run on: a flat world with a set of objects, recording every
//! engine call the sim makes.
use super::*;
use std::cell::RefCell;

/// A structure the fake host offers to work on.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FakeSite {
    pub id: u32,
    pub template: u32,
    pub at: Vec3,
    pub angle: f32,
    pub percent: f32,
    pub kind: i32,
    /// Someone else is repairing it.
    pub busy: bool,
}

/// A flat world with a set of objects; records engine calls.
#[derive(Default)]
pub struct FakeHost {
    pub objects: RefCell<Vec<ObjInfo>>,
    pub controlled: RefCell<Vec<u32>>,
    pub fx: RefCell<Vec<(u32, String, u32)>>,
    pub damage_log: RefCell<Vec<(u32, u32, f32)>>,
    pub disabled: RefCell<Vec<(u32, i32, f32)>>,
    pub spawned: RefCell<Vec<(u32, String)>>,
    pub destroyed: RefCell<Vec<u32>>,
    pub played_fx: RefCell<Vec<String>>,
    pub smoked: RefCell<Vec<(Vec3, f32)>>,
    /// Building: the build menu, and the structures to work on (laid out, or damaged).
    pub options: RefCell<Vec<crate::building::BuildOption>>,
    pub sites: RefCell<Vec<FakeSite>>,
    /// Everywhere is water too deep to stand in (BO2's plant check refuses it).
    pub deep_water: std::cell::Cell<bool>,
    /// The ground's surface (`surface_at`); dirt when unset.
    pub ground: std::cell::Cell<Option<Surface>>,
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
        !self.deep_water.get()
    }
    fn surface_at(&self, _x: f32, _y: f32) -> Surface {
        if self.deep_water.get() {
            Surface::Water
        } else {
            self.ground.get().unwrap_or(Surface::Dirt)
        }
    }
    fn extent(&self) -> (Vec3, Vec3) {
        (Vec3::new(-5000.0, -5000.0, -100.0), Vec3::new(5000.0, 5000.0, 1000.0))
    }
    fn object(&self, id: u32, viewer: u32) -> Option<ObjInfo> {
        self.objects.borrow().iter().find(|o| o.id == id).map(|o| self.rel(*o, viewer))
    }
    fn query(&self, center: Vec3, radius: f32, viewer: u32, out: &mut Vec<ObjInfo>) {
        for o in self.objects.borrow().iter() {
            if o.in_query(center, radius) {
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
    fn damage(&self, shooter: u32, victim: u32, amount: f32, _kind: i32) -> bool {
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
    fn spawn(&self, template: &str, _owner: u32, _pos: Vec3, _yaw: f32) -> u32 {
        let mut s = self.spawned.borrow_mut();
        let id = 10_000 + s.len() as u32;
        s.push((id, template.to_string()));
        id
    }
    fn destroy(&self, id: u32) {
        self.destroyed.borrow_mut().push(id);
    }
    fn disable(&self, id: u32, kind: i32, seconds: f32) {
        self.disabled.borrow_mut().push((id, kind, seconds));
    }
    fn fx(&self, name: &str, _pos: Vec3) {
        self.played_fx.borrow_mut().push(name.to_string());
    }
    fn smoke(&self, center: Vec3, radius: f32) {
        self.smoked.borrow_mut().push((center, radius));
    }
    fn build_options(&self, _general: u32) -> Vec<crate::building::BuildOption> {
        self.options.borrow().clone()
    }
    fn can_place(&self, _general: u32, _template: u32, pos: Vec3, _angle: f32) -> bool {
        self.sites.borrow().iter().all(|s| (s.at - pos).length_xy() > 80.0)
    }
    fn place(&self, _general: u32, template: u32, pos: Vec3, angle: f32) -> u32 {
        let mut sites = self.sites.borrow_mut();
        let id = 20_000 + sites.len() as u32;
        sites.push(FakeSite { id, template, at: pos, angle, ..Default::default() });
        id
    }
    fn work_site(&self, _general: u32, pos: Vec3, reach: f32) -> Option<crate::building::Site> {
        // Footprints 40 across.
        self.sites.borrow().iter().filter(|s| s.percent < 100.0 && (s.at - pos).length_xy() <= 20.0 + reach)
            .map(|s| crate::building::Site { id: s.id, template: s.template, kind: s.kind, percent: s.percent, ..Default::default() })
            .next()
    }
    fn work(&self, _general: u32, site: u32, rate: f32) -> f32 {
        let mut sites = self.sites.borrow_mut();
        let s = sites.iter_mut().find(|s| s.id == site).unwrap();
        if s.busy {
            return -1.0;
        }
        s.percent = (s.percent + 10.0 * rate).min(100.0);
        s.percent
    }
    fn capture(&self, id: u32, new_owner: u32) {
        let owner = self.objects.borrow().iter().find(|o| o.id == new_owner).map(|o| o.owner_player);
        if let Some(owner) = owner {
            for o in self.objects.borrow_mut().iter_mut().filter(|o| o.id == id) {
                o.owner_player = owner;
            }
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

