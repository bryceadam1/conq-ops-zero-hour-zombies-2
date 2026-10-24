//! The native viewmodel's C ABI (appended to `zhbo_core.h`; see CONTRACT.md "Native viewmodel").
//!
//! Called from the engine's main thread by `ZhboViewmodel.cpp`: each frame
//! [`zhbo_native_viewmodel`], then the file requests ([`zhbo_native_wanted`] /
//! [`zhbo_native_provide`]) and the log lines ([`zhbo_native_take_log`]); [`zhbo_native_bone`] for
//! effects at the gun's tags; [`zhbo_native_reset`] when a game tears down.

use std::collections::{HashSet, VecDeque};
use std::ffi::{c_char, CStr};
use std::sync::Mutex;

use super::{load, Files, GunPose, Loaded, Posed, Request};
use zhbo_native::anim::Anim;

/// A scene light: towards the light (world space) and its display-encoded colour.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ZhboNativeLight {
    pub dir: [f32; 3],
    pub color: [f32; 3],
}

/// One frame's request (`ZhboNativeIn` in `zhbo_core.h`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ZhboNativeIn {
    pub eye: [f32; 3],
    pub dir: [f32; 3],
    /// The gun's view-space offset and yaw/pitch/roll (radians), as the W3D path takes them.
    pub offset: [f32; 3],
    pub angles: [f32; 3],
    pub clip: i32,
    /// Converted weapon index (`ZV<vm><faction>`), and the faction letter.
    pub vm: i32,
    pub faction: i32,
    pub time: f32,
    pub ads: f32,
    /// Animation playback rate (1 = BO2's own).
    pub rate: f32,
    pub ambient: [f32; 3],
    pub lights: [ZhboNativeLight; 3],
    pub light_count: i32,
    /// The engine's scene view (drawn into) and the view the pass runs before (the composite).
    pub target_view: u16,
    pub before_view: u16,
    /// `ZHBO_BO2_DEBUG`'s term view (0 off).
    pub debug: f32,
}

const _: () = assert!(std::mem::size_of::<ZhboNativeLight>() == 24);
const _: () = assert!(std::mem::size_of::<ZhboNativeIn>() == 168);

/// `zhbo_native_viewmodel`'s answers.
pub const NOT_NATIVE: i32 = 0;
pub const DRAWN: i32 = 1;
pub const LOADING: i32 = 2;

#[derive(Default)]
pub(crate) struct State {
    pub(crate) files: Files,
    pub(crate) loaded: Option<Loaded>,
    failed: HashSet<(i32, u8)>,
    pub(crate) posed: Posed,
    pub(crate) log: VecDeque<String>,
    logged: HashSet<String>,
    bad_anims: HashSet<String>,
    #[cfg(feature = "renderer")]
    pub(crate) gpu: super::gpu::Gpu,
}

impl State {
    pub(crate) fn log_once(&mut self, msg: String) {
        if self.logged.len() < 256 && self.logged.insert(msg.clone()) {
            self.log.push_back(format!("native viewmodel: {msg}"));
        }
    }

    /// Bring the clip's animation in (decoded once, kept by path).
    fn anim(&mut self, path: &str) -> Option<()> {
        let loaded = self.loaded.as_ref()?;
        if loaded.anims.contains_key(path) || self.bad_anims.contains(path) {
            return loaded.anims.contains_key(path).then_some(());
        }
        let bytes = self.files.get(path)?;
        match Anim::from_bytes(&bytes) {
            Ok(a) => {
                self.files.forget(path);
                self.loaded.as_mut()?.anims.insert(path.to_string(), a);
                Some(())
            }
            Err(e) => {
                self.bad_anims.insert(path.to_string());
                self.log_once(format!("{path}: {e}"));
                None
            }
        }
    }

