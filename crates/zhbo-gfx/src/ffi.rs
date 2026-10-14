//! C ABI over [`Renderer`], called by the C++ adapter that defines the `bgfx::` functions
//! (host/zhbo/bgfx_wgpu.cpp). Handles are bgfx's u16 indices; `0xFFFF` is invalid. All calls come
//! from the engine's render thread; the mutex only makes the global sound.

use std::ffi::{c_char, c_void, CStr};
use std::sync::Mutex;

use crate::runtime::{BufferKind, InitDesc, Renderer, ResourceKind, VertexLayout, WindowKind, INVALID};

static RENDERER: Mutex<Option<Renderer>> = Mutex::new(None);
static CALLBACKS: Mutex<(Option<TraceCb>, Option<ScreenshotCb>)> = Mutex::new((None, None));

pub type TraceCb = extern "C" fn(msg: *const c_char);
pub type ScreenshotCb = extern "C" fn(path: *const c_char, width: u32, height: u32, pitch: u32, data: *const u8, size: u32);

#[repr(C)]
pub struct ZhgfxInit {
    /// 0 headless, 1 Win32 HWND, 2 CAMetalLayer, 3 X11, 4 Wayland.
    pub kind: u32,
    pub window: *mut c_void,
    pub display: *mut c_void,
    pub width: u32,
    pub height: u32,
    pub reset: u32,
    pub transient_vb_size: u32,
    pub transient_ib_size: u32,
}

/// The members of a `bgfx::VertexLayout` the renderer needs.
#[repr(C)]
pub struct ZhgfxLayout {
    pub stride: u16,
    pub offset: [u16; 18],
    pub attributes: [u16; 18],
}

#[repr(C)]
#[derive(Default)]
pub struct ZhgfxStats {
    pub num_draw: u32,
    pub num_blit: u32,
    pub num_textures: u32,
    pub num_frame_buffers: u32,
    pub transient_vb_used: u32,
    pub transient_ib_used: u32,
    pub errors: u32,
    pub cpu_frame_ns: u64,
}

fn with<R>(default: R, f: impl FnOnce(&mut Renderer) -> R) -> R {
    match RENDERER.lock() {
        Ok(mut g) => g.as_mut().map(f).unwrap_or(default),
        Err(_) => default,
    }
}

unsafe fn bytes<'a>(data: *const u8, size: u32) -> Option<&'a [u8]> {
    (!data.is_null() && size > 0).then(|| unsafe { std::slice::from_raw_parts(data, size as usize) })
}

unsafe fn layout(l: *const ZhgfxLayout) -> Option<VertexLayout> {
    let l = unsafe { l.as_ref()? };
    Some(VertexLayout::from_bgfx(l.stride, &l.offset, &l.attributes))
}

fn trace(msg: &str) {
    let cb = CALLBACKS.lock().ok().and_then(|c| c.0);
    match cb {
        Some(cb) => {
            let c = std::ffi::CString::new(msg.replace('\0', " ")).unwrap_or_default();
            cb(c.as_ptr());
        }
        None => eprintln!("{msg}"),
    }
}

#[no_mangle]
pub extern "C" fn zhgfx_set_callbacks(trace: Option<TraceCb>, screenshot: Option<ScreenshotCb>) {
    if let Ok(mut c) = CALLBACKS.lock() {
        *c = (trace, screenshot);
    }
}

/// # Safety
/// `init` must point to a valid `ZhgfxInit` whose handles stay valid while the renderer lives.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_init(init: *const ZhgfxInit) -> bool {
    let Some(i) = (unsafe { init.as_ref() }) else { return false };
    let kind = match i.kind {
        1 => WindowKind::Win32,
        2 => WindowKind::MetalLayer,
        3 => WindowKind::Xlib,
        4 => WindowKind::Wayland,
        _ => WindowKind::Headless,
    };
    let desc = InitDesc {
        kind,
        window: i.window,
        display: i.display,
        width: i.width,
        height: i.height,
        reset: i.reset,
        transient_vb_size: i.transient_vb_size,
        transient_ib_size: i.transient_ib_size,
    };
    match Renderer::new(&desc, Box::new(trace)) {
        Ok(mut r) => {
            r.set_screenshot_fn(Box::new(|path, w, h, pitch, data| {
                let cb = CALLBACKS.lock().ok().and_then(|c| c.1);
                if let Some(cb) = cb {
                    let p = std::ffi::CString::new(path).unwrap_or_default();
                    cb(p.as_ptr(), w, h, pitch, data.as_ptr(), data.len() as u32);
                }
            }));
            trace(&format!("zhbo-gfx: renderer up on {}", r.adapter_name()));
            if let Ok(mut g) = RENDERER.lock() {
                *g = Some(r);
            }
            true
        }
        Err(e) => {
            trace(&format!("zhbo-gfx: init failed: {e}"));
            false
        }
    }
}

