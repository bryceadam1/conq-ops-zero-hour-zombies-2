//! The renderer: bgfx's immediate-mode API (views, per-draw state, `submit`, `frame`) on wgpu.
//!
//! Like bgfx, draws are recorded into views during the frame and executed in view order at
//! `frame()`. Resource creation and updates go straight to the queue, which orders them before the
//! frame's command buffer, matching bgfx's "updates land before the frame's draws" behaviour. Draws
//! hold clones of the wgpu objects they use, so destroying a handle mid-frame is safe.
//!
//! The game renders into an offscreen backbuffer that is then copied to the window surface. That
//! keeps the backbuffer format fixed (BGRA8, as bgfx's screenshot callback expects), allows MSAA
//! and screenshots on every platform, and lets the renderer run headless in tests.

mod encode;
mod frame;
pub mod math;
mod native;
mod resources;
mod shock;

pub use shock::{ShockDraw, ShockLayer};
pub use native::{NativeBlend, NativeBlip, NativeCull, NativeDisplay, NativeDraw, NativeFrame, NativeLight, NativeOptics, NativeReticle, NativeReticleKind, NativeSonar, NativeSurface, NativeVertex, NATIVE_MAX_BLIPS, NATIVE_MAX_BONES, NATIVE_MAX_LIGHTS};

use std::collections::HashMap;
use std::sync::Arc;

use crate::blob::ShaderBlob;
use crate::shader::{Stage, UniformKind};
use crate::state::{self, FormatPlan};
use math::Mtx;

pub const INVALID: u16 = u16::MAX;
pub const MAX_VIEWS: usize = 256;
pub const MAX_STREAMS: usize = 4;
pub const MAX_TEXTURE_SLOTS: usize = 16;

pub const STATE_DEFAULT: u64 = 0x7 | 0x8 | state::STATE_WRITE_Z | (1 << 4) | (1 << 36) | state::STATE_MSAA;

pub const DISCARD_BINDINGS: u8 = 0x01;
pub const DISCARD_INDEX_BUFFER: u8 = 0x02;
pub const DISCARD_INSTANCE_DATA: u8 = 0x04;
pub const DISCARD_STATE: u8 = 0x08;
pub const DISCARD_TRANSFORM: u8 = 0x10;
pub const DISCARD_VERTEX_STREAMS: u8 = 0x20;

pub const BUFFER_INDEX32: u16 = 0x1000;

pub const TEXTURE_RT: u64 = 0x10_0000_0000;
pub const TEXTURE_RT_MSAA_SHIFT: u32 = 36;
pub const TEXTURE_RT_WRITE_ONLY: u64 = 0x80_0000_0000;
pub const TEXTURE_SRGB: u64 = 0x2000_0000_0000;

pub const RESET_MSAA_SHIFT: u32 = 4;
pub const RESET_VSYNC: u32 = 0x80;
pub const RESET_SRGB_BACKBUFFER: u32 = 0x8000;

/// How the window handle passed to `init` is to be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowKind {
    /// No window: render offscreen only (tests, captures).
    Headless,
    Win32,
    /// A `CAMetalLayer*` (what the engine passes on macOS).
    MetalLayer,
    Xlib,
    Wayland,
}

#[derive(Clone, Copy, Debug)]
pub struct InitDesc {
    pub kind: WindowKind,
    pub window: *mut std::ffi::c_void,
    pub display: *mut std::ffi::c_void,
    pub width: u32,
    pub height: u32,
    pub reset: u32,
    pub transient_vb_size: u32,
    pub transient_ib_size: u32,
}

/// Messages from the renderer, routed to the engine's bgfx callback (or stderr).
pub type TraceFn = Box<dyn Fn(&str) + Send + Sync>;
/// `(path, width, height, pitch, bgra8 data)` for a finished screenshot.
pub type ScreenshotFn = Box<dyn Fn(&str, u32, u32, u32, &[u8]) + Send + Sync>;

// --- handle pools -----------------------------------------------------------------------------

/// Handles pack a generation into the high 4 bits: `(generation << 12) | slot`. Destroying and
/// recreating a resource hands out a new generation for the slot, so a stale handle is rejected by
/// `get`/`get_mut` instead of silently operating on the slot's new occupant (bgfx's plain indices
/// would alias). `INVALID` (all 16 bits set) is reserved and never produced: slots stop at
/// `HANDLE_INDEX_MASK` and generations at 15. The C++ side treats handles as opaque `uint16_t`
/// (see `zhbo_gfx.h`), so the packing is invisible there.
const HANDLE_INDEX_MASK: u16 = 0x0FFF;
const HANDLE_GEN_SHIFT: u32 = 12;

