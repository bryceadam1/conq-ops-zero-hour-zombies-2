//! The boundary to the engine. `host/zhbo/ZhboBridge.cpp` fills [`LogicApi`] and [`ClientApi`] with
//! function pointers into Zero Hour; [`CLogicHost`]/[`CClientHost`] adapt them to the traits the
//! simulation uses. Tests implement the traits with fakes instead.
//!
//! Keep this file in sync with `host/zhbo/zhbo_core.h`.

use crate::damage::Blow;
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

/// The ABI's damage kinds that the shield hook reports back (`zhbo_logic_shield_damage`): the first
/// three of [`crate::damage::ZH_DAMAGE_TYPES`], which `LogicApi.damage` carries for every Zero Hour
/// damage type (the core's own hits name their BO2 means of death, [`LogicHost::damage`]).
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

    /// Zero Hour's bounding sphere of the object (`GeometryInfo`): its centre (half the height
    /// above the position, except a sphere's, which is the position) and radius (a sphere's
    /// radius; a cylinder's radius or half height, whichever is larger; a box's half diagonal).
    pub fn bounding_sphere(&self) -> (Vec3, f32) {
        let half = self.height * 0.5;
        match self.geom_type {
            geom::SPHERE => (self.pos, self.major_radius),
            geom::BOX => (self.pos + Vec3::new(0.0, 0.0, half), (self.major_radius * self.major_radius + self.minor_radius * self.minor_radius + half * half).sqrt()),
            _ => (self.pos + Vec3::new(0.0, 0.0, half), self.major_radius.max(half)),
        }
    }

    /// Whether [`LogicHost::query`] around `center` with `radius` returns this object: its
    /// bounding sphere comes within `radius` of `center` (Zero Hour's `FROM_BOUNDINGSPHERE_3D`,
    /// `PartitionManager` `distCalcProc_BoundaryAndBoundary_3D`). The test hosts filter by it.
    pub fn in_query(&self, center: Vec3, radius: f32) -> bool {
        let (c, r) = self.bounding_sphere();
        ((c - center).length() - r).max(0.0) < radius
    }

    /// Whether its position is within `radius` of `center`: for the callers of
    /// [`LogicHost::query`] whose reach is BO2's distance to an origin, not to a solid.
    pub fn origin_within(&self, center: Vec3, radius: f32) -> bool {
        (self.pos - center).length() <= radius
    }
}

