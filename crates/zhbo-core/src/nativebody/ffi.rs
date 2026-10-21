//! The native third person's C ABI (appended to `zhbo_core.h`; see CONTRACT.md "The native third
//! person"). Called from the engine's client frame by `ZhboViewmodel.cpp` for each General the sim
//! reports (`zhbo_client_native_poses`): each frame one [`zhbo_native_body`], then the file
//! requests ([`zhbo_native_wanted`]/[`zhbo_native_provide`]) and log lines
//! ([`zhbo_native_take_log`]) — the same channels the viewmodel uses; [`zhbo_native_body_reset`]
//! on a game reset.

use std::collections::{HashSet, VecDeque};
use std::ffi::c_char;
use std::sync::Mutex;

use super::{load, Loaded, Mode, Posed, Request};
use crate::nativevm::ffi::ZhboNativeLight;

/// One general's third-person frame.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ZhboNativeBodyIn {
    /// The General object id (logging only).
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

#[derive(Default)]
pub(crate) struct State {
    pub(crate) files: crate::nativevm::Files,
    pub(crate) loaded: Option<Loaded>,
    failed: HashSet<(u8, i32)>,
    pub(crate) posed: Posed,
    pub(crate) log: VecDeque<String>,
    logged: HashSet<String>,
    #[cfg(feature = "renderer")]
    pub(crate) gpu: super::gpu::Gpu,
}

impl State {
    /// The lines logged so far (tests).
    #[cfg(test)]
    pub(crate) fn logged(&self) -> impl Iterator<Item = &str> {
        self.logged.iter().map(String::as_str)
    }

    pub(crate) fn log_once(&mut self, msg: String) {
        if self.logged.len() < 256 && self.logged.insert(msg.clone()) {
            self.log.push_back(format!("native body: {msg}"));
        }
    }

    /// One general's frame. `enabled` is `ZHBO_NATIVE_BODY != 0`; without the renderer it loads and
    /// poses, then answers "not native".
    pub(crate) fn frame(&mut self, inp: &ZhboNativeBodyIn, enabled: i32) -> i32 {
        if enabled <= 0 || inp.vm < 0 || inp.faction <= 0 {
            return NOT_NATIVE;
        }
        let mode = match inp.mode {
            2 => Mode::Body,
            _ => Mode::Weapon,
        };
        // Milestone 2 draws only the weapon; the body mode lands with milestone 3.
        if mode == Mode::Body {
            return NOT_NATIVE;
        }
        // The weapon-only mode is placed by the engine's tag: without one there is nowhere to draw.
        if mode == Mode::Weapon && inp.tag_valid == 0 {
            return NOT_NATIVE;
        }
        let faction = inp.faction as u8;
        let key = (faction, inp.vm);
        if self.failed.contains(&key) {
            return NOT_NATIVE;
        }
        if self.loaded.as_ref().is_none_or(|l| l.key != key) {
            #[cfg(feature = "renderer")]
            self.gpu.release();
            self.loaded = None;
            let req = request(inp, mode);
            match load(&mut self.files, &req) {
                Err(e) => {
                    self.failed.insert(key);
                    self.log_once(e);
                    return NOT_NATIVE;
                }
                Ok(None) => return LOADING,
                Ok(Some(l)) => {
                    self.log_once(format!("ZW{:02}{} composed: {} bones, {} vertices, {} surfaces", l.key.1, l.key.0 as char, l.skeleton.bones.len(), l.mesh.vertices.len(), l.mesh.surfaces.len()));
                    self.loaded = Some(l);
                }
            }
        }
        let req = request(inp, mode);
        let Some(loaded) = self.loaded.as_ref() else { return NOT_NATIVE };
        loaded.pose(&req, None, &mut self.posed);
        #[cfg(feature = "renderer")]
        {
            super::gpu::draw(self, inp)
        }
        #[cfg(not(feature = "renderer"))]
        NOT_NATIVE
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
    }
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
        Some(inp) => with(|s| s.frame(inp, crate::config::get().native_body as i32)),
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
            let mode = if inp.mode == 2 { Mode::Body } else { Mode::Weapon };
            (mode == Mode::Body || inp.tag_valid != 0) as i32
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
        let Some(m) = s.loaded.as_ref().and_then(|l| l.bone_world(&s.posed, &name)) else { return 0 };
        let flat: Vec<f32> = m.iter().flatten().copied().collect();
        unsafe { std::ptr::copy_nonoverlapping(flat.as_ptr(), out, 12) };
        1
    })
}

/// Forget everything (a game reset tears the scene down).
#[no_mangle]
pub extern "C" fn zhbo_native_body_reset() {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    #[cfg(feature = "renderer")]
    if let Some(s) = g.as_mut() {
        s.gpu.release();
    }
    *g = None;
}

/// A mutable look at the state for the tests.
#[cfg(test)]
pub(crate) fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    with(f)
}
