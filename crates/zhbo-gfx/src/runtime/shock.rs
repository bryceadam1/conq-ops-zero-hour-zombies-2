//! BO2's shellshock screen layers (t6mp's two backend commands, fed by the cgame's
//! `FUN_00510460` blurred and `FUN_005105c0` flashed; the math is `zhbo_core::shellshock`'s):
//!
//! * **blurred** — the `shellshock` material (`SRC_ALPHA, INV_SRC_ALPHA`, colour = texture,
//!   alpha = the vertex alpha): the screen saved last frame is laid over this frame's at `alpha`,
//!   and the result is saved again, so the image trails behind the view;
//! * **flashed** — on the shock's first frame the frame is grabbed (no draw); after it, the
//!   `shellshock_flashed` material (`ONE, ONE`, `texture · vertex.a + vertex.rgb`): the grab at
//!   `shot` plus `white`, added over the view.
//!
//! One layer per frame (the last submitted wins), run with the native pass: immediately before its
//! `before_view` (the scene composite), into the `target_view`'s colour, so the HUD (the UI view)
//! stays on top. The saved screen is the target's own resolved colour, copied; it is rebuilt (and
//! the trail restarts) when the target's size or format changes. Bad input is refused, logged
//! once and counted in `stats().native_skipped`; nothing here panics in the frame path.

use super::{Renderer, INVALID, MAX_VIEWS};

/// What the shock draws this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShockLayer {
    /// Lay the saved screen over the view at `alpha` (0..1), then save the result.
    Blurred { alpha: f32 },
    /// Save the view as the grab (a flashed shock's first frame); nothing is drawn.
    Grab,
    /// Add the grab at `shot` and `white` (both 0..1) over the view.
    Flashed { white: f32, shot: f32 },
}

/// One frame's shock layer and where it runs (as `NativeDraw`'s views).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShockDraw {
    pub target_view: u16,
    pub before_view: u16,
    pub layer: ShockLayer,
}

const SHADER: &str = r#"
struct U { p: vec4<f32> };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var saved: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}
fn at(p: vec4<f32>) -> vec3<f32> {
    let size = vec2<i32>(textureDimensions(saved));
    return textureLoad(saved, clamp(vec2<i32>(p.xy), vec2<i32>(0), size - 1), 0).rgb;
}
// `shellshock`: the texture at the vertex alpha.
@fragment fn fs_blurred(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(at(p), u.p.y);
}
// `shellshock_flashed` (its DXBC: `o.rgb = t.rgb * v.a + v.rgb`, `o.a = 1`): white in rgb, shot in a.
@fragment fn fs_flashed(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(at(p) * u.p.w + vec3<f32>(u.p.z), 1.0);
}
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Key {
    format: wgpu::TextureFormat,
    samples: u32,
    flashed: bool,
}

struct Saved {
    texture: wgpu::Texture,
    group: wgpu::BindGroup,
    size: (u32, u32),
    format: wgpu::TextureFormat,
    view_format: wgpu::TextureFormat,
    /// Holds a frame (a first blurred frame or a missed grab saves before it draws).
    valid: bool,
    /// The frame number it was last saved in: a blurred trail continues only from the frame
    /// before (a gap — the shock ended, another began — restarts it).
    frame: u32,
}

#[derive(Default)]
pub(crate) struct ShockState {
    queued: Option<ShockDraw>,
    module: Option<wgpu::ShaderModule>,
    bgl: Option<wgpu::BindGroupLayout>,
    layout: Option<wgpu::PipelineLayout>,
    uniform: Option<wgpu::Buffer>,
    pipelines: std::collections::HashMap<Key, Option<wgpu::RenderPipeline>>,
    saved: Option<Saved>,
    /// Problems already logged (each once).
    reported: std::collections::HashSet<String>,
    /// Layers drawn (grabs included), for tests and stats.
    pub(crate) drawn: u32,
}

fn finite01(v: f32) -> bool {
    v.is_finite() && (0.0..=1.0).contains(&v)
}

