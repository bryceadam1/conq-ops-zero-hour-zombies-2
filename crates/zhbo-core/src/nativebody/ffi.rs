//! The native third person's C ABI (appended to `zhbo_core.h`; see CONTRACT.md "The native third
//! person"). Called from the engine's client frame by `ZhboViewmodel.cpp` for each General the sim
//! reports (`zhbo_client_native_poses`): each frame one [`zhbo_native_body`], then the file
//! requests ([`zhbo_native_wanted`]/[`zhbo_native_provide`]) and log lines
//! ([`zhbo_native_take_log`]) — the same channels the viewmodel uses; [`zhbo_native_body_reset`]
//! on a game reset.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::c_char;
use std::sync::Mutex;

use super::{load, Loaded, Mode, Posed, Request};
use crate::nativevm::ffi::ZhboNativeLight;

/// One general's third-person frame.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ZhboNativeBodyIn {
    /// The General object id: each keeps his own composed rig.
    pub general: u32,
    /// `ZHBO_NATIVE_BODY`'s mode: 1 weapon only (the W3D body draws), 2 body and weapon.
    pub mode: i32,
    /// Converted weapon index (`WeaponDef::viewmodel`), or -1 for no weapon in hand.
    pub vm: i32,
    /// The faction letter (`U`, `C`, `G`).
    pub faction: i32,
    /// The body clip (`zhbo_native::thirdperson::state`), seconds into it and its playback rate.
    pub state: i32,
    pub time: f32,
    pub rate: f32,
    /// The General's feet (world units) and yaw (radians, 0 = +X, counter-clockwise).
    pub pos: [f32; 3],
    pub yaw: f32,
    /// The camera's world position, for the shader's specular term.
    pub eye: [f32; 3],
    /// The engine drawable's `tag_weapon_right` world transform: 3 rows of 4 (W3D `Matrix3D`,
    /// translation in each row's fourth lane). Used by the weapon-only mode.
    pub tag: [f32; 12],
    pub tag_valid: i32,
    /// Display-encoded scene ambient and lights, as [`ZhboNativeLight`].
    pub ambient: [f32; 3],
    pub lights: [ZhboNativeLight; 3],
    pub light_count: i32,
    /// The engine's scene view (drawn into) and the view the pass runs before (the composite).
    pub target_view: u16,
    pub before_view: u16,
    /// `ZHBO_BO2_DEBUG`'s term view (0 off).
    pub debug: f32,
}

const _: () = assert!(std::mem::size_of::<ZhboNativeBodyIn>() == 204);

/// `zhbo_native_body`'s answers, the same numbers as the viewmodel's.
pub const NOT_NATIVE: i32 = 0;
pub const DRAWN: i32 = 1;
pub const LOADING: i32 = 2;

/// `ZhboNativeBodyIn.mode` for a dropped weapon lying on the ground (`ZHBO_BODY_LYING`, appended
/// for ABI 32): `general` is its Zero Hour object, `pos`/`yaw` where it lies; no tag.
pub const LYING: i32 = 3;

/// What the bridge asks the core for each General: which weapon and which body clip. Presentation
/// only (`Sim::native_poses`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ZhboNativePose {
    pub general: u32,
    /// Converted weapon index, or -1 for none.
    pub weapon: i32,
    /// The stance (0 stand, 1 crouch, 2 prone) and its body clip id.
    pub stance: i32,
    pub state: i32,
    /// Seconds into the clip and its playback rate.
    pub time: f32,
    pub rate: f32,
    /// The second gun is in hand (BO2's pistol stance).
    pub sidearm: i32,
}

const _: () = assert!(std::mem::size_of::<ZhboNativePose>() == 28);

/// One General's composed rig and its GPU handles. Each General keeps his own, so several on
/// screen with different weapons (or camos) do not evict each other's every frame.
pub(crate) struct Rig {
    pub(crate) loaded: Loaded,
    #[cfg(feature = "renderer")]
    pub(crate) gpu: super::gpu::Gpu,
}

