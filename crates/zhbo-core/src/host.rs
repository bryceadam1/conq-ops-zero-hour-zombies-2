//! The boundary to the engine. `host/zhbo/ZhboBridge.cpp` fills [`LogicApi`] and [`ClientApi`] with
//! function pointers into Zero Hour; [`CLogicHost`]/[`CClientHost`] adapt them to the traits the
//! simulation uses. Tests implement the traits with fakes instead.
//!
//! Keep this file in sync with `host/zhbo/zhbo_core.h`.

use crate::math::Vec3;
use std::ffi::{c_char, CString};

pub mod objflags {
    pub const STRUCTURE: u32 = 1 << 0;
    pub const INFANTRY: u32 = 1 << 1;
    pub const VEHICLE: u32 = 1 << 2;
    pub const AIRCRAFT: u32 = 1 << 3;
    pub const DEAD: u32 = 1 << 4;
    /// Relationship to the querying object's owner.
    pub const ENEMY: u32 = 1 << 5;
    pub const ALLY: u32 = 1 << 6;
    /// Shrubs, projectiles, mines and other things that never block movement or bullets.
    pub const NO_COLLIDE: u32 = 1 << 7;
    /// Airborne right now (helicopters in flight, jets).
    pub const AIRBORNE: u32 = 1 << 8;
    /// A drone (scout, battle, hellfire, spy): the Black Hat can hack it.
    pub const DRONE: u32 = 1 << 9;
    /// A missile or shell in flight (also `NO_COLLIDE`): the Trophy System shoots it down.
    pub const PROJECTILE: u32 = 1 << 10;
    /// Client queries only: hidden from the local player by Zero Hour's shroud or fog.
    pub const SHROUDED: u32 = 1 << 11;
    /// Firing a weapon right now.
    pub const FIRING: u32 = 1 << 12;
}

/// How damage is dealt (Zero Hour damage types decide what armour applies).
pub mod damage_kind {
    pub const SMALL_ARMS: i32 = 0;
    pub const MELEE: i32 = 1;
    pub const EXPLOSION: i32 = 2;
}

/// Zero Hour disabled states the logic can apply.
pub mod disable_kind {
    /// Stunned infantry and vehicles (concussion, flashbang, shock charge).
    pub const PARALYZED: i32 = 0;
    /// Vehicles and structures shut down (EMP grenade).
    pub const EMP: i32 = 1;
}

