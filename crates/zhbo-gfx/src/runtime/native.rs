//! The native BO2 model pass (Track R): skinned BO2 models drawn by the renderer itself, not
//! through the engine's W3D meshes.
//!
//! The core uploads a model's mesh once ([`Renderer::native_create_mesh`]: 56-byte vertices with
//! four bone influences, `u16` indices, surfaces with their blend and textures) and, each frame,
//! queues a draw with the bone palette it posed ([`Renderer::native_submit`]). At `frame()` the
//! queued draws run in one render pass **just before view `before_view`** (the engine's scene
//! composite), into view `target_view`'s frame buffer (the engine's scene) with the target view's
//! rect, so the viewmodel sits in the scene before bloom and the tonemap see it. The pass has a
//! depth buffer of its own, cleared, so the viewmodel draws over the world (BO2 draws its
//! viewmodel in a separate depth range) and its parts sort among themselves. Opaque surfaces draw
//! first, then blended ones in BO2's sort-key order. Each surface culls the faces its material's
//! `cullFace` says to (R4); the pass keeps BO2's index order, whose front faces are clockwise in
//! the framebuffer (the engine's content winds the other way — the W3D conversion swaps for that,
//! `zhbo-assets` `character.rs`), so pipelines here use `FrontFace::Cw`. Shading is in display
//! space, like the scene it joins (see `native.wgsl`).
//!
//! Submits are validated (handles, view ids, palette size against the mesh's bones, finite
//! matrices) and refused with a one-time log line; nothing in the frame path panics.

use std::collections::HashMap;
use std::sync::Arc;

use super::{Pool, Renderer, INVALID, MAX_VIEWS};

/// Bones a palette may hold (one uniform array in the shader).
pub const NATIVE_MAX_BONES: usize = 256;
/// Lights a frame may carry.
pub const NATIVE_MAX_LIGHTS: usize = 3;

/// Characters the MMS screen can show in one frame ([`NativeOptics::blips`]).
pub const NATIVE_MAX_BLIPS: usize = 8;

/// The frame block: view_proj, eye, ambient, 3 + 3 light vectors, params, screen, then the optics
/// (`optics`, `forward`, `depth` and two vectors per blip).
const FRAME_BYTES: u64 = (64 + 16 * 2 + 16 * 6 + 16 + 16 + 16 * 3 + 32 * NATIVE_MAX_BLIPS) as u64;
/// Where the frame block's `screen` vector sits (after view_proj, eye, ambient, 3 + 3 light
/// vectors and params).
const SCREEN_OFFSET: usize = 64 + 16 * 2 + 16 * 6 + 16;
/// Where the frame block's `depth` vector sits (the scene projection's depth terms, filled at
/// upload): after `screen`, `optics` and `forward`.
const DEPTH_OFFSET: usize = SCREEN_OFFSET + 16 * 3;
/// The scene's depth as the MMS screen reads it (raw device depth, resolved from the target's
/// depth attachment by [`DEPTH_RESOLVE`]).
const SCENE_Z_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;
const PALETTE_BYTES: u64 = (NATIVE_MAX_BONES * 64) as u64;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Copies a depth attachment's sample 0 into the MMS screen's `R32Float` scene depth (a full-target
/// triangle; `TEXTURE` is the single- or multisampled depth type).
const DEPTH_RESOLVE: &str = "
@group(0) @binding(0) var depth: TEXTURE;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(textureLoad(depth, vec2<i32>(p.xy), 0), 0.0, 0.0, 1.0);
}
";

/// One vertex as the core uploads it (`#[repr(C)]`, 56 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NativeVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub bones: [u16; 4],
    pub weights: [f32; 4],
}

const VERTEX_BYTES: u64 = std::mem::size_of::<NativeVertex>() as u64;
const _: () = assert!(VERTEX_BYTES == 56);

/// How a surface draws over what is behind it (BO2's render state; `zhbo_native::model::Blend`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NativeBlend {
    #[default]
    Opaque,
    AlphaTest,
    Alpha,
    Premultiplied,
    Additive,
    AdditiveAlpha,
}

impl NativeBlend {
    fn blended(self) -> bool {
        !matches!(self, NativeBlend::Opaque | NativeBlend::AlphaTest)
    }
    fn state(self) -> Option<wgpu::BlendState> {
        use wgpu::{BlendComponent, BlendFactor as F, BlendOperation::Add, BlendState};
        let comp = |src, dst| BlendComponent { src_factor: src, dst_factor: dst, operation: Add };
        let both = |src, dst| Some(BlendState { color: comp(src, dst), alpha: comp(src, dst) });
        match self {
            NativeBlend::Opaque | NativeBlend::AlphaTest => None,
            NativeBlend::Alpha => both(F::SrcAlpha, F::OneMinusSrcAlpha),
            NativeBlend::Premultiplied => both(F::One, F::OneMinusSrcAlpha),
            NativeBlend::Additive => both(F::One, F::One),
            NativeBlend::AdditiveAlpha => both(F::SrcAlpha, F::One),
        }
    }
}

/// Which of a surface's faces draw (BO2's `cullFace`; `zhbo_native::model::Cull`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NativeCull {
    /// Both faces (BO2's double-sided materials: alpha-tested cutouts, glow planes).
    #[default]
    None,
    /// The back faces: BO2's ordinary lit surfaces.
    Back,
    Front,
}

/// BO2's lens reticle on a surface (`zhbo_native::model::Reticle`; the `sw4_3d_reticle_dynamic`
/// technique). The surface's `color` is the reticle image (R shape, G glow, B background), sampled
/// not at the mesh's UVs but at `0.5 + scale · (T·v, −B·v)`: the eye→surface direction in the
/// surface's tangent frame, so the reticle sits at infinity on the optic's axis.
/// Which of BO2's reticle techniques (`zhbo_native::model::ReticleKind`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NativeReticleKind {
    /// Projected to infinity, faded in by the aim.
    #[default]
    Dynamic,
    /// Projected, no aim fade, colours lerped to the alt pair by `NativeFrame::reticle_alt`.
    TwoColors,
    /// The image at the mesh UVs, screen-space noise, no aim fade.
    Static,
    /// The code's UI texture at the mesh UVs (`color`; `INVALID` binds black), two shapes and two
    /// glows, opaque output added; the target finder's diamonds on the frame's
    /// [`NativeOptics::blips`] are drawn into its alt channels.
    Ui3d,
}