impl Rig {
    fn release(&mut self) {
        #[cfg(feature = "renderer")]
        self.gpu.release();
    }
}

#[derive(Default)]
pub(crate) struct State {
    pub(crate) files: crate::nativevm::Files,
    /// The rigs by General object id.
    rigs: HashMap<u32, Rig>,
    /// The General posed last: `posed` and [`zhbo_native_body_bone`] describe his rig.
    last: Option<u32>,
    failed: HashSet<super::RigKey>,
    pub(crate) posed: Posed,
    /// The torso clips played so far, shared by every General's rig (the body manifests name the
    /// same blobs).
    torso: HashMap<u16, super::AnimClip>,
    pub(crate) log: VecDeque<String>,
    logged: HashSet<String>,
    /// Each General's muzzle in his last drawn pose: the held weapon's `tag_flash` world transform
    /// (body mode, the gun in hand), for the effects of his shots (`zhbo_native_body_flash`).
    flash: HashMap<u32, [[f32; 4]; 3]>,
    /// The movement states drawn so far (`state::split`'s state), each logged once.
    moves: HashSet<u16>,
    /// Each General's presentation between frames: his movement clip's blend and his aim.
    present: HashMap<u32, Present>,
    /// The tuning pack's default movement blend (seconds), read once.
    default_blend: Option<f32>,
}

/// One General's body between frames (presentation only).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Present {
    /// The clock of his last frame (the request's `time`).
    time: f32,
    /// His movement clip, and the one it is blending in over: `(clip id, start, seconds)`.
    state: u16,
    from: Option<(u16, f32, f32)>,
    aim: super::aim::Aim,
}

impl State {
    /// The rig posed last (its bones are what `posed` holds).
    pub(crate) fn loaded(&self) -> Option<&Loaded> {
        self.last.and_then(|g| self.rigs.get(&g)).map(|r| &r.loaded)
    }

    /// How many Generals hold a rig (tests).
    #[cfg(test)]
    pub(crate) fn rig_count(&self) -> usize {
        self.rigs.len()
    }

    /// Drop the rigs of dropped weapons no longer lying (picked up, expired, cleaned up): each drop
    /// is a new object, so its rig would otherwise outlive it until the game resets.
    pub(crate) fn retain_dropped(&mut self, lying: &[u32]) {
        let gone: Vec<u32> = self.rigs.iter().filter(|(g, r)| r.loaded.key.2 == Mode::Lying as u8 && !lying.contains(g)).map(|(g, _)| *g).collect();
        for g in gone {
            if let Some(mut r) = self.rigs.remove(&g) {
                r.release();
            }
            if self.last == Some(g) {
                self.last = None;
            }
        }
    }

    fn release_all(&mut self) {
        for r in self.rigs.values_mut() {
            r.release();
        }
        self.rigs.clear();
        self.flash.clear();
        self.present.clear();
        self.last = None;
    }

    /// General `general`'s muzzle as last drawn ([`State::flash`]).
    pub(crate) fn flash_of(&self, general: u32) -> Option<[[f32; 4]; 3]> {
        self.flash.get(&general).copied()
    }

    /// The lines logged so far (tests).
    #[cfg(test)]
    pub(crate) fn logged(&self) -> impl Iterator<Item = &str> {
        self.logged.iter().map(String::as_str)
    }

    /// Ask for torso clip `id` (`rig`'s body names its blob) until it has loaded.
    fn want_torso(&mut self, rig: &Loaded, id: u16) {
        if self.torso.contains_key(&id) {
            return;
        }
        let Some(path) = rig.torso_path(id).map(str::to_string) else { return };
        if self.files.is_missing(&path) {
            return;
        }
        if let Some(bytes) = self.files.get(&path) {
            match super::AnimClip::from_bytes(&bytes) {
                Ok(a) => {
                    self.torso.insert(id, a);
                }
                Err(e) => self.log_once(format!("{path}: {e}")),
            }
            self.files.forget(&path);
        }
    }

