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

mod frame;
pub mod math;

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

struct Pool<T> {
    items: Vec<Option<T>>,
    free: Vec<u16>,
}

impl<T> Default for Pool<T> {
    fn default() -> Self {
        Pool { items: Vec::new(), free: Vec::new() }
    }
}

impl<T> Pool<T> {
    fn insert(&mut self, t: T) -> u16 {
        if let Some(h) = self.free.pop() {
            self.items[h as usize] = Some(t);
            return h;
        }
        if self.items.len() >= INVALID as usize {
            return INVALID;
        }
        self.items.push(Some(t));
        (self.items.len() - 1) as u16
    }
    fn get(&self, h: u16) -> Option<&T> {
        self.items.get(h as usize).and_then(Option::as_ref)
    }
    fn get_mut(&mut self, h: u16) -> Option<&mut T> {
        self.items.get_mut(h as usize).and_then(Option::as_mut)
    }
    fn remove(&mut self, h: u16) -> Option<T> {
        let t = self.items.get_mut(h as usize)?.take();
        if t.is_some() {
            self.free.push(h);
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
    pub attribs: [Option<(u16, wgpu::VertexFormat)>; 18],
}

impl VertexLayout {
    /// From the raw `VertexLayout` members: `m_stride`, `m_offset[]`, `m_attributes[]` (bgfx's
    /// encoding: `num - 1` in bits 0-1, type in bits 3-5, normalized in bit 7, asInt in bit 8).
    pub fn from_bgfx(stride: u16, offsets: &[u16; 18], encoded: &[u16; 18]) -> VertexLayout {
        let mut attribs = [None; 18];
        for i in 0..18 {
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
    /// Returns the offset in units of `stride`, or `None` if there is not enough room.
    fn alloc(&mut self, num: u32, stride: u32) -> Option<(u32, usize)> {
        let stride = stride.max(1) as usize;
        let align = lcm(stride, 4);
        let start = self.used.next_multiple_of(align);
        let end = start + num as usize * stride;
        if end > self.data.len() {
            return None;
        }
        self.used = end;
        Some(((start / stride) as u32, start))
    }
    fn avail(&self, num: u32, stride: u32) -> u32 {
        let stride = stride.max(1) as usize;
        let start = self.used.next_multiple_of(lcm(stride, 4));
        let room = self.data.len().saturating_sub(start) / stride;
        room.min(num as usize) as u32
    }
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
}

/// Bits for `Caps::formats`, as bgfx reports them.
pub const CAPS_FORMAT_TEXTURE_2D: u32 = 0x1;
pub const CAPS_FORMAT_TEXTURE_FRAMEBUFFER: u32 = 0x1000;
pub const CAPS_FORMAT_TEXTURE_MSAA: u32 = 0x4000;

// --- the renderer -----------------------------------------------------------------------------

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
    pub(crate) backbuffer: Option<frame::Backbuffer>,

    serial: u32,
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
    /// Formats probed for use as render targets (see `renderable`).
    renderable: HashMap<wgpu::TextureFormat, bool>,
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
        let mut r = Renderer {
            border_supported: features.contains(wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER),
            bc_supported: features.contains(wgpu::Features::TEXTURE_COMPRESSION_BC),
            view_formats_supported: adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::VIEW_FORMATS),
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
            backbuffer: None,
            serial: 0,
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
            renderable: HashMap::new(),
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

    pub(crate) fn log(&self, msg: &str) {
        (self.trace)(msg);
    }

    /// Resize the backbuffer and apply reset flags (MSAA, vsync, sRGB).
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
        self.backbuffer = None; // rebuilt at the next frame at the new size
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
        s.num_textures = self.textures.live() as u32;
        s.num_frame_buffers = self.frame_buffers.live() as u32;
        s.errors = self.errors.load(std::sync::atomic::Ordering::Relaxed);
        s
    }

    // --- shaders and programs --------------------------------------------------------------

    pub fn create_shader(&mut self, data: &[u8]) -> u16 {
        let blob = match ShaderBlob::decode(data) {
            Ok(b) => b,
            Err(e) => {
                self.log(&format!("zhbo-gfx: createShader: {e}"));
                return INVALID;
            }
        };
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(blob.wgsl.as_str().into()),
        });
        self.shaders.insert(Arc::new(Shader { blob, module }))
    }

    pub fn destroy_shader(&mut self, h: u16) {
        self.shaders.remove(h);
    }

