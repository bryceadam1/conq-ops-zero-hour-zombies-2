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
//! first, then blended ones in BO2's sort-key order.
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

const FRAME_BYTES: u64 = 256;
const PALETTE_BYTES: u64 = (NATIVE_MAX_BONES * 64) as u64;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

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

/// One surface of a mesh: an index range and how it draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeSurface {
    pub first_index: u32,
    pub index_count: u32,
    pub blend: NativeBlend,
    pub emissive: bool,
    /// A [`Renderer::native_create_texture`] handle (sRGB colour), or `INVALID` for white.
    pub color: u16,
    /// The packed normal/gloss/specular map (linear), or `INVALID` for a flat matte surface.
    pub maps: u16,
    /// BO2's sort key: blended surfaces draw in its order.
    pub sort_key: u8,
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
}

/// One queued draw: a mesh, its palette (world-space skinning matrices, column-major), and where.
pub struct NativeDraw<'a> {
    pub target_view: u16,
    pub before_view: u16,
    pub mesh: u16,
    pub palette: &'a [[f32; 16]],
    pub frame: NativeFrame,
}

struct NativeTexture {
    view: wgpu::TextureView,
}

struct Surface {
    first_index: u32,
    index_count: u32,
    blend: NativeBlend,
    sort_key: u8,
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
    color: wgpu::TextureFormat,
    samples: u32,
}

pub(crate) struct NativeState {
    textures: Pool<NativeTexture>,
    meshes: Pool<Arc<Mesh>>,
    queue: Vec<Queued>,
    /// This frame's uniforms: per draw, a frame block and a palette block, each aligned.
    ring: Vec<u8>,
    gpu_ring: Option<(wgpu::Buffer, wgpu::BindGroup, u64)>,
    pipelines: HashMap<PipelineKey, Option<wgpu::RenderPipeline>>,
    depth: Option<(wgpu::TextureView, u32, u32, u32)>,
    module: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    bgl0: wgpu::BindGroupLayout,
    bgl1: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    white: wgpu::TextureView,
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
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: wgpu::BufferSize::new(16) },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("native"), bind_group_layouts: &[Some(&bgl0), Some(&bgl1)], immediate_size: 0 });
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
            ring: Vec::new(),
            gpu_ring: None,
            pipelines: HashMap::new(),
            depth: None,
            module,
            layout,
            bgl0,
            bgl1,
            sampler,
            white: one_pixel(device, queue, [255; 4], wgpu::TextureFormat::Rgba8UnormSrgb),
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

impl Renderer {
    /// An RGBA8 texture for native surfaces, with a box-filtered mip chain; `srgb` for colour
    /// maps (sampled to linear), not for packed data maps. `INVALID` if the data is the wrong size.
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

    /// Upload a mesh. Refused (`INVALID`, logged) if a surface's range leaves the indices, an
    /// index leaves the vertices, a bone index is past [`NATIVE_MAX_BONES`], or a texture handle
    /// is stale. Surfaces keep their textures alive after the textures are destroyed.
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
            if s.index_count == 0 || s.index_count % 3 != 0 || end > indices.len() as u64 {
                return refuse(self, format!("surface {k} indices {}..{end} of {}", s.first_index, indices.len()));
            }
            let view = |r: &Renderer, h: u16, fallback: &wgpu::TextureView| -> Option<wgpu::TextureView> {
                if h == INVALID {
                    Some(fallback.clone())
                } else {
                    r.native.textures.get(h).map(|t| t.view.clone())
                }
            };
            let (Some(color), Some(maps)) = (view(self, s.color, &self.native.white), view(self, s.maps, &self.native.flat)) else {
                return refuse(self, format!("surface {k} names a destroyed texture"));
            };
            let params = [
                if s.blend == NativeBlend::AlphaTest { 1.0f32 } else { 0.0 },
                if s.emissive { 1.0 } else { 0.0 },
                if s.maps != INVALID { 1.0 } else { 0.0 },
                0.0,
            ];
            let material = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("native material"),
                size: 16,
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
                ],
            });
            built.push(Surface { first_index: s.first_index, index_count: s.index_count, blend: s.blend, sort_key: s.sort_key, bind_group });
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
        } else if !d.frame.view_proj.is_none_or(|m| finite(&m)) || !finite(&d.frame.eye) || d.palette.iter().any(|m| !finite(m)) {
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
        block.extend([f.light_count.min(NATIVE_MAX_LIGHTS as u32) as f32, f.debug, 0.0, 0.0].iter().flat_map(|c| c.to_le_bytes()));
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
        for q in &self.native.queue {
            if q.view_proj_from_view {
                let v = &self.views[q.target_view as usize];
                let vp = super::math::mul(&v.view, &v.proj);
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
        let constants = [("encode_srgb", if key.color.is_srgb() { 0.0 } else { 1.0 })];
        let blended = key.blend.blended();
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
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
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
                entry_point: Some("fs"),
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
        let (now, later): (Vec<Queued>, Vec<Queued>) = std::mem::take(&mut self.native.queue).into_iter().partition(|q| before.is_none_or(|b| q.before_view == b));
        self.native.queue = later;
        let mut targets: Vec<u16> = now.iter().map(|q| q.target_view).collect();
        targets.sort_unstable();
        targets.dedup();
        for tv in targets {
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
            let Some((_, frame_group, _)) = self.native.gpu_ring.clone() else { return };
            let draws: Vec<&Queued> = now.iter().filter(|q| q.target_view == tv).collect();
            // Pipelines first (building needs &mut self, the pass borrows the encoder).
            let mut plans = Vec::new();
            for blended in [false, true] {
                for q in &draws {
                    for s in q.mesh.surfaces.iter().filter(|s| s.blend.blended() == blended) {
                        let key = PipelineKey { blend: s.blend, color: color_format, samples: target.samples };
                        if let Some(p) = self.native_pipeline(key) {
                            plans.push((p, *q, s));
                        }
                    }
                }
            }
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
            for (pipeline, q, s) in plans {
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &frame_group, &[q.frame_offset, q.palette_offset]);
                pass.set_bind_group(1, &s.bind_group, &[]);
                pass.set_vertex_buffer(0, q.mesh.vertices.slice(..));
                pass.set_index_buffer(q.mesh.indices.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(s.first_index..s.first_index + s.index_count, 0, 0..1);
            }
        }
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
}