    pub(crate) fn log_once(&mut self, msg: String) {
        if self.logged.len() < 256 && self.logged.insert(msg.clone()) {
            self.log.push_back(format!("native body: {msg}"));
        }
    }

    /// One general's frame. `enabled` is `ZHBO_NATIVE_BODY != 0`; without the renderer it loads and
    /// poses, then answers "not native".
    pub(crate) fn frame(&mut self, inp: &ZhboNativeBodyIn, enabled: i32, camo: Option<&str>) -> i32 {
        if (1..=255).contains(&inp.faction) {
            note_faction(inp.general, inp.faction as u8);
        }
        if enabled <= 0 || inp.vm < 0 || inp.faction <= 0 {
            return NOT_NATIVE;
        }
        let mode = match inp.mode {
            2 => Mode::Body,
            LYING => Mode::Lying,
            _ => Mode::Weapon,
        };
        // The weapon-only mode is placed by the engine's tag: without one there is nowhere to draw.
        // Body mode is placed by the General's own transform and needs no engine tag.
        if mode == Mode::Weapon && inp.tag_valid == 0 {
            return NOT_NATIVE;
        }
        let faction = inp.faction as u8;
        let stowed = if mode == Mode::Body { stowed_of(inp.general) } else { -1 };
        let offhands = if mode == Mode::Body { offhands_of(inp.general) } else { [-1; 2] };
        let key = (faction, inp.vm, mode as u8, stowed, offhands);
        if self.failed.contains(&key) {
            return NOT_NATIVE;
        }
        let general = inp.general;
        // A corpse keeps the rig he died with: his record (and with it his stowed weapon, offhands
        // and camo) is gone, and recomposing would show the W3D body while it loads.
        let corpse = torso_of(general).is_some_and(|a| a.action == zhbo_native::thirdperson::torso::DEATH);
        if let Some(a) = torso_of(general).filter(|_| corpse) {
            let name = crate::torso::get().clip(a.clip).map_or("?", |c| c.name.as_str());
            self.log_once(format!("General {general} falls by {name}"));
        }
        let keep = corpse && self.rigs.get(&general).is_some_and(|r| (r.loaded.key.0, r.loaded.key.1, r.loaded.key.2) == (key.0, key.1, key.2));
        if !keep && self.rigs.get(&general).is_none_or(|r| r.loaded.key != key || r.loaded.camo.as_deref() != camo) {
            // His weapon or camo changed: drop his old rig (another General's stays).
            if let Some(mut old) = self.rigs.remove(&general) {
                old.release();
            }
            let req = request(inp, mode);
            match load(&mut self.files, &req, camo) {
                Err(e) => {
                    self.failed.insert(key);
                    self.log_once(e);
                    return NOT_NATIVE;
                }
                Ok(None) => return LOADING,
                Ok(Some(l)) => {
                    // Per General, with the camo: the multiplayer test matches this against the
                    // other machine's own "in hand" line for the same General.
                    self.log_once(format!(
                        "General {general} weapon {} camo {} composed (ZW{:02}{}): {} bones, {} vertices, {} surfaces",
                        l.key.1,
                        camo.unwrap_or("none"),
                        l.key.1,
                        l.key.0 as char,
                        l.skeleton.bones.len(),
                        l.mesh.vertices.len(),
                        l.mesh.surfaces.len()
                    ));
                    self.rigs.insert(general, Rig { loaded: l, #[cfg(feature = "renderer")] gpu: Default::default() });
                }
            }
        }
        let req = request(inp, mode);
        #[cfg_attr(not(feature = "renderer"), allow(unused_mut))]
        let Some(mut rig) = self.rigs.remove(&general) else { return NOT_NATIVE };
        // The body clip for this movement state, or the bind pose when the weapon-only mode (or a
        // body with no clip) is drawing.
        let clip = rig.loaded.clip_for(inp.state as u16);
        // A movement change cross-fades over the new clip's blend time (the script's
        // `blendtime`/`blendouttime`, else the tuning pack's default; `FUN_008a8c40`), the old clip
        // playing on beneath; and the aim controllers follow his view pitch.
        let default_blend = *self.default_blend.get_or_insert_with(|| crate::tuning::get().player_anim_blend_time / 1000.0);
        let p = self.present.entry(general).or_insert(Present { time: req.time, state: req.state, ..Default::default() });
        let ms = ((req.time - p.time) * 1000.0).clamp(0.0, 100.0);
        p.time = req.time;
        if req.state != p.state {
            let len = crate::torso::get().move_blend(req.state).unwrap_or(default_blend);
            p.from = Some((p.state, req.time, len));
            p.state = req.state;
        }
        let blend = p.from.map(|(id, start, len)| (id, ((req.time - start) / len.max(1e-3)).clamp(0.0, 1.0)));
        if blend.is_some_and(|(_, w)| w >= 1.0) {
            p.from = None;
        }
        let from = blend.and_then(|(id, w)| rig.loaded.clip_for(id).map(|c| (c, w)));
        let (pitch, shield) = aim_of(general).unwrap_or((0.0, false));
        let prone = {
            use zhbo_native::thirdperson::state;
            matches!(state::split_direction(state::split(req.state).1).0, state::PRONE_IDLE | state::PRONE_CRAWL)
        };
        let pitches = p.aim.step(super::aim::AimIn { view_pitch: pitch, prone, shield }, ms);
        // The upper body's action over it, while the client times one for him (`client::remote`).
        // Its clip loads the first time anyone plays it; until then the body runs without it.
        let action = torso_of(general).filter(|_| mode == Mode::Body);
        if let Some(a) = action {
            self.want_torso(&rig.loaded, a.clip);
        }
        // A death is played through and held (the corpse); every other action fades out at its end.
        let torso = action.and_then(|a| self.torso.get(&a.clip).map(|c| (c, a.t, a.action == zhbo_native::thirdperson::torso::DEATH)));
        // A corpse's death clip owns the whole body: no aim on it.
        let aimed = (!corpse).then_some(&pitches);
        rig.loaded.pose_full(&req, clip, from, torso, aimed, &mut self.posed);
        // A grenade's pin and throw: BO2 draws the offhand on `tag_inhand` as the entity's one
        // weapon (t6mp `FUN_0052a4c0` attaches only the entity's current weapon; `FUN_0052a470`
        // puts a grenade on `tag_inhand`), so the gun in hand is not drawn meanwhile.
        let in_hand = action.filter(|a| matches!(a.action, zhbo_native::thirdperson::torso::THROW | zhbo_native::thirdperson::torso::PRIME));
        // A corpse has dropped his gun (BO2's `dropweaponfordeath`).
        if in_hand.is_some() || action.is_some_and(|a| a.action == zhbo_native::thirdperson::torso::DEATH) {
            rig.loaded.hide_weapon(&mut self.posed);
        }
        rig.loaded.show_offhand(&mut self.posed, in_hand.map(|a| if a.lethal { 0 } else { 1 }));
        // Where his shots' flash, brass and tracer start: the drawn gun's muzzle (none while it is
        // hidden, or in the other modes, where the W3D body's fire bone serves).
        let gun_drawn = mode == Mode::Body && in_hand.is_none() && !action.is_some_and(|a| a.action == zhbo_native::thirdperson::torso::DEATH);
        match rig.loaded.bone_world(&self.posed, super::FLASH_TAG).filter(|_| gun_drawn).map(|m| super::orthonormal(&m)) {
            Some(m) => {
                self.flash.insert(general, m);
            }
            None => {
                self.flash.remove(&general);
            }
        }
        if mode == Mode::Body {
            let movement = zhbo_native::thirdperson::state::split(req.state).1;
            if self.moves.insert(movement) {
                let name = clip.map_or("none", |c| c.name.as_str());
                self.log.push_back(format!("native body: General {general} body clip {name} (movement state {movement})"));
            }
        }
        self.last = Some(general);
        #[cfg(feature = "renderer")]
        let answer = super::gpu::draw(self, &mut rig, inp);
        #[cfg(not(feature = "renderer"))]
        let answer = NOT_NATIVE;
        self.rigs.insert(general, rig);
        answer
    }
}

fn request(inp: &ZhboNativeBodyIn, mode: Mode) -> Request {
    // A 3-row `Matrix3D`: rows 0-2, four lanes each (the 12 `tag` floats are row-major).
    let tag = (inp.tag_valid != 0).then(|| [[inp.tag[0], inp.tag[1], inp.tag[2], inp.tag[3]], [inp.tag[4], inp.tag[5], inp.tag[6], inp.tag[7]], [inp.tag[8], inp.tag[9], inp.tag[10], inp.tag[11]]]);
    Request {
        vm: inp.vm,
        faction: inp.faction as u8,
        mode,
        state: inp.state.clamp(0, u16::MAX as i32) as u16,
        time: inp.time,
        rate: if inp.rate > 0.0 && inp.rate.is_finite() { inp.rate } else { 1.0 },
        pos: inp.pos,
        yaw: inp.yaw,
        tag,
        stowed: if mode == Mode::Body { stowed_of(inp.general) } else { -1 },
        offhands: if mode == Mode::Body { offhands_of(inp.general) } else { [-1; 2] },
    }
}

presentation_global! {
    /// Lock-free per-General stowed weapon (`general << 32 | vm + 1`), published by the core after each
    /// logic step (`Sim::stowed_weapons`): presentation only, read by the third-person frame.
    fn stowed() -> [std::sync::atomic::AtomicU64; crate::sim::MAX_PLAYERS] = [const { std::sync::atomic::AtomicU64::new(0) }; crate::sim::MAX_PLAYERS];
}

presentation_global! {
    /// Each General's lethal and tactical (`general << 32 | (lethal vm + 1) << 16 | tactical vm + 1`),
    /// published with the stowed weapon (`Sim::offhand_weapons`). Presentation only.
    fn offhands() -> [std::sync::atomic::AtomicU64; crate::sim::MAX_PLAYERS] = [const { std::sync::atomic::AtomicU64::new(0) }; crate::sim::MAX_PLAYERS];
}

pub fn publish_offhands(rows: &[(u32, [i32; 2])]) {
    use std::sync::atomic::Ordering;
    for (i, slot) in offhands().iter().enumerate() {
        let v = rows.get(i).map_or(0, |&(g, [l, t])| ((g as u64) << 32) | (((l + 1) as u64 & 0xffff) << 16) | ((t + 1) as u64 & 0xffff));
        slot.store(v, Ordering::Relaxed);
    }
}

fn offhands_of(general: u32) -> [i32; 2] {
    use std::sync::atomic::Ordering;
    offhands().iter().map(|s| s.load(Ordering::Relaxed)).find(|&v| v != 0 && (v >> 32) as u32 == general).map_or([-1; 2], |v| [((v >> 16) & 0xffff) as i32 - 1, (v & 0xffff) as i32 - 1])
}

pub(crate) fn publish_stowed(pairs: &[(u32, i32)]) {
    use std::sync::atomic::Ordering;
    for (i, slot) in stowed().iter().enumerate() {
        let v = pairs.get(i).map_or(0, |&(g, vm)| ((g as u64) << 32) | (vm + 1) as u32 as u64);
        slot.store(v, Ordering::Relaxed);
    }
}

fn stowed_of(general: u32) -> i32 {
    use std::sync::atomic::Ordering;
    stowed().iter().map(|s| s.load(Ordering::Relaxed)).find(|&v| v != 0 && (v >> 32) as u32 == general).map_or(-1, |v| (v & 0xffff_ffff) as i32 - 1)
}

/// Each General's faction letter as the bridge names it to [`zhbo_native_body`] (`general << 8 |
/// letter`): the client's voice for him (`sounds::voice_event`). Presentation only.
static FACTIONS: [std::sync::atomic::AtomicU64; 64] = [const { std::sync::atomic::AtomicU64::new(0) }; 64];

fn note_faction(general: u32, letter: u8) {
    use std::sync::atomic::Ordering;
    if general == 0 || letter == 0 || faction_of(general) == Some(letter) {
        return;
    }
    let slot = &FACTIONS[general as usize % FACTIONS.len()];
    slot.store(((general as u64) << 8) | letter as u64, Ordering::Relaxed);
}

/// General `general`'s faction letter, once the bridge has drawn him.
pub fn faction_of(general: u32) -> Option<u8> {
    use std::sync::atomic::Ordering;
    let v = FACTIONS[general as usize % FACTIONS.len()].load(Ordering::Relaxed);
    ((v >> 8) as u32 == general && v != 0).then_some((v & 0xff) as u8)
}

presentation_global! {
    /// Each General's torso action this frame (`general << 32 | action << 28 | lethal << 27 | clip
    /// << 14 | milliseconds in`), published by the client (`client::remote`; the headless third-person
    /// render's `--torso` too): the body pass layers it. Presentation only.
    fn torso_slots() -> [std::sync::atomic::AtomicU64; 2 * crate::sim::MAX_PLAYERS] = [const { std::sync::atomic::AtomicU64::new(0) }; 2 * crate::sim::MAX_PLAYERS];
}

/// One General's torso action as the body pass takes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TorsoAction {
    pub general: u32,
    /// `zhbo_native::thirdperson::torso`'s action (`RELOAD`, `THROW`, ...).
    pub action: u8,
    /// The body manifest's clip id and seconds into it.
    pub clip: u16,
    pub t: f32,
    /// A pin or throw's offhand: the lethal (else the tactical).
    pub lethal: bool,
}

pub fn publish_torso(torsos: impl Iterator<Item = TorsoAction>) {
    use std::sync::atomic::Ordering;
    let mut it = torsos;
    for slot in torso_slots().iter() {
        let v = it.next().map_or(0, |a| {
            ((a.general as u64) << 32) | (((a.action & 0xf) as u64) << 28) | ((a.lethal as u64) << 27) | (((a.clip & 0x1fff) as u64) << 14) | (a.t * 1000.0).clamp(0.0, 16383.0) as u64
        });
        slot.store(v, Ordering::Relaxed);
    }
}

/// General `general`'s torso action, if he is in one.
pub(crate) fn torso_of(general: u32) -> Option<TorsoAction> {
    use std::sync::atomic::Ordering;
    torso_slots().iter().map(|s| s.load(Ordering::Relaxed)).find(|&v| v != 0 && (v >> 32) as u32 == general).map(|v| TorsoAction {
        general,
        action: ((v >> 28) & 0xf) as u8,
        lethal: (v >> 27) & 1 != 0,
        clip: ((v >> 14) & 0x1fff) as u16,
        t: (v & 0x3fff) as f32 / 1000.0,
    })
}

/// Each General's aim (`general << 32 | shield << 16 | view pitch in hundredths of a degree`, BO2's
/// sign: positive looking down), published by the client each frame (`publish_aim`): the body's
/// aim controllers read it (`aim`). Presentation only.
static AIM: [std::sync::atomic::AtomicU64; crate::sim::MAX_PLAYERS] = [const { std::sync::atomic::AtomicU64::new(0) }; crate::sim::MAX_PLAYERS];

/// Publish the Generals' aims: `(general, view pitch in degrees with BO2's sign, holding a riot
/// shield)`.
pub fn publish_aim(aims: impl Iterator<Item = (u32, f32, bool)>) {
    use std::sync::atomic::Ordering;
    let mut it = aims;
    for slot in AIM.iter() {
        let v = it.next().map_or(0, |(g, pitch, shield)| ((g as u64) << 32) | ((shield as u64) << 16) | ((pitch * 100.0).round().clamp(-32768.0, 32767.0) as i16 as u16) as u64);
        slot.store(v, Ordering::Relaxed);
    }
}

/// General `general`'s published aim: (view pitch, degrees, BO2's sign; a riot shield in hand).
pub(crate) fn aim_of(general: u32) -> Option<(f32, bool)> {
    use std::sync::atomic::Ordering;
    AIM.iter().map(|s| s.load(Ordering::Relaxed)).find(|&v| v != 0 && (v >> 32) as u32 == general).map(|v| ((v & 0xffff) as u16 as i16 as f32 / 100.0, (v >> 16) & 1 != 0))
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(State::default))
}