struct Pool<T> {
    items: Vec<Option<T>>,
    /// Per-slot generation; a fresh slot starts at 1 so handle 0 is never handed out.
    gens: Vec<u8>,
    free: Vec<u16>,
}

impl<T> Default for Pool<T> {
    fn default() -> Self {
        Pool { items: Vec::new(), gens: Vec::new(), free: Vec::new() }
    }
}

impl<T> Pool<T> {
    fn insert(&mut self, t: T) -> u16 {
        if let Some(slot) = self.free.pop() {
            let g = &mut self.gens[slot as usize];
            *g = g.wrapping_add(1) & 0xF; // generations wrap; 16 reuses shadow an old handle
            self.items[slot as usize] = Some(t);
            return ((*g as u16) << HANDLE_GEN_SHIFT) | slot;
        }
        if self.items.len() >= HANDLE_INDEX_MASK as usize {
            return INVALID;
        }
        self.gens.push(1);
        self.items.push(Some(t));
        (1 << HANDLE_GEN_SHIFT) | (self.items.len() - 1) as u16
    }
    /// The slot for a handle, if the handle's generation matches the slot's.
    fn slot(&self, h: u16) -> Option<usize> {
        let slot = (h & HANDLE_INDEX_MASK) as usize;
        if self.gens.get(slot).copied() == Some((h >> HANDLE_GEN_SHIFT) as u8) && slot < self.items.len() {
            Some(slot)
        } else {
            None
        }
    }
    fn get(&self, h: u16) -> Option<&T> {
        self.slot(h).and_then(|s| self.items[s].as_ref())
    }
    fn get_mut(&mut self, h: u16) -> Option<&mut T> {
        match self.slot(h) {
            Some(s) => self.items[s].as_mut(),
            None => None,
        }
    }
    fn remove(&mut self, h: u16) -> Option<T> {
        let slot = self.slot(h)?;
        let t = self.items[slot].take();
        if t.is_some() {
            self.free.push(h & HANDLE_INDEX_MASK);
        }
        t
    }
    fn live(&self) -> usize {
        self.items.iter().filter(|i| i.is_some()).count()
    }
}

// --- resources --------------------------------------------------------------------------------

/// A decoded `bgfx::VertexLayout`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VertexLayout {
    pub stride: u16,
    /// Per `bgfx::Attrib`: offset and wgpu format.
    pub attribs: [Option<(u16, wgpu::VertexFormat)>; crate::state::ATTRIB_COUNT],
}

impl VertexLayout {
    /// From the raw `VertexLayout` members: `m_stride`, `m_offset[]`, `m_attributes[]` (bgfx's
    /// encoding: `num - 1` in bits 0-1, type in bits 3-5, normalized in bit 7, asInt in bit 8).
    pub fn from_bgfx(
        stride: u16,
        offsets: &[u16; crate::state::ATTRIB_COUNT],
        encoded: &[u16; crate::state::ATTRIB_COUNT],
    ) -> VertexLayout {
        let mut attribs = [None; crate::state::ATTRIB_COUNT];
        for i in 0..crate::state::ATTRIB_COUNT {
            let e = encoded[i];
            if e == u16::MAX {
                continue;
            }
            let num = (e & 3) as u8 + 1;
            let ty = ((e >> 3) & 7) as u8;
            let normalized = (e >> 7) & 1 != 0;
            let as_int = (e >> 8) & 1 != 0;
            attribs[i] = state::vertex_format(ty, num, normalized, as_int).map(|f| (offsets[i], f));
        }
        VertexLayout { stride, attribs }
    }
}

pub(crate) struct Shader {
    blob: ShaderBlob,
    module: wgpu::ShaderModule,
}

/// Where in a stage's uniform block a predefined uniform goes.
#[derive(Clone, Copy, Debug)]
enum Predefined {
    ViewRect,
    ViewTexel,
    View,
    InvView,
    Proj,
    InvProj,
    ViewProj,
    InvViewProj,
    Model,
    ModelView,
    ModelViewProj,
    AlphaRef,
}

fn predefined(name: &str) -> Option<Predefined> {
    Some(match name {
        "u_viewRect" => Predefined::ViewRect,
        "u_viewTexel" => Predefined::ViewTexel,
        "u_view" => Predefined::View,
        "u_invView" => Predefined::InvView,
        "u_proj" => Predefined::Proj,
        "u_invProj" => Predefined::InvProj,
        "u_viewProj" => Predefined::ViewProj,
        "u_invViewProj" => Predefined::InvViewProj,
        "u_model" => Predefined::Model,
        "u_modelView" => Predefined::ModelView,
        "u_modelViewProj" => Predefined::ModelViewProj,
        "u_alphaRef4" => Predefined::AlphaRef,
        _ => return None,
    })
}

