//! The native viewmodel's renderer side (wgpu builds): upload the composed mesh and its textures
//! once per viewmodel, submit the posed palette each frame.

use zhbo_gfx::ffi::with_renderer;
use zhbo_gfx::runtime::{NativeDraw, NativeFrame, NativeLight, NativeSurface, NativeVertex, INVALID};

use super::ffi::{State, ZhboNativeIn, DRAWN, LOADING, NOT_NATIVE};
use super::{native_blend, native_cull, Mesh};

/// The handles of the viewmodel on the GPU.
#[derive(Default)]
pub(crate) struct Gpu {
    key: Option<(i32, u8)>,
    mesh: Option<u16>,
    /// The same mesh without the base optic, submitted at full ADS. `None` when the weapon has no
    /// optic part.
    mesh_aimed: Option<u16>,
    textures: Vec<u16>,
}

impl Gpu {
    pub(crate) fn release(&mut self) {
        let (mesh, mesh_aimed, textures) = (self.mesh.take(), self.mesh_aimed.take(), std::mem::take(&mut self.textures));
        self.key = None;
        with_renderer((), |r| {
            if let Some(m) = mesh {
                r.native_destroy_mesh(m);
            }
            if let Some(m) = mesh_aimed {
                r.native_destroy_mesh(m);
            }
            for t in textures {
                r.native_destroy_texture(t);
            }
        });
    }
}

/// Upload (once its textures are in) and submit. `LOADING` while textures are still coming.
pub(crate) fn draw(s: &mut State, inp: &ZhboNativeIn) -> i32 {
    let Some(loaded) = s.loaded.as_ref() else { return NOT_NATIVE };
    if s.gpu.key != Some(loaded.key) {
        // Every texture the surfaces name: decoded once in, absent ones draw white/flat.
        let mut paths: Vec<(String, bool)> = Vec::new();
        for surf in &loaded.mesh.surfaces {
            // Display-space shading (native.wgsl): every map uploads raw.
            for (p, srgb) in [(&surf.color, false), (&surf.maps, false)] {
                if let Some(p) = p {
                    if !paths.iter().any(|(q, _)| q == p) {
                        paths.push((p.clone(), srgb));
                    }
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
            // Both meshes share the texture handles; only the optic's surfaces differ.
            let mut upload = |m: &Mesh| {
                let vertices: Vec<NativeVertex> = m.vertices.iter().map(|v| NativeVertex { position: v.position, normal: v.normal, uv: v.uv, bones: v.bones, weights: v.weights }).collect();
                let surfaces: Vec<NativeSurface> = m
                    .surfaces
                    .iter()
                    .map(|s| NativeSurface { first_index: s.first_index, index_count: s.index_count, blend: native_blend(s.blend), emissive: s.emissive, color: handle(&s.color), maps: handle(&s.maps), sort_key: s.sort_key, cull: native_cull(s.cull) })
                    .collect();
                r.native_create_mesh(&vertices, &m.indices, &surfaces)
            };
            let m = upload(mesh);
            let aimed = loaded.mesh_aimed.as_ref().map(|m| upload(m));
            Some((m, aimed, handles.into_iter().map(|(_, h)| h).filter(|&h| h != INVALID).collect::<Vec<_>>()))
        });
        for (p, _) in &paths {
            s.files.forget(p);
        }
        for p in problems {
            s.log_once(p);
        }
        match uploaded {
            Some((m, aimed, textures)) if m != INVALID => {
                s.gpu = Gpu { key: Some(key), mesh: Some(m), mesh_aimed: aimed, textures };
            }
            Some((_, _, textures)) => {
                with_renderer((), |r| textures.into_iter().for_each(|t| r.native_destroy_texture(t)));
                s.log_once(format!("ZV{:02}{}: the renderer refused the mesh (see zhbo-gfx.log)", key.0, key.1 as char));
                return NOT_NATIVE;
            }
            None => return NOT_NATIVE,
        }
    }
    // At full ADS the base optic is hidden and the screen-space overlay takes over; the aimed
    // mesh carries the gun and magazine only. The draw swaps handles, not the blob.
    let mesh = if inp.ads >= 1.0 { s.gpu.mesh_aimed.or(s.gpu.mesh) } else { s.gpu.mesh };
    if inp.ads >= 1.0 && s.gpu.mesh_aimed.is_some() {
        s.log_once("native viewmodel: base optic hidden at full ADS; the screen overlay takes over".into());
    }
    let Some(mesh) = mesh else { return NOT_NATIVE };
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
