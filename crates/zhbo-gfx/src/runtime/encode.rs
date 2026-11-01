/*
** Conquer Ops - Zero Hour renderer: the per-draw API.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! The `FrameState` seam of [`Renderer`]: view setup, draw state (render state, stencil,
//! transforms, buffers, textures), `submit`, `discard` and `blit` — what the engine sets between frames.

use super::*;

impl Renderer {
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
                let pool = if kind == BufferKind::Static { &self.res.index_buffers } else { &self.res.dynamic_index_buffers };
                let Some(b) = pool.get(h) else {
                    self.current.index = None;
                    return;
                };
                (BufferSrc::Gpu(b.buffer.clone()), b.index32, b.size)
            }
            // "All remaining" (num == u32::MAX) counts from `first` to the end of what the engine
            // has allocated in the arena so far, not to the end of the (much larger) GPU buffer.
            BufferKind::Transient { index32 } => (BufferSrc::TransientIndex, index32, self.transient_ib.used as u32),
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
                let pool = if kind == BufferKind::Static { &self.res.vertex_buffers } else { &self.res.dynamic_vertex_buffers };
                let Some(b) = pool.get(h) else {
                    *slot = None;
                    return;
                };
                (BufferSrc::Gpu(b.buffer.clone()), b.layout, b.size)
            }
            // Transient: `h` is the layout id; "all remaining" counts vertices up to the arena's
            // used bytes (see `set_index_buffer`).
            BufferKind::Transient { .. } => (BufferSrc::TransientVertex, h, self.transient_vb.used as u32),
        };
        let layout = if layout_override != INVALID { layout_override } else { layout };
        let stride = self.res.layouts.get(layout as usize).map(|l| l.stride as u32).unwrap_or(0).max(1);
        let num = if num == u32::MAX { (size / stride).saturating_sub(start) } else { num };
        *slot = Some(StreamBinding { src, offset: start as u64 * stride as u64, layout, num_vertices: num });
    }

    pub fn set_instance_data(&mut self, kind: BufferKind, h: u16, start: u32, num: u32, stride: u32) {
        let src = match kind {
            BufferKind::Static => self.res.vertex_buffers.get(h).map(|b| BufferSrc::Gpu(b.buffer.clone())),
            BufferKind::Dynamic => self.res.dynamic_vertex_buffers.get(h).map(|b| BufferSrc::Gpu(b.buffer.clone())),
            BufferKind::Transient { .. } => Some(BufferSrc::TransientVertex),
        };
        let stride = if stride == 0 {
            match kind {
                BufferKind::Static => self.res.vertex_buffers.get(h),
                BufferKind::Dynamic => self.res.dynamic_vertex_buffers.get(h),
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
        *slot = self.res.textures.get(h).map(|t| TextureBinding {
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
        if let (Some(p), true) = (self.res.programs.get(program).cloned(), (v as usize) < MAX_VIEWS) {
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
    pub(super) fn write_user_uniforms(&mut self, u: &StageUniforms) -> u32 {
        let offset = self.uniform_ring.len().next_multiple_of(self.uniform_align as usize);
        self.uniform_ring.resize(offset + u.size as usize, 0);
        for (name, at, floats) in &u.user {
            let Some(uni) = self.res.uniform_names.get(name).and_then(|&h| self.res.uniforms.get(h)) else { continue };
            let n = (*floats as usize).min(uni.value.len());
            let dst = &mut self.uniform_ring[offset + *at as usize..offset + *at as usize + n * 4];
            for (chunk, v) in dst.as_chunks_mut::<4>().0.iter_mut().zip(&uni.value[..n]) {
                chunk.copy_from_slice(&v.to_le_bytes());
            }
        }
        offset as u32
    }

    #[allow(clippy::too_many_arguments)]
    pub fn blit(&mut self, v: u16, dst: u16, dst_mip: u8, dx: u16, dy: u16, dz: u16, src: u16, src_mip: u8, sx: u16, sy: u16, sz: u16, w: u16, h: u16, d: u16) {
        let (Some(dst_t), Some(src_t)) = (self.res.textures.get(dst).cloned(), self.res.textures.get(src).cloned()) else { return };
        let sw = mip_extent(src_t.width, src_mip);
        let sh = mip_extent(src_t.height, src_mip);
        let dw = mip_extent(dst_t.width, dst_mip);
        let dh = mip_extent(dst_t.height, dst_mip);
        // Clamp to both mips: a copy reaching past either is a wgpu validation error that would
        // take the whole frame's commands with it.
        let width = (w as u32).min(sw.saturating_sub(sx as u32)).min(dw.saturating_sub(dx as u32));
        let height = (h as u32).min(sh.saturating_sub(sy as u32)).min(dh.saturating_sub(dy as u32));
        let depth = (d as u32)
            .min(src_t.layers.saturating_sub(sz as u32))
            .min(dst_t.layers.saturating_sub(dz as u32));
        if width == 0 || height == 0 || depth == 0 {
            self.report("zhbo-gfx: skipped a blit: the clamped region or layer range is empty".into());
            return;
        }
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
}