struct StageUniforms {
    size: u32,
    /// (name, offset, float count) of user uniforms.
    user: Vec<(String, u32, u32)>,
    predefined: Vec<(Predefined, u32)>,
}

impl StageUniforms {
    fn new(blob: &ShaderBlob) -> Option<StageUniforms> {
        if blob.uniforms.is_empty() {
            return None;
        }
        let mut user = Vec::new();
        let mut pre = Vec::new();
        for u in &blob.uniforms {
            match predefined(&u.name) {
                Some(p) => pre.push((p, u.offset)),
                None => {
                    let floats = match u.kind {
                        UniformKind::Vec4 => 4,
                        UniformKind::Mat4 => 16,
                    } * u.count;
                    user.push((u.name.clone(), u.offset, floats));
                }
            }
        }
        Some(StageUniforms { size: blob.uniform_block_size, user, predefined: pre })
    }
}

pub(crate) struct Program {
    serial: u32,
    vs: Arc<Shader>,
    fs: Option<Arc<Shader>>,
    vs_uniforms: Option<StageUniforms>,
    fs_uniforms: Option<StageUniforms>,
    /// Texture slots either stage samples: (slot, is array).
    slots: Vec<(u32, bool)>,
    bgl0: Option<wgpu::BindGroupLayout>,
}

#[derive(Clone)]
pub(crate) struct Texture {
    serial: u32,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// Single-sample view of each array layer, for render attachments.
    layer_views: Vec<wgpu::TextureView>,
    /// The multisampled render target this texture resolves from, if it is an MSAA RT.
    msaa: Option<wgpu::TextureView>,
    /// Sample count draws into this texture use (1 unless it is an MSAA target).
    samples: u32,
    plan: FormatPlan,
    width: u32,
    height: u32,
    layers: u32,
    mips: u32,
    flags: u64,
    array: bool,
}

impl Texture {
    fn is_depth(&self) -> bool {
        self.plan.format.is_depth_stencil_format()
    }
    fn sample_count(&self) -> u32 {
        self.texture.sample_count()
    }
}

struct FrameBuffer {
    colors: Vec<Arc<Texture>>,
    depth: Option<Arc<Texture>>,
    width: u32,
    height: u32,
    /// Textures created for (or handed to) this frame buffer, destroyed with it.
    owned: Vec<u16>,
}

struct Buffer {
    buffer: wgpu::Buffer,
    size: u32,
    layout: u16,
    index32: bool,
    /// CPU copy of a dynamic buffer's contents (empty for static buffers).
    shadow: Vec<u8>,
}

fn shadow_copy(size: u32, data: Option<&[u8]>) -> Vec<u8> {
    let mut v = vec![0u8; (size.max(4) as usize).next_multiple_of(4)];
    if let Some(d) = data {
        v[..d.len()].copy_from_slice(d);
    }
    v
}

struct Uniform {
    name: String,
    refs: u32,
    value: Vec<f32>,
}

/// A fixed-size CPU arena that the engine writes transient data into; uploaded at `frame()`.
struct Arena {
    data: Box<[u8]>,
    used: usize,
    gpu: wgpu::Buffer,
}

impl Arena {
    fn new(device: &wgpu::Device, size: usize, usage: wgpu::BufferUsages, label: &str) -> Arena {
        let size = size.max(1024).next_multiple_of(4);
        Arena {
            data: vec![0u8; size].into_boxed_slice(),
            used: 0,
            gpu: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size as u64,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        }
    }
    /// Allocate `num * stride` bytes at an offset that is a multiple of `stride` and of 4.
    /// Returns the offset in units of `stride`, or `None` if there is not enough room (or the
    /// request overflows).
    fn alloc(&mut self, num: u32, stride: u32) -> Option<(u32, usize)> {
        let (units, start, end) = alloc_in(self.used, self.data.len(), num, stride)?;
        self.used = end;
        Some((units, start))
    }
    fn avail(&self, num: u32, stride: u32) -> u32 {
        let stride = stride.max(1) as usize;
        let start = self.used.next_multiple_of(lcm(stride, 4));
        let room = self.data.len().saturating_sub(start) / stride;
        room.min(num as usize) as u32
    }
}

