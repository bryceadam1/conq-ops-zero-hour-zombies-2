/*
** Conquer Ops - Zero Hour renderer: resource creation and destruction.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! The `Resources` seam of [`Renderer`]: shaders and programs, uniforms, vertex layouts,
//! static/dynamic/transient buffers, textures and frame buffers (`create_*`, `update_*`, `destroy`).

use super::*;

impl Renderer {
    // --- shaders and programs --------------------------------------------------------------

    pub fn create_shader(&mut self, data: &[u8]) -> u16 {
        let blob = match ShaderBlob::decode(data) {
            Ok(b) => b,
            Err(e) => {
                self.log(&format!("zhbo-gfx: createShader: {e}"));
                return INVALID;
            }
        };
        // Bad WGSL is a creation-time validation error; catch it here so the failure is named at
        // the shader that caused it instead of at every pipeline built from it.
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(blob.wgsl.as_str().into()),
        });
        if let Some(e) = block_on(scope.pop()) {
            self.report(format!("zhbo-gfx: createShader: WGSL failed to compile: {e}"));
            return INVALID;
        }
        let h = self.res.shaders.insert(Arc::new(Shader { blob, module }));
        if h == INVALID {
            self.report("zhbo-gfx: createShader: resource pool exhausted".to_string());
        }
        h
    }

    pub fn destroy_shader(&mut self, h: u16) {
        self.res.shaders.remove(h);
    }

    pub fn create_program(&mut self, vs: u16, fs: u16, destroy_shaders: bool) -> u16 {
        let Some(vsh) = self.res.shaders.get(vs).cloned() else { return INVALID };
        let fsh = self.res.shaders.get(fs).cloned();
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
            self.res.shaders.remove(vs);
            self.res.shaders.remove(fs);
        }
        let h = self.res.programs.insert(Arc::new(program));
        if h == INVALID {
            self.report("zhbo-gfx: createProgram: resource pool exhausted".to_string());
        }
        h
    }

    pub fn destroy_program(&mut self, h: u16) {
        self.res.programs.remove(h);
    }

    // --- uniforms --------------------------------------------------------------------------

    /// `bgfx::UniformType`: 0 Sampler, 2 Vec4, 3 Mat3, 4 Mat4.
    pub fn create_uniform(&mut self, name: &str, ty: u8, num: u16) -> u16 {
        if let Some(&h) = self.res.uniform_names.get(name) {
            if let Some(u) = self.res.uniforms.get_mut(h) {
                u.refs += 1;
                return h;
            }
        }
        let floats = match ty {
            3 => 12,
            4 => 16,
            _ => 4,
        } * num.max(1) as usize;
        let h = self.res.uniforms.insert(Uniform { name: name.to_string(), refs: 1, value: vec![0.0; floats] });
        if h == INVALID {
            self.report("zhbo-gfx: createUniform: resource pool exhausted".into());
        }
        if h != INVALID {
            self.res.uniform_names.insert(name.to_string(), h);
        }
        h
    }

    pub fn destroy_uniform(&mut self, h: u16) {
        let Some(u) = self.res.uniforms.get_mut(h) else { return };
        u.refs -= 1;
        if u.refs == 0 {
            let u = self.res.uniforms.remove(h).unwrap();
            self.res.uniform_names.remove(&u.name);
        }
    }

    /// Set `num` elements (vec4 or mat4, as created) of a uniform. Values are sticky.
    pub fn set_uniform(&mut self, h: u16, value: &[f32]) {
        if let Some(u) = self.res.uniforms.get_mut(h) {
            let n = value.len().min(u.value.len());
            u.value[..n].copy_from_slice(&value[..n]);
        }
    }

    // --- vertex layouts and buffers --------------------------------------------------------

    pub fn register_layout(&mut self, layout: VertexLayout) -> u16 {
        if let Some(&id) = self.res.layout_ids.get(&layout) {
            return id;
        }
        let id = self.res.layouts.len() as u16;
        self.res.layouts.push(layout.clone());
        self.res.layout_ids.insert(layout, id);
        id
    }

    pub(crate) fn layout(&self, id: u16) -> Option<&VertexLayout> {
        self.res.layouts.get(id as usize)
    }

    pub(super) fn make_buffer(&self, size: u32, data: Option<&[u8]>, usage: wgpu::BufferUsages) -> wgpu::Buffer {
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
    pub(super) fn write_padded(&self, buffer: &wgpu::Buffer, offset: u64, data: &[u8]) {
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
        let h = self.res.vertex_buffers.insert(Buffer {
            buffer,
            size: data.len() as u32,
            layout,
            index32: false,
            shadow: Vec::new(),
        });
        if h == INVALID {
            self.report("zhbo-gfx: createVertexBuffer: resource pool exhausted".to_string());
        }
        h
    }

    pub fn create_index_buffer(&mut self, data: &[u8], flags: u16) -> u16 {
        let buffer = self.make_buffer(data.len() as u32, Some(data), wgpu::BufferUsages::INDEX);
        let h = self.res.index_buffers.insert(Buffer {
            buffer,
            size: data.len() as u32,
            layout: INVALID,
            index32: flags & BUFFER_INDEX32 != 0,
            shadow: Vec::new(),
        });
        if h == INVALID {
            self.report("zhbo-gfx: createIndexBuffer: resource pool exhausted".to_string());
        }
        h
    }

    pub fn create_dynamic_vertex_buffer(&mut self, num: u32, data: Option<&[u8]>, layout: VertexLayout) -> u16 {
        let size = data.map(|d| d.len() as u32).unwrap_or(num * layout.stride as u32);
        let layout = self.register_layout(layout);
        let buffer = self.make_buffer(size, data, wgpu::BufferUsages::VERTEX);
        let shadow = shadow_copy(size, data);
        let h = self.res.dynamic_vertex_buffers.insert(Buffer { buffer, size, layout, index32: false, shadow });
        if h == INVALID {
            self.report("zhbo-gfx: createDynamicVertexBuffer: resource pool exhausted".to_string());
        }
        h
    }

    pub fn create_dynamic_index_buffer(&mut self, num: u32, data: Option<&[u8]>, flags: u16) -> u16 {
        let index32 = flags & BUFFER_INDEX32 != 0;
        let size = data.map(|d| d.len() as u32).unwrap_or(num * if index32 { 4 } else { 2 });
        let buffer = self.make_buffer(size, data, wgpu::BufferUsages::INDEX);
        let shadow = shadow_copy(size, data);
        let h = self.res.dynamic_index_buffers.insert(Buffer { buffer, size, layout: INVALID, index32, shadow });
        if h == INVALID {
            self.report("zhbo-gfx: createDynamicIndexBuffer: resource pool exhausted".to_string());
        }
        h
    }

    pub fn update_dynamic_vertex_buffer(&mut self, h: u16, start_vertex: u32, data: &[u8]) {
        let Some(b) = self.res.dynamic_vertex_buffers.get(h) else { return };
        let stride = self.layout(b.layout).map(|l| l.stride as usize).unwrap_or(1);
        self.update_dynamic(false, h, start_vertex as usize * stride, data);
    }

    pub fn update_dynamic_index_buffer(&mut self, h: u16, start_index: u32, data: &[u8]) {
        let Some(b) = self.res.dynamic_index_buffers.get(h) else { return };
        let isize = if b.index32 { 4 } else { 2 };
        self.update_dynamic(true, h, start_index as usize * isize, data);
    }

    /// Write `data` at byte `offset` of a dynamic buffer. bgfx allows any element offset, but
    /// `queue.write_buffer` needs 4-byte alignment, so the CPU shadow copy is updated and the
    /// enclosing aligned range uploaded. A write past the end grows the buffer (bgfx's
    /// ALLOW_RESIZE), keeping its contents.
    pub(super) fn update_dynamic(&mut self, index: bool, h: u16, offset: usize, data: &[u8]) {
        let usage = if index { wgpu::BufferUsages::INDEX } else { wgpu::BufferUsages::VERTEX };
        let end = offset + data.len();
        let pool = if index { &mut self.res.dynamic_index_buffers } else { &mut self.res.dynamic_vertex_buffers };
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
            ResourceKind::VertexBuffer => drop(self.res.vertex_buffers.remove(h)),
            ResourceKind::IndexBuffer => drop(self.res.index_buffers.remove(h)),
            ResourceKind::DynamicVertexBuffer => drop(self.res.dynamic_vertex_buffers.remove(h)),
            ResourceKind::DynamicIndexBuffer => drop(self.res.dynamic_index_buffers.remove(h)),
            ResourceKind::Shader => self.destroy_shader(h),
            ResourceKind::Program => self.destroy_program(h),
            ResourceKind::Texture => drop(self.res.textures.remove(h)),
            ResourceKind::FrameBuffer => {
                if let Some(fb) = self.res.frame_buffers.remove(h) {
                    for t in fb.owned {
                        self.res.textures.remove(t);
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
        // A multisampled depth is bound as a texture only by the native pass's MMS screen (its
        // scene-depth resolve), where the device allows it.
        let depth_msaa_usage = if depth_msaa && self.msaa_depth_sampleable(base_format, samples) {
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING
        } else {
            wgpu::TextureUsages::RENDER_ATTACHMENT
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width, height, depth_or_array_layers: layers },
            mip_level_count: if depth_msaa { 1 } else { mips },
            sample_count: if depth_msaa { samples } else { 1 },
            dimension: wgpu::TextureDimension::D2,
            format: base_format,
            usage: if depth_msaa { depth_msaa_usage } else { usage },
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
        let h = self.res.textures.insert(Arc::new(tex));
        if h == INVALID {
            self.report("zhbo-gfx: createTexture2D: resource pool exhausted".to_string());
        }
        h
    }

    /// Whether `format` can be a render target on this device. Format tables and downlevel
    /// rules differ per backend (R32F is not renderable on GL ES-class devices), so ask the device:
    /// create a tiny target inside a validation error scope, once per format.
    /// Whether a multisampled depth texture of `format` can also be bound for sampling (probed once
    /// per format and count inside an error scope, like [`Self::renderable`]).
    pub(super) fn msaa_depth_sampleable(&mut self, format: wgpu::TextureFormat, samples: u32) -> bool {
        if let Some(&ok) = self.res.msaa_depth_sampleable.get(&(format, samples)) {
            return ok;
        }
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let _probe = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("msaa depth probe"),
            size: wgpu::Extent3d { width: 4, height: 4, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let ok = block_on(scope.pop()).is_none();
        self.res.msaa_depth_sampleable.insert((format, samples), ok);
        ok
    }

    pub(super) fn renderable(&mut self, format: wgpu::TextureFormat) -> bool {
        if let Some(&ok) = self.res.renderable.get(&format) {
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
        self.res.renderable.insert(format, ok);
        ok
    }

    pub(super) fn supported_samples(&self, format: wgpu::TextureFormat, wanted: u32) -> u32 {
        frame::supported_samples_for(&self.adapter, format, wanted)
    }

    /// Upload every layer and mip from bgfx's layout (layer-major, then mips).
    pub(super) fn upload_all(&self, tex: &Texture, data: &[u8]) {
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
    pub(super) fn write_region(&self, tex: &Texture, layer: u32, mip: u32, x: u32, y: u32, w: u32, h: u32, data: &[u8], pitch: Option<u32>) {
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
                let needed = pitch as u64 * (bh as u64 - 1) + tight as u64;
                if (data.len() as u64) < needed {
                    self.log(&format!(
                        "zhbo-gfx: texture upload short: texture {} layer {layer} mip {mip} {w}x{h}: needs {needed} bytes, got {}",
                        tex.serial,
                        data.len()
                    ));
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
                let needed = pitch as u64 * (h as u64 - 1) + tight as u64;
                if (data.len() as u64) < needed {
                    self.log(&format!(
                        "zhbo-gfx: texture upload short: texture {} layer {layer} mip {mip} {w}x{h}: needs {needed} bytes, got {}",
                        tex.serial,
                        data.len()
                    ));
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
        let Some(tex) = self.res.textures.get(h).cloned() else { return };
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
            let Some(tex) = self.res.textures.get(t).cloned() else { return INVALID };
            if tex.is_depth() {
                depth = Some(tex);
            } else {
                colors.push(tex);
            }
        }
        let Some(first) = colors.first().or(depth.as_ref()) else { return INVALID };
        let (width, height) = (first.width, first.height);
        let owned = if destroy_textures { textures.to_vec() } else { Vec::new() };
        let h = self.res.frame_buffers.insert(FrameBuffer { colors, depth, width, height, owned });
        if h == INVALID {
            self.report("zhbo-gfx: createFrameBuffer: resource pool exhausted".to_string());
        }
        h
    }

    pub fn create_frame_buffer_sized(&mut self, width: u32, height: u32, bgfx_format: u32, flags: u64) -> u16 {
        let t = self.create_texture_2d(width, height, false, 1, bgfx_format, flags | TEXTURE_RT, None);
        if t == INVALID {
            return INVALID;
        }
        self.create_frame_buffer(&[t], true)
    }
}