/// Engine services available inside the deterministic logic step.
pub trait LogicHost {
    fn log(&self, msg: &str);
    fn ground_height(&self, x: f32, y: f32) -> f32;
    fn is_walkable(&self, x: f32, y: f32) -> bool;
    /// The ground's surface at a world point (a [`crate::impacts::Surface`]): water where the
    /// logic's water table covers the ground, else the map's tile class there
    /// ([`crate::terrain`]). **Sim state**: a grenade's bounce scales by it, the plant check refuses
    /// deep water by it, and the impact and blast cells it picks ride the events, so it is read
    /// only from logic-side data every machine holds identically (docs/CONTRACT.md).
    fn surface_at(&self, _x: f32, _y: f32) -> crate::impacts::Surface {
        crate::impacts::Surface::Default
    }
    fn extent(&self) -> (Vec3, Vec3);
    /// Object by id, with relationship flags relative to `viewer`'s owner.
    fn object(&self, id: u32, viewer: u32) -> Option<ObjInfo>;
    /// Objects whose bounding sphere comes within `radius` of `center` (3D; [`ObjInfo::in_query`]),
    /// relationship relative to `viewer`. A superset of what reaches: callers measure each
    /// object's own solid, or its centre, for what they need.
    fn query(&self, center: Vec3, radius: f32, viewer: u32, out: &mut Vec<ObjInfo>);
    fn set_pose(&self, id: u32, pos: Vec3, yaw: f32, moving: bool);
    /// Suspend (true) or resume (false) the object's own AI and physics.
    fn set_controlled(&self, id: u32, controlled: bool);
    /// Play a Zero Hour weapon's fire FX/sound from `shooter` toward `target`/`pos` without damage.
    fn fire_fx(&self, shooter: u32, fx_weapon: &str, target: u32, pos: Vec3);
    /// Deal damage as `shooter` by `blow` (BO2's means of death, weapon and direction): the engine
    /// takes it as the Zero Hour damage type the tuning pack's `[MeansOfDeath]` table names for
    /// the means ([`crate::damage`]), so its armour applies. Returns true if the victim died from it.
    fn damage(&self, shooter: u32, victim: u32, amount: f32, blow: Blow) -> bool;
    /// The last blow [`LogicHost::damage`] dealt `victim` in this step, when the host records
    /// them (the step's scoring host, `sim::score`): the presentation of his hurt (its side, BO2's
    /// `vDir`). None by default.
    fn last_blow(&self, _victim: u32) -> Option<Blow> {
        None
    }
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
    /// The structures `general` can build (his build menu), with whether each can be placed now.
    fn build_options(&self, _general: u32) -> Vec<crate::building::BuildOption> {
        Vec::new()
    }
    /// Whether `template` can go down at `pos` (facing `angle`) for `general`.
    fn can_place(&self, _general: u32, _template: u32, _pos: Vec3, _angle: f32) -> bool {
        false
    }
    /// Pay for `template` and lay out its construction site at `pos`; the site's id, or 0.
    fn place(&self, _general: u32, _template: u32, _pos: Vec3, _angle: f32) -> u32 {
        0
    }
    /// The closest structure `general` can work on (one under construction he can build, or a
    /// damaged one he can repair, by Zero Hour's rules for a dozer) within `reach` of the edge of
    /// its footprint, from `pos`.
    fn work_site(&self, _general: u32, _pos: Vec3, _reach: f32) -> Option<crate::building::Site> {
        None
    }
    /// One logic frame of `general` working on `site`, at `rate` of a dozer's speed; how far along
    /// it is after (100 when done), or negative if someone else has it.
    fn work(&self, _general: u32, _site: u32, _rate: f32) -> f32 {
        -1.0
    }
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
    /// Play a sound event at `gain` × its own volume (ABI: `ClientApi::sound_gain`); a host
    /// without it plays the event at its own volume.
    fn sound_gain(&self, event: &str, at: Option<Vec3>, _gain: f32) {
        self.sound(event, at);
    }
    /// Set looping sound event `event` (a 2D `LOOP` event) to `gain` × its own volume, starting it
    /// if it is not playing; `gain <= 0` stops it (ABI: `ClientApi::sound_loop`). A host without it
    /// plays no loops.
    fn sound_loop(&self, _event: &str, _gain: f32) {}
    /// Clip later 2D images to a screen rectangle; `None` turns clipping off.
    fn clip(&self, _rect: Option<(f32, f32, f32, f32)>) {}
    /// Draw Zero Hour's own radar (the whole map) in a screen rectangle, if the local player has
    /// radar. Returns where `you` (a world point) is on it, or `None` when there is no radar.
    fn radar(&self, _x: f32, _y: f32, _w: f32, _h: f32, _you: Vec3) -> Option<(f32, f32)> {
        None
    }
    /// The structures `general` can build (as [`LogicHost::build_options`]), for the HUD.
    fn build_options(&self, _general: u32) -> Vec<crate::building::BuildOption> {
        Vec::new()
    }
    /// Whether `template` can go down at `pos` (for the placement ghost's colour).
    fn can_place(&self, _general: u32, _template: u32, _pos: Vec3, _angle: f32) -> bool {
        false
    }
    /// Show the placement ghost of `template` at `pos` (legal or not); `template` 0 hides it.
    fn ghost(&self, _template: u32, _pos: Vec3, _angle: f32, _legal: bool) {}
    /// Draw a MappedImage on an arbitrary quad: `corners` take the image's top-left, top-right,
    /// bottom-right and bottom-left in that order (BO2 rotates its damage indicators and crosshair
    /// pieces). Returns false when unsupported or the image is missing, so callers fall back.
    fn image_quad(&self, _name: &str, _corners: [(f32, f32); 4], _argb: u32) -> bool {
        false
    }
    /// A player's display name (the spectator's switch bar names the followed player, BO2's
    /// `DeadSpectate.playerName`; ABI: `ClientApi::player_name`). A host without it names nobody.
    fn player_name(&self, _player: i32) -> Option<String> {
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
    pub build_options: Option<extern "C" fn(general: u32, out: *mut crate::building::BuildOption, cap: i32) -> i32>,
    pub can_place: Option<extern "C" fn(general: u32, template: u32, pos: *const Vec3, angle: f32) -> i32>,
    pub place: Option<extern "C" fn(general: u32, template: u32, pos: *const Vec3, angle: f32) -> u32>,
    pub work_site: Option<extern "C" fn(general: u32, pos: *const Vec3, reach: f32, out: *mut crate::building::Site) -> u32>,
    pub work: Option<extern "C" fn(general: u32, site: u32, rate: f32) -> f32>,
    pub choose_class: Option<extern "C" fn(player: i32, general: u32, index: i32)>,
    /// ABI 26 (R8d): the water table at a point (`ZHBO_SURF_WATER`, else `ZHBO_SURF_DEFAULT`);
    /// the ground under it comes from the map grid (`zhbo_terrain_load`). An older bridge that
    /// answers the tile class itself still works: any non-default answer is taken as it is.
    pub surface_at: Option<extern "C" fn(x: f32, y: f32) -> i32>,
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
    pub build_options: Option<extern "C" fn(general: u32, out: *mut crate::building::BuildOption, cap: i32) -> i32>,
    pub can_place: Option<extern "C" fn(general: u32, template: u32, pos: *const Vec3, angle: f32) -> i32>,
    pub ghost: Option<extern "C" fn(template: u32, pos: *const Vec3, angle: f32, legal: i32)>,
    /// `sound` at `gain` × the event's volume (a third-person shot's layers at BO2's distance
    /// curve, `soundlayers`). Appended after `ghost`.
    pub sound_gain: Option<extern "C" fn(event: *const c_char, at: *const Vec3, gain: f32)>,
    /// A looping 2D event at `gain` × its volume, started if needed; `gain <= 0` stops it (a
    /// shellshock's ringing, `shellshock::Presented`). Appended after `sound_gain`.
    pub sound_loop: Option<extern "C" fn(event: *const c_char, gain: f32)>,
    /// A player's display name as UTF-8 into `out` (at most `cap` bytes, NUL-terminated); returns
    /// its length, 0 when there is none (the spectator's switch bar). Appended after `sound_loop`.
    pub player_name: Option<extern "C" fn(player: i32, out: *mut c_char, cap: i32) -> i32>,
}

fn build_options_via(f: Option<extern "C" fn(u32, *mut crate::building::BuildOption, i32) -> i32>, general: u32) -> Vec<crate::building::BuildOption> {
    let Some(f) = f else { return Vec::new() };
    let mut out = vec![crate::building::BuildOption::default(); 24];
    let n = f(general, out.as_mut_ptr(), out.len() as i32).clamp(0, out.len() as i32) as usize;
    out.truncate(n);
    out
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
        self.0.is_walkable.is_none_or(|f| f(x, y) != 0)
    }