/// The CPU side of [`Arena::alloc`], factored out so hostile sizes are unit-testable: returns
/// `(offset in units of stride, start byte, end byte)` or `None` when `num * stride` overflows or
/// does not fit.
fn alloc_in(used: usize, cap: usize, num: u32, stride: u32) -> Option<(u32, usize, usize)> {
    let stride = stride.max(1) as usize;
    let align = lcm(stride, 4);
    let start = used.next_multiple_of(align);
    let end = start.checked_add((num as usize).checked_mul(stride)?)?;
    if end > cap {
        return None;
    }
    Some(((start / stride) as u32, start, end))
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn lcm(a: usize, b: usize) -> usize {
    a / gcd(a, b) * b
}

// --- per-draw state ---------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) enum BufferSrc {
    Gpu(wgpu::Buffer),
    TransientVertex,
    TransientIndex,
}

#[derive(Clone)]
pub(crate) struct StreamBinding {
    src: BufferSrc,
    offset: u64,
    layout: u16,
    num_vertices: u32,
}

#[derive(Clone)]
pub(crate) struct IndexBinding {
    src: BufferSrc,
    offset: u64,
    format: wgpu::IndexFormat,
    first: u32,
    num: u32,
}

#[derive(Clone)]
pub(crate) struct InstanceBinding {
    src: BufferSrc,
    offset: u64,
    stride: u32,
    num: u32,
}

#[derive(Clone)]
pub(crate) struct TextureBinding {
    texture: Arc<Texture>,
    /// bgfx sampler flags, already resolved from `UINT32_MAX` to the texture's own.
    sampler: u32,
}

#[derive(Clone)]
struct DrawState {
    state: u64,
    rgba: u32,
    stencil: (u32, u32),
    transform: Option<(u32, u16)>,
    streams: [Option<StreamBinding>; MAX_STREAMS],
    index: Option<IndexBinding>,
    instance: Option<InstanceBinding>,
    textures: [Option<TextureBinding>; MAX_TEXTURE_SLOTS],
}

impl Default for DrawState {
    fn default() -> Self {
        DrawState {
            state: STATE_DEFAULT,
            rgba: 0,
            stencil: (0, 0),
            transform: None,
            streams: Default::default(),
            index: None,
            instance: None,
            textures: Default::default(),
        }
    }
}

/// A frame buffer's colour textures, depth texture, width and height.
pub(crate) type FrameBufferTargets = (Vec<Arc<Texture>>, Option<Arc<Texture>>, u32, u32);

pub(crate) struct Draw {
    seq: u32,
    view: u16,
    depth: u32,
    program: Arc<Program>,
    state: DrawState,
    /// Offsets of the vertex and fragment uniform blocks in the frame's uniform ring.
    ubo: [Option<u32>; 2],
}