    /// One frame, when `enabled` (`ZHBO_NATIVE_VIEWMODEL`). Pure apart from the renderer submit
    /// (`gpu`); without the renderer it loads and poses, then answers "not native".
    pub(crate) fn frame(&mut self, inp: &ZhboNativeIn, enabled: bool) -> i32 {
        if !enabled {
            return NOT_NATIVE;
        }
        let key = (inp.vm, inp.faction as u8);
        if inp.vm < 0 || self.failed.contains(&key) {
            return NOT_NATIVE;
        }
        if self.loaded.as_ref().is_none_or(|l| l.key != key) {
            #[cfg(feature = "renderer")]
            self.gpu.release();
            self.loaded = None;
            match load(&mut self.files, inp.vm, inp.faction as u8, inp.ads) {
                Err(e) => {
                    self.failed.insert(key);
                    self.log_once(e);
                    return NOT_NATIVE;
                }
                Ok(None) => return LOADING,
                Ok(Some(l)) => {
                    self.log_once(format!("ZV{:02}{} composed: {} bones, {} vertices, {} surfaces", l.key.0, l.key.1 as char, l.skeleton.bones.len(), l.mesh.vertices.len(), l.mesh.surfaces.len()));
                    self.loaded = Some(l);
                }
            }
        }
        let req = request(inp);
        let path = self.loaded.as_ref().and_then(|l| l.clip_path(req.clip)).map(str::to_string);
        // A dual-wield pair's left half carries its own clip (`_left_*`) under the instance bit.
        // It is a separate animation over the same composite: the base clip never tracks the
        // left arm or the `_lh` gun, so without this the left gun stays at its bind pose.
        let left_path = self
            .loaded
            .as_ref()
            .and_then(|l| l.clip_path(req.clip | zhbo_native::viewmodel::INSTANCE_LEFT as i32))
            .map(str::to_string)
            .filter(|p| Some(p.as_str()) != path.as_deref());
        if let Some(p) = &path {
            self.anim(p);
        }
        if let Some(p) = &left_path {
            self.anim(p);
        }
        let Some(loaded) = self.loaded.as_ref() else { return NOT_NATIVE };
        let anim = path.as_deref().and_then(|p| loaded.anims.get(p));
        let left = left_path.as_deref().and_then(|p| loaded.anims.get(p));
        loaded.pose(&req, anim, left, &mut self.posed);
        #[cfg(feature = "renderer")]
        {
            super::gpu::draw(self, inp)
        }
        #[cfg(not(feature = "renderer"))]
        NOT_NATIVE
    }
}

pub(crate) fn request(inp: &ZhboNativeIn) -> Request {
    Request {
        vm: inp.vm,
        faction: inp.faction as u8,
        clip: inp.clip,
        time: inp.time,
        rate: if inp.rate > 0.0 && inp.rate.is_finite() { inp.rate } else { 1.0 },
        ads: inp.ads,
        eye: inp.eye,
        dir: inp.dir,
        gun: GunPose::from_offset_angles(inp.offset, inp.angles),
    }
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(State::default))
}

/// Draw the first-person viewmodel natively this frame. Returns 1 (drawn: hide the W3D model),
/// 2 (still loading files: keep the W3D model this frame) or 0 (not native: the W3D path draws).
///
/// # Safety
/// `inp` must point to a valid `ZhboNativeIn`.
#[no_mangle]
pub unsafe extern "C" fn zhbo_native_viewmodel(inp: *const ZhboNativeIn) -> i32 {
    match unsafe { inp.as_ref() } {
        Some(inp) => with(|s| s.frame(inp, crate::config::get().native_viewmodel)),
        None => NOT_NATIVE,
    }
}

/// The `i`-th file the native viewmodel is waiting for, NUL-terminated into `buf`. Returns its
/// length, or -1 past the last one (or if `buf` is too small).
///
/// # Safety
/// `buf` must hold `cap` bytes.
#[no_mangle]
pub unsafe extern "C" fn zhbo_native_wanted(i: i32, buf: *mut c_char, cap: i32) -> i32 {
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
pub unsafe extern "C" fn zhbo_native_provide(path: *const c_char, data: *const u8, len: i32) {
    if path.is_null() {
        return;
    }
    let path = unsafe { CStr::from_ptr(path) }.to_string_lossy().into_owned();
    let bytes = (len >= 0).then(|| if len == 0 || data.is_null() { &[][..] } else { unsafe { std::slice::from_raw_parts(data, len as usize) } });
    with(|s| s.files.provide(&path, bytes));
}

/// Pop one log line into `buf` (NUL-terminated, truncated to fit). Returns its length, -1 if none.
///
/// # Safety
/// `buf` must hold `cap` bytes.
#[no_mangle]
pub unsafe extern "C" fn zhbo_native_take_log(buf: *mut c_char, cap: i32) -> i32 {
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

/// Bone `name`'s world transform in the last native pose, as 3 rows of 4 (`Matrix3D`). Returns 1,
/// or 0 when there is no such bone or nothing is posed.
///
/// # Safety
/// `name` must be NUL-terminated; `out` must hold 12 floats.
#[no_mangle]
pub unsafe extern "C" fn zhbo_native_bone(name: *const c_char, out: *mut f32) -> i32 {
    if name.is_null() || out.is_null() {
        return 0;
    }
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
    with(|s| {
        let Some(m) = s.loaded.as_ref().and_then(|l| l.bone_world(&s.posed, &name)) else { return 0 };
        let flat: Vec<f32> = m.iter().flatten().copied().collect();
        unsafe { std::ptr::copy_nonoverlapping(flat.as_ptr(), out, 12) };
        1
    })
}

/// Forget everything (a game reset tears the scene down; the renderer's resources go too).
#[no_mangle]
pub extern "C" fn zhbo_native_reset() {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = g.as_mut() {
        #[cfg(feature = "renderer")]
        s.gpu.release();
        let _ = s;
    }
    *g = None;
    // The third person's state holds its own GPU handles and file cache; a game reset drops both.
    crate::nativebody::ffi::zhbo_native_body_reset();
}

/// A mutable look at the state for the tests.
#[cfg(test)]
pub(crate) fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    with(f)
}