    fn surface_at(&self, x: f32, y: f32) -> crate::impacts::Surface {
        // The engine answers the water table (`TerrainLogic::isUnderwater`, logic state); the
        // ground under it is the map file's tile class (`crate::terrain`, the grid the bridge
        // loads per map, never the client's terrain visual). An engine without the query (a shell,
        // a test) and no loaded map report the default cell.
        match crate::impacts::Surface::from_i32(self.0.surface_at.map_or(0, |f| f(x, y))) {
            crate::impacts::Surface::Default => crate::terrain::surface_at(x, y),
            water => water,
        }
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

    fn damage(&self, shooter: u32, victim: u32, amount: f32, blow: Blow) -> bool {
        let kind = crate::tuning::get().means_of_death.code(blow.how);
        // The sim's own blow: the engine's damage hook does not report it back
        // ([`crate::enginedamage`]).
        crate::enginedamage::dealing(|| self.0.damage.is_some_and(|f| f(shooter, victim, amount, kind) != 0))
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

    fn build_options(&self, general: u32) -> Vec<crate::building::BuildOption> {
        build_options_via(self.0.build_options, general)
    }

    fn can_place(&self, general: u32, template: u32, pos: Vec3, angle: f32) -> bool {
        self.0.can_place.is_some_and(|f| f(general, template, &pos, angle) != 0)
    }

    fn place(&self, general: u32, template: u32, pos: Vec3, angle: f32) -> u32 {
        self.0.place.map_or(0, |f| f(general, template, &pos, angle))
    }

    fn work_site(&self, general: u32, pos: Vec3, reach: f32) -> Option<crate::building::Site> {
        let f = self.0.work_site?;
        let mut site = crate::building::Site::default();
        let id = f(general, &pos, reach, &mut site);
        (id != 0).then_some(crate::building::Site { id, ..site })
    }

    fn work(&self, general: u32, site: u32, rate: f32) -> f32 {
        self.0.work.map_or(-1.0, |f| f(general, site, rate))
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
        self.0.image.is_some_and(|f| f(cstring(name).as_ptr(), x, y, w, h, argb) != 0)
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

    fn sound_loop(&self, event: &str, gain: f32) {
        if let Some(f) = self.0.sound_loop { // content: presentation (the host API's sound_loop callback, not ShockDef::sound_loop)
            f(cstring(event).as_ptr(), gain);
        }
    }

    fn sound_gain(&self, event: &str, at: Option<Vec3>, gain: f32) {
        let Some(f) = self.0.sound_gain else { return self.sound(event, at) };
        let name = cstring(event);
        match at {
            Some(p) => f(name.as_ptr(), &p, gain),
            None => f(name.as_ptr(), std::ptr::null(), gain),
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

    fn build_options(&self, general: u32) -> Vec<crate::building::BuildOption> {
        build_options_via(self.0.build_options, general)
    }

    fn can_place(&self, general: u32, template: u32, pos: Vec3, angle: f32) -> bool {
        self.0.can_place.is_some_and(|f| f(general, template, &pos, angle) != 0)
    }

    fn ghost(&self, template: u32, pos: Vec3, angle: f32, legal: bool) {
        if let Some(f) = self.0.ghost {
            f(template, &pos, angle, legal as i32);
        }
    }

    fn player_name(&self, player: i32) -> Option<String> {
        let f = self.0.player_name?;
        let mut buf = [0u8; 64];
        let n = f(player, buf.as_mut_ptr() as *mut c_char, buf.len() as i32);
        let n = usize::try_from(n).ok().filter(|&n| n > 0)?.min(buf.len() - 1);
        Some(String::from_utf8_lossy(&buf[..n]).into_owned())
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