pub(crate) struct Blit {
    view: u16,
    dst: Arc<Texture>,
    dst_mip: u32,
    dst_origin: wgpu::Origin3d,
    src: Arc<Texture>,
    src_mip: u32,
    src_origin: wgpu::Origin3d,
    size: (u32, u32, u32),
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Clear {
    flags: u16,
    rgba: u32,
    depth: f32,
    stencil: u8,
}

#[derive(Clone)]
pub(crate) struct View {
    rect: (u16, u16, u16, u16),
    clear: Clear,
    mode: u8,
    frame_buffer: u16,
    view: Mtx,
    proj: Mtx,
    touched: bool,
}

impl Default for View {
    fn default() -> Self {
        View {
            rect: (0, 0, 1, 1),
            clear: Clear { flags: 0, rgba: 0x000000ff, depth: 1.0, stencil: 0 },
            mode: 0,
            frame_buffer: INVALID,
            view: math::IDENTITY,
            proj: math::IDENTITY,
            touched: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub num_draw: u32,
    pub num_blit: u32,
    pub num_textures: u32,
    pub num_frame_buffers: u32,
    pub transient_vb_used: u32,
    pub transient_ib_used: u32,
    pub cpu_frame_ns: u64,
    pub errors: u32,
    /// Native BO2 draws refused last frame (`native_submit`'s validation).
    pub native_skipped: u32,
}

/// Bits for `Caps::formats`, as bgfx reports them.
pub const CAPS_FORMAT_TEXTURE_2D: u32 = 0x1;
pub const CAPS_FORMAT_TEXTURE_FRAMEBUFFER: u32 = 0x1000;
pub const CAPS_FORMAT_TEXTURE_MSAA: u32 = 0x4000;

// --- the renderer -----------------------------------------------------------------------------

/// The renderer's resource pools and the layout registry. `pool_insert` reports exhaustion;
/// every handle carries a generation (see `Pool`).
pub(crate) struct Resources {
    shaders: Pool<Arc<Shader>>,
    programs: Pool<Arc<Program>>,
    textures: Pool<Arc<Texture>>,
    frame_buffers: Pool<FrameBuffer>,
    vertex_buffers: Pool<Buffer>,
    index_buffers: Pool<Buffer>,
    dynamic_vertex_buffers: Pool<Buffer>,
    dynamic_index_buffers: Pool<Buffer>,
    uniforms: Pool<Uniform>,
    uniform_names: HashMap<String, u16>,
    layouts: Vec<VertexLayout>,
    layout_ids: HashMap<VertexLayout, u16>,
    /// Formats probed for use as render targets (see `renderable`).
    renderable: HashMap<wgpu::TextureFormat, bool>,
    /// Multisampled depth formats (and counts) that can also be sampled.
    msaa_depth_sampleable: HashMap<(wgpu::TextureFormat, u32), bool>,
}

impl Resources {
    fn new() -> Resources {
        Resources {
            shaders: Pool::default(),
            programs: Pool::default(),
            textures: Pool::default(),
            frame_buffers: Pool::default(),
            vertex_buffers: Pool::default(),
            index_buffers: Pool::default(),
            dynamic_vertex_buffers: Pool::default(),
            dynamic_index_buffers: Pool::default(),
            uniforms: Pool::default(),
            uniform_names: HashMap::new(),
            layouts: Vec::new(),
            layout_ids: HashMap::new(),
            renderable: HashMap::new(),
            msaa_depth_sampleable: HashMap::new(),
        }
    }
}

pub struct Renderer {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) adapter: wgpu::Adapter,
    pub(crate) surface: Option<(wgpu::Surface<'static>, wgpu::SurfaceConfiguration)>,
    pub(crate) trace: Arc<TraceFn>,
    pub(crate) screenshot_fn: Option<ScreenshotFn>,
    pub(crate) errors: Arc<std::sync::atomic::AtomicU32>,

    // backbuffer
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) reset_flags: u32,
    /// The offscreen target draws render into; rebuilt by `reset` (created there, never `None`).
    pub(crate) backbuffer: frame::Backbuffer,

    serial: u32,
    /// The resource pools and the layout registry (refactor plan 2.2: narrow responsibilities).
    res: Resources,

    transient_vb: Arena,
    transient_ib: Arena,

    // frame state
    pub(crate) views: Vec<View>,
    pub(crate) view_order: Vec<u16>,
    pub(crate) draws: Vec<Draw>,
    pub(crate) blits: Vec<Blit>,
    pub(crate) transforms: Vec<Mtx>,
    /// CPU copy of this frame's uniform blocks; predefined uniforms are patched in at `frame()`.
    pub(crate) uniform_ring: Vec<u8>,
    pub(crate) uniform_align: u32,
    pub(crate) screenshots: Vec<(u16, String)>,
    current: DrawState,
    pub(crate) frame_number: u32,
    pub(crate) stats: Stats,
    pub(crate) border_supported: bool,
    /// Whether the GPU samples BC (DXT) textures; without it they are decoded on upload.
    pub(crate) bc_supported: bool,
    /// Whether textures can be viewed in another format (sRGB views); not on GL.
    pub(crate) view_formats_supported: bool,
    pub(crate) cache: frame::Caches,
    /// The native BO2 model pass (`native.rs`).
    pub(crate) native: native::NativeState,
    /// BO2's shellshock screen layers (`shock.rs`).
    pub(crate) shock: shock::ShockState,
}

/// Run a wgpu future to completion. wgpu's native futures are ready immediately or after a
/// device poll, so a spin with a no-op waker is enough.
pub(crate) fn block_on<F: std::future::Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::yield_now();
    }
}

impl Renderer {
    pub fn new(desc: &InitDesc, trace: TraceFn) -> Result<Renderer, String> {
        let trace: Arc<TraceFn> = Arc::new(trace);
        let backends = match std::env::var("ZHBO_GFX_BACKEND").ok().as_deref() {
            Some("vulkan") => wgpu::Backends::VULKAN,
            Some("dx12") => wgpu::Backends::DX12,
            Some("metal") => wgpu::Backends::METAL,
            Some("gl") => wgpu::Backends::GL,
            _ => wgpu::Backends::all(),
        };
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            flags: wgpu::InstanceFlags::from_build_config().with_env(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });

        let surface = match desc.kind {
            WindowKind::Headless => None,
            _ => Some(unsafe { create_surface(&instance, desc)? }),
        };
        let force_fallback = std::env::var_os("ZHBO_GFX_FALLBACK").is_some();
        let adapter = pick_adapter(&instance, backends, surface.as_ref(), force_fallback)?;
        let info = adapter.get_info();
        trace(&format!("zhbo-gfx: adapter {} ({:?}, {:?})", info.name, info.backend, info.device_type));