impl NativeReticleKind {
    fn code(self) -> f32 {
        match self {
            NativeReticleKind::Dynamic => 0.0,
            NativeReticleKind::TwoColors => 1.0,
            NativeReticleKind::Static => 2.0,
            NativeReticleKind::Ui3d => 3.0,
        }
    }
}

/// BO2's millimetre-scanner screen on a surface (`zhbo_native::model::Sonar`; the
/// `sw4_3d_sonar_attachment_lens2` technique): drawn in screen space from the scene's depth and
/// the frame's [`NativeOptics`] blips, not from a colour map.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeSonar {
    /// The `Lookup` profile and `Static_Noise_Map` handles (`INVALID`: white).
    pub lookup: u16,
    pub noise: u16,
    /// The shader's constants in `zhbo_native::model::Sonar::constants` order: PulseInterval,
    /// PulseTime, MaxDepth, MaxPulseDepth; MinDepth, LineNoise, BackgroundNoise, hdrAmount;
    /// ScatterSize.xy; EdgeSize, EdgeIntensity, EdgeMinDist, EdgeMaxDist; Pulse/Player/Outline
    /// lookup scales, Min_Player_Intensity; EnemiesColor + Player_Color_Multiplier; PulseColor +
    /// multiplier; EdgeColor + multiplier. Distances are BO2 inches.
    pub constants: [f32; 30],
}

/// One character on the MMS screen: a capsule standing on `pos` (world units), seen through
/// walls at `intensity` (BO2's per-character `weaponParam0.x`, `t6mp` `FUN_004e7b40`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NativeBlip {
    pub pos: [f32; 3],
    pub radius: f32,
    pub height: f32,
    pub intensity: f32,
}

/// What the optic displays read besides the material: the MMS screen's clock, view and blips.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NativeOptics {
    /// Seconds (BO2's `gameTime`): the pulse's phase and the noise's per-frame jitter.
    pub time: f32,
    /// The view's forward (unit, world space): depths are along it, as the depth buffer's.
    pub forward: [f32; 3],
    /// BO2 inches per world unit: the screen's constants are inches.
    pub inches_per_unit: f32,
    pub blips: [NativeBlip; NATIVE_MAX_BLIPS],
    pub blip_count: u32,
}

/// The target finder's display for a ui3d reticle (`zhbo_native::display`): the surface's `color`
/// is the idle frame; these are the spotted frame, the sprite atlas and the diamond's rectangle in
/// it and its size (a fraction of the window, i.e. of the lens's UVs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeDisplay {
    pub spotted: u16,
    pub atlas: u16,
    pub diamond_uv: [f32; 4],
    pub diamond_size: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeReticle {
    /// The noise map (`Detail_Map`) handle, or `INVALID` for none (white).
    pub noise: u16,
    pub scale: f32,
    pub color: [f32; 3],
    pub glow: [f32; 3],
    pub glow_falloff: f32,
    pub emissive: f32,
    pub noise_scale: f32,
    pub noise_amount: f32,
    pub background: f32,
    pub kind: NativeReticleKind,
    /// `Reticle_Alt_Color` / `Glow_Alt_Color` (two-colour and ui3d).
    pub color_alt: [f32; 3],
    pub glow_alt: [f32; 3],
    /// The target finder's display (ui3d only; `None` draws the empty display).
    pub display: Option<NativeDisplay>,
}

impl NativeReticle {
    fn finite(&self) -> bool {
        self.display.is_none_or(|d| finite(&d.diamond_uv) && d.diamond_size.is_finite()) && finite(&[self.scale, self.glow_falloff, self.emissive, self.noise_scale, self.noise_amount, self.background]) && finite(&self.color) && finite(&self.glow) && finite(&self.color_alt) && finite(&self.glow_alt)
    }
}

/// One surface of a mesh: an index range and how it draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeSurface {
    pub first_index: u32,
    pub index_count: u32,
    pub blend: NativeBlend,
    pub emissive: bool,
    /// A [`Renderer::native_create_texture`] handle (the colour map), or `INVALID` for white.
    pub color: u16,
    /// The packed normal/gloss/specular map (linear), or `INVALID` for a flat matte surface.
    pub maps: u16,
    /// The chosen camo layer's texture handle, or `INVALID` for none.
    pub camo: u16,
    /// BO2's specular colour map (raw, like `maps`), or `INVALID`: the highlight and reflection
    /// then take `maps`' scalar specular level, uncoloured.
    pub specular: u16,
    /// The camo layer's UV transform: offset.xy, scale, gain. Ignored when `camo` is `INVALID`.
    pub camo_uv: [f32; 4],
    /// BO2's sort key: blended surfaces draw in its order.
    pub sort_key: u8,
    pub cull: NativeCull,
    /// The lens reticle technique, for a reticle surface (`None`: BO2's lit shading).
    pub reticle: Option<NativeReticle>,
    /// The MMS screen technique (takes precedence over `reticle`).
    pub sonar: Option<NativeSonar>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NativeLight {
    /// Towards the light, world space.
    pub direction: [f32; 3],
    /// Display-encoded, as Zero Hour's lights are.
    pub color: [f32; 3],
}

/// What a draw is lit and seen by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeFrame {
    /// World to clip, column-major (`clip = view_proj * world`), depth 0..1. `None`: the target
    /// view's own view and projection as the engine leaves them at `frame()` (BO2 draws its
    /// viewmodel with the world's FOV, `cg_fov`; there is no viewmodel FOV: G4).
    pub view_proj: Option<[f32; 16]>,
    pub eye: [f32; 3],
    pub ambient: [f32; 3],
    pub lights: [NativeLight; NATIVE_MAX_LIGHTS],
    pub light_count: u32,
    /// `ZHBO_BO2_DEBUG`'s views (0 off).
    pub debug: f32,
    /// BO2's `fWeaponPosFrac` (0 hip, 1 aimed): the lens reticle fades in by `saturate(2·ads)`
    /// (`weaponParam0.w`, `t6mp` `FUN_00529dd0`).
    pub ads: f32,
    /// 1 while the player's optics are knocked out (`weaponParam0.y`: BO2's EMP), which hides the
    /// reticle; 0 otherwise.
    pub optics_off: f32,
    /// `weaponParam0.z`: how far a two-colour reticle has turned to its alt colours (the target
    /// finder's "enemy spotted" colour); 0 the primary.
    pub reticle_alt: f32,
    /// The MMS screen's inputs.
    pub optics: NativeOptics,
}