/// Draw one General's third-person rig natively this frame. Returns 1 (drawn: the bridge hides the
/// W3D weapon/body), 2 (files still coming: keep drawing W3D) or 0 (not native).
///
/// # Safety
/// `inp` must point to a valid `ZhboNativeBodyIn`.
#[no_mangle]
pub unsafe extern "C" fn zhbo_native_body(inp: *const ZhboNativeBodyIn) -> i32 {
    match unsafe { inp.as_ref() } {
        Some(inp) => {
            // Read before taking the third person's lock: the sim's state has its own.
            let camo = crate::ffi::third_person_camo(inp.general, inp.vm);
            with(|s| s.frame(inp, crate::config::get().native_body as i32, camo.as_deref()))
        }
        None => NOT_NATIVE,
    }
}

/// `ZHBO_NATIVE_BODY`'s mode (0 off, 1 the weapon in the W3D body's hand, 2 the body too): the
/// bridge skips its third-person work when this is 0.
#[no_mangle]
pub extern "C" fn zhbo_native_body_enabled() -> i32 {
    crate::config::get().native_body as i32
}

/// The weapon-only mode needs the engine's tag; a body-mode weapon rides the native body's own
/// bone, so the bridge's tag is ignored there. The bridge logs a missing tag once per General.
///
/// # Safety
/// `inp` must point to a valid `ZhboNativeBodyIn`.
#[no_mangle]
pub unsafe extern "C" fn zhbo_native_body_ready(inp: *const ZhboNativeBodyIn) -> i32 {
    match unsafe { inp.as_ref() } {
        Some(inp) => {
            (inp.mode == 2 || inp.mode == LYING || inp.tag_valid != 0) as i32
        }
        None => 0,
    }
}