        let wanted = wgpu::Features::TEXTURE_COMPRESSION_BC
            | wgpu::Features::FLOAT32_FILTERABLE
            | wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER
            | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
            | wgpu::Features::DEPTH_CLIP_CONTROL;
        let features = adapter.features() & wanted;
        if !features.contains(wgpu::Features::TEXTURE_COMPRESSION_BC) {
            trace(&format!("zhbo-gfx: {} has no BC (DXT) sampling; textures are decoded on upload", info.name));
        }
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("zhbo-gfx"),
            required_features: features,
            // The renderer needs no compute and only modest limits; ask for what the adapter has.
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .map_err(|e| format!("could not open {}: {e}", info.name))?;

        let errors = Arc::new(std::sync::atomic::AtomicU32::new(0));
        {
            let trace = trace.clone();
            let errors = errors.clone();
            device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
                // Log the first few in full; a broken pipeline would otherwise flood the log every frame.
                let n = errors.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if n < 32 {
                    let detail = match &e {
                        wgpu::Error::Validation { description, .. } | wgpu::Error::Internal { description, .. } => description.clone(),
                        other => error_chain(other),
                    };
                    trace(&format!("zhbo-gfx: wgpu error: {detail}"));
                }
            }));
        }

        let surface = surface.map(|s| {
            let caps = s.get_capabilities(&adapter);
            // The game writes display-ready values, so prefer a non-sRGB 8-bit surface.
            let format = caps
                .formats
                .iter()
                .copied()
                .find(|f| matches!(f, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm))
                .unwrap_or(caps.formats[0]);
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: desc.width.max(1),
                height: desc.height.max(1),
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
                alpha_mode: caps.alpha_modes[0],
                view_formats: vec![],
                color_space: wgpu::SurfaceColorSpace::Auto,
            };
            (s, config)
        });

        let uniform_align = device.limits().min_uniform_buffer_offset_alignment;
        let transient_vb = Arena::new(
            &device,
            if desc.transient_vb_size == 0 { 6 << 20 } else { desc.transient_vb_size as usize },
            wgpu::BufferUsages::VERTEX,
            "transient vb",
        );
        let transient_ib = Arena::new(
            &device,
            if desc.transient_ib_size == 0 { 2 << 20 } else { desc.transient_ib_size as usize },
            wgpu::BufferUsages::INDEX,
            "transient ib",
        );
        let cache = frame::Caches::new(&device);
        let native = native::NativeState::new(&device, &queue, uniform_align);
        let view_formats_supported = adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::VIEW_FORMATS);
        let backbuffer = frame::build_backbuffer(&device, &adapter, view_formats_supported, desc.width.max(1), desc.height.max(1), desc.reset);
        let mut r = Renderer {
            border_supported: features.contains(wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER),
            bc_supported: features.contains(wgpu::Features::TEXTURE_COMPRESSION_BC),
            view_formats_supported,
            device,
            queue,
            adapter,
            surface,
            trace,
            screenshot_fn: None,
            errors,
            width: desc.width.max(1),
            height: desc.height.max(1),
            reset_flags: desc.reset,
            backbuffer,
            serial: 0,
            res: Resources::new(),
            transient_vb,
            transient_ib,
            views: vec![View::default(); MAX_VIEWS],
            view_order: (0..MAX_VIEWS as u16).collect(),
            draws: Vec::new(),
            blits: Vec::new(),
            transforms: Vec::new(),
            uniform_ring: Vec::new(),
            uniform_align,
            screenshots: Vec::new(),
            current: DrawState::default(),
            frame_number: 0,
            stats: Stats::default(),
            cache,
            native,
            shock: Default::default(),
        };
        r.reset(desc.width, desc.height, desc.reset);
        Ok(r)
    }

    pub fn adapter_name(&self) -> String {
        let info = self.adapter.get_info();
        format!("{} ({:?})", info.name, info.backend)
    }

    pub fn set_screenshot_fn(&mut self, f: ScreenshotFn) {
        self.screenshot_fn = Some(f);
    }

    fn next_serial(&mut self) -> u32 {
        self.serial += 1;
        self.serial
    }

    /// (Re)build the offscreen backbuffer for the current size and reset flags.
    fn rebuild_backbuffer(&mut self) {
        self.backbuffer = frame::build_backbuffer(&self.device, &self.adapter, self.view_formats_supported, self.width, self.height, self.reset_flags);
    }

    pub(crate) fn log(&self, msg: &str) {
        (self.trace)(msg);
    }

    /// Resize the backbuffer and apply reset flags (MSAA, vsync, sRGB). Also (re)builds the
    /// backbuffer itself, so a backbuffer always exists once the renderer does.
    pub fn reset(&mut self, width: u32, height: u32, flags: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        self.reset_flags = flags;
        if let Some((surface, config)) = &mut self.surface {
            config.width = self.width;
            config.height = self.height;
            let caps = surface.get_capabilities(&self.adapter);
            config.present_mode = if flags & RESET_VSYNC != 0 {
                wgpu::PresentMode::Fifo
            } else if caps.present_modes.contains(&wgpu::PresentMode::Immediate) {
                wgpu::PresentMode::Immediate
            } else if caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
                wgpu::PresentMode::Mailbox
            } else {
                wgpu::PresentMode::Fifo
            };
            surface.configure(&self.device, config);
        }
        self.rebuild_backbuffer();
    }

    // --- capabilities ----------------------------------------------------------------------

    /// `Caps::formats` bits for a bgfx texture format.
    pub fn format_caps(&mut self, bgfx_format: u32) -> u32 {
        let Some(plan) = state::map_format(bgfx_format).map(|p| if self.bc_supported { p } else { state::without_bc(p) }) else { return 0 };
        let features = self.adapter.get_texture_format_features(plan.format);
        let mut caps = 0;
        if features.allowed_usages.contains(wgpu::TextureUsages::TEXTURE_BINDING) || plan.format.is_depth_stencil_format() {
            caps |= CAPS_FORMAT_TEXTURE_2D;
        }
        if plan.format.is_depth_stencil_format() || self.renderable(plan.format) {
            caps |= CAPS_FORMAT_TEXTURE_FRAMEBUFFER;
            if features.flags.sample_count_supported(4) {
                caps |= CAPS_FORMAT_TEXTURE_MSAA;
            }
        }
        caps
    }

    pub fn max_texture_size(&self) -> u32 {
        self.device.limits().max_texture_dimension_2d
    }

    pub fn stats(&self) -> Stats {
        let mut s = self.stats;
        s.num_textures = self.res.textures.live() as u32;
        s.num_frame_buffers = self.res.frame_buffers.live() as u32;
        s.errors = self.errors.load(std::sync::atomic::Ordering::Relaxed);
        s
    }

    pub(crate) fn frame_buffer_targets(&self, fb: u16) -> Option<FrameBufferTargets> {
        let f = self.res.frame_buffers.get(fb)?;
        Some((f.colors.clone(), f.depth.clone(), f.width, f.height))
    }
}