/// One queued draw: a mesh, its palette (world-space skinning matrices, column-major), and where.
pub struct NativeDraw<'a> {
    pub target_view: u16,
    pub before_view: u16,
    pub mesh: u16,
    pub palette: &'a [[f32; 16]],
    pub frame: NativeFrame,
}

/// The shader's `Material` uniform: params, camo UV, specular flags, the reticle's five constant
/// blocks and the MMS screen's eight (sixteen `vec4<f32>`).
const MATERIAL_BYTES: u64 = 256;

/// Which fragment stage a surface draws with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum Stage {
    #[default]
    Lit,
    Reticle,
    Sonar,
}

impl Stage {
    fn entry(self) -> &'static str {
        match self {
            Stage::Lit => "fs",
            Stage::Reticle => "fs_reticle",
            Stage::Sonar => "fs_sonar",
        }
    }
}

struct NativeTexture {
    view: wgpu::TextureView,
}

struct Surface {
    first_index: u32,
    index_count: u32,
    blend: NativeBlend,
    sort_key: u8,
    cull: NativeCull,
    stage: Stage,
    bind_group: wgpu::BindGroup,
}

struct Mesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    surfaces: Vec<Surface>,
    /// The highest bone index any vertex uses: a palette must cover it.
    max_bone: u16,
}

struct Queued {
    target_view: u16,
    before_view: u16,
    mesh: Arc<Mesh>,
    frame_offset: u32,
    palette_offset: u32,
    /// Fill the frame block's matrix from the target view at upload.
    view_proj_from_view: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PipelineKey {
    blend: NativeBlend,
    cull: NativeCull,
    stage: Stage,
    color: wgpu::TextureFormat,
    samples: u32,
}

pub(crate) struct NativeState {
    textures: Pool<NativeTexture>,
    meshes: Pool<Arc<Mesh>>,
    queue: Vec<Queued>,
    /// Scratch reused by every `native_encode` (no per-frame allocation once warm): the draws
    /// encoded now, their target views, and one view's pipeline plan (pipeline, draw, surface).
    encoding: Vec<Queued>,
    targets: Vec<u16>,
    plans: Vec<(wgpu::RenderPipeline, usize, usize)>,
    /// This frame's uniforms: per draw, a frame block and a palette block, each aligned.
    ring: Vec<u8>,
    gpu_ring: Option<(wgpu::Buffer, wgpu::BindGroup, u64)>,
    pipelines: HashMap<PipelineKey, Option<wgpu::RenderPipeline>>,
    depth: Option<(wgpu::TextureView, u32, u32, u32)>,
    module: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    bgl0: wgpu::BindGroupLayout,
    bgl1: wgpu::BindGroupLayout,
    /// Group 2: the scene's depth for the MMS screen (`scene_z`).
    bgl2: wgpu::BindGroupLayout,
    /// The resolved scene depth (view, width, height), rebuilt on resize, and the 1x1 "sky" the
    /// pass binds without it.
    scene_z: Option<(wgpu::TextureView, u32, u32)>,
    sky_z: wgpu::BindGroup,
    /// The depth resolve: its pipelines (single-sample, multisampled; `None` failed) and layouts.
    resolve: [Option<Option<(wgpu::RenderPipeline, wgpu::BindGroupLayout)>>; 2],
    sampler: wgpu::Sampler,
    white: wgpu::TextureView,
    /// The ui3d reticle's absent code texture: nothing added.
    black: wgpu::TextureView,
    flat: wgpu::TextureView,
    align: u64,
    /// Draws refused this frame (validation), for the stats.
    pub(crate) skipped: u32,
}

fn one_pixel(device: &wgpu::Device, queue: &wgpu::Queue, rgba: [u8; 4], format: wgpu::TextureFormat) -> wgpu::TextureView {
    let t = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("native default"),
        size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(t.as_image_copy(), &rgba, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: None }, wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 });
    t.create_view(&Default::default())
}

impl NativeState {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue, align: u32) -> NativeState {
        let ubo = |binding, size, visibility| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: wgpu::BufferSize::new(size) },
            count: None,
        };
        let vf = wgpu::ShaderStages::VERTEX_FRAGMENT;
        let bgl0 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("native frame"), entries: &[ubo(0, FRAME_BYTES, vf), ubo(1, PALETTE_BYTES, wgpu::ShaderStages::VERTEX)] });
        let tex = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let bgl1 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("native material"),
            entries: &[
                tex(0),
                tex(1),
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: wgpu::BufferSize::new(MATERIAL_BYTES) },
                    count: None,
                },
                tex(4),
                tex(5),
            ],
        });
        let bgl2 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("native scene depth"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            }],
        });
        let sky = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("native sky depth"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SCENE_Z_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(sky.as_image_copy(), &1.0f32.to_le_bytes(), wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: None }, wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 });
        let sky_view = sky.create_view(&Default::default());
        let sky_z = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("native sky depth"), layout: &bgl2, entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&sky_view) }] });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("native"), bind_group_layouts: &[Some(&bgl0), Some(&bgl1), Some(&bgl2)], immediate_size: 0 });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("native"), source: wgpu::ShaderSource::Wgsl(include_str!("native.wgsl").into()) });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("native"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });
        NativeState {
            textures: Pool::default(),
            meshes: Pool::default(),
            queue: Vec::new(),
            encoding: Vec::new(),
            targets: Vec::new(),
            plans: Vec::new(),
            ring: Vec::new(),
            gpu_ring: None,
            pipelines: HashMap::new(),
            depth: None,
            module,
            layout,
            bgl0,
            bgl1,
            bgl2,
            scene_z: None,
            sky_z,
            resolve: [None, None],
            sampler,
            white: one_pixel(device, queue, [255; 4], wgpu::TextureFormat::Rgba8Unorm),
            black: one_pixel(device, queue, [0; 4], wgpu::TextureFormat::Rgba8Unorm),
            flat: one_pixel(device, queue, [128, 128, 0, 0], wgpu::TextureFormat::Rgba8Unorm),
            align: align.max(16) as u64,
            skipped: 0,
        }
    }
}

