//! `frame()`: turn the recorded views into render passes, present, and take screenshots.

use std::collections::HashMap;
use std::sync::Arc;

use super::math::{self, Mtx};
use super::{BufferSrc, Draw, Predefined, Program, Renderer, Texture, INVALID, MAX_VIEWS};
use crate::state::{self, PipelineState, SamplerKey, StencilFace};

pub(crate) struct Backbuffer {
    color: wgpu::Texture,
    /// The view draws target (sRGB-encoding when the reset flags ask for it).
    target: wgpu::TextureView,
    /// Plain view, for presenting and screenshots.
    plain: wgpu::TextureView,
    format: wgpu::TextureFormat,
    msaa: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
    samples: u32,
    width: u32,
    height: u32,
    flags: u32,
}

const BACKBUFFER_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

#[derive(Clone, PartialEq, Eq, Hash)]
struct PipelineKey {
    program: u32,
    unfilterable: u32,
    state: PipelineState,
    stencil: Option<(StencilFace, StencilFace)>,
    streams: [Option<u16>; super::MAX_STREAMS],
    instance_stride: Option<u32>,
    colors: Vec<wgpu::TextureFormat>,
    depth: Option<wgpu::TextureFormat>,
    samples: u32,
}

/// Which buffer feeds each vertex-buffer slot of a pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotSrc {
    Stream(usize),
    Instance,
    Zero,
}

struct CachedPipeline {
    pipeline: wgpu::RenderPipeline,
    slots: Vec<SlotSrc>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct ClearKey {
    colors: Vec<wgpu::TextureFormat>,
    depth: Option<wgpu::TextureFormat>,
    samples: u32,
    flags: u16,
}

pub(crate) struct Caches {
    pipelines: HashMap<PipelineKey, Option<CachedPipeline>>,
    layouts: HashMap<(u32, u32), (wgpu::PipelineLayout, Option<wgpu::BindGroupLayout>)>,
    samplers: HashMap<(SamplerKey, bool), wgpu::Sampler>,
    empty_bgl: wgpu::BindGroupLayout,
    dummy_2d: wgpu::TextureView,
    dummy_array: wgpu::TextureView,
    zero: wgpu::Buffer,
    uniforms: wgpu::Buffer,
    present: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    present_bgl: wgpu::BindGroupLayout,
    present_module: wgpu::ShaderModule,
    nearest: wgpu::Sampler,
    clear: HashMap<ClearKey, wgpu::RenderPipeline>,
    clear_bgl: wgpu::BindGroupLayout,
    clear_module: wgpu::ShaderModule,
    /// Draw problems already reported (so the log is not flooded every frame).
    reported: std::collections::HashSet<String>,
}

const PRESENT_WGSL: &str = r"
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
struct O { @builtin(position) p: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs(@builtin(vertex_index) i: u32) -> O {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: O;
    o.p = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    o.uv = uv;
    return o;
}
@fragment fn fs(i: O) -> @location(0) vec4<f32> { return textureSample(t, s, i.uv); }
";

const CLEAR_WGSL: &str = r"
struct C { color: vec4<f32>, depth: vec4<f32> }
@group(0) @binding(0) var<uniform> c: C;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, c.depth.x, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> { return c.color; }
";

impl Caches {
    pub(crate) fn new(device: &wgpu::Device) -> Caches {
        let dummy = |layers: u32, dim| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("dummy"),
                    size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: layers },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor { dimension: Some(dim), ..Default::default() })
        };
        let tex_entry = |binding, filterable| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let present_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("present"),
            entries: &[
                tex_entry(0, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });
        let clear_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("clear"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        Caches {
            pipelines: HashMap::new(),
            layouts: HashMap::new(),
            samplers: HashMap::new(),
            empty_bgl: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("empty"), entries: &[] }),
            dummy_2d: dummy(1, wgpu::TextureViewDimension::D2),
            dummy_array: dummy(1, wgpu::TextureViewDimension::D2Array),
            zero: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("zero attributes"),
                size: 64,
                usage: wgpu::BufferUsages::VERTEX,
                mapped_at_creation: false,
            }),
            uniforms: uniform_buffer(device, 64 << 10),
            present: HashMap::new(),
            present_bgl,
            present_module: device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("present"),
                source: wgpu::ShaderSource::Wgsl(PRESENT_WGSL.into()),
            }),
            nearest: device.create_sampler(&wgpu::SamplerDescriptor::default()),
            clear: HashMap::new(),
            clear_bgl,
            clear_module: device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("clear"),
                source: wgpu::ShaderSource::Wgsl(CLEAR_WGSL.into()),
            }),
            reported: Default::default(),
        }
    }
}