/// The `i`-th file the native third person is waiting for (NUL-terminated into `buf`), or -1.
///
/// # Safety
/// `buf` must hold `cap` bytes.
#[no_mangle]
pub unsafe extern "C" fn zhbo_native_body_wanted(i: i32, buf: *mut c_char, cap: i32) -> i32 {
    with(|s| {
        let Some(path) = s.files.wanted().nth(i.max(0) as usize) else { return -1 };
        if buf.is_null() || path.len() + 1 > cap.max(0) as usize {
            return -1;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(path.as_ptr(), buf as *mut u8, path.len());
            *buf.add(path.len()) = 0;
        }
        path.len() as i32
    })
}

/// The bridge's answer to a request: the file's bytes, or `len < 0` when the archive lacks it.
///
/// # Safety
/// `path` must be NUL-terminated; `data` must hold `len` bytes when `len >= 0`.
#[no_mangle]
pub unsafe extern "C" fn zhbo_native_body_provide(path: *const c_char, data: *const u8, len: i32) {
    if path.is_null() {
        return;
    }
    let path = unsafe { std::ffi::CStr::from_ptr(path) }.to_string_lossy().into_owned();
    let bytes = (len >= 0).then(|| if len == 0 || data.is_null() { &[][..] } else { unsafe { std::slice::from_raw_parts(data, len as usize) } });
    with(|s| s.files.provide(&path, bytes));
}