/// Box-filtered mip chain of an RGBA8 image (`data` is level 0).
fn mips(width: u32, height: u32, data: &[u8]) -> Vec<(u32, u32, Vec<u8>)> {
    let mut out = vec![(width, height, data.to_vec())];
    while let Some((w, h, d)) = out.last().filter(|(w, h, _)| *w > 1 || *h > 1) {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let px = |sx: u32, sy: u32| d[((sy.min(h - 1) * w + sx.min(w - 1)) * 4 + c) as usize] as u32;
                    let sum = px(2 * x, 2 * y) + px(2 * x + 1, 2 * y) + px(2 * x, 2 * y + 1) + px(2 * x + 1, 2 * y + 1);
                    next[((y * nw + x) * 4 + c) as usize] = ((sum + 2) / 4) as u8;
                }
            }
        }
        out.push((nw, nh, next));
    }
    out
}

fn finite(m: &[f32]) -> bool {
    m.iter().all(|v| v.is_finite())
}

/// The scene projection with its near plane pulled in for the native viewmodel. The viewmodel sits
/// at the eye, so the scene's near (`W3DView.cpp`: `NearZ = MAP_XY_FACTOR`, 10 world units) cuts the
/// rear of the gun and the optic base into jagged holes; BO2's viewmodel has its own near close to
/// the eye. The FOV/aspect and the far are kept, so the viewmodel still uses the scene's projection.
fn viewmodel_projection(proj: &[f32; 16]) -> [f32; 16] {
    /// The viewmodel's near, in world units (1 unit = 6 inches): the harness's `NEAR`, close enough
    /// that the arms at the eye do not clip into a hole.
    const NEAR: f32 = 0.1;
    let (m10, m14) = (proj[10], proj[14]);
    if !(m10 > 1.0 && m14 < 0.0) {
        return *proj;
    }
    let near = -m14 / m10;
    let far = near * m10 / (m10 - 1.0);
    if !near.is_finite() || !far.is_finite() || near <= NEAR || far <= NEAR {
        return *proj;
    }
    let mut out = *proj;
    out[10] = far / (far - NEAR);
    out[14] = -NEAR * far / (far - NEAR);
    out
}

