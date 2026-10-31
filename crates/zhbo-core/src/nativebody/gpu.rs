//! The native third person's renderer side (wgpu builds): upload the composed rig once and submit
//! its posed palette each frame. The same channels as the viewmodel's `nativevm::gpu`, but the
//! draw's frame carries the world camera's view/projection (`view_proj = None`) instead of the
//! eye's own.

use zhbo_gfx::ffi::with_renderer;
use zhbo_gfx::runtime::{NativeDraw, NativeFrame, NativeLight, NativeSurface, NativeVertex, INVALID};

use super::ffi::{Rig, State, ZhboNativeBodyIn, DRAWN, LOADING, NOT_NATIVE};
use crate::nativevm::{native_blend, native_cull};

/// The rig's handles on the GPU.
#[derive(Default)]
pub(crate) struct Gpu {
    key: Option<(u8, i32, u8)>,
    mesh: Option<u16>,
    textures: Vec<u16>,
}

impl Gpu {
    pub(crate) fn release(&mut self) {
        let (mesh, textures) = (self.mesh.take(), std::mem::take(&mut self.textures));
        self.key = None;
        with_renderer((), |r| {
            if let Some(m) = mesh {
                r.native_destroy_mesh(m);
            }
            for t in textures {
                r.native_destroy_texture(t);
            }
        });
    }
}

/// Upload (once its textures are in) and submit. `LOADING` while textures are still coming.
pub(crate) fn draw(s: &mut State, rig: &mut Rig, inp: &ZhboNativeBodyIn) -> i32 {
    let loaded = &rig.loaded;
    if rig.gpu.key != Some(loaded.key) {
        // Every texture the surfaces name: decoded once in, absent ones draw white/flat. Display
        // space shading (native.wgsl): every map uploads raw.
        let mut paths: Vec<(String, bool)> = Vec::new();
        for surf in &loaded.mesh.surfaces {
            for p in [&surf.color, &surf.maps].into_iter().flatten() {
                if !paths.iter().any(|(q, _)| q == p) {
                    paths.push((p.clone(), false));
                }
            }
        }
        let mut images = Vec::with_capacity(paths.len());
        let mut waiting = false;
        let mut problems = Vec::new();
        for (p, srgb) in &paths {
            if s.files.is_missing(p) {
                images.push((p.clone(), None));
                continue;
            }
            match s.files.get(p) {
                Some(bytes) => match zhbo_native::tga::decode(&bytes) {
                    Ok(img) => images.push((p.clone(), Some((img, *srgb)))),
                    Err(e) => {
                        problems.push(format!("{p}: {e}"));
                        images.push((p.clone(), None));
                    }
                },
                None => waiting = true,
            }
        }
        if waiting {
            return LOADING;
        }
        let mesh = &loaded.mesh;
        let key = loaded.key;
        let uploaded = with_renderer(None, |r| {
            let handles: Vec<(String, u16)> = images
                .iter()
                .map(|(p, img)| (p.clone(), img.as_ref().map_or(INVALID, |(i, srgb)| r.native_create_texture(i.width, i.height, &i.rgba, *srgb))))
                .collect();
            let handle = |p: &Option<String>| p.as_ref().and_then(|p| handles.iter().find(|(q, _)| q == p)).map_or(INVALID, |(_, h)| *h);
            let vertices: Vec<NativeVertex> = mesh.vertices.iter().map(|v| NativeVertex { position: v.position, normal: v.normal, uv: v.uv, bones: v.bones, weights: v.weights }).collect();
            let surfaces: Vec<NativeSurface> = mesh
                .surfaces
                .iter()
                .map(|s| NativeSurface { first_index: s.first_index, index_count: s.index_count, blend: native_blend(s.blend), emissive: s.emissive, color: handle(&s.color), maps: handle(&s.maps), camo: INVALID, camo_uv: [0.0, 0.0, 1.0, 1.0], sort_key: s.sort_key, cull: native_cull(s.cull) })
                .collect();
            let m = r.native_create_mesh(&vertices, &mesh.indices, &surfaces);
            Some((m, handles.into_iter().map(|(_, h)| h).filter(|&h| h != INVALID).collect::<Vec<_>>()))
        });
        for (p, _) in &paths {
            s.files.forget(p);
        }
        for p in problems {
            s.log_once(p);
        }
        match uploaded {
            Some((m, textures)) if m != INVALID => {
                rig.gpu = Gpu { key: Some(key), mesh: Some(m), textures };
            }
            Some((_, textures)) => {
                with_renderer((), |r| textures.into_iter().for_each(|t| r.native_destroy_texture(t)));
                s.log_once(format!("ZW{:02}{}: the renderer refused the mesh (see zhbo-gfx.log)", key.1, key.0 as char));
                return NOT_NATIVE;
            }
            None => return NOT_NATIVE,
        }
    }
    let Some(mesh) = rig.gpu.mesh else { return NOT_NATIVE };
    let mut lights = [NativeLight::default(); 3];
    for (l, i) in lights.iter_mut().zip(&inp.lights) {
        *l = NativeLight { direction: i.dir, color: i.color };
    }
    let frame = NativeFrame { view_proj: None, eye: inp.eye, ambient: inp.ambient, lights, light_count: inp.light_count.clamp(0, 3) as u32, debug: inp.debug };
    let palette = &s.posed.palette;
    let draw = NativeDraw { target_view: inp.target_view, before_view: inp.before_view, mesh, palette, frame };
    if with_renderer(false, |r| r.native_submit(&draw)) {
        DRAWN
    } else {
        NOT_NATIVE
    }
}