/// Pop one log line into `buf` (NUL-terminated, truncated to fit). Returns its length, -1 if none.
///
/// # Safety
/// `buf` must hold `cap` bytes.
#[no_mangle]
pub unsafe extern "C" fn zhbo_native_body_take_log(buf: *mut c_char, cap: i32) -> i32 {
    with(|s| {
        if buf.is_null() || cap <= 0 {
            return -1;
        }
        let Some(line) = s.log.pop_front() else { return -1 };
        let mut n = line.len().min(cap as usize - 1);
        while !line.is_char_boundary(n) {
            n -= 1;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(line.as_ptr(), buf as *mut u8, n);
            *buf.add(n) = 0;
        }
        n as i32
    })
}

/// A bone's world transform in the last third-person pose (3 rows of 4, the bridge's `Matrix3D`).
/// Returns 1, or 0 when there is no such bone or nothing is posed.
///
/// # Safety
/// `name` must be NUL-terminated; `out` must hold 12 floats.
#[no_mangle]
pub unsafe extern "C" fn zhbo_native_body_bone(name: *const c_char, out: *mut f32) -> i32 {
    if name.is_null() || out.is_null() {
        return 0;
    }
    let name = unsafe { std::ffi::CStr::from_ptr(name) }.to_string_lossy();
    with(|s| {
        let Some(m) = s.loaded().and_then(|l| l.bone_world(&s.posed, &name)) else { return 0 };
        let flat: Vec<f32> = m.iter().flatten().copied().collect();
        unsafe { std::ptr::copy_nonoverlapping(flat.as_ptr(), out, 12) };
        1
    })
}