impl Renderer {
    /// An RGBA8 texture for native surfaces, with a box-filtered mip chain. `srgb` samples it
    /// decoded to linear; the native shader works in display space, so its maps are uploaded
    /// without it. `INVALID` if the data is the wrong size.
    pub fn native_create_texture(&mut self, width: u32, height: u32, rgba: &[u8], srgb: bool) -> u16 {
        let max = self.device.limits().max_texture_dimension_2d;
        if width == 0 || height == 0 || width > max || height > max || rgba.len() != (width as usize) * (height as usize) * 4 {
            self.report(format!("zhbo-gfx: native texture {width}x{height} with {} bytes refused", rgba.len()));
            return INVALID;
        }
        let levels = mips(width, height, rgba);
        let format = if srgb { wgpu::TextureFormat::Rgba8UnormSrgb } else { wgpu::TextureFormat::Rgba8Unorm };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("native"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: levels.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (mip, (w, h, d)) in levels.iter().enumerate() {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: mip as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                d,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: None },
                wgpu::Extent3d { width: *w, height: *h, depth_or_array_layers: 1 },
            );
        }
        let h = self.native.textures.insert(NativeTexture { view: texture.create_view(&Default::default()) });
        if h == INVALID {
            self.report("zhbo-gfx: native texture pool full".into());
        }
        h
    }

    pub fn native_destroy_texture(&mut self, h: u16) {
        self.native.textures.remove(h);
    }

    /// Upload a mesh. A surface with no indices is a no-op draw: it is skipped and logged, not an
    /// error, but a mesh left with no drawable surface at all is refused. Refused (`INVALID`,
    /// logged) if a surface's count is not whole triangles, its range leaves the indices, an index
    /// leaves the vertices, a bone index is past [`NATIVE_MAX_BONES`], or a texture handle is
    /// stale. Surfaces keep their textures alive after the textures are destroyed.
    pub fn native_create_mesh(&mut self, vertices: &[NativeVertex], indices: &[u16], surfaces: &[NativeSurface]) -> u16 {
        let refuse = |r: &mut Renderer, why: String| {
            r.report(format!("zhbo-gfx: native mesh refused: {why}"));
            INVALID
        };
        if vertices.is_empty() || indices.is_empty() || !indices.len().is_multiple_of(3) {
            return refuse(self, format!("{} vertices, {} indices", vertices.len(), indices.len()));
        }
        if let Some(i) = indices.iter().find(|&&i| i as usize >= vertices.len()) {
            return refuse(self, format!("index {i} of {} vertices", vertices.len()));
        }
        let max_bone = vertices.iter().flat_map(|v| v.bones).max().unwrap_or(0);
        if max_bone as usize >= NATIVE_MAX_BONES {
            return refuse(self, format!("bone {max_bone} past {NATIVE_MAX_BONES}"));
        }
        if !vertices.iter().all(|v| finite(&v.position) && finite(&v.normal) && finite(&v.uv) && finite(&v.weights)) {
            return refuse(self, "non-finite vertex data".into());
        }
        let mut built = Vec::with_capacity(surfaces.len());
        for (k, s) in surfaces.iter().enumerate() {
            let end = s.first_index as u64 + s.index_count as u64;
            if s.index_count == 0 {
                // A surface with no triangles is a no-op draw, not corruption. The composer never
                // sends one (a wholly hidden surface drops at compose), but the ABI must not lose a
                // whole mesh over one. `report` dedups by message and this runs at mesh upload, not
                // per frame, so the line costs nothing.
                self.report(format!("zhbo-gfx: native mesh surface {k} of {} has no indices; skipped", surfaces.len()));
                continue;
            }
            if s.index_count % 3 != 0 || end > indices.len() as u64 {
                return refuse(self, format!("surface {k} indices {}..{end} of {}", s.first_index, indices.len()));
            }
            let view = |r: &Renderer, h: u16, fallback: &wgpu::TextureView| -> Option<wgpu::TextureView> {
                if h == INVALID {
                    Some(fallback.clone())
                } else {
                    r.native.textures.get(h).map(|t| t.view.clone())
                }
            };
            if s.sonar.is_some_and(|so| !finite(&so.constants)) {
                self.report(format!("zhbo-gfx: native mesh surface {k}: non-finite sonar constants; skipped"));
                continue;
            }
            if s.reticle.is_some_and(|r| !r.finite()) {
                // Log-or-skip: a reticle with a broken constant is left off, the mesh still draws.
                self.report(format!("zhbo-gfx: native mesh surface {k}: non-finite reticle constants; skipped"));
                continue;
            }
            // A reticle surface binds its noise map where the lit surfaces bind their packed maps.
            // The MMS screen binds its lookup profile as the colour and its noise as the maps.
            let (maps_handle, maps_fallback) = match (s.sonar, s.reticle) {
                (Some(so), _) => (so.noise, &self.native.white),
                (None, Some(r)) => (r.noise, &self.native.white),
                (None, None) => (s.maps, &self.native.flat),
            };
            let color_handle = s.sonar.map_or(s.color, |so| so.lookup);
            let display = s.reticle.and_then(|r| r.display);
            let color_fallback = if s.reticle.is_some_and(|r| r.kind == NativeReticleKind::Ui3d) { &self.native.black } else { &self.native.white };
            let (Some(color), Some(maps), Some(camo), Some(specular)) = (
                view(self, color_handle, color_fallback),
                view(self, maps_handle, maps_fallback),
                // A display binds its spotted frame and atlas where lit surfaces bind camo/spec.
                view(self, display.map_or(s.camo, |d| d.spotted), if display.is_some() { &self.native.black } else { &self.native.white }),
                view(self, display.map_or(s.specular, |d| d.atlas), if display.is_some() { &self.native.black } else { &self.native.white }),
            ) else {
                return refuse(self, format!("surface {k} names a destroyed texture"));
            };
            let params = [
                if s.blend == NativeBlend::AlphaTest { 1.0f32 } else { 0.0 },
                if s.emissive { 1.0 } else { 0.0 },
                if s.maps != INVALID { 1.0 } else { 0.0 },
                if s.camo != INVALID { 1.0 } else { 0.0 },
                s.camo_uv[0],
                s.camo_uv[1],
                s.camo_uv[2],
                s.camo_uv[3],
                // The specular colour only lights a surface that has the packed maps (its gloss).
                if s.specular != INVALID && s.maps != INVALID { 1.0 } else { 0.0 },
                0.0,
                0.0,
                0.0,
            ];
            let r = s.reticle.unwrap_or(NativeReticle { noise: INVALID, scale: 0.0, color: [0.0; 3], glow: [0.0; 3], glow_falloff: 1.0, emissive: 0.0, noise_scale: 1.0, noise_amount: 0.0, background: 0.0, kind: NativeReticleKind::Dynamic, color_alt: [0.0; 3], glow_alt: [0.0; 3], display: None });
            let reticle = [
                r.scale,
                r.glow_falloff,
                r.emissive,
                r.noise_scale,
                r.color[0],
                r.color[1],
                r.color[2],
                r.noise_amount,
                r.glow[0],
                r.glow[1],
                r.glow[2],
                r.background,
                r.color_alt[0],
                r.color_alt[1],
                r.color_alt[2],
                r.kind.code(),
                r.glow_alt[0],
                r.glow_alt[1],
                r.glow_alt[2],
                0.0,
            ];
            let mut sonar = [0.0f32; 32];
            if let Some(so) = s.sonar {
                sonar[..30].copy_from_slice(&so.constants);
            } else if let Some(d) = display {
                // The display's block: the diamond's atlas rectangle, its size, present.
                sonar[..6].copy_from_slice(&[d.diamond_uv[0], d.diamond_uv[1], d.diamond_uv[2], d.diamond_uv[3], d.diamond_size, 1.0]);
            }
            let params: Vec<f32> = params.iter().chain(&reticle).chain(&sonar).copied().collect();
            let material = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("native material"),
                size: MATERIAL_BYTES,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&material, 0, &params.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>());
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("native material"),
                layout: &self.native.bgl1,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&color) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&maps) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.native.sampler) },
                    wgpu::BindGroupEntry { binding: 3, resource: material.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&camo) },
                    wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&specular) },
                ],
            });
            let stage = if s.sonar.is_some() {
                Stage::Sonar
            } else if s.reticle.is_some() {
                Stage::Reticle
            } else {
                Stage::Lit
            };
            built.push(Surface { first_index: s.first_index, index_count: s.index_count, blend: s.blend, sort_key: s.sort_key, cull: s.cull, stage, bind_group });
        }
        if built.is_empty() {
            // `DRAWN` hides the W3D body in third person (CONTRACT.md), so a mesh that can only draw
            // nothing must report failure and let the caller fall back.
            return refuse(self, format!("{} surfaces, none with indices", surfaces.len()));
        }
        // Opaque first, then blended surfaces in BO2's sort order.
        built.sort_by_key(|s| (s.blend.blended(), s.sort_key));
        let mut vbytes = Vec::with_capacity(vertices.len() * VERTEX_BYTES as usize);
        for v in vertices {
            for f in v.position.iter().chain(&v.normal).chain(&v.uv) {
                vbytes.extend_from_slice(&f.to_le_bytes());
            }
            for b in v.bones {
                vbytes.extend_from_slice(&b.to_le_bytes());
            }
            for f in v.weights {
                vbytes.extend_from_slice(&f.to_le_bytes());
            }
        }
        let mut ibytes: Vec<u8> = indices.iter().flat_map(|i| i.to_le_bytes()).collect();
        ibytes.resize(ibytes.len().next_multiple_of(4), 0);
        let buffer = |bytes: &[u8], usage, label| {
            let b = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: bytes.len() as u64, usage: usage | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            self.queue.write_buffer(&b, 0, bytes);
            b
        };
        let mesh = Mesh { vertices: buffer(&vbytes, wgpu::BufferUsages::VERTEX, "native vertices"), indices: buffer(&ibytes, wgpu::BufferUsages::INDEX, "native indices"), surfaces: built, max_bone };
        let h = self.native.meshes.insert(Arc::new(mesh));
        if h == INVALID {
            self.report("zhbo-gfx: native mesh pool full".into());
        }
        h
    }

    pub fn native_destroy_mesh(&mut self, h: u16) {
        self.native.meshes.remove(h);
    }

    /// Queue a draw for this frame. False (logged once per reason, counted in
    /// `stats().native_skipped`) if it was refused.
    pub fn native_submit(&mut self, d: &NativeDraw) -> bool {
        let why = if d.target_view as usize >= MAX_VIEWS || d.before_view as usize >= MAX_VIEWS {
            Some(format!("view {} / {} out of range", d.target_view, d.before_view))
        } else if !d.frame.view_proj.is_none_or(|m| finite(&m)) || !finite(&d.frame.eye) || !finite(&[d.frame.ads, d.frame.optics_off, d.frame.reticle_alt, d.frame.optics.time, d.frame.optics.inches_per_unit])
            || !finite(&d.frame.optics.forward)
            || d.frame.optics.blips.iter().any(|b| !finite(&b.pos) || !finite(&[b.radius, b.height, b.intensity])) || d.palette.iter().any(|m| !finite(m)) {
            Some("non-finite matrices".into())
        } else if d.palette.len() > NATIVE_MAX_BONES {
            Some(format!("{} bones past {NATIVE_MAX_BONES}", d.palette.len()))
        } else {
            match self.native.meshes.get(d.mesh) {
                None => Some(format!("mesh {:#06x} is not live", d.mesh)),
                Some(m) if d.palette.len() <= m.max_bone as usize => Some(format!("palette of {} for a mesh skinned to bone {}", d.palette.len(), m.max_bone)),
                _ => None,
            }
        };
        if let Some(why) = why {
            self.native.skipped += 1;
            self.report(format!("zhbo-gfx: native draw refused: {why}"));
            return false;
        }
        let Some(mesh) = self.native.meshes.get(d.mesh).cloned() else { return false };
        let align = self.native.align as usize;
        let ring = &mut self.native.ring;
        ring.resize(ring.len().next_multiple_of(align), 0);
        let frame_offset = ring.len() as u32;
        let f = &d.frame;
        let mut block = Vec::with_capacity(FRAME_BYTES as usize);
        block.extend(f.view_proj.unwrap_or(super::math::IDENTITY).iter().flat_map(|v| v.to_le_bytes()));
        for v in [[f.eye[0], f.eye[1], f.eye[2], 1.0], [f.ambient[0], f.ambient[1], f.ambient[2], 1.0]] {
            block.extend(v.iter().flat_map(|c| c.to_le_bytes()));
        }
        for l in &f.lights {
            block.extend([l.direction[0], l.direction[1], l.direction[2], 0.0].iter().flat_map(|c| c.to_le_bytes()));
        }
        for l in &f.lights {
            block.extend([l.color[0], l.color[1], l.color[2], 0.0].iter().flat_map(|c| c.to_le_bytes()));
        }
        block.extend([f.light_count.min(NATIVE_MAX_LIGHTS as u32) as f32, f.debug, f.ads, f.optics_off].iter().flat_map(|c| c.to_le_bytes()));
        // `screen`: the target's size (filled at upload, when the target is known), the alt lerp.
        block.extend([1.0f32, 1.0, f.reticle_alt, 0.0].iter().flat_map(|c| c.to_le_bytes()));
        // The optics: clock, scale and blip count; the view's forward; the depth terms (filled at
        // upload from the target view's projection); the blips.
        let o = &f.optics;
        let count = o.blip_count.min(NATIVE_MAX_BLIPS as u32);
        block.extend([o.time, o.inches_per_unit, count as f32, 0.0].iter().flat_map(|c| c.to_le_bytes()));
        block.extend([o.forward[0], o.forward[1], o.forward[2], 0.0].iter().flat_map(|c| c.to_le_bytes()));
        block.extend([0.0f32; 4].iter().flat_map(|c| c.to_le_bytes()));
        for b in &o.blips {
            block.extend([b.pos[0], b.pos[1], b.pos[2], b.radius, b.height, b.intensity, 0.0, 0.0].iter().flat_map(|c| c.to_le_bytes()));
        }
        block.resize(FRAME_BYTES as usize, 0);
        ring.extend_from_slice(&block);
        ring.resize(ring.len().next_multiple_of(align), 0);
        let palette_offset = ring.len() as u32;
        for m in d.palette {
            ring.extend(m.iter().flat_map(|c| c.to_le_bytes()));
        }
        ring.resize(palette_offset as usize + PALETTE_BYTES as usize, 0);
        self.native.queue.push(Queued { target_view: d.target_view, before_view: d.before_view, mesh, frame_offset, palette_offset, view_proj_from_view: d.frame.view_proj.is_none() });
        true
    }

    /// Upload this frame's native uniforms (called by `frame()` before encoding).
    pub(crate) fn native_upload(&mut self) {

        if self.native.queue.is_empty() {
            return;
        }
        // The engine's transforms are final now: `view * proj` in bx's row-vector order is the
        // column-major `proj * view` the shader multiplies by.
        // The screen-space noise of the static and ui3d reticles divides the pixel position by the
        // target's size (BO2's `renderTargetSize`).
        let sizes: Vec<(usize, [f32; 2])> = self
            .native
            .queue
            .iter()
            .map(|q| {
                let fb = self.views[q.target_view as usize].frame_buffer;
                let size = self.target_for(fb).map_or([1.0, 1.0], |t| [t.width.max(1) as f32, t.height.max(1) as f32]);
                (q.frame_offset as usize + SCREEN_OFFSET, size)
            })
            .collect();
        for (at, size) in sizes {
            for (k, c) in size.iter().enumerate() {
                self.native.ring[at + 4 * k..at + 4 * k + 4].copy_from_slice(&c.to_le_bytes());
            }
        }
        // The MMS screen linearizes the scene's depth with the target view's own projection
        // (depth = p10 + p14 / z, wgpu's 0..1 depth), whatever matrix the draw itself takes.
        for q in &self.native.queue {
            let p = &self.views[q.target_view as usize].proj;
            let at = q.frame_offset as usize + DEPTH_OFFSET;
            for (k, c) in [p[10], p[14]].iter().enumerate() {
                self.native.ring[at + 4 * k..at + 4 * k + 4].copy_from_slice(&c.to_le_bytes());
            }
        }
        for q in &self.native.queue {
            if q.view_proj_from_view {
                let v = &self.views[q.target_view as usize];
                let vp = super::math::mul(&v.view, &viewmodel_projection(&v.proj));
                let at = q.frame_offset as usize;
                for (k, c) in vp.iter().enumerate() {
                    self.native.ring[at + 4 * k..at + 4 * k + 4].copy_from_slice(&c.to_le_bytes());
                }
            }
        }
        let needed = self.native.ring.len() as u64;
        if self.native.gpu_ring.as_ref().is_none_or(|(_, _, size)| *size < needed) {
            let size = needed.next_power_of_two().max(64 << 10);
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("native ring"), size, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            let bind = |binding, size| wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &buffer, offset: 0, size: wgpu::BufferSize::new(size) }),
            };
            let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("native frame"), layout: &self.native.bgl0, entries: &[bind(0, FRAME_BYTES), bind(1, PALETTE_BYTES)] });
            self.native.gpu_ring = Some((buffer, group, size));
        }
        if let Some((buffer, _, _)) = &self.native.gpu_ring {
            self.queue.write_buffer(buffer, 0, &self.native.ring);
        }
    }

    fn native_pipeline(&mut self, key: PipelineKey) -> Option<wgpu::RenderPipeline> {
        if let Some(p) = self.native.pipelines.get(&key) {
            return p.clone();
        }
        let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Uint16x4, 4 => Float32x4];
        let constants = [("decode_output", if key.color.is_srgb() { 1.0 } else { 0.0 })];
        let blended = key.blend.blended();
        let cull_mode = match key.cull {
            NativeCull::None => None,
            NativeCull::Back => Some(wgpu::Face::Back),
            NativeCull::Front => Some(wgpu::Face::Front),
        };
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("native"),
            layout: Some(&self.native.layout),
            vertex: wgpu::VertexState {
                module: &self.native.module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout { array_stride: VERTEX_BYTES, step_mode: wgpu::VertexStepMode::Vertex, attributes: &attrs })],
            },
            // BO2 and the engine use opposite front-face winding (the W3D conversion swaps the
            // index order for that, verified in game: zhbo-assets `character.rs`); this pass keeps
            // BO2's own order, so its front faces are clockwise in the framebuffer.
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, front_face: wgpu::FrontFace::Cw, cull_mode, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(!blended),
                depth_compare: Some(if blended { wgpu::CompareFunction::LessEqual } else { wgpu::CompareFunction::Less }),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState { count: key.samples, ..Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &self.native.module,
                entry_point: Some(key.stage.entry()),
                compilation_options: wgpu::PipelineCompilationOptions { constants: &constants, ..Default::default() },
                targets: &[Some(wgpu::ColorTargetState { format: key.color, blend: key.blend.state(), write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let failed = super::block_on(scope.pop());
        let built = match failed {
            Some(e) => {
                self.report(format!("zhbo-gfx: native pipeline {:?}/{:?}x{} failed: {e}", key.blend, key.color, key.samples));
                None
            }
            None => Some(pipeline),
        };
        self.native.pipelines.insert(key, built.clone());
        built
    }

    /// The scene's depth for the MMS screen: the target's depth attachment resolved (sample 0) into
    /// a single-sample `R32Float` the size of the target, bound as group 2. `None` (the pass then
    /// binds the sky) when the target has no sampleable depth or the resolve cannot be built; the
    /// screen then shows its blips and noise without the world's outlines.
    fn native_scene_z(&mut self, encoder: &mut wgpu::CommandEncoder, fb: u16, width: u32, height: u32) -> Option<wgpu::BindGroup> {
        let (depth, samples) = self.scene_depth_view(fb)?;
        let ms = usize::from(samples > 1);
        if self.native.resolve[ms].is_none() {
            let built = self.native_resolve_pipeline(samples > 1);
            if built.is_none() {
                self.report(format!("zhbo-gfx: native scene-depth resolve ({samples} samples) unavailable; the MMS screen draws without the world"));
            }
            self.native.resolve[ms] = Some(built);
        }
        let Some(Some((pipeline, bgl))) = self.native.resolve[ms].clone() else { return None };
        if self.native.scene_z.as_ref().is_none_or(|(_, w, h)| (*w, *h) != (width, height)) {
            let view = self
                .device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("native scene depth"),
                    size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: SCENE_Z_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default());
            self.native.scene_z = Some((view, width, height));
        }
        let (scene_z, _, _) = self.native.scene_z.clone()?;
        let input = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("native depth resolve"), layout: &bgl, entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&depth) }] });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("native depth resolve"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &scene_z, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::WHITE), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &input, &[]);
            pass.draw(0..3, 0..1);
        }
        Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("native scene depth"), layout: &self.native.bgl2, entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&scene_z) }] }))
    }

    /// The depth-only, sampleable view of a frame buffer's depth attachment (the backbuffer's for
    /// `INVALID`) and its sample count, if it has one that can be bound.
    fn scene_depth_view(&self, fb: u16) -> Option<(wgpu::TextureView, u32)> {
        if fb == INVALID {
            return self.backbuffer.depth_sampled.clone().map(|v| (v, self.backbuffer.samples));
        }
        let (_, depth, _, _) = self.frame_buffer_targets(fb)?;
        let d = depth?;
        (d.texture.usage().contains(wgpu::TextureUsages::TEXTURE_BINDING) && !d.array).then(|| (d.view.clone(), d.sample_count()))
    }

    fn native_resolve_pipeline(&mut self, multisampled: bool) -> Option<(wgpu::RenderPipeline, wgpu::BindGroupLayout)> {
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let bgl = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("native depth resolve"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled },
                count: None,
            }],
        });
        let source = DEPTH_RESOLVE.replace("TEXTURE", if multisampled { "texture_depth_multisampled_2d" } else { "texture_depth_2d" });
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("native depth resolve"), source: wgpu::ShaderSource::Wgsl(source.into()) });
        let layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("native depth resolve"), bind_group_layouts: &[Some(&bgl)], immediate_size: 0 });
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("native depth resolve"),
            layout: Some(&layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: SCENE_Z_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
            multiview_mask: None,
            cache: None,
        });
        super::block_on(scope.pop()).is_none().then_some((pipeline, bgl))
    }

    fn native_depth(&mut self, width: u32, height: u32, samples: u32) -> wgpu::TextureView {
        if let Some((view, w, h, s)) = &self.native.depth {
            if (*w, *h, *s) == (width, height, samples) {
                return view.clone();
            }
        }
        let view = self
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("native depth"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default());
        self.native.depth = Some((view.clone(), width, height, samples));
        view
    }

    /// Encode the queued draws that run before view `before` (every remaining one for `None`, at
    /// the end of the frame), one pass per target view.
    pub(crate) fn native_encode(&mut self, encoder: &mut wgpu::CommandEncoder, before: Option<u16>) {
        if self.native.queue.is_empty() {
            return;
        }
        // The draws due now leave the queue in order; the rest wait for their view.
        let mut now = std::mem::take(&mut self.native.encoding);
        now.clear();
        now.extend(self.native.queue.extract_if(.., |q| before.is_none_or(|b| q.before_view == b)));
        let mut targets = std::mem::take(&mut self.native.targets);
        targets.clear();
        targets.extend(now.iter().map(|q| q.target_view));
        targets.sort_unstable();
        targets.dedup();
        let mut plans = std::mem::take(&mut self.native.plans);
        for &tv in &targets {
            let view = self.views[tv as usize].clone();
            let Some(target) = self.target_for(view.frame_buffer) else {
                self.report(format!("zhbo-gfx: native pass: view {tv} has no target"));
                continue;
            };
            let (Some(&color_format), Some((color, resolve))) = (target.color_formats.first(), target.colors.first()) else { continue };
            let (rx, ry, rw, rh) = view.rect;
            let x = (rx as u32).min(target.width);
            let y = (ry as u32).min(target.height);
            let (w, h) = ((rw as u32).min(target.width - x), (rh as u32).min(target.height - y));
            if w == 0 || h == 0 {
                continue;
            }
            let depth = self.native_depth(target.width, target.height, target.samples);
            let Some((_, frame_group, _)) = self.native.gpu_ring.clone() else { break };
            // Pipelines first (building needs &mut self, the pass borrows the encoder).
            plans.clear();
            for blended in [false, true] {
                for (qi, q) in now.iter().enumerate().filter(|(_, q)| q.target_view == tv) {
                    for (si, s) in q.mesh.surfaces.iter().enumerate().filter(|(_, s)| s.blend.blended() == blended) {
                        let key = PipelineKey { blend: s.blend, cull: s.cull, stage: s.stage, color: color_format, samples: target.samples };
                        if let Some(p) = self.native_pipeline(key) {
                            plans.push((p, qi, si));
                        }
                    }
                }
            }
            // The MMS screen reads the scene's depth: resolve it first (the sky without one).
            let scene_z = if plans.iter().any(|&(_, qi, si)| now[qi].mesh.surfaces[si].stage == Stage::Sonar) { self.native_scene_z(encoder, view.frame_buffer, target.width, target.height) } else { None };
            let scene_group = scene_z.unwrap_or_else(|| self.native.sky_z.clone());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("native"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: resolve.as_ref(),
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
            pass.set_scissor_rect(x, y, w, h);
            pass.set_bind_group(2, &scene_group, &[]);
            for (pipeline, qi, si) in &plans {
                let (q, s) = (&now[*qi], &now[*qi].mesh.surfaces[*si]);
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &frame_group, &[q.frame_offset, q.palette_offset]);
                pass.set_bind_group(1, &s.bind_group, &[]);
                pass.set_vertex_buffer(0, q.mesh.vertices.slice(..));
                pass.set_index_buffer(q.mesh.indices.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(s.first_index..s.first_index + s.index_count, 0, 0..1);
            }
        }
        // Back to the scratch: the meshes are released with the draws, the capacity is kept.
        plans.clear();
        now.clear();
        self.native.plans = plans;
        self.native.targets = targets;
        self.native.encoding = now;
    }

    /// Forget this frame's draws (called at the end of `frame()`).
    pub(crate) fn native_end_frame(&mut self) {
        self.native.queue.clear();
        self.native.ring.clear();
    }

    /// Live native meshes and textures.
    pub fn native_live(&self) -> (usize, usize) {
        (self.native.meshes.live(), self.native.textures.live())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mips_halve_down_to_one_pixel() {
        let data = vec![200u8; 8 * 4 * 4];
        let m = mips(8, 4, &data);
        assert_eq!(m.iter().map(|l| (l.0, l.1)).collect::<Vec<_>>(), vec![(8, 4), (4, 2), (2, 1), (1, 1)]);
        assert!(m.iter().all(|(w, h, d)| d.len() == (w * h * 4) as usize && d.iter().all(|&c| c == 200)));
    }

    #[test]
    fn blend_modes_write_depth_only_when_solid() {
        assert!(NativeBlend::Opaque.state().is_none() && !NativeBlend::AlphaTest.blended());
        assert!(NativeBlend::Alpha.blended() && NativeBlend::Additive.state().is_some());
    }

    #[test]
    fn the_native_pass_pulls_the_near_plane_in_for_the_viewmodel() {
        // A scene projection with Zero Hour's 10-unit near and a 100-unit far (wgpu depth 0..1).
        let (n, f) = (10.0f32, 100.0f32);
        let proj = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, f / (f - n), 1.0, 0.0, 0.0, -n * f / (f - n), 0.0];
        let out = viewmodel_projection(&proj);
        assert_eq!((out[0], out[5]), (1.0, 1.0), "the FOV and aspect are untouched");
        assert!((out[10] - 100.0 / (100.0 - 0.1)).abs() < 1e-5);
        assert!((out[14] + 0.1 * 100.0 / (100.0 - 0.1)).abs() < 1e-5);
        // A projection already closer than the viewmodel near is left alone.
        let close = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0 / (1.0 - 0.05), 1.0, 0.0, 0.0, -0.05 / (1.0 - 0.05), 0.0];
        assert_eq!(viewmodel_projection(&close), close);
    }
}