fn uniform_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("uniform ring"),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// A resolved render target for one view.
struct Target {
    /// (attachment view, resolve view) per colour target.
    colors: Vec<(wgpu::TextureView, Option<wgpu::TextureView>)>,
    color_formats: Vec<wgpu::TextureFormat>,
    depth: Option<(wgpu::TextureView, wgpu::TextureFormat)>,
    samples: u32,
    width: u32,
    height: u32,
}

fn texture_view_format(t: &Texture) -> wgpu::TextureFormat {
    if t.flags & super::TEXTURE_SRGB != 0 { t.plan.format.add_srgb_suffix() } else { t.plan.format }
}

fn bgfx_color(rgba: u32) -> [f32; 4] {
    let c = state::clear_color(rgba);
    [c.r as f32, c.g as f32, c.b as f32, c.a as f32]
}

impl Renderer {
    fn ensure_backbuffer(&mut self) {
        let msaa_bits = (self.reset_flags >> super::RESET_MSAA_SHIFT) & 7;
        if let Some(bb) = &self.backbuffer {
            if bb.width == self.width && bb.height == self.height && bb.flags == self.reset_flags {
                return;
            }
        }
        let wanted = if msaa_bits == 0 { 1 } else { 1 << msaa_bits };
        let samples = self.supported_samples(BACKBUFFER_FORMAT, wanted).min(self.supported_samples(DEPTH_FORMAT, wanted));
        let size = wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 };
        let srgb = BACKBUFFER_FORMAT.add_srgb_suffix();
        let format = if self.reset_flags & super::RESET_SRGB_BACKBUFFER != 0 { srgb } else { BACKBUFFER_FORMAT };
        let (base, view_formats): (_, &[wgpu::TextureFormat]) =
            if self.view_formats_supported { (BACKBUFFER_FORMAT, &[srgb]) } else { (format, &[]) };
        let color = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("backbuffer"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: base,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats,
        });
        let target = color.create_view(&wgpu::TextureViewDescriptor { format: Some(format), ..Default::default() });
        let plain = color.create_view(&Default::default());
        let msaa = (samples > 1).then(|| {
            self.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("backbuffer msaa"),
                    size,
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        });
        let depth = self
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("backbuffer depth"),
                size,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default());
        self.backbuffer = Some(Backbuffer {
            color,
            target,
            plain,
            format,
            msaa,
            depth,
            samples,
            width: self.width,
            height: self.height,
            flags: self.reset_flags,
        });
    }

    fn target_for(&self, fb: u16) -> Option<Target> {
        if fb == INVALID {
            let bb = self.backbuffer.as_ref()?;
            let color = match &bb.msaa {
                Some(m) => (m.clone(), Some(bb.target.clone())),
                None => (bb.target.clone(), None),
            };
            return Some(Target {
                colors: vec![color],
                color_formats: vec![bb.format],
                depth: Some((bb.depth.clone(), DEPTH_FORMAT)),
                samples: bb.samples,
                width: bb.width,
                height: bb.height,
            });
        }
        let (colors, depth, width, height) = self.frame_buffer_targets(fb)?;
        let mut samples = 1;
        let mut out = Vec::new();
        let mut formats = Vec::new();
        for c in &colors {
            match &c.msaa {
                Some(m) => {
                    samples = c.samples;
                    out.push((m.clone(), Some(c.layer_views[0].clone())));
                }
                None => out.push((c.layer_views[0].clone(), None)),
            }
            formats.push(texture_view_format(c));
        }
        if let Some(d) = &depth {
            if colors.is_empty() {
                samples = d.sample_count();
            }
        }
        Some(Target {
            colors: out,
            color_formats: formats,
            depth: depth.as_ref().map(|d| (d.layer_views[0].clone(), d.plan.format)),
            samples,
            width,
            height,
        })
    }

    /// Execute the frame. Returns the frame number, like `bgfx::frame`.
    pub fn frame(&mut self) -> u32 {
        let started = std::time::Instant::now();
        self.ensure_backbuffer();

        // Transient data the engine wrote this frame.
        for arena in [&self.transient_vb, &self.transient_ib] {
            if arena.used > 0 {
                let n = arena.used.next_multiple_of(4).min(arena.data.len());
                self.queue.write_buffer(&arena.gpu, 0, &arena.data[..n]);
            }
        }
        self.stats.transient_vb_used = self.transient_vb.used as u32;
        self.stats.transient_ib_used = self.transient_ib.used as u32;

        self.patch_predefined_uniforms();
        if !self.uniform_ring.is_empty() {
            let needed = self.uniform_ring.len().next_multiple_of(4) as u64;
            if self.cache.uniforms.size() < needed {
                self.cache.uniforms = uniform_buffer(&self.device, needed.next_power_of_two());
            }
            self.uniform_ring.resize(needed as usize, 0);
            self.queue.write_buffer(&self.cache.uniforms, 0, &self.uniform_ring);
        }

        // Bucket draws by view.
        let mut by_view: Vec<Vec<usize>> = vec![Vec::new(); MAX_VIEWS];
        for (i, d) in self.draws.iter().enumerate() {
            by_view[d.view as usize].push(i);
        }

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        let mut group0: HashMap<u32, wgpu::BindGroup> = HashMap::new();
        let mut group1: HashMap<(u32, u32, Vec<(u32, u32)>), wgpu::BindGroup> = HashMap::new();
        let mut visited = [false; MAX_VIEWS];
        let mut num_draw = 0;
        for pos in 0..MAX_VIEWS {
            let v = self.view_order[pos] as usize;
            if v >= MAX_VIEWS || visited[v] {
                continue;
            }
            visited[v] = true;

            for b in self.blits.iter().filter(|b| b.view as usize == v) {
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo { texture: &b.src.texture, mip_level: b.src_mip, origin: b.src_origin, aspect: wgpu::TextureAspect::All },
                    wgpu::TexelCopyTextureInfo { texture: &b.dst.texture, mip_level: b.dst_mip, origin: b.dst_origin, aspect: wgpu::TextureAspect::All },
                    wgpu::Extent3d { width: b.size.0, height: b.size.1, depth_or_array_layers: b.size.2 },
                );
            }

            let view = self.views[v].clone();
            let mut draws = std::mem::take(&mut by_view[v]);
            if draws.is_empty() && !view.touched {
                continue;
            }
            let Some(target) = self.target_for(view.frame_buffer) else { continue };
            match view.mode {
                2 => draws.sort_by_key(|&i| (self.draws[i].depth, self.draws[i].seq)),
                3 => draws.sort_by_key(|&i| (std::cmp::Reverse(self.draws[i].depth), self.draws[i].seq)),
                _ => {}
            }

            // The view rect, clamped to the target (wgpu rejects viewports outside it).
            let (rx, ry, rw, rh) = view.rect;
            let x = (rx as u32).min(target.width);
            let y = (ry as u32).min(target.height);
            let w = (rw as u32).min(target.width - x);
            let h = (rh as u32).min(target.height - y);
            if w == 0 || h == 0 {
                continue;
            }
            let full = x == 0 && y == 0 && w == target.width && h == target.height;
            let clear = view.clear;
            let cc = bgfx_color(clear.rgba);
            let load_color = |flags: u16| {
                if full && flags & state::CLEAR_COLOR != 0 {
                    wgpu::LoadOp::Clear(wgpu::Color { r: cc[0] as f64, g: cc[1] as f64, b: cc[2] as f64, a: cc[3] as f64 })
                } else {
                    wgpu::LoadOp::Load
                }
            };
            let color_attachments: Vec<Option<wgpu::RenderPassColorAttachment>> = target
                .colors
                .iter()
                .map(|(view_, resolve)| {
                    Some(wgpu::RenderPassColorAttachment {
                        view: view_,
                        depth_slice: None,
                        resolve_target: resolve.as_ref(),
                        ops: wgpu::Operations { load: load_color(clear.flags), store: wgpu::StoreOp::Store },
                    })
                })
                .collect();
            let depth_attachment = target.depth.as_ref().map(|(dv, fmt)| wgpu::RenderPassDepthStencilAttachment {
                view: dv,
                depth_ops: Some(wgpu::Operations {
                    load: if full && clear.flags & state::CLEAR_DEPTH != 0 { wgpu::LoadOp::Clear(clear.depth) } else { wgpu::LoadOp::Load },
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: fmt.has_stencil_aspect().then(|| wgpu::Operations {
                    load: if full && clear.flags & state::CLEAR_STENCIL != 0 { wgpu::LoadOp::Clear(clear.stencil as u32) } else { wgpu::LoadOp::Load },
                    store: wgpu::StoreOp::Store,
                }),
            });

            // Partial clears are drawn as a quad inside the pass.
            let partial_clear = (!full && clear.flags & 7 != 0).then(|| self.clear_quad(&target, clear));

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &color_attachments,
                depth_stencil_attachment: depth_attachment,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
            pass.set_scissor_rect(x, y, w, h);
            if let Some(Some((pipeline, bind_group))) = &partial_clear {
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, bind_group, &[]);
                pass.set_stencil_reference(clear.stencil as u32);
                pass.draw(0..3, 0..1);
            }

            for &i in &draws {
                if self.encode_draw(&mut pass, i, &target, &mut group0, &mut group1) {
                    num_draw += 1;
                }
            }
        }

        // Screenshots: copy into readback buffers now, map after submit.
        let mut readbacks = Vec::new();
        for (fb, path) in std::mem::take(&mut self.screenshots) {
            let (texture, format) = if fb == INVALID {
                let bb = self.backbuffer.as_ref().unwrap();
                (bb.color.clone(), BACKBUFFER_FORMAT)
            } else {
                match self.frame_buffer_targets(fb).and_then(|t| t.0.first().cloned()) {
                    Some(t) => (t.texture.clone(), t.plan.format),
                    None => continue,
                }
            };
            let (w, h) = (texture.width(), texture.height());
            let pitch = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("screenshot"),
                size: (pitch * h) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(pitch), rows_per_image: Some(h) },
                },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            readbacks.push((buffer, path, w, h, pitch, format));
        }

        // Present: copy the backbuffer to the window.
        let mut surface_texture = None;
        if let Some((surface, config)) = &self.surface {
            match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => surface_texture = Some(t),
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => surface.configure(&self.device, config),
                _ => {}
            }
        }
        if let Some(st) = &surface_texture {
            let format = st.texture.format();
            let pipeline = self.present_pipeline(format);
            let bb = self.backbuffer.as_ref().unwrap();
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("present"),
                layout: &self.cache.present_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&bb.plain) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.cache.nearest) },
                ],
            });
            let view = st.texture.create_view(&Default::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("present"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        self.queue.submit([encoder.finish()]);
        if let Some(st) = surface_texture {
            self.queue.present(st);
        }

        for (buffer, path, w, h, pitch, format) in readbacks {
            buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
            let Ok(data) = buffer.slice(..).get_mapped_range() else { continue };
            let mut bgra = Vec::with_capacity((w * h * 4) as usize);
            for row in 0..h as usize {
                let line = &data[row * pitch as usize..row * pitch as usize + w as usize * 4];
                if matches!(format, wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb) {
                    for px in line.chunks_exact(4) {
                        bgra.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
                    }
                } else {
                    bgra.extend_from_slice(line);
                }
            }
            drop(data);
            if let Some(f) = &self.screenshot_fn {
                f(&path, w, h, w * 4, &bgra);
            }
        }

        // Reset per-frame state.
        self.stats.num_draw = num_draw;
        self.stats.num_blit = self.blits.len() as u32;
        self.stats.cpu_frame_ns = started.elapsed().as_nanos() as u64;
        self.draws.clear();
        self.blits.clear();
        self.transforms.clear();
        self.uniform_ring.clear();
        self.transient_vb.used = 0;
        self.transient_ib.used = 0;
        for v in &mut self.views {
            v.touched = false;
        }
        self.frame_number = self.frame_number.wrapping_add(1);
        self.frame_number
    }

    /// Fill in bgfx's predefined uniforms now that the view transforms are final.
    fn patch_predefined_uniforms(&mut self) {
        let mut view_cache: HashMap<u16, [Mtx; 6]> = HashMap::new();
        for d in &self.draws {
            let view = &self.views[d.view as usize];
            let m = *view_cache.entry(d.view).or_insert_with(|| {
                let vp = math::mul(&view.view, &view.proj);
                [view.view, math::inverse(&view.view), view.proj, math::inverse(&view.proj), vp, math::inverse(&vp)]
            });
            let model = d
                .state
                .transform
                .and_then(|(i, _)| self.transforms.get(i as usize))
                .copied()
                .unwrap_or(math::IDENTITY);
            let (rx, ry, rw, rh) = view.rect;
            let alpha_ref = state::alpha_ref(d.state.state) as f32 / 255.0;
            for (stage, base) in [(&d.program.vs_uniforms, d.ubo[0]), (&d.program.fs_uniforms, d.ubo[1])] {
                let (Some(u), Some(base)) = (stage, base) else { continue };
                for &(p, at) in &u.predefined {
                    let vec4;
                    let mtx;
                    let values: &[f32] = match p {
                        Predefined::ViewRect => {
                            vec4 = [rx as f32, ry as f32, rw as f32, rh as f32];
                            &vec4
                        }
                        Predefined::ViewTexel => {
                            vec4 = [1.0 / (rw.max(1) as f32), 1.0 / (rh.max(1) as f32), 0.0, 0.0];
                            &vec4
                        }
                        Predefined::View => &m[0],
                        Predefined::InvView => &m[1],
                        Predefined::Proj => &m[2],
                        Predefined::InvProj => &m[3],
                        Predefined::ViewProj => &m[4],
                        Predefined::InvViewProj => &m[5],
                        Predefined::Model => &model,
                        Predefined::ModelView => {
                            mtx = math::mul(&model, &m[0]);
                            &mtx
                        }
                        Predefined::ModelViewProj => {
                            mtx = math::mul(&model, &m[4]);
                            &mtx
                        }
                        Predefined::AlphaRef => {
                            vec4 = [alpha_ref, 0.0, 0.0, 0.0];
                            &vec4
                        }
                    };
                    let start = (base + at) as usize;
                    for (chunk, v) in self.uniform_ring[start..start + values.len() * 4].chunks_exact_mut(4).zip(values) {
                        chunk.copy_from_slice(&v.to_le_bytes());
                    }
                }
            }
        }
    }

    fn report(&mut self, msg: String) {
        if self.cache.reported.len() < 256 && self.cache.reported.insert(msg.clone()) {
            self.log(&msg);
        }
    }

    /// Encode one draw. Returns false (and reports why, once) if it had to be skipped; a draw
    /// that would fail validation must never reach the pass, or the whole frame is lost.
    fn encode_draw(
        &mut self,
        pass: &mut wgpu::RenderPass<'_>,
        i: usize,
        target: &Target,
        group0: &mut HashMap<u32, wgpu::BindGroup>,
        group1: &mut HashMap<(u32, u32, Vec<(u32, u32)>), wgpu::BindGroup>,
    ) -> bool {
        let d: &Draw = &self.draws[i];
        let program = d.program.clone();
        let st = d.state.clone();
        let ubo = d.ubo;

        // Which bound textures cannot be filtered (depth, or 32-bit float without the feature).
        let features = self.device.features();
        let mut unfilterable = 0u32;
        let mut textures: Vec<(u32, Option<Arc<Texture>>, u32)> = Vec::new();
        for &(slot, array) in &program.slots {
            let binding = st.textures.get(slot as usize).and_then(|t| t.clone());
            let binding = binding.filter(|b| b.texture.array == array && b.texture.sample_count() == 1);
            if let Some(b) = &binding {
                let aspect = if b.texture.is_depth() { wgpu::TextureAspect::DepthOnly } else { wgpu::TextureAspect::All };
                let filterable = matches!(
                    b.texture.plan.format.sample_type(Some(aspect), Some(features)),
                    Some(wgpu::TextureSampleType::Float { filterable: true })
                );
                if !filterable {
                    unfilterable |= 1 << slot;
                }
            }
            textures.push((slot, binding.as_ref().map(|b| b.texture.clone()), binding.map(|b| b.sampler).unwrap_or(0)));
        }

        let decoded = state::decode_state(st.state);
        let stencil = state::decode_stencil(st.stencil.0).map(|f| (f, state::decode_stencil(st.stencil.1).unwrap_or(f)));
        let key = PipelineKey {
            program: program.serial,
            unfilterable,
            state: decoded,
            stencil: if target.depth.as_ref().is_some_and(|(_, f)| f.has_stencil_aspect()) { stencil } else { None },
            streams: std::array::from_fn(|s| st.streams[s].as_ref().map(|b| b.layout)),
            instance_stride: st.instance.as_ref().map(|i| i.stride),
            colors: target.color_formats.clone(),
            depth: target.depth.as_ref().map(|(_, f)| *f),
            samples: target.samples,
        };
        if !self.cache.pipelines.contains_key(&key) {
            let built = self.build_pipeline(&program, &key);
            self.cache.pipelines.insert(key.clone(), built);
        }
        let Some(Some(cached)) = self.cache.pipelines.get(&key) else { return false };
        let slots = cached.slots.clone();

        // Validate buffer ranges before touching the pass.
        let (tvb, tib) = (self.transient_vb.gpu.clone(), self.transient_ib.gpu.clone());
        let gpu = |src: &BufferSrc| match src {
            BufferSrc::Gpu(b) => b.clone(),
            BufferSrc::TransientVertex => tvb.clone(),
            BufferSrc::TransientIndex => tib.clone(),
        };
        let buffer_size = |src: &BufferSrc| gpu(src).size();
        let mut vertex_count = u32::MAX;
        let mut binds: Vec<(wgpu::Buffer, u64)> = Vec::new();
        for s in &slots {
            match *s {
                SlotSrc::Stream(n) => {
                    let Some(b) = &st.streams[n] else { return false };
                    let size = buffer_size(&b.src);
                    let stride = self.layout(b.layout).map(|l| l.stride as u64).unwrap_or(0);
                    if b.offset % 4 != 0 || b.offset > size {
                        self.report(format!("zhbo-gfx: skipped a draw: vertex stream offset {} is unaligned or past the buffer", b.offset));
                        return false;
                    }
                    let fits = if stride == 0 { u32::MAX } else { ((size - b.offset) / stride) as u32 };
                    vertex_count = vertex_count.min(b.num_vertices.min(fits));
                    binds.push((gpu(&b.src), b.offset));
                }
                SlotSrc::Instance => {
                    let Some(inst) = &st.instance else { return false };
                    if inst.offset % 4 != 0 || inst.offset + inst.num as u64 * inst.stride as u64 > buffer_size(&inst.src) {
                        self.report("zhbo-gfx: skipped a draw: instance data out of range".into());
                        return false;
                    }
                    binds.push((gpu(&inst.src), inst.offset));
                }
                SlotSrc::Zero => binds.push((self.cache.zero.clone(), 0)),
            }
        }
        let instances = st.instance.as_ref().map(|i| i.num).unwrap_or(1);
        if instances == 0 {
            return false;
        }
        let index = match &st.index {
            Some(ib) => {
                let isize = if ib.format == wgpu::IndexFormat::Uint32 { 4 } else { 2 };
                let end = ib.offset + (ib.first as u64 + ib.num as u64) * isize;
                if ib.num == 0 || end > buffer_size(&ib.src) || ib.offset % isize != 0 {
                    if ib.num != 0 {
                        self.report("zhbo-gfx: skipped a draw: index range past the buffer".into());
                    }
                    return false;
                }
                Some((gpu(&ib.src), ib.offset, ib.format, ib.first, ib.num))
            }
            None => {
                if vertex_count == 0 || vertex_count == u32::MAX {
                    return false;
                }
                None
            }
        };

        // Bind group 0: this program's uniform blocks in the frame ring.
        if let Some(bgl0) = &program.bgl0 {
            if !group0.contains_key(&program.serial) {
                let mut entries = Vec::new();
                for (binding, u) in [(0, &program.vs_uniforms), (1, &program.fs_uniforms)] {
                    if let Some(u) = u {
                        entries.push(wgpu::BindGroupEntry {
                            binding,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &self.cache.uniforms,
                                offset: 0,
                                size: std::num::NonZeroU64::new(u.size as u64),
                            }),
                        });
                    }
                }
                let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: bgl0, entries: &entries });
                group0.insert(program.serial, bg);
            }
        }

        // Bind group 1: textures and samplers.
        let bg1_key = (
            program.serial,
            unfilterable,
            textures.iter().map(|(_, t, s)| (t.as_ref().map(|t| t.serial).unwrap_or(0), *s)).collect::<Vec<_>>(),
        );
        let layout_bgl1 = self.cache.layouts.get(&(program.serial, unfilterable)).and_then(|(_, b)| b.clone());
        if let Some(bgl1) = &layout_bgl1 {
            if !group1.contains_key(&bg1_key) {
                let mut samplers = Vec::new();
                for (slot, _, flags) in &textures {
                    let nonfiltering = unfilterable & (1 << slot) != 0;
                    let mut key = state::decode_sampler(*flags, self.border_supported);
                    key.compare = None;
                    if nonfiltering {
                        key.mag = wgpu::FilterMode::Nearest;
                        key.min = wgpu::FilterMode::Nearest;
                        key.mip = wgpu::MipmapFilterMode::Nearest;
                        key.anisotropy = 1;
                    }
                    let device = &self.device;
                    let s = self.cache.samplers.entry((key, nonfiltering)).or_insert_with(|| device.create_sampler(&key.descriptor())).clone();
                    samplers.push(s);
                }
                let mut entries = Vec::new();
                for ((slot, tex, _), sampler) in textures.iter().zip(&samplers) {
                    let array = program.slots.iter().any(|&(s, a)| s == *slot && a);
                    let view = match tex {
                        Some(t) => &t.view,
                        None if array => &self.cache.dummy_array,
                        None => &self.cache.dummy_2d,
                    };
                    entries.push(wgpu::BindGroupEntry { binding: slot * 2, resource: wgpu::BindingResource::TextureView(view) });
                    entries.push(wgpu::BindGroupEntry { binding: slot * 2 + 1, resource: wgpu::BindingResource::Sampler(sampler) });
                }
                let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: bgl1, entries: &entries });
                group1.insert(bg1_key.clone(), bg);
            }
        }

        let Some(Some(cached)) = self.cache.pipelines.get(&key) else { return false };
        pass.set_pipeline(&cached.pipeline);
        if let Some(bg) = group0.get(&program.serial) {
            let mut offsets = Vec::new();
            if program.vs_uniforms.is_some() {
                offsets.push(ubo[0].unwrap_or(0));
            }
            if program.fs_uniforms.is_some() {
                offsets.push(ubo[1].unwrap_or(0));
            }
            pass.set_bind_group(0, bg, &offsets);
        }
        if let Some(bg) = group1.get(&bg1_key) {
            pass.set_bind_group(1, bg, &[]);
        }
        let c = bgfx_color(st.rgba);
        pass.set_blend_constant(wgpu::Color { r: c[0] as f64, g: c[1] as f64, b: c[2] as f64, a: c[3] as f64 });
        if let Some((front, _)) = key.stencil {
            pass.set_stencil_reference(front.reference as u32);
        }
        for (slot, (buffer, offset)) in binds.iter().enumerate() {
            pass.set_vertex_buffer(slot as u32, buffer.slice(*offset..));
        }
        match index {
            Some((buffer, offset, format, first, num)) => {
                pass.set_index_buffer(buffer.slice(offset..), format);
                pass.draw_indexed(first..first + num, 0, 0..instances);
            }
            None => pass.draw(0..vertex_count, 0..instances),
        }
        true
    }

    fn pipeline_layout(&mut self, program: &Program, unfilterable: u32) -> wgpu::PipelineLayout {
        let key = (program.serial, unfilterable);
        if let Some((l, _)) = self.cache.layouts.get(&key) {
            return l.clone();
        }
        let mut entries = Vec::new();
        for &(slot, array) in &program.slots {
            let filterable = unfilterable & (1 << slot) == 0;
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: slot * 2,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable },
                    view_dimension: if array { wgpu::TextureViewDimension::D2Array } else { wgpu::TextureViewDimension::D2 },
                    multisampled: false,
                },
                count: None,
            });
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: slot * 2 + 1,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Sampler(if filterable {
                    wgpu::SamplerBindingType::Filtering
                } else {
                    wgpu::SamplerBindingType::NonFiltering
                }),
                count: None,
            });
        }
        let bgl1 = (!entries.is_empty())
            .then(|| self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("textures"), entries: &entries }));
        let g0 = program.bgl0.as_ref().unwrap_or(&self.cache.empty_bgl);
        let g1 = bgl1.as_ref().unwrap_or(&self.cache.empty_bgl);
        let layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(g0), Some(g1)],
            immediate_size: 0,
        });
        self.cache.layouts.insert(key, (layout.clone(), bgl1));
        layout
    }

    fn build_pipeline(&mut self, program: &Program, key: &PipelineKey) -> Option<CachedPipeline> {
        let layout = self.pipeline_layout(program, key.unfilterable);

        // Vertex buffer slots: each stream supplies the attributes the shader wants that it has
        // (first stream wins), instance data supplies i_dataN, and a zero buffer covers the rest.
        let mut slots: Vec<SlotSrc> = Vec::new();
        let mut strides: Vec<(u64, wgpu::VertexStepMode)> = Vec::new();
        let mut attrs: Vec<Vec<wgpu::VertexAttribute>> = Vec::new();
        let mut assigned: Vec<u32> = Vec::new();
        let shader_attrs = &program.vs.blob.attributes;
        for (n, layout_id) in key.streams.iter().enumerate() {
            let Some(l) = layout_id.and_then(|id| self.layout(id)) else { continue };
            let mut list = Vec::new();
            for a in shader_attrs {
                if assigned.contains(&a.location) || a.name.starts_with("i_data") {
                    continue;
                }
                let Some(idx) = state::ATTRIB_SEMANTICS.iter().position(|s| *s == a.semantic) else { continue };
                if let Some((offset, format)) = l.attribs[idx] {
                    list.push(wgpu::VertexAttribute { format, offset: offset as u64, shader_location: a.location });
                    assigned.push(a.location);
                }
            }
            slots.push(SlotSrc::Stream(n));
            strides.push((l.stride as u64, wgpu::VertexStepMode::Vertex));
            attrs.push(list);
        }
        if let Some(stride) = key.instance_stride {
            let mut list = Vec::new();
            for a in shader_attrs {
                if let Some(n) = a.name.strip_prefix("i_data").and_then(|n| n.parse::<u64>().ok()) {
                    if (n + 1) * 16 <= stride as u64 {
                        list.push(wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: n * 16, shader_location: a.location });
                        assigned.push(a.location);
                    }
                }
            }
            slots.push(SlotSrc::Instance);
            strides.push((stride as u64, wgpu::VertexStepMode::Instance));
            attrs.push(list);
        }
        let missing: Vec<wgpu::VertexAttribute> = shader_attrs
            .iter()
            .filter(|a| !assigned.contains(&a.location))
            .map(|a| wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 0, shader_location: a.location })
            .collect();
        if !missing.is_empty() {
            slots.push(SlotSrc::Zero);
            strides.push((0, wgpu::VertexStepMode::Vertex));
            attrs.push(missing);
        }
        let buffers: Vec<Option<wgpu::VertexBufferLayout>> = strides
            .iter()
            .zip(&attrs)
            .map(|(&(array_stride, step_mode), attributes)| Some(wgpu::VertexBufferLayout { array_stride, step_mode, attributes }))
            .collect();

        let s = &key.state;
        let targets: Vec<Option<wgpu::ColorTargetState>> = key
            .colors
            .iter()
            .map(|&format| {
                // Blending on a non-blendable format (e.g. R32F) is a validation error.
                let blendable = format.guaranteed_format_features(self.device.features()).flags.contains(wgpu::TextureFormatFeatureFlags::BLENDABLE);
                Some(wgpu::ColorTargetState { format, blend: if blendable { s.blend } else { None }, write_mask: s.write_mask })
            })
            .collect();
        let depth_stencil = key.depth.map(|format| {
            let stencil = match key.stencil {
                Some((front, back)) => wgpu::StencilState { front: front.face, back: back.face, read_mask: front.read_mask as u32, write_mask: 0xff },
                None => wgpu::StencilState::default(),
            };
            wgpu::DepthStencilState {
                format,
                depth_write_enabled: Some(s.depth_write),
                depth_compare: Some(state::effective_depth_compare(s)),
                stencil,
                bias: wgpu::DepthBiasState::default(),
            }
        });
        let topology = s.topology;
        let fs = program.fs.as_ref();
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &program.vs.module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                buffers: &buffers,
            },
            primitive: wgpu::PrimitiveState {
                topology,
                strip_index_format: None,
                front_face: s.front_face,
                cull_mode: s.cull_mode,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil,
            multisample: wgpu::MultisampleState {
                count: key.samples,
                mask: !0,
                alpha_to_coverage_enabled: s.alpha_to_coverage && key.samples > 1,
            },
            fragment: fs.map(|f| wgpu::FragmentState {
                module: &f.module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        });
        if let Some(e) = super::block_on(scope.pop()) {
            self.report(format!("zhbo-gfx: pipeline for program {} failed: {e}", program.serial));
            return None;
        }
        Some(CachedPipeline { pipeline, slots })
    }

    fn present_pipeline(&mut self, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        if let Some(p) = self.cache.present.get(&format) {
            return p.clone();
        }
        let layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("present"),
            bind_group_layouts: &[Some(&self.cache.present_bgl)],
            immediate_size: 0,
        });
        let p = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("present"),
            layout: Some(&layout),
            vertex: wgpu::VertexState { module: &self.cache.present_module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &self.cache.present_module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        self.cache.present.insert(format, p.clone());
        p
    }

    /// Pipeline and uniforms for clearing part of a target (a view rect smaller than it).
    fn clear_quad(&mut self, target: &Target, clear: super::Clear) -> Option<(wgpu::RenderPipeline, wgpu::BindGroup)> {
        let key = ClearKey {
            colors: target.color_formats.clone(),
            depth: target.depth.as_ref().map(|(_, f)| *f),
            samples: target.samples,
            flags: clear.flags & 7,
        };
        if !self.cache.clear.contains_key(&key) {
            let layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("clear"),
                bind_group_layouts: &[Some(&self.cache.clear_bgl)],
                immediate_size: 0,
            });
            let write = if clear.flags & state::CLEAR_COLOR != 0 { wgpu::ColorWrites::ALL } else { wgpu::ColorWrites::empty() };
            let targets: Vec<Option<wgpu::ColorTargetState>> = key
                .colors
                .iter()
                .enumerate()
                .map(|(i, &format)| Some(wgpu::ColorTargetState { format, blend: None, write_mask: if i == 0 { write } else { wgpu::ColorWrites::empty() } }))
                .collect();
            let stencil_face = wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Always,
                fail_op: wgpu::StencilOperation::Replace,
                depth_fail_op: wgpu::StencilOperation::Replace,
                pass_op: wgpu::StencilOperation::Replace,
            };
            let depth_stencil = key.depth.map(|format| wgpu::DepthStencilState {
                format,
                depth_write_enabled: Some(clear.flags & state::CLEAR_DEPTH != 0),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: if format.has_stencil_aspect() && clear.flags & state::CLEAR_STENCIL != 0 {
                    wgpu::StencilState { front: stencil_face, back: stencil_face, read_mask: 0xff, write_mask: 0xff }
                } else {
                    wgpu::StencilState::default()
                },
                bias: Default::default(),
            });
            let p = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("clear"),
                layout: Some(&layout),
                vertex: wgpu::VertexState { module: &self.cache.clear_module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
                primitive: Default::default(),
                depth_stencil,
                multisample: wgpu::MultisampleState { count: key.samples, ..Default::default() },
                fragment: Some(wgpu::FragmentState {
                    module: &self.cache.clear_module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &targets,
                }),
                multiview_mask: None,
                cache: None,
            });
            self.cache.clear.insert(key.clone(), p);
        }
        let pipeline = self.cache.clear.get(&key)?.clone();
        let c = bgfx_color(clear.rgba);
        let mut data = [0u8; 32];
        for (chunk, v) in data.chunks_exact_mut(4).zip(c.iter().chain(&[clear.depth, 0.0, 0.0, 0.0])) {
            chunk.copy_from_slice(&v.to_le_bytes());
        }
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("clear"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&buffer, 0, &data);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("clear"),
            layout: &self.cache.clear_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() }],
        });
        Some((pipeline, bind_group))
    }
}