/// General `general`'s muzzle in his last drawn pose (3 rows of 4, the bridge's `Matrix3D`, an
/// orthonormal frame with X down the barrel, as the W3D fire bone's): the held weapon's `tag_flash`
/// while the body mode draws him with the gun in hand. Returns 1, or 0 when there is none (another
/// mode, the gun hidden, nothing drawn yet) and the engine's own placement stands. Presentation
/// only: the bridge plays his shots' effects there (host patch 0029).
///
/// # Safety
/// `out` must hold 12 floats.
#[no_mangle]
pub unsafe extern "C" fn zhbo_native_body_flash(general: u32, out: *mut f32) -> i32 {
    with(|s| unsafe { zhbo_native_body_flash_in(s, general, out) })
}

/// [`zhbo_native_body_flash`] on a given state (the tests hold the state's lock already).
///
/// # Safety
/// `out` must hold 12 floats when not null.
pub(crate) unsafe fn zhbo_native_body_flash_in(s: &State, general: u32, out: *mut f32) -> i32 {
    let Some(m) = s.flash_of(general).filter(|_| !out.is_null()) else { return 0 };
    let flat: Vec<f32> = m.iter().flatten().copied().collect();
    unsafe { std::ptr::copy_nonoverlapping(flat.as_ptr(), out, 12) };
    1
}

/// Keep only the dropped weapons' rigs still lying (`objects`): the core calls this with every
/// frame's list (`zhbo_client_dropped_weapons`).
pub(crate) fn retain_dropped(objects: &[u32]) {
    with(|s| s.retain_dropped(objects));
}

/// Forget everything (a game reset tears the scene down).
#[no_mangle]
pub extern "C" fn zhbo_native_body_reset() {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = g.as_mut() {
        s.release_all();
    }
    *g = None;
}

/// A mutable look at the state for the tests. It also holds [`crate::ffi::state_lock`]: the
/// logic step the ffi tests drive republishes the stowed-weapon mirror this state reads.
#[cfg(test)]
pub(crate) fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let _g = crate::ffi::state_lock();
    with(f)
}