#[no_mangle]
pub extern "C" fn zhgfx_shutdown() {
    if let Ok(mut g) = RENDERER.lock() {
        *g = None;
    }
}

#[no_mangle]
pub extern "C" fn zhgfx_reset(width: u32, height: u32, flags: u32) {
    with((), |r| r.reset(width, height, flags));
}

#[no_mangle]
pub extern "C" fn zhgfx_frame() -> u32 {
    with(0, |r| r.frame())
}

/// Writes the adapter name (NUL-terminated, truncated to `cap`).
///
/// # Safety
/// `out` must have room for `cap` bytes.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_adapter_name(out: *mut c_char, cap: u32) {
    if out.is_null() || cap == 0 {
        return;
    }
    let name = with(String::new(), |r| r.adapter_name());
    let n = name.len().min(cap as usize - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(name.as_ptr() as *const c_char, out, n);
        *out.add(n) = 0;
    }
}

#[no_mangle]
pub extern "C" fn zhgfx_format_caps(bgfx_format: u32) -> u32 {
    with(0, |r| r.format_caps(bgfx_format))
}

#[no_mangle]
pub extern "C" fn zhgfx_max_texture_size() -> u32 {
    with(0, |r| r.max_texture_size())
}

/// # Safety
/// `out` must be valid for writes.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_stats(out: *mut ZhgfxStats) {
    let Some(out) = (unsafe { out.as_mut() }) else { return };
    let s = with(Default::default(), |r| r.stats());
    *out = ZhgfxStats {
        num_draw: s.num_draw,
        num_blit: s.num_blit,
        num_textures: s.num_textures,
        num_frame_buffers: s.num_frame_buffers,
        transient_vb_used: s.transient_vb_used,
        transient_ib_used: s.transient_ib_used,
        errors: s.errors,
        cpu_frame_ns: s.cpu_frame_ns,
    };
}

/// # Safety
/// `data` must be valid for `size` bytes.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_create_shader(data: *const u8, size: u32) -> u16 {
    let Some(d) = (unsafe { bytes(data, size) }) else { return INVALID };
    with(INVALID, |r| r.create_shader(d))
}

#[no_mangle]
pub extern "C" fn zhgfx_create_program(vs: u16, fs: u16, destroy_shaders: bool) -> u16 {
    with(INVALID, |r| r.create_program(vs, fs, destroy_shaders))
}

/// # Safety
/// `name` must be a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_create_uniform(name: *const c_char, ty: u8, num: u16) -> u16 {
    if name.is_null() {
        return INVALID;
    }
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned();
    with(INVALID, |r| r.create_uniform(&name, ty, num))
}

/// # Safety
/// `value` must hold `floats` floats.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_set_uniform(h: u16, value: *const f32, floats: u32) {
    if value.is_null() {
        return;
    }
    let v = unsafe { std::slice::from_raw_parts(value, floats as usize) };
    with((), |r| r.set_uniform(h, v));
}

/// `kind`: 0 vertex buffer, 1 index buffer, 2 dynamic vertex buffer, 3 dynamic index buffer,
/// 4 shader, 5 program, 6 texture, 7 frame buffer, 8 uniform.
#[no_mangle]
pub extern "C" fn zhgfx_destroy(kind: u32, h: u16) {
    let kind = match kind {
        0 => ResourceKind::VertexBuffer,
        1 => ResourceKind::IndexBuffer,
        2 => ResourceKind::DynamicVertexBuffer,
        3 => ResourceKind::DynamicIndexBuffer,
        4 => ResourceKind::Shader,
        5 => ResourceKind::Program,
        6 => ResourceKind::Texture,
        7 => ResourceKind::FrameBuffer,
        8 => ResourceKind::Uniform,
        _ => return,
    };
    with((), |r| r.destroy(kind, h));
}

/// # Safety
/// `l` must point to a valid layout.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_register_layout(l: *const ZhgfxLayout) -> u16 {
    let Some(l) = (unsafe { layout(l) }) else { return INVALID };
    with(INVALID, |r| r.register_layout(l))
}

/// # Safety
/// `data`/`l` must be valid.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_create_vertex_buffer(data: *const u8, size: u32, l: *const ZhgfxLayout) -> u16 {
    let (Some(d), Some(l)) = (unsafe { bytes(data, size) }, unsafe { layout(l) }) else { return INVALID };
    with(INVALID, |r| r.create_vertex_buffer(d, l))
}

/// # Safety
/// `data` must be valid for `size` bytes.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_create_index_buffer(data: *const u8, size: u32, flags: u16) -> u16 {
    let Some(d) = (unsafe { bytes(data, size) }) else { return INVALID };
    with(INVALID, |r| r.create_index_buffer(d, flags))
}