impl Renderer {
    /// Queue this frame's shock layer. `false` (logged once, counted as skipped) for a view out of
    /// range or a value that is not a finite 0..1.
    pub fn shock_submit(&mut self, d: &ShockDraw) -> bool {
        let ok_views = (d.target_view as usize) < MAX_VIEWS && (d.before_view as usize) < MAX_VIEWS && d.target_view != INVALID;
        let ok_values = match d.layer {
            ShockLayer::Blurred { alpha } => finite01(alpha),
            ShockLayer::Grab => true,
            ShockLayer::Flashed { white, shot } => finite01(white) && finite01(shot),
        };
        if !(ok_views && ok_values) {
            self.native.skipped += 1;
            self.shock_report(format!("zhbo-gfx: shock layer refused: {d:?}"));
            return false;
        }
        self.shock.queued = Some(*d);
        true
    }

    fn shock_report(&mut self, msg: String) {
        if self.shock.reported.insert(msg.clone()) {
            self.report(msg);
        }
    }

    /// Run the queued layer if it is due before view `before` (`None`: at the end of the frame).
    pub(crate) fn shock_encode(&mut self, encoder: &mut wgpu::CommandEncoder, before: Option<u16>) {
        let Some(d) = self.shock.queued else { return };
        if before.is_some_and(|b| b != d.before_view) {
            return;
        }
        self.shock.queued = None;
        let view = self.views[d.target_view as usize].clone();
        let (Some(target), Some(source)) = (self.target_for(view.frame_buffer), self.color_source(view.frame_buffer)) else {
            self.native.skipped += 1;
            self.shock_report(format!("zhbo-gfx: shock layer: view {} has no colour target", d.target_view));
            return;
        };
        if !source.usage().contains(wgpu::TextureUsages::COPY_SRC) || source.sample_count() != 1 {
            self.native.skipped += 1;
            self.shock_report(format!("zhbo-gfx: shock layer: view {}'s colour cannot be copied", d.target_view));
            return;
        }
        let (Some(&color_format), Some((color, resolve))) = (target.color_formats.first(), target.colors.first()) else { return };
        let (rx, ry, rw, rh) = view.rect;
        let x = (rx as u32).min(target.width);
        let y = (ry as u32).min(target.height);
        let (w, h) = ((rw as u32).min(target.width - x), (rh as u32).min(target.height - y));
        if w == 0 || h == 0 {
            return;
        }
        if !self.shock_saved(&source, color_format) {
            return;
        }
        let (save, draw) = match d.layer {
            ShockLayer::Grab => (true, None),
            ShockLayer::Blurred { alpha } => (true, Some((false, [0.0, alpha, 0.0, 0.0]))),
            ShockLayer::Flashed { white, shot } => (false, Some((true, [1.0, 0.0, white, shot]))),
        };
        let continuing = |s: &Saved| matches!(d.layer, ShockLayer::Flashed { .. } | ShockLayer::Grab) || s.frame == self.frame_number.wrapping_sub(1);
        let valid = self.shock.saved.as_ref().is_some_and(|s| s.valid && continuing(s));
        // Nothing to lay over yet: take this frame first (a blurred shock's first frame, or a
        // flash whose grab frame was missed).
        if !valid {
            self.shock_copy(encoder, &source);
        }
        if let Some((flashed, params)) = draw.filter(|_| valid || matches!(d.layer, ShockLayer::Flashed { .. })) {
            let Some(pipeline) = self.shock_pipeline(Key { format: color_format, samples: target.samples, flashed }) else {
                self.native.skipped += 1;
                return;
            };
            let (Some(uniform), Some(saved)) = (self.shock.uniform.as_ref(), self.shock.saved.as_ref()) else { return };
            let mut bytes = [0u8; 16];
            for (b, v) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(params) {
                b.copy_from_slice(&v.to_le_bytes());
            }
            self.queue.write_buffer(uniform, 0, &bytes);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shock"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: resolve.as_ref(),
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
            pass.set_scissor_rect(x, y, w, h);
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &saved.group, &[]);
            pass.draw(0..3, 0..1);
        }
        if save && valid {
            self.shock_copy(encoder, &source);
        }
        self.shock.drawn += 1;
    }

    /// Copy the target's resolved colour into the saved screen.
    fn shock_copy(&mut self, encoder: &mut wgpu::CommandEncoder, source: &wgpu::Texture) {
        let frame = self.frame_number;
        let Some(saved) = self.shock.saved.as_mut() else { return };
        encoder.copy_texture_to_texture(
            source.as_image_copy(),
            saved.texture.as_image_copy(),
            wgpu::Extent3d { width: saved.size.0, height: saved.size.1, depth_or_array_layers: 1 },
        );
        saved.valid = true;
        saved.frame = frame;
    }

    /// The saved screen for `source` viewed as `color_format`, (re)built when either changes.
    /// `false` when it cannot be (logged).
    fn shock_saved(&mut self, source: &wgpu::Texture, color_format: wgpu::TextureFormat) -> bool {
        let size = (source.width(), source.height());
        let format = source.format();
        if format.remove_srgb_suffix() != color_format.remove_srgb_suffix() {
            self.native.skipped += 1;
            self.shock_report(format!("zhbo-gfx: shock layer: target format {color_format:?} is not its texture's {format:?}"));
            return false;
        }
        // Read back as the target is written: the same sRGB-ness (a view format when they differ).
        let view_format = if color_format == format || self.view_formats_supported { color_format } else { format };
        if self.shock.saved.as_ref().is_some_and(|s| s.size == size && s.format == format && s.view_format == view_format) {
            return true;
        }
        self.shock_init();
        let (Some(bgl), Some(uniform)) = (self.shock.bgl.clone(), self.shock.uniform.clone()) else { return false };
        let view_formats = [view_format];
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shock saved screen"),
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: if view_format != format { &view_formats } else { &[] },
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor { format: Some(view_format), ..Default::default() });
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shock"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
            ],
        });
        self.shock.saved = Some(Saved { texture, group, size, format, view_format, valid: false, frame: 0 });
        true
    }

    fn shock_init(&mut self) {
        if self.shock.module.is_some() {
            return;
        }
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("shock"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let bgl = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shock"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: wgpu::BufferSize::new(16) },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
            ],
        });
        let layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("shock"), bind_group_layouts: &[Some(&bgl)], immediate_size: 0 });
        let uniform = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("shock"), size: 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.shock.module = Some(module);
        self.shock.bgl = Some(bgl);
        self.shock.layout = Some(layout);
        self.shock.uniform = Some(uniform);
    }

    fn shock_pipeline(&mut self, key: Key) -> Option<wgpu::RenderPipeline> {
        if let Some(p) = self.shock.pipelines.get(&key) {
            return p.clone();
        }
        self.shock_init();
        let (Some(module), Some(layout)) = (self.shock.module.clone(), self.shock.layout.clone()) else { return None };
        let blend = if key.flashed {
            // `shellshock_flashed`'s state bits 0x19218922: ONE, ONE, add.
            let one = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add };
            wgpu::BlendState { color: one, alpha: wgpu::BlendComponent::OVER }
        } else {
            // `shellshock`'s state bits 0x19288165: SRC_ALPHA, INV_SRC_ALPHA, add.
            wgpu::BlendState::ALPHA_BLENDING
        };
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shock"),
            layout: Some(&layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState { count: key.samples, ..Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(if key.flashed { "fs_flashed" } else { "fs_blurred" }),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: key.format, blend: Some(blend), write_mask: wgpu::ColorWrites::COLOR })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let built = super::block_on(scope.pop()).is_none().then_some(pipeline);
        if built.is_none() {
            self.shock_report(format!("zhbo-gfx: shock layer pipeline {key:?} failed"));
        }
        self.shock.pipelines.insert(key, built.clone());
        built
    }

    /// Shock layers run since the renderer started (grabs included).
    pub fn shock_drawn(&self) -> u32 {
        self.shock.drawn
    }

    /// Forget an unrun layer (called at the end of `frame()`).
    pub(crate) fn shock_end_frame(&mut self) {
        self.shock.queued = None;
    }
}
