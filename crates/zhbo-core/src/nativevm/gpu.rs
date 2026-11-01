//! The native viewmodel's renderer side (wgpu builds): upload the composed mesh and its textures
//! once per viewmodel, submit the posed palette each frame.

use zhbo_gfx::ffi::with_renderer;
use zhbo_gfx::runtime::{NativeBlip, NativeDraw, NativeFrame, NativeLight, NativeOptics, NativeSurface, NativeVertex, ShockDraw, ShockLayer, INVALID, NATIVE_MAX_BLIPS};

use super::ffi::{State, ZhboNativeIn, DRAWN, LOADING, NOT_NATIVE};
use super::{local_optics, native_surface, set_lens_reticle_drawn, Mesh, ART_SCALE};

/// The handles of the viewmodel on the GPU.
#[derive(Default)]
pub(crate) struct Gpu {
    key: Option<(i32, u8, u64, u32)>,
    mesh: Option<u16>,
    /// The ADS part list's mesh, submitted at full ADS. `None` when no part swaps.
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

/// Queue this frame's shellshock screen layer ([`super::local_shock`]) into the scene the viewmodel
/// draws in: a flashed shock grabs on the first frame of each new shock (by its serial) and adds
/// the grab and the white after it; a blurred one lays its saved screen over at its alpha.
pub(crate) fn shock(inp: &ZhboNativeIn) {
    static GRABBED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    use std::sync::atomic::Ordering::Relaxed;
    // Between shocks the grab is forgotten, so a new game's first shock (serials restart) grabs.
    let Some(s) = super::local_shock().filter(|s| s.flashed || s.blur_alpha > 0.0) else {
        GRABBED.store(0, Relaxed);
        return;
    };
    let layer = if s.flashed {
        if GRABBED.swap(s.serial, Relaxed) != s.serial {
            ShockLayer::Grab
        } else {
            ShockLayer::Flashed { white: s.white.clamp(0.0, 1.0), shot: s.shot.clamp(0.0, 1.0) }
        }
    } else {
        GRABBED.store(0, Relaxed);
        ShockLayer::Blurred { alpha: s.blur_alpha.clamp(0.0, 1.0) }
    };
    let draw = ShockDraw { target_view: inp.target_view, before_view: inp.before_view, layer };
    super::set_shock_layer_drawn(with_renderer(false, |r| r.shock_submit(&draw)));
}

/// Upload (once its textures are in) and submit. `LOADING` while textures are still coming.
pub(crate) fn draw(s: &mut State, inp: &ZhboNativeIn) -> i32 {
    let drawn = submit(s, inp);
    // The HUD's 2D optic reticle gives way only to a lens reticle that is actually drawing.
    let aimed = inp.ads >= super::OPTIC_VIEWMODEL_SWITCH;
    let lens = drawn == DRAWN && s.loaded.as_ref().is_some_and(|l| l.drawn_mesh(aimed).surfaces.iter().any(|s| s.reticle.is_some()));
    set_lens_reticle_drawn(lens);
    drawn
}

fn submit(s: &mut State, inp: &ZhboNativeIn) -> i32 {
    let Some(loaded) = s.loaded.as_ref() else { return NOT_NATIVE };
    if s.gpu.key != Some(loaded.key) {
        // Every texture the surfaces of both meshes name (display-space shading, native.wgsl:
        // every map uploads raw): decoded once in, absent ones draw white/flat.
        let paths: Vec<(String, bool)> = loaded.texture_paths().into_iter().map(|p| (p, false)).collect();
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
                    .map(|s| native_surface(s, handle))
                    .collect();
                r.native_create_mesh(&vertices, &m.indices, &surfaces)
            };
            let m = upload(mesh);
            let aimed = loaded.mesh_aimed.as_ref().map(upload);
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
    // From BO2's optic switch (full ADS) the aimed mesh draws: each paired part shows its ADS
    // model. The draw swaps handles, not the blob.
    let aimed = inp.ads >= super::OPTIC_VIEWMODEL_SWITCH;
    let mesh = if aimed { s.gpu.mesh_aimed.or(s.gpu.mesh) } else { s.gpu.mesh };
    if aimed && s.gpu.mesh_aimed.is_some() {
        s.log_once("native viewmodel: the ADS part list draws at full ADS (ADS models)".into());
    }
    let Some(mesh) = mesh else { return NOT_NATIVE };
    let optics = local_optics();
    let mut lights = [NativeLight::default(); 3];
    for (l, i) in lights.iter_mut().zip(&inp.lights) {
        *l = NativeLight { direction: i.dir, color: i.color };
    }
    let frame = NativeFrame { view_proj: None, eye: inp.eye, ambient: inp.ambient, lights, light_count: inp.light_count.clamp(0, 3) as u32, debug: inp.debug,
        // BO2's fWeaponPosFrac drives the lens reticle's fade (`weaponParam0.w`); the mod has no
        // effect that knocks optics out (BO2's EMP, `weaponParam0.y`).
        ads: inp.ads, optics_off: 0.0,
        // The target finder's "enemy spotted" (`weaponParam0.z`, the sim's `optics::Optics::alt`)
        // and the MMS screen's characters.
        reticle_alt: optics.alt,
        optics: native_optics(&optics, inp.dir) };
    let palette = &s.posed.palette;
    let draw = NativeDraw { target_view: inp.target_view, before_view: inp.before_view, mesh, palette, frame };
    if with_renderer(false, |r| r.native_submit(&draw)) {
        DRAWN
    } else {
        NOT_NATIVE
    }
}

/// The MMS screen's inputs: the clock, the view's forward, BO2 inches per world unit (the art's
/// scale) and the sim's characters.
fn native_optics(o: &super::LocalOptics, dir: [f32; 3]) -> NativeOptics {
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt().max(1e-6);
    let mut out = NativeOptics { time: o.time, forward: dir.map(|c| c / len), inches_per_unit: 1.0 / ART_SCALE, blip_count: o.blips.len().min(NATIVE_MAX_BLIPS) as u32, ..Default::default() };
    for (slot, b) in out.blips.iter_mut().zip(&o.blips) {
        *slot = NativeBlip { pos: [b.pos.x, b.pos.y, b.pos.z], radius: b.radius, height: b.height, intensity: b.intensity };
    }
    out
}