/// # Safety
/// `data` may be null; otherwise valid for `size` bytes. `l` must be valid.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_create_dynamic_vertex_buffer(num: u32, data: *const u8, size: u32, l: *const ZhgfxLayout) -> u16 {
    let Some(l) = (unsafe { layout(l) }) else { return INVALID };
    let d = unsafe { bytes(data, size) };
    with(INVALID, |r| r.create_dynamic_vertex_buffer(num, d, l))
}

/// # Safety
/// `data` may be null; otherwise valid for `size` bytes.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_create_dynamic_index_buffer(num: u32, data: *const u8, size: u32, flags: u16) -> u16 {
    let d = unsafe { bytes(data, size) };
    with(INVALID, |r| r.create_dynamic_index_buffer(num, d, flags))
}

/// # Safety
/// `data` must be valid for `size` bytes.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_update_dynamic_buffer(index: bool, h: u16, start: u32, data: *const u8, size: u32) {
    let Some(d) = (unsafe { bytes(data, size) }) else { return };
    with((), |r| if index { r.update_dynamic_index_buffer(h, start, d) } else { r.update_dynamic_vertex_buffer(h, start, d) });
}

#[no_mangle]
pub extern "C" fn zhgfx_transient_avail(index: bool, num: u32, stride: u32) -> u32 {
    with(0, |r| r.transient_avail(index, num, stride))
}

/// Allocates transient memory; returns the pointer (null if out of room) and writes the start
/// (in units of `stride`).
///
/// # Safety
/// `start` must be valid for writes.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_transient_alloc(index: bool, num: u32, stride: u32, start: *mut u32) -> *mut u8 {
    match with(None, |r| r.transient_alloc(index, num, stride)) {
        Some((ptr, s)) => {
            if let Some(start) = unsafe { start.as_mut() } {
                *start = s;
            }
            ptr
        }
        None => std::ptr::null_mut(),
    }
}

/// # Safety
/// `data` may be null; otherwise valid for `size` bytes.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_create_texture_2d(width: u32, height: u32, has_mips: bool, layers: u32, format: u32, flags: u64, data: *const u8, size: u32) -> u16 {
    let d = unsafe { bytes(data, size) };
    with(INVALID, |r| r.create_texture_2d(width, height, has_mips, layers, format, flags, d))
}

/// `pitch == 0xFFFF` means tightly packed rows.
///
/// # Safety
/// `data` must be valid for `size` bytes.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_update_texture_2d(h: u16, layer: u32, mip: u32, x: u32, y: u32, w: u32, height: u32, data: *const u8, size: u32, pitch: u32) {
    let Some(d) = (unsafe { bytes(data, size) }) else { return };
    let pitch = (pitch != 0xFFFF).then_some(pitch);
    with((), |r| r.update_texture_2d(h, layer, mip, x, y, w, height, d, pitch));
}

/// # Safety
/// `textures` must hold `num` handles.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_create_frame_buffer(num: u8, textures: *const u16, destroy_textures: bool) -> u16 {
    if textures.is_null() {
        return INVALID;
    }
    let t = unsafe { std::slice::from_raw_parts(textures, num as usize) }.to_vec();
    with(INVALID, |r| r.create_frame_buffer(&t, destroy_textures))
}

#[no_mangle]
pub extern "C" fn zhgfx_create_frame_buffer_sized(width: u32, height: u32, format: u32, flags: u64) -> u16 {
    with(INVALID, |r| r.create_frame_buffer_sized(width, height, format, flags))
}

#[no_mangle]
pub extern "C" fn zhgfx_set_view_rect(v: u16, x: u16, y: u16, w: u16, h: u16) {
    with((), |r| r.set_view_rect(v, x, y, w, h));
}

#[no_mangle]
pub extern "C" fn zhgfx_set_view_clear(v: u16, flags: u16, rgba: u32, depth: f32, stencil: u8) {
    with((), |r| r.set_view_clear(v, flags, rgba, depth, stencil));
}

#[no_mangle]
pub extern "C" fn zhgfx_set_view_mode(v: u16, mode: u8) {
    with((), |r| r.set_view_mode(v, mode));
}

#[no_mangle]
pub extern "C" fn zhgfx_set_view_frame_buffer(v: u16, fb: u16) {
    with((), |r| r.set_view_frame_buffer(v, fb));
}

/// # Safety
/// Each matrix pointer is null or points to 16 floats.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_set_view_transform(v: u16, view: *const f32, proj: *const f32) {
    let m = |p: *const f32| unsafe { (p as *const [f32; 16]).as_ref() }.copied();
    let (view, proj) = (m(view), m(proj));
    with((), |r| r.set_view_transform(v, view.as_ref(), proj.as_ref()));
}