/// Bytes in one mip level of `w`x`h` texels in the source (bgfx) format.
fn level_bytes(plan: &FormatPlan, w: u32, h: u32) -> usize {
    match plan.source_block() {
        Some((dim, bytes)) => (w.div_ceil(dim) * h.div_ceil(dim) * bytes) as usize,
        None => (w * h * plan.source_bytes_per_texel()) as usize,
    }
}

/// Size of a mip level, treating an out-of-range mip as 1 (no shift-overflow panics on hostile
/// mip indices).
fn mip_extent(size: u32, mip: u8) -> u32 {
    size.checked_shr(mip as u32).unwrap_or(1).max(1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferKind {
    Static,
    Dynamic,
    Transient { index32: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    VertexBuffer,
    IndexBuffer,
    DynamicVertexBuffer,
    DynamicIndexBuffer,
    Shader,
    Program,
    Texture,
    FrameBuffer,
    Uniform,
}

/// Rank the adapters: a hardware GPU beats a software one, BC (DXT) sampling beats decoding the
/// game's textures on the CPU, and the native APIs beat GL. wgpu's own choice ignores BC, which
/// picks e.g. a VM's GL driver over its D3D12 one. `software` inverts the first rule (tests).
fn pick_adapter(
    instance: &wgpu::Instance,
    backends: wgpu::Backends,
    surface: Option<&wgpu::Surface<'_>>,
    software: bool,
) -> Result<wgpu::Adapter, String> {
    let score = |a: &wgpu::Adapter| {
        let info = a.get_info();
        let hardware = !matches!(info.device_type, wgpu::DeviceType::Cpu);
        let bc = a.features().contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
        let native = !matches!(info.backend, wgpu::Backend::Gl);
        let discrete = matches!(info.device_type, wgpu::DeviceType::DiscreteGpu);
        (hardware != software, bc, native, discrete)
    };
    block_on(instance.enumerate_adapters(backends))
        .into_iter()
        .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
        .max_by_key(score)
        .ok_or_else(|| "no graphics adapter that can draw to the window".to_string())
}

unsafe fn create_surface(instance: &wgpu::Instance, desc: &InitDesc) -> Result<wgpu::Surface<'static>, String> {
    use raw_window_handle as rwh;
    use std::num::NonZeroIsize;
    use std::ptr::NonNull;
    let target = match desc.kind {
        WindowKind::Headless => unreachable!(),
        WindowKind::Win32 => {
            let hwnd = NonZeroIsize::new(desc.window as isize).ok_or("null HWND")?;
            wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(rwh::RawDisplayHandle::Windows(rwh::WindowsDisplayHandle::new())),
                raw_window_handle: rwh::RawWindowHandle::Win32(rwh::Win32WindowHandle::new(hwnd)),
            }
        }
        #[cfg(target_vendor = "apple")]
        WindowKind::MetalLayer => wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(desc.window),
        #[cfg(not(target_vendor = "apple"))]
        WindowKind::MetalLayer => return Err("a Metal layer was passed on a non-Apple platform".into()),
        WindowKind::Xlib => {
            let display = NonNull::new(desc.display).ok_or("null X11 display")?;
            wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(rwh::RawDisplayHandle::Xlib(rwh::XlibDisplayHandle::new(Some(display), 0))),
                raw_window_handle: rwh::RawWindowHandle::Xlib(rwh::XlibWindowHandle::new(desc.window as usize as _)),
            }
        }
        WindowKind::Wayland => {
            let display = NonNull::new(desc.display).ok_or("null Wayland display")?;
            let surface = NonNull::new(desc.window).ok_or("null Wayland surface")?;
            wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(rwh::RawDisplayHandle::Wayland(rwh::WaylandDisplayHandle::new(display))),
                raw_window_handle: rwh::RawWindowHandle::Wayland(rwh::WaylandWindowHandle::new(surface)),
            }
        }
    };
    unsafe { instance.create_surface_unsafe(target) }.map_err(|e| format!("could not create a surface for the window: {e}"))
}