    pub fn create_program(&mut self, vs: u16, fs: u16, destroy_shaders: bool) -> u16 {
        let Some(vsh) = self.shaders.get(vs).cloned() else { return INVALID };
        let fsh = self.shaders.get(fs).cloned();
        if vsh.blob.stage != Stage::Vertex {
            self.log("zhbo-gfx: createProgram: first shader is not a vertex shader");
            return INVALID;
        }
        let mut slots: Vec<(u32, bool)> = Vec::new();
        for s in vsh.blob.samplers.iter().chain(fsh.iter().flat_map(|f| f.blob.samplers.iter())) {
            if !slots.iter().any(|(slot, _)| *slot == s.slot) {
                slots.push((s.slot, s.array));
            }
        }
        slots.sort();
        let vs_uniforms = StageUniforms::new(&vsh.blob);
        let fs_uniforms = fsh.as_ref().and_then(|f| StageUniforms::new(&f.blob));
        let mut entries = Vec::new();
        for (binding, u, vis) in [
            (0, &vs_uniforms, wgpu::ShaderStages::VERTEX),
            (1, &fs_uniforms, wgpu::ShaderStages::FRAGMENT),
        ] {
            if let Some(u) = u {
                entries.push(wgpu::BindGroupLayoutEntry {
                    binding,
                    visibility: vis,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: std::num::NonZeroU64::new(u.size as u64),
                    },
                    count: None,
                });
            }
        }
        let bgl0 = (!entries.is_empty()).then(|| {
            self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("uniforms"), entries: &entries })
        });
        let serial = self.next_serial();
        let program = Program { serial, vs: vsh, fs: fsh, vs_uniforms, fs_uniforms, slots, bgl0 };
        if destroy_shaders {
            self.shaders.remove(vs);
            self.shaders.remove(fs);
        }
        self.programs.insert(Arc::new(program))
    }

    pub fn destroy_program(&mut self, h: u16) {
        self.programs.remove(h);
    }

    // --- uniforms --------------------------------------------------------------------------

    /// `bgfx::UniformType`: 0 Sampler, 2 Vec4, 3 Mat3, 4 Mat4.
    pub fn create_uniform(&mut self, name: &str, ty: u8, num: u16) -> u16 {
        if let Some(&h) = self.uniform_names.get(name) {
            if let Some(u) = self.uniforms.get_mut(h) {
                u.refs += 1;
                return h;
            }
        }
        let floats = match ty {
            3 => 12,
            4 => 16,
            _ => 4,
        } * num.max(1) as usize;
        let h = self.uniforms.insert(Uniform { name: name.to_string(), refs: 1, value: vec![0.0; floats] });
        self.uniform_names.insert(name.to_string(), h);
        h
    }

    pub fn destroy_uniform(&mut self, h: u16) {
        let Some(u) = self.uniforms.get_mut(h) else { return };
        u.refs -= 1;
        if u.refs == 0 {
            let u = self.uniforms.remove(h).unwrap();
            self.uniform_names.remove(&u.name);
        }
    }

    /// Set `num` elements (vec4 or mat4, as created) of a uniform. Values are sticky.
    pub fn set_uniform(&mut self, h: u16, value: &[f32]) {
        if let Some(u) = self.uniforms.get_mut(h) {
            let n = value.len().min(u.value.len());
            u.value[..n].copy_from_slice(&value[..n]);
        }
    }

    // --- vertex layouts and buffers --------------------------------------------------------

    pub fn register_layout(&mut self, layout: VertexLayout) -> u16 {
        if let Some(&id) = self.layout_ids.get(&layout) {
            return id;
        }
        let id = self.layouts.len() as u16;
        self.layouts.push(layout.clone());
        self.layout_ids.insert(layout, id);
        id
    }

    pub(crate) fn layout(&self, id: u16) -> Option<&VertexLayout> {
        self.layouts.get(id as usize)
    }

    fn make_buffer(&self, size: u32, data: Option<&[u8]>, usage: wgpu::BufferUsages) -> wgpu::Buffer {
        let size = (size.max(4) as u64).next_multiple_of(4);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if let Some(d) = data {
            self.write_padded(&buffer, 0, d);
        }
        buffer
    }

    /// `queue.write_buffer` needs 4-byte sizes; pad the tail.
    fn write_padded(&self, buffer: &wgpu::Buffer, offset: u64, data: &[u8]) {
        let aligned = data.len() & !3;
        if aligned > 0 {
            self.queue.write_buffer(buffer, offset, &data[..aligned]);
        }
        if aligned < data.len() && offset + aligned as u64 + 4 <= buffer.size() {
            let mut tail = [0u8; 4];
            tail[..data.len() - aligned].copy_from_slice(&data[aligned..]);
            self.queue.write_buffer(buffer, offset + aligned as u64, &tail);
        }
    }

    pub fn create_vertex_buffer(&mut self, data: &[u8], layout: VertexLayout) -> u16 {
        let layout = self.register_layout(layout);
        let buffer = self.make_buffer(data.len() as u32, Some(data), wgpu::BufferUsages::VERTEX);
        self.vertex_buffers.insert(Buffer { buffer, size: data.len() as u32, layout, index32: false, shadow: Vec::new() })
    }

    pub fn create_index_buffer(&mut self, data: &[u8], flags: u16) -> u16 {
        let buffer = self.make_buffer(data.len() as u32, Some(data), wgpu::BufferUsages::INDEX);
        self.index_buffers.insert(Buffer { buffer, size: data.len() as u32, layout: INVALID, index32: flags & BUFFER_INDEX32 != 0, shadow: Vec::new() })
    }

    pub fn create_dynamic_vertex_buffer(&mut self, num: u32, data: Option<&[u8]>, layout: VertexLayout) -> u16 {
        let size = data.map(|d| d.len() as u32).unwrap_or(num * layout.stride as u32);
        let layout = self.register_layout(layout);
        let buffer = self.make_buffer(size, data, wgpu::BufferUsages::VERTEX);
        let shadow = shadow_copy(size, data);
        self.dynamic_vertex_buffers.insert(Buffer { buffer, size, layout, index32: false, shadow })
    }

    pub fn create_dynamic_index_buffer(&mut self, num: u32, data: Option<&[u8]>, flags: u16) -> u16 {
        let index32 = flags & BUFFER_INDEX32 != 0;
        let size = data.map(|d| d.len() as u32).unwrap_or(num * if index32 { 4 } else { 2 });
        let buffer = self.make_buffer(size, data, wgpu::BufferUsages::INDEX);
        let shadow = shadow_copy(size, data);
        self.dynamic_index_buffers.insert(Buffer { buffer, size, layout: INVALID, index32, shadow })
    }

    pub fn update_dynamic_vertex_buffer(&mut self, h: u16, start_vertex: u32, data: &[u8]) {
        let Some(b) = self.dynamic_vertex_buffers.get(h) else { return };
        let stride = self.layout(b.layout).map(|l| l.stride as usize).unwrap_or(1);
        self.update_dynamic(false, h, start_vertex as usize * stride, data);
    }

    pub fn update_dynamic_index_buffer(&mut self, h: u16, start_index: u32, data: &[u8]) {
        let Some(b) = self.dynamic_index_buffers.get(h) else { return };
        let isize = if b.index32 { 4 } else { 2 };
        self.update_dynamic(true, h, start_index as usize * isize, data);
    }

    /// Write `data` at byte `offset` of a dynamic buffer. bgfx allows any element offset, but
    /// `queue.write_buffer` needs 4-byte alignment, so the CPU shadow copy is updated and the
    /// enclosing aligned range uploaded. A write past the end grows the buffer (bgfx's
    /// ALLOW_RESIZE), keeping its contents.
    fn update_dynamic(&mut self, index: bool, h: u16, offset: usize, data: &[u8]) {
        let usage = if index { wgpu::BufferUsages::INDEX } else { wgpu::BufferUsages::VERTEX };
        let end = offset + data.len();
        let pool = if index { &mut self.dynamic_index_buffers } else { &mut self.dynamic_vertex_buffers };
        let Some(b) = pool.get_mut(h) else { return };
        let grow = end > b.shadow.len();
        if grow {
            b.shadow.resize(end.next_multiple_of(4), 0);
        }
        b.shadow[offset..end].copy_from_slice(data);
        let (start, stop) = (offset & !3, end.next_multiple_of(4).min(b.shadow.len()));
        if grow {
            b.size = end as u32;
            b.buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: b.shadow.len() as u64,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&b.buffer, 0, &b.shadow);
        } else {
            self.queue.write_buffer(&b.buffer, start as u64, &b.shadow[start..stop]);
        }
    }

    /// Kinds for `destroy`.
    pub fn destroy(&mut self, kind: ResourceKind, h: u16) {
        match kind {
            ResourceKind::VertexBuffer => drop(self.vertex_buffers.remove(h)),
            ResourceKind::IndexBuffer => drop(self.index_buffers.remove(h)),
            ResourceKind::DynamicVertexBuffer => drop(self.dynamic_vertex_buffers.remove(h)),
            ResourceKind::DynamicIndexBuffer => drop(self.dynamic_index_buffers.remove(h)),
            ResourceKind::Shader => self.destroy_shader(h),
            ResourceKind::Program => self.destroy_program(h),
            ResourceKind::Texture => drop(self.textures.remove(h)),
            ResourceKind::FrameBuffer => {
                if let Some(fb) = self.frame_buffers.remove(h) {
                    for t in fb.owned {
                        self.textures.remove(t);
                    }
                }
            }
            ResourceKind::Uniform => self.destroy_uniform(h),
        }
    }

    // --- transient buffers -----------------------------------------------------------------

    pub fn transient_avail(&self, index: bool, num: u32, stride: u32) -> u32 {
        if index { self.transient_ib.avail(num, stride) } else { self.transient_vb.avail(num, stride) }
    }

    /// Allocate transient memory; returns (pointer, start in units of `stride`).
    pub fn transient_alloc(&mut self, index: bool, num: u32, stride: u32) -> Option<(*mut u8, u32)> {
        let arena = if index { &mut self.transient_ib } else { &mut self.transient_vb };
        let (start, byte) = arena.alloc(num, stride)?;
        Some((unsafe { arena.data.as_mut_ptr().add(byte) }, start))
    }

    // --- textures --------------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub fn create_texture_2d(&mut self, width: u32, height: u32, has_mips: bool, layers: u32, bgfx_format: u32, flags: u64, data: Option<&[u8]>) -> u16 {
        let Some(plan) = state::map_format(bgfx_format).map(|p| if self.bc_supported { p } else { state::without_bc(p) }) else {
            self.log(&format!("zhbo-gfx: unsupported texture format {bgfx_format}"));
            return INVALID;
        };
        let (bw, bh) = plan.format.block_dimensions();
        // Block-compressed textures must be whole blocks; D3D allowed e.g. 2x2 DXT.
        let width = width.max(1).next_multiple_of(bw);
        let height = height.max(1).next_multiple_of(bh);
        let mips = if has_mips { 32 - width.max(height).leading_zeros() } else { 1 };
        let layers = layers.max(1);
        let rt = flags & (TEXTURE_RT | TEXTURE_RT_WRITE_ONLY | (7 << TEXTURE_RT_MSAA_SHIFT)) != 0;
        let mut plan = plan;
        if rt && !plan.format.is_depth_stencil_format() && !self.renderable(plan.format) {
            // e.g. R32F (scene depth, shadow maps) is not renderable on GL downlevel devices. The
            // shaders read `.r`, so any renderable float format with a red channel will do.
            let substitutes = [
                wgpu::TextureFormat::R32Float,
                wgpu::TextureFormat::Rgba32Float,
                wgpu::TextureFormat::R16Float,
                wgpu::TextureFormat::Rgba16Float,
                wgpu::TextureFormat::Rgba8Unorm,
            ];
            match substitutes.into_iter().find(|&f| self.renderable(f)) {
                Some(f) => {
                    self.log(&format!("zhbo-gfx: {:?} render targets are not supported here; using {f:?}", plan.format));
                    plan.format = f;
                }
                None => {
                    self.log(&format!("zhbo-gfx: no renderable substitute for {:?}", plan.format));
                    return INVALID;
                }
            }
        }
        let mut usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC;
        if rt {
            usage |= wgpu::TextureUsages::RENDER_ATTACHMENT;
        }
        let msaa_bits = ((flags >> TEXTURE_RT_MSAA_SHIFT) & 7) as u32;
        let wanted_samples = if rt && msaa_bits >= 2 { 1 << (msaa_bits - 1) } else { 1 };
        let samples = self.supported_samples(plan.format, wanted_samples);
        // sRGB sampling is a view of the texture where the API can reinterpret formats; where it
        // cannot (GL), an sRGB texture is created as sRGB outright.
        let srgb = plan.format.add_srgb_suffix();
        let wants_srgb = flags & TEXTURE_SRGB != 0;
        let (base_format, view_formats, view_format) = if self.view_formats_supported {
            (plan.format, if srgb != plan.format { vec![srgb] } else { vec![] }, wants_srgb.then_some(srgb))
        } else {
            (if wants_srgb { srgb } else { plan.format }, vec![], None)
        };
        // Depth targets cannot be resolved, so an MSAA depth target is itself multisampled.
        let depth_msaa = plan.format.is_depth_stencil_format() && samples > 1;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width, height, depth_or_array_layers: layers },
            mip_level_count: if depth_msaa { 1 } else { mips },
            sample_count: if depth_msaa { samples } else { 1 },
            dimension: wgpu::TextureDimension::D2,
            format: base_format,
            usage: if depth_msaa { wgpu::TextureUsages::RENDER_ATTACHMENT } else { usage },
            view_formats: &view_formats,
        });
        let msaa = (samples > 1 && !depth_msaa).then(|| {
            self.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("msaa rt"),
                    size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    // Must match the view it resolves into.
                    format: view_format.unwrap_or(base_format),
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        });
        let array = layers > 1;
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: view_format,
            dimension: Some(if array { wgpu::TextureViewDimension::D2Array } else { wgpu::TextureViewDimension::D2 }),
            aspect: if plan.format.is_depth_stencil_format() { wgpu::TextureAspect::DepthOnly } else { wgpu::TextureAspect::All },
            ..Default::default()
        });
        let layer_views = (0..layers)
            .map(|l| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    format: view_format,
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_mip_level: 0,
                    mip_level_count: Some(1),
                    base_array_layer: l,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let serial = self.next_serial();
        let tex = Texture { serial, texture, view, layer_views, msaa, samples, plan, width, height, layers, mips, flags, array };
        if let Some(data) = data {
            if !depth_msaa {
                self.upload_all(&tex, data);
            }
        }
        self.textures.insert(Arc::new(tex))
    }

    /// Whether `format` can be a render target on this device. Format tables and downlevel
    /// rules differ per backend (R32F is not renderable on GL ES-class devices), so ask the device:
    /// create a tiny target inside a validation error scope, once per format.
    fn renderable(&mut self, format: wgpu::TextureFormat) -> bool {
        if let Some(&ok) = self.renderable.get(&format) {
            return ok;
        }
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let _probe = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("format probe"),
            size: wgpu::Extent3d { width: 4, height: 4, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let ok = block_on(scope.pop()).is_none();
        self.renderable.insert(format, ok);
        ok
    }

    fn supported_samples(&self, format: wgpu::TextureFormat, wanted: u32) -> u32 {
        let flags = self.adapter.get_texture_format_features(format).flags;
        let mut n = wanted;
        while n > 1 && !flags.sample_count_supported(n) {
            n /= 2;
        }
        n.max(1)
    }

    /// Upload every layer and mip from bgfx's layout (layer-major, then mips).
    fn upload_all(&self, tex: &Texture, data: &[u8]) {
        let mut offset = 0usize;
        for layer in 0..tex.layers {
            for mip in 0..tex.mips {
                let w = (tex.width >> mip).max(1);
                let h = (tex.height >> mip).max(1);
                let size = level_bytes(&tex.plan, w, h);
                if offset + size > data.len() {
                    return;
                }
                self.write_region(tex, layer, mip, 0, 0, w, h, &data[offset..offset + size], None);
                offset += size;
            }
        }
    }

    /// Upload a region in the bgfx format; `pitch` is the source row pitch in bytes (tight if None).
    #[allow(clippy::too_many_arguments)]
    fn write_region(&self, tex: &Texture, layer: u32, mip: u32, x: u32, y: u32, w: u32, h: u32, data: &[u8], pitch: Option<u32>) {
        let mip_w = (tex.width >> mip).max(1);
        let mip_h = (tex.height >> mip).max(1);
        let w = w.min(mip_w.saturating_sub(x));
        let h = h.min(mip_h.saturating_sub(y));
        if w == 0 || h == 0 {
            return;
        }
        let conversion = tex.plan.conversion;
        let converted;
        // (texel bytes, bytes per row, rows, copy extent)
        let (bytes, bytes_per_row, rows, extent): (&[u8], u32, u32, (u32, u32)) = match tex.plan.source_block() {
            Some((dim, block_bytes)) => {
                let (bw, bh) = (w.div_ceil(dim), h.div_ceil(dim));
                let tight = bw * block_bytes;
                let pitch = pitch.unwrap_or(tight);
                if (data.len() as u64) < pitch as u64 * (bh as u64 - 1) + tight as u64 {
                    self.log(&format!("zhbo-gfx: texture upload short: {} bytes for {w}x{h}", data.len()));
                    return;
                }
                if conversion == state::Conversion::None {
                    // Compressed copies cover whole blocks, up to the mip's physical (block-rounded) size.
                    let extent = (
                        w.next_multiple_of(dim).min(mip_w.next_multiple_of(dim)),
                        h.next_multiple_of(dim).min(mip_h.next_multiple_of(dim)),
                    );
                    (data, pitch, bh, extent)
                } else {
                    converted = state::decode_bc(conversion, data, pitch, bw, bh);
                    (&converted, bw * dim * 4, h, (w, h))
                }
            }
            None => {
                let bpp = tex.plan.source_bytes_per_texel();
                let tight = w * bpp;
                let pitch = pitch.unwrap_or(tight);
                if (data.len() as u64) < pitch as u64 * (h as u64 - 1) + tight as u64 {
                    self.log(&format!("zhbo-gfx: texture upload short: {} bytes for {w}x{h}", data.len()));
                    return;
                }
                if conversion == state::Conversion::None {
                    (data, pitch, h, (w, h))
                } else {
                    // Repack row by row (the source may have padding) and expand.
                    let mut packed = Vec::with_capacity((tight * h) as usize);
                    for r in 0..h as usize {
                        let start = r * pitch as usize;
                        packed.extend_from_slice(&data[start..start + tight as usize]);
                    }
                    converted = state::convert_texels(conversion, &packed).into_owned();
                    (&converted, w * 4, h, (w, h))
                }
            }
        };
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex.texture,
                mip_level: mip,
                origin: wgpu::Origin3d { x, y, z: layer },
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bytes_per_row), rows_per_image: Some(rows) },
            wgpu::Extent3d { width: extent.0, height: extent.1, depth_or_array_layers: 1 },
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_texture_2d(&mut self, h: u16, layer: u32, mip: u32, x: u32, y: u32, w: u32, height: u32, data: &[u8], pitch: Option<u32>) {
        let Some(tex) = self.textures.get(h).cloned() else { return };
        if mip >= tex.mips || layer >= tex.layers {
            return;
        }
        self.write_region(&tex, layer, mip, x, y, w, height, data, pitch);
    }

    // --- frame buffers ---------------------------------------------------------------------

    pub fn create_frame_buffer(&mut self, textures: &[u16], destroy_textures: bool) -> u16 {
        let mut colors = Vec::new();
        let mut depth = None;
        for &t in textures {
            let Some(tex) = self.textures.get(t).cloned() else { return INVALID };
            if tex.is_depth() {
                depth = Some(tex);
            } else {
                colors.push(tex);
            }
        }
        let Some(first) = colors.first().or(depth.as_ref()) else { return INVALID };
        let (width, height) = (first.width, first.height);
        let owned = if destroy_textures { textures.to_vec() } else { Vec::new() };
        self.frame_buffers.insert(FrameBuffer { colors, depth, width, height, owned })
    }

    pub fn create_frame_buffer_sized(&mut self, width: u32, height: u32, bgfx_format: u32, flags: u64) -> u16 {
        let t = self.create_texture_2d(width, height, false, 1, bgfx_format, flags | TEXTURE_RT, None);
        if t == INVALID {
            return INVALID;
        }
        self.create_frame_buffer(&[t], true)
    }

    // --- views -----------------------------------------------------------------------------

    pub fn set_view_rect(&mut self, v: u16, x: u16, y: u16, w: u16, h: u16) {
        if let Some(view) = self.views.get_mut(v as usize) {
            view.rect = (x, y, w, h);
        }
    }

    pub fn set_view_clear(&mut self, v: u16, flags: u16, rgba: u32, depth: f32, stencil: u8) {
        if let Some(view) = self.views.get_mut(v as usize) {
            view.clear = Clear { flags, rgba, depth, stencil };
        }
    }

    pub fn set_view_mode(&mut self, v: u16, mode: u8) {
        if let Some(view) = self.views.get_mut(v as usize) {
            view.mode = mode;
        }
    }

    pub fn set_view_frame_buffer(&mut self, v: u16, fb: u16) {
        if let Some(view) = self.views.get_mut(v as usize) {
            view.frame_buffer = fb;
        }
    }

    pub fn set_view_transform(&mut self, v: u16, view_mtx: Option<&Mtx>, proj: Option<&Mtx>) {
        if let Some(view) = self.views.get_mut(v as usize) {
            view.view = view_mtx.copied().unwrap_or(math::IDENTITY);
            view.proj = proj.copied().unwrap_or(math::IDENTITY);
        }
    }

    /// `setViewOrder(first, num, order)`: positions `first..first+num` execute `order[i]`.
    pub fn set_view_order(&mut self, first: u16, order: Option<&[u16]>, num: u16) {
        let end = (first as usize + num as usize).min(MAX_VIEWS);
        for (i, pos) in (first as usize..end).enumerate() {
            self.view_order[pos] = match order {
                Some(o) => o.get(i).copied().unwrap_or(pos as u16),
                None => pos as u16,
            };
        }
    }

    pub fn touch(&mut self, v: u16) {
        if let Some(view) = self.views.get_mut(v as usize) {
            view.touched = true;
        }
    }

    pub fn request_screenshot(&mut self, fb: u16, path: &str) {
        self.screenshots.push((fb, path.to_string()));
    }

    // --- draw state ------------------------------------------------------------------------

    pub fn set_state(&mut self, state: u64, rgba: u32) {
        self.current.state = state;
        self.current.rgba = rgba;
    }

    pub fn set_stencil(&mut self, front: u32, back: u32) {
        self.current.stencil = (front, back);
    }

    /// Store matrices in this frame's transform cache; returns the first index.
    pub fn alloc_transform(&mut self, mtx: &[Mtx]) -> u32 {
        let start = self.transforms.len() as u32;
        self.transforms.extend_from_slice(mtx);
        start
    }

    pub fn set_transform_cached(&mut self, cache: u32, num: u16) {
        self.current.transform = Some((cache, num));
    }

    pub fn set_index_buffer(&mut self, kind: BufferKind, h: u16, first: u32, num: u32) {
        let (src, index32, size) = match kind {
            BufferKind::Static | BufferKind::Dynamic => {
                let pool = if kind == BufferKind::Static { &self.index_buffers } else { &self.dynamic_index_buffers };
                let Some(b) = pool.get(h) else {
                    self.current.index = None;
                    return;
                };
                (BufferSrc::Gpu(b.buffer.clone()), b.index32, b.size)
            }
            BufferKind::Transient { index32 } => (BufferSrc::TransientIndex, index32, u32::MAX),
        };
        let isize = if index32 { 4 } else { 2 };
        let num = if num == u32::MAX { (size / isize).saturating_sub(first) } else { num };
        // Transient: `first` is in indices from the arena start; bind at that byte offset.
        let (offset, first) = match kind {
            BufferKind::Transient { .. } => (first as u64 * isize as u64, 0),
            _ => (0, first),
        };
        self.current.index = Some(IndexBinding {
            src,
            offset,
            format: if index32 { wgpu::IndexFormat::Uint32 } else { wgpu::IndexFormat::Uint16 },
            first,
            num,
        });
    }

    /// `start` is in vertices; `layout_override` is a layout id (INVALID: the buffer's own).
    pub fn set_vertex_buffer(&mut self, stream: u8, kind: BufferKind, h: u16, start: u32, num: u32, layout_override: u16) {
        let Some(slot) = self.current.streams.get_mut(stream as usize) else { return };
        let (src, layout, size) = match kind {
            BufferKind::Static | BufferKind::Dynamic => {
                let pool = if kind == BufferKind::Static { &self.vertex_buffers } else { &self.dynamic_vertex_buffers };
                let Some(b) = pool.get(h) else {
                    *slot = None;
                    return;
                };
                (BufferSrc::Gpu(b.buffer.clone()), b.layout, b.size)
            }
            BufferKind::Transient { .. } => (BufferSrc::TransientVertex, h, u32::MAX),
        };
        let layout = if layout_override != INVALID { layout_override } else { layout };
        let stride = self.layouts.get(layout as usize).map(|l| l.stride as u32).unwrap_or(0).max(1);
        let num = if num == u32::MAX { (size / stride).saturating_sub(start) } else { num };
        *slot = Some(StreamBinding { src, offset: start as u64 * stride as u64, layout, num_vertices: num });
    }

    pub fn set_instance_data(&mut self, kind: BufferKind, h: u16, start: u32, num: u32, stride: u32) {
        let src = match kind {
            BufferKind::Static => self.vertex_buffers.get(h).map(|b| BufferSrc::Gpu(b.buffer.clone())),
            BufferKind::Dynamic => self.dynamic_vertex_buffers.get(h).map(|b| BufferSrc::Gpu(b.buffer.clone())),
            BufferKind::Transient { .. } => Some(BufferSrc::TransientVertex),
        };
        let stride = if stride == 0 {
            match kind {
                BufferKind::Static => self.vertex_buffers.get(h),
                BufferKind::Dynamic => self.dynamic_vertex_buffers.get(h),
                _ => None,
            }
            .and_then(|b| self.layout(b.layout))
            .map(|l| l.stride as u32)
            .unwrap_or(64)
        } else {
            stride
        };
        self.current.instance = src.map(|src| InstanceBinding { src, offset: start as u64 * stride as u64, stride, num });
    }

    /// `flags == u32::MAX` uses the texture's creation sampler flags.
    pub fn set_texture(&mut self, stage: u8, h: u16, flags: u32) {
        let Some(slot) = self.current.textures.get_mut(stage as usize) else { return };
        *slot = self.textures.get(h).map(|t| TextureBinding {
            texture: t.clone(),
            sampler: if flags == u32::MAX { t.flags as u32 } else { flags },
        });
    }

    pub fn discard(&mut self, flags: u8) {
        let c = &mut self.current;
        if flags & DISCARD_BINDINGS != 0 {
            c.textures = Default::default();
        }
        if flags & DISCARD_INDEX_BUFFER != 0 {
            c.index = None;
        }
        if flags & DISCARD_INSTANCE_DATA != 0 {
            c.instance = None;
        }
        if flags & DISCARD_STATE != 0 {
            c.state = STATE_DEFAULT;
            c.rgba = 0;
            c.stencil = (0, 0);
        }
        if flags & DISCARD_TRANSFORM != 0 {
            c.transform = None;
        }
        if flags & DISCARD_VERTEX_STREAMS != 0 {
            c.streams = Default::default();
        }
    }

    /// Record a draw with the current state into view `v`.
    pub fn submit(&mut self, v: u16, program: u16, depth: u32, discard: u8) {
        if let (Some(p), true) = (self.programs.get(program).cloned(), (v as usize) < MAX_VIEWS) {
            let ubo = [
                p.vs_uniforms.as_ref().map(|u| self.write_user_uniforms(u)),
                p.fs_uniforms.as_ref().map(|u| self.write_user_uniforms(u)),
            ];
            let seq = self.draws.len() as u32;
            self.draws.push(Draw { seq, view: v, depth, program: p, state: self.current.clone(), ubo });
            self.views[v as usize].touched = true;
        }
        self.discard(discard);
    }

    /// Append a uniform block with the current user uniform values; predefined ones are filled in
    /// at `frame()`, when the view transforms are final.
    fn write_user_uniforms(&mut self, u: &StageUniforms) -> u32 {
        let offset = self.uniform_ring.len().next_multiple_of(self.uniform_align as usize);
        self.uniform_ring.resize(offset + u.size as usize, 0);
        for (name, at, floats) in &u.user {
            let Some(uni) = self.uniform_names.get(name).and_then(|&h| self.uniforms.get(h)) else { continue };
            let n = (*floats as usize).min(uni.value.len());
            let dst = &mut self.uniform_ring[offset + *at as usize..offset + *at as usize + n * 4];
            for (chunk, v) in dst.chunks_exact_mut(4).zip(&uni.value[..n]) {
                chunk.copy_from_slice(&v.to_le_bytes());
            }
        }
        offset as u32
    }

    #[allow(clippy::too_many_arguments)]
    pub fn blit(&mut self, v: u16, dst: u16, dst_mip: u8, dx: u16, dy: u16, dz: u16, src: u16, src_mip: u8, sx: u16, sy: u16, sz: u16, w: u16, h: u16, d: u16) {
        let (Some(dst_t), Some(src_t)) = (self.textures.get(dst).cloned(), self.textures.get(src).cloned()) else { return };
        let sw = (src_t.width >> src_mip).max(1);
        let sh = (src_t.height >> src_mip).max(1);
        let width = (w as u32).min(sw.saturating_sub(sx as u32));
        let height = (h as u32).min(sh.saturating_sub(sy as u32));
        let depth = (d as u32).min(src_t.layers.saturating_sub(sz as u32)).max(1);
        self.blits.push(Blit {
            view: v,
            dst: dst_t,
            dst_mip: dst_mip as u32,
            dst_origin: wgpu::Origin3d { x: dx as u32, y: dy as u32, z: dz as u32 },
            src: src_t,
            src_mip: src_mip as u32,
            src_origin: wgpu::Origin3d { x: sx as u32, y: sy as u32, z: sz as u32 },
            size: (width, height, depth),
        });
    }

    pub(crate) fn frame_buffer_targets(&self, fb: u16) -> Option<(Vec<Arc<Texture>>, Option<Arc<Texture>>, u32, u32)> {
        let f = self.frame_buffers.get(fb)?;
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