pub mod geom {
    pub const SPHERE: i32 = 0;
    pub const CYLINDER: i32 = 1;
    pub const BOX: i32 = 2;
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ObjInfo {
    pub id: u32,
    pub owner_player: i32,
    pub flags: u32,
    pub pos: Vec3,
    pub yaw: f32,
    pub geom_type: i32,
    pub major_radius: f32,
    pub minor_radius: f32,
    pub height: f32,
    pub health: f32,
    pub max_health: f32,
    /// The owner's team colour (ARGB), as Zero Hour's radar shows it.
    pub color: u32,
}

impl ObjInfo {
    pub fn has(&self, flag: u32) -> bool {
        self.flags & flag != 0
    }
}

/// Engine services available inside the deterministic logic step.
pub trait LogicHost {
    fn log(&self, msg: &str);
    fn ground_height(&self, x: f32, y: f32) -> f32;
    fn is_walkable(&self, x: f32, y: f32) -> bool;
    fn extent(&self) -> (Vec3, Vec3);
    /// Object by id, with relationship flags relative to `viewer`'s owner.
    fn object(&self, id: u32, viewer: u32) -> Option<ObjInfo>;
    /// Objects whose position is within `radius` of `center` (3D), relationship relative to `viewer`.
    fn query(&self, center: Vec3, radius: f32, viewer: u32, out: &mut Vec<ObjInfo>);
    fn set_pose(&self, id: u32, pos: Vec3, yaw: f32, moving: bool);
    /// Suspend (true) or resume (false) the object's own AI and physics.
    fn set_controlled(&self, id: u32, controlled: bool);
    /// Play a Zero Hour weapon's fire FX/sound from `shooter` toward `target`/`pos` without damage.
    fn fire_fx(&self, shooter: u32, fx_weapon: &str, target: u32, pos: Vec3);
    /// Deal damage (a [`damage_kind`]) as `shooter`. Returns true if the victim died from it.
    fn damage(&self, shooter: u32, victim: u32, amount: f32, kind: i32) -> bool;
    fn heal(&self, id: u32, amount: f32);
    /// Create a Zero Hour object (a projectile's model) owned by `owner`'s player; 0 if it can't.
    fn spawn(&self, template: &str, owner: u32, pos: Vec3, yaw: f32) -> u32;
    fn destroy(&self, id: u32);
    /// Disable an object for `seconds` (a [`disable_kind`]).
    fn disable(&self, id: u32, kind: i32, seconds: f32);
    /// Play a Zero Hour FXList at a point.
    fn fx(&self, name: &str, pos: Vec3);
    /// Move an object to `new_owner`'s side (the Black Hat's hack).
    fn capture(&self, id: u32, new_owner: u32);
    /// A smoke screen this frame: nothing shoots into or out of the circle.
    fn smoke(&self, center: Vec3, radius: f32);
}

/// Rendering services for the HUD (screen pixels, ARGB colours).
pub trait ClientHost {
    fn screen_size(&self) -> (f32, f32);
    fn fill_rect(&self, x: f32, y: f32, w: f32, h: f32, argb: u32);
    fn line(&self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, argb: u32);
    /// `align`: 0 left, 1 centre, 2 right. `size`: font point size.
    fn text(&self, x: f32, y: f32, text: &str, argb: u32, size: i32, align: i32);
    /// Draw a MappedImage (e.g. BO2 art from `ConquerOps_BO2.big`) tinted by `argb`.
    /// Returns false if the image is not available, so callers can fall back to primitives.
    fn image(&self, name: &str, x: f32, y: f32, w: f32, h: f32, argb: u32) -> bool;
    /// Objects near `center`, relationship relative to `viewer` (read-only, for the minimap).
    fn query(&self, center: Vec3, radius: f32, viewer: u32, out: &mut Vec<ObjInfo>);
    /// A player's current funds.
    fn cash(&self, player: i32) -> i32;
    /// Play a sound event (`sounds::*`), at `at` for positional sounds.
    fn sound(&self, _event: &str, _at: Option<Vec3>) {}
    /// Clip later 2D images to a screen rectangle; `None` turns clipping off.
    fn clip(&self, _rect: Option<(f32, f32, f32, f32)>) {}
    /// Draw Zero Hour's own radar (the whole map) in a screen rectangle, if the local player has
    /// radar. Returns where `you` (a world point) is on it, or `None` when there is no radar.
    fn radar(&self, _x: f32, _y: f32, _w: f32, _h: f32, _you: Vec3) -> Option<(f32, f32)> {
        None
    }
}

// ---------------------------------------------------------------------------------------------
// C ABI
// ---------------------------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LogicApi {
    pub log: Option<extern "C" fn(msg: *const c_char)>,
    pub ground_height: Option<extern "C" fn(x: f32, y: f32) -> f32>,
    pub is_walkable: Option<extern "C" fn(x: f32, y: f32) -> i32>,
    pub extent: Option<extern "C" fn(lo: *mut Vec3, hi: *mut Vec3)>,
    pub object: Option<extern "C" fn(id: u32, viewer: u32, out: *mut ObjInfo) -> i32>,
    pub query: Option<extern "C" fn(center: *const Vec3, radius: f32, viewer: u32, out: *mut ObjInfo, cap: i32) -> i32>,
    pub set_pose: Option<extern "C" fn(id: u32, pos: *const Vec3, yaw: f32, moving: i32)>,
    pub set_controlled: Option<extern "C" fn(id: u32, controlled: i32)>,
    pub fire_fx: Option<extern "C" fn(shooter: u32, fx_weapon: *const c_char, target: u32, pos: *const Vec3)>,
    pub damage: Option<extern "C" fn(shooter: u32, victim: u32, amount: f32, kind: i32) -> i32>,
    pub heal: Option<extern "C" fn(id: u32, amount: f32)>,
    pub spawn: Option<extern "C" fn(template: *const c_char, owner: u32, pos: *const Vec3, yaw: f32) -> u32>,
    pub destroy: Option<extern "C" fn(id: u32)>,
    pub disable: Option<extern "C" fn(id: u32, kind: i32, seconds: f32)>,
    pub fx: Option<extern "C" fn(name: *const c_char, pos: *const Vec3)>,
    pub capture: Option<extern "C" fn(id: u32, new_owner: u32)>,
    pub smoke: Option<extern "C" fn(center: *const Vec3, radius: f32)>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClientApi {
    pub screen_size: Option<extern "C" fn(w: *mut f32, h: *mut f32)>,
    pub fill_rect: Option<extern "C" fn(x: f32, y: f32, w: f32, h: f32, argb: u32)>,
    pub line: Option<extern "C" fn(x0: f32, y0: f32, x1: f32, y1: f32, width: f32, argb: u32)>,
    pub text: Option<extern "C" fn(x: f32, y: f32, utf8: *const c_char, argb: u32, size: i32, align: i32)>,
    pub image: Option<extern "C" fn(name: *const c_char, x: f32, y: f32, w: f32, h: f32, argb: u32) -> i32>,
    pub query: Option<extern "C" fn(center: *const Vec3, radius: f32, viewer: u32, out: *mut ObjInfo, cap: i32) -> i32>,
    pub cash: Option<extern "C" fn(player: i32) -> i32>,
    pub sound: Option<extern "C" fn(event: *const c_char, at: *const Vec3)>,
    pub clip: Option<extern "C" fn(x: f32, y: f32, w: f32, h: f32)>,
    pub radar: Option<extern "C" fn(x: f32, y: f32, w: f32, h: f32, you: *const Vec3, you_px: *mut Vec3) -> i32>,
}

fn cstring(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

pub struct CLogicHost(pub LogicApi);

impl LogicHost for CLogicHost {
    fn log(&self, msg: &str) {
        if let Some(f) = self.0.log {
            f(cstring(msg).as_ptr());
        }
    }

    fn ground_height(&self, x: f32, y: f32) -> f32 {
        self.0.ground_height.map_or(0.0, |f| f(x, y))
    }

    fn is_walkable(&self, x: f32, y: f32) -> bool {
        self.0.is_walkable.map_or(true, |f| f(x, y) != 0)
    }

    fn extent(&self) -> (Vec3, Vec3) {
        let mut lo = Vec3::new(-1e6, -1e6, -1e6);
        let mut hi = Vec3::new(1e6, 1e6, 1e6);
        if let Some(f) = self.0.extent {
            f(&mut lo, &mut hi);
        }
        (lo, hi)
    }

    fn object(&self, id: u32, viewer: u32) -> Option<ObjInfo> {
        let f = self.0.object?;
        let mut info = ObjInfo::default();
        (f(id, viewer, &mut info) != 0).then_some(info)
    }

    fn query(&self, center: Vec3, radius: f32, viewer: u32, out: &mut Vec<ObjInfo>) {
        query_into(self.0.query, center, radius, viewer, out);
    }

    fn set_pose(&self, id: u32, pos: Vec3, yaw: f32, moving: bool) {
        if let Some(f) = self.0.set_pose {
            f(id, &pos, yaw, moving as i32);
        }
    }

    fn set_controlled(&self, id: u32, controlled: bool) {
        if let Some(f) = self.0.set_controlled {
            f(id, controlled as i32);
        }
    }

    fn fire_fx(&self, shooter: u32, fx_weapon: &str, target: u32, pos: Vec3) {
        if let Some(f) = self.0.fire_fx {
            f(shooter, cstring(fx_weapon).as_ptr(), target, &pos);
        }
    }

    fn damage(&self, shooter: u32, victim: u32, amount: f32, kind: i32) -> bool {
        self.0.damage.map_or(false, |f| f(shooter, victim, amount, kind) != 0)
    }

    fn heal(&self, id: u32, amount: f32) {
        if let Some(f) = self.0.heal {
            f(id, amount);
        }
    }

    fn spawn(&self, template: &str, owner: u32, pos: Vec3, yaw: f32) -> u32 {
        self.0.spawn.map_or(0, |f| f(cstring(template).as_ptr(), owner, &pos, yaw))
    }

    fn destroy(&self, id: u32) {
        if let Some(f) = self.0.destroy {
            f(id);
        }
    }

    fn disable(&self, id: u32, kind: i32, seconds: f32) {
        if let Some(f) = self.0.disable {
            f(id, kind, seconds);
        }
    }

    fn fx(&self, name: &str, pos: Vec3) {
        if let Some(f) = self.0.fx {
            f(cstring(name).as_ptr(), &pos);
        }
    }

    fn capture(&self, id: u32, new_owner: u32) {
        if let Some(f) = self.0.capture {
            f(id, new_owner);
        }
    }

    fn smoke(&self, center: Vec3, radius: f32) {
        if let Some(f) = self.0.smoke {
            f(&center, radius);
        }
    }
}

pub struct CClientHost(pub ClientApi);

impl ClientHost for CClientHost {
    fn screen_size(&self) -> (f32, f32) {
        let (mut w, mut h) = (800.0, 600.0);
        if let Some(f) = self.0.screen_size {
            f(&mut w, &mut h);
        }
        (w, h)
    }

    fn fill_rect(&self, x: f32, y: f32, w: f32, h: f32, argb: u32) {
        if let Some(f) = self.0.fill_rect {
            f(x, y, w, h, argb);
        }
    }

    fn line(&self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, argb: u32) {
        if let Some(f) = self.0.line {
            f(x0, y0, x1, y1, width, argb);
        }
    }

    fn text(&self, x: f32, y: f32, text: &str, argb: u32, size: i32, align: i32) {
        if let Some(f) = self.0.text {
            f(x, y, cstring(text).as_ptr(), argb, size, align);
        }
    }

    fn image(&self, name: &str, x: f32, y: f32, w: f32, h: f32, argb: u32) -> bool {
        self.0.image.map_or(false, |f| f(cstring(name).as_ptr(), x, y, w, h, argb) != 0)
    }

    fn query(&self, center: Vec3, radius: f32, viewer: u32, out: &mut Vec<ObjInfo>) {
        query_into(self.0.query, center, radius, viewer, out);
    }

    fn cash(&self, player: i32) -> i32 {
        self.0.cash.map_or(0, |f| f(player))
    }

    fn sound(&self, event: &str, at: Option<Vec3>) {
        if let Some(f) = self.0.sound {
            let name = cstring(event);
            match at {
                Some(p) => f(name.as_ptr(), &p),
                None => f(name.as_ptr(), std::ptr::null()),
            }
        }
    }

    fn radar(&self, x: f32, y: f32, w: f32, h: f32, you: Vec3) -> Option<(f32, f32)> {
        let f = self.0.radar?;
        let mut px = Vec3::default();
        (f(x, y, w, h, &you, &mut px) != 0).then_some((px.x, px.y))
    }

    fn clip(&self, rect: Option<(f32, f32, f32, f32)>) {
        if let Some(f) = self.0.clip {
            let (x, y, w, h) = rect.unwrap_or((0.0, 0.0, 0.0, 0.0));
            f(x, y, w, h);
        }
    }
}

type QueryFn = extern "C" fn(*const Vec3, f32, u32, *mut ObjInfo, i32) -> i32;

fn query_into(f: Option<QueryFn>, center: Vec3, radius: f32, viewer: u32, out: &mut Vec<ObjInfo>) {
    let Some(f) = f else { return };
    const CAP: usize = 512;
    let start = out.len();
    out.resize(start + CAP, ObjInfo::default());
    let n = f(&center, radius, viewer, out[start..].as_mut_ptr(), CAP as i32).clamp(0, CAP as i32) as usize;
    out.truncate(start + n);
}