/// An error and its sources on one line (wgpu's top-level message is often just "Validation Error").
pub(crate) fn error_chain(e: &dyn std::error::Error) -> String {
    let mut out = e.to_string();
    let mut src = e.source();
    while let Some(s) = src {
        out.push_str(": ");
        out.push_str(&s.to_string());
        src = s.source();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_alloc_rejects_overflowing_and_oversized_requests() {
        // Hostile u32 inputs must not panic in debug or wrap in release (0.1b).
        assert_eq!(alloc_in(0, 1024, u32::MAX, u32::MAX), None);
        assert_eq!(alloc_in(0, 1024, u32::MAX, 1), None);
        assert_eq!(alloc_in(0, 1024, 1, u32::MAX), None);
        // And a fitting request still lands on an offset aligned to stride and 4.
        assert_eq!(alloc_in(3, 1024, 2, 8), Some((1, 8, 24)));
        // A zero-size request allocates nothing, even in an empty arena.
        assert_eq!(alloc_in(0, 0, 0, 0), Some((0, 0, 0)));
    }

    #[test]
    fn pool_handles_carry_a_generation() {
        let mut pool: Pool<u32> = Pool::default();
        let a = pool.insert(1);
        assert_ne!(a, INVALID);
        assert_eq!(pool.get(a), Some(&1));
        // A stale handle (destroy + recreate) must not see the slot's new occupant (0.1c).
        let removed = pool.remove(a);
        assert_eq!(removed, Some(1));
        assert_eq!(pool.get(a), None);
        let b = pool.insert(2);
        assert_ne!(b, a, "reused slot gets a new generation");
        assert_eq!(b & HANDLE_INDEX_MASK, a & HANDLE_INDEX_MASK, "same slot");
        assert_eq!(pool.get(b), Some(&2));
        assert_eq!(pool.get(a), None, "stale handle rejected");
        assert_eq!(pool.get_mut(a), None);
        // Removing through a stale handle must not free the live slot.
        assert_eq!(pool.remove(a), None);
        assert_eq!(pool.get(b), Some(&2));
        assert_eq!(pool.remove(b), Some(2));
        // Generation wraps after 16 reuses; INVALID is never handed out.
        let mut first = None;
        for _ in 0..64 {
            let h = pool.insert(0);
            assert_ne!(h, INVALID);
            first.get_or_insert(h);
            assert_eq!(pool.remove(h), Some(0));
        }
        assert_ne!(first.unwrap(), INVALID);
    }

    #[test]
    fn pool_reports_full_instead_of_aliasing_invalid() {
        let mut pool: Pool<u8> = Pool::default();
        // Fill to the capacity cap (HANDLE_INDEX_MASK slots, keeping 0xFFFF reserved).
        for _ in 0..HANDLE_INDEX_MASK {
            let h = pool.insert(0);
            assert_ne!(h, INVALID);
        }
        assert_eq!(pool.insert(0), INVALID);
        assert_eq!(pool.get(INVALID), None, "INVALID never decodes to a slot");
    }
}