/// # Safety
/// `order` is null (reset) or holds `num` view ids.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_set_view_order(first: u16, num: u16, order: *const u16) {
    let o = (!order.is_null()).then(|| unsafe { std::slice::from_raw_parts(order, num as usize) }.to_vec());
    with((), |r| r.set_view_order(first, o.as_deref(), num));
}

#[no_mangle]
pub extern "C" fn zhgfx_touch(v: u16) {
    with((), |r| r.touch(v));
}

/// # Safety
/// `path` must be a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_request_screenshot(fb: u16, path: *const c_char) {
    if path.is_null() {
        return;
    }
    let p = unsafe { CStr::from_ptr(path) }.to_string_lossy().into_owned();
    with((), |r| r.request_screenshot(fb, &p));
}

#[no_mangle]
pub extern "C" fn zhgfx_set_state(state: u64, rgba: u32) {
    with((), |r| r.set_state(state, rgba));
}

#[no_mangle]
pub extern "C" fn zhgfx_set_stencil(front: u32, back: u32) {
    with((), |r| r.set_stencil(front, back));
}

/// Stores `num` matrices; returns the cache index (use with `zhgfx_set_transform_cached`).
///
/// # Safety
/// `mtx` must hold `num * 16` floats.
#[no_mangle]
pub unsafe extern "C" fn zhgfx_alloc_transform(mtx: *const f32, num: u16) -> u32 {
    if mtx.is_null() || num == 0 {
        return 0;
    }
    let m = unsafe { std::slice::from_raw_parts(mtx as *const [f32; 16], num as usize) }.to_vec();
    with(0, |r| r.alloc_transform(&m))
}

#[no_mangle]
pub extern "C" fn zhgfx_set_transform_cached(cache: u32, num: u16) {
    with((), |r| r.set_transform_cached(cache, num));
}

fn buffer_kind(kind: u32) -> BufferKind {
    match kind {
        1 => BufferKind::Dynamic,
        2 => BufferKind::Transient { index32: false },
        3 => BufferKind::Transient { index32: true },
        _ => BufferKind::Static,
    }
}

/// `kind`: 0 static, 1 dynamic, 2 transient 16-bit, 3 transient 32-bit. For transient buffers
/// `first` is the absolute index in the transient arena.
#[no_mangle]
pub extern "C" fn zhgfx_set_index_buffer(kind: u32, h: u16, first: u32, num: u32) {
    with((), |r| r.set_index_buffer(buffer_kind(kind), h, first, num));
}

/// `kind`: 0 static, 1 dynamic, 2 transient (then `h` is the layout id). `num == u32::MAX`: all.
#[no_mangle]
pub extern "C" fn zhgfx_set_vertex_buffer(stream: u8, kind: u32, h: u16, start: u32, num: u32, layout: u16) {
    with((), |r| r.set_vertex_buffer(stream, buffer_kind(kind), h, start, num, layout));
}

#[no_mangle]
pub extern "C" fn zhgfx_clear_vertex_buffer(stream: u8) {
    with((), |r| r.set_vertex_buffer(stream, BufferKind::Static, INVALID, 0, 0, INVALID));
}

/// `kind`: 0 static, 1 dynamic, 2 transient. `num == 0` clears instance data.
#[no_mangle]
pub extern "C" fn zhgfx_set_instance_data(kind: u32, h: u16, start: u32, num: u32, stride: u32) {
    with((), |r| {
        if num == 0 {
            r.discard(crate::runtime::DISCARD_INSTANCE_DATA)
        } else {
            r.set_instance_data(buffer_kind(kind), h, start, num, stride)
        }
    });
}

#[no_mangle]
pub extern "C" fn zhgfx_set_texture(stage: u8, h: u16, flags: u32) {
    with((), |r| r.set_texture(stage, h, flags));
}

#[no_mangle]
pub extern "C" fn zhgfx_submit(v: u16, program: u16, depth: u32, discard: u8) {
    with((), |r| r.submit(v, program, depth, discard));
}

#[no_mangle]
pub extern "C" fn zhgfx_discard(flags: u8) {
    with((), |r| r.discard(flags));
}

#[allow(clippy::too_many_arguments)]
#[no_mangle]
pub extern "C" fn zhgfx_blit(v: u16, dst: u16, dst_mip: u8, dx: u16, dy: u16, dz: u16, src: u16, src_mip: u8, sx: u16, sy: u16, sz: u16, w: u16, h: u16, d: u16) {
    with((), |r| r.blit(v, dst, dst_mip, dx, dy, dz, src, src_mip, sx, sy, sz, w, h, d));
}
