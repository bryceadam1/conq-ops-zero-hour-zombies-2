//! The native first-person viewmodel (Track R): the converter's BO2 blobs
//! (`zhbo_native`), composed, posed and drawn through the renderer's native BO2 pass instead of
//! the W3D conversion. The default since the native pass out-verified the W3D one
//! (`config::Config::native_viewmodel`: `ZHBO_NATIVE_VIEWMODEL=0` turns it off); the W3D path is
//! the fallback.
//!
//! Presentation only: nothing here feeds the sim. The bridge (`ZhboViewmodel.cpp`) calls
//! [`ffi::zhbo_native_viewmodel`] each frame with what it already gets for the W3D model (clip, time,
//! ADS fraction, the eye and the gun's offset and angles) plus the scene's lights; when this
//! returns "drawn" the bridge hides the W3D model.
//!
//! **Files.** The core cannot open the archive itself, so it asks: [`Files`] records each path it
//! needs, the bridge reads them (`TheFileSystem`) and hands the bytes back, and loading resumes the
//! next frame. A viewmodel draws once its manifest and models are in; clips and textures fill in
//! as they arrive (bind pose, white).
//!
//! **Placement.** The model's frame is the eye's (X along the view, Y left, Z up) times the gun's
//! view-space transform ([`GunPose`], built by the client's `viewmodel::Motion` from the weapon
//! def's gun-position numbers — `weapons::gunpos`, G4), with BO2's inches scaled to the art's
//! scale ([`place`]). **Aiming is the authored 0x55/0x56 ADS animation**, a `tag_torso`-only layer
//! over the action clip ([`Loaded::pose`]); the eye-frame marker slide ([`place::aligned`]) is the
//! fallback for a weapon with no ADS clip, or one whose clip has not loaded yet — and the W3D
//! path's rule. Projection is the scene's own (`NativeFrame::view_proj = None`): BO2 has no
//! viewmodel FOV (G4).

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use zhbo_native::anim::Anim;
use zhbo_native::math::{axis_angle, mat4, mat4_mul, qmul, Mat4, Xf};
use zhbo_native::model::{Blend, Cull, Model, Vertex};
use zhbo_native::pose::{PartRef, Skeleton};
use zhbo_native::viewmodel::Viewmodel;

#[cfg(feature = "renderer")]
mod gpu;
pub mod ffi;
pub mod place;

/// BO2 inches to the art's world units: [`zhbo_native::ART_SCALE`], the same factor as the sim's
/// [`crate::units::ZH_PER_INCH`] (one source; the converter's `character::SCALE` reads it too).
pub const ART_SCALE: f32 = zhbo_native::ART_SCALE;

/// The marker the converter places for aiming (`zhbo_assets::native::AIM_TAG`).
pub const AIM_TAG: &str = zhbo_native::AIM_TAG;

/// A surface's blend as the renderer's pipeline enum. Shared with the third person so the two
/// native paths cannot drift.
#[cfg(feature = "renderer")]
pub(crate) fn native_blend(b: Blend) -> zhbo_gfx::runtime::NativeBlend {
    use zhbo_gfx::runtime::NativeBlend;
    match b {
        Blend::Opaque => NativeBlend::Opaque,
        Blend::AlphaTest => NativeBlend::AlphaTest,
        Blend::Alpha => NativeBlend::Alpha,
        Blend::Premultiplied => NativeBlend::Premultiplied,
        Blend::Additive { alpha: false } => NativeBlend::Additive,
        Blend::Additive { alpha: true } => NativeBlend::AdditiveAlpha,
    }
}

/// A surface's cull mode as the renderer's pipeline enum.
#[cfg(feature = "renderer")]
pub(crate) fn native_cull(c: Cull) -> zhbo_gfx::runtime::NativeCull {
    use zhbo_gfx::runtime::NativeCull;
    match c {
        Cull::None => NativeCull::None,
        Cull::Back => NativeCull::Back,
        Cull::Front => NativeCull::Front,
    }
}

/// Files the core asked the bridge for, and what came back.
#[derive(Default)]
pub struct Files {
    data: HashMap<String, Arc<[u8]>>,
    missing: BTreeSet<String>,
    wanted: BTreeSet<String>,
}

impl Files {
    /// The file's bytes, or `None` (and a request) until the bridge provides it.
    pub fn get(&mut self, path: &str) -> Option<Arc<[u8]>> {
        if let Some(d) = self.data.get(path) {
            return Some(d.clone());
        }
        if !self.missing.contains(path) {
            self.wanted.insert(path.to_string());
        }
        None
    }
    pub fn is_missing(&self, path: &str) -> bool {
        self.missing.contains(path)
    }
    /// The bridge's answer: the bytes, or `None` when the archive has no such file.
    pub fn provide(&mut self, path: &str, bytes: Option<&[u8]>) {
        self.wanted.remove(path);
        match bytes {
            Some(b) => {
                self.data.insert(path.to_string(), Arc::from(b));
            }
            None => {
                self.missing.insert(path.to_string());
            }
        }
    }
    pub fn wanted(&self) -> impl Iterator<Item = &str> {
        self.wanted.iter().map(String::as_str)
    }
    /// Drop the cached bytes of files whose content now lives elsewhere (decoded, on the GPU).
    pub fn forget(&mut self, path: &str) {
        self.data.remove(path);
    }
}

/// The gun's transform in the eye's frame (X forward, Y left, Z up), in world units: where the
/// viewmodel's origin sits and how it is turned. The core's `viewmodel::Motion` fills it from the
/// weapon def's gun-position numbers (`weapons::gunpos`, G4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GunPose(pub Xf);

impl GunPose {
    /// The W3D path's transform from the core's offset and angles (radians: yaw about Z, pitch
    /// raising the muzzle, roll about X), `ZhboViewmodel.cpp` `Rotate_Z(yaw)`, `Rotate_Y(-pitch)`,
    /// `Rotate_X(roll)` applied in that order.
    pub fn from_offset_angles(offset: [f32; 3], angles: [f32; 3]) -> GunPose {
        let q = qmul(qmul(axis_angle([0.0, 0.0, 1.0], angles[0]), axis_angle([0.0, 1.0, 0.0], -angles[1])), axis_angle([1.0, 0.0, 0.0], angles[2]));
        GunPose((q, offset))
    }
}

/// One frame's request from the bridge.
#[derive(Clone, Copy, Debug)]
pub struct Request {
    /// Converted weapon index (`WeaponDef::viewmodel`) and the faction letter of the arms.
    pub vm: i32,
    pub faction: u8,
    /// Clip id (BO2 slot, alt mode's after `ALT_CLIPS`) and seconds into it.
    pub clip: i32,
    pub time: f32,
    /// Animation playback rate: a state that lasts `duration × multiplier` in BO2 (G3: perks and
    /// attachments scale weapon states) plays its clip at `1 / multiplier`.
    pub rate: f32,
    /// 0 hip .. 1 fully aimed.
    pub ads: f32,
    pub eye: [f32; 3],
    pub dir: [f32; 3],
    pub gun: GunPose,
}

/// A surface of the composed mesh, with its textures by archive path.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceSpec {
    pub first_index: u32,
    pub index_count: u32,
    pub blend: Blend,
    pub emissive: bool,
    pub color: Option<String>,
    pub maps: Option<String>,
    pub sort_key: u8,
    pub cull: Cull,
}

/// The composed viewmodel's one mesh: every part's LOD 0 with its bones renumbered into the
/// composed skeleton, so one palette skins it all.
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
    pub surfaces: Vec<SurfaceSpec>,
}

/// Concatenate the parts' LOD 0. `u16` indices: a composed viewmodel stays far below 65,536
/// vertices (an arms + rifle + optic composite is ~20k); a larger one is refused. Takes references
/// so a body rig can borrow its own models while the weapon's are owned beside them.
pub fn build_mesh<'a>(skel: &Skeleton, models: impl IntoIterator<Item = &'a Model>) -> Result<Mesh, String> {
    build_mesh_visible(skel, models, |_| true)
}

/// Which parts are drawn at `ads`: a base optic (a part whose model carries `tag_scope`/`tag_sights`)
/// is hidden at full ADS, when the screen-space overlay replaces it. The gun and magazine remain.
/// Only the first-person path uses this; the third-person `nativebody` keeps `build_mesh`'s
/// all-visible default. The hide also requires the overlay art to be present: a material the
/// install lacks cannot draw, so the physical scope stays rather than leaving neither sight.
pub fn visible_parts(models: &[Model], ads: f32, overlay_art_present: bool) -> Vec<bool> {
    models
        .iter()
        .map(|m| {
            let optic = m.bones.iter().any(|b| b.name == "tag_scope" || b.name == "tag_sights");
            !(optic && ads >= 1.0 && overlay_art_present)
        })
        .collect()
}

/// As [`build_mesh`], but skips the parts whose mask entry is false. The skeleton still holds every
/// part's bones (the pose and the aim marker keep working); only the hidden parts' surfaces drop.
/// A surface whose triangles are *all* hidden drops with them, as the W3D path drops an all-hidden
/// mesh (`zhbo_assets::character::skin_hiding`): the composed mesh never carries a zero-index
/// surface, because an empty surface is never a drawable surface — only "entirely hidden".
pub fn build_mesh_visible<'a>(
    skel: &Skeleton,
    models: impl IntoIterator<Item = &'a Model>,
    visible: impl Fn(usize) -> bool,
) -> Result<Mesh, String> {
    let mut mesh = Mesh::default();
    for (i, (part, model)) in skel.parts.iter().zip(models).enumerate() {
        if !visible(i) {
            continue;
        }
        for s in model.lod0() {
            let base = mesh.vertices.len();
            if base + s.vertices.len() > u16::MAX as usize {
                return Err(format!("{}: more than 65535 vertices composed", model.name));
            }
            // Hide tags are a triangle omission, as the W3D path does (`skin_hiding`): a hidden
            // bone's weights are dropped and the visible weights renormalized. The palette zeroes
            // a hidden bone, so a leftover weight would pull that vertex to the model origin and
            // tear the receiver under an optic.
            let visible = |bone: u16| !skel.bones.get(bone as usize).is_some_and(|b| b.hidden);
            mesh.vertices.extend(s.vertices.iter().map(|v| {
                let mut out = Vertex { bones: v.bones.map(|b| (b as usize + part.first) as u16), ..*v };
                let total: f32 = out.bones.iter().zip(v.weights.iter()).filter(|(b, _)| visible(**b)).map(|(_, w)| *w).sum();
                if total > 0.0 && total < 1.0 {
                    for (b, w) in out.bones.iter().zip(out.weights.iter_mut()) {
                        *w = if visible(*b) { *w / total } else { 0.0 };
                    }
                }
                out
            }));
            let first = mesh.indices.len() as u32;
            for tri in s.indices.as_chunks::<3>().0 {
                let hidden = tri.iter().any(|&i| {
                    let v = &s.vertices[i as usize];
                    let heaviest = v
                        .bones
                        .iter()
                        .zip(v.weights.iter())
                        .fold((0u16, 0.0f32), |a, w| if *w.1 > a.1 { (*w.0, *w.1) } else { a })
                        .0;
                    !visible(heaviest + part.first as u16)
                });
                if !hidden {
                    mesh.indices.extend(tri.iter().map(|&i| i + base as u16));
                }
            }
            // Every triangle hidden: the surface has nothing to draw, and BO2 has no empty surface.
            // Push no record for it, exactly as the W3D path drops an all-hidden mesh
            // (`character::skin_hiding`). The skipped surface's vertices stay in the mesh above:
            // nothing indexes them, and reordering the vertex push to drop them would need a second
            // triangle pass for no visible gain.
            let index_count = (mesh.indices.len() as u32) - first;
            if index_count == 0 {
                continue;
            }
            let m = model.materials.get(s.material as usize);
            mesh.surfaces.push(SurfaceSpec {
                first_index: first,
                index_count,
                blend: m.map_or(Blend::Opaque, |m| m.blend),
                emissive: m.is_some_and(|m| m.emissive),
                color: m.and_then(|m| m.color.clone()),
                maps: m.and_then(|m| m.maps.clone()),
                sort_key: m.map_or(0, |m| m.sort_key),
                cull: m.map_or(Cull::None, |m| m.cull),
            });
        }
    }
    Ok(mesh)
}

/// A composed viewmodel, ready to pose.
pub struct Loaded {
    pub key: (i32, u8),
    pub manifest: Viewmodel,
    pub skeleton: Skeleton,
    pub mesh: Mesh,
    /// The same mesh with the base optic's surfaces dropped: what the first-person draw submits at
    /// full ADS, when the 2D scope overlay replaces the physical scope. `None` when the weapon has
    /// no optic part (the gun and magazine are all there is).
    pub mesh_aimed: Option<Mesh>,
    pub aim: Option<usize>,
    pub anims: HashMap<String, Anim>,
}

/// Whether the weapon's screen-space ADS overlay art is in the archive. The HUD's image fallback
/// consults TheMappedImageCollection; the archive's `ZHBO_BO2.ini` is that collection's source.
/// A material absent from the INI cannot draw, so the base optic must stay. The INI request may
/// still be in flight: `waiting` is set and the caller retries next frame.
fn overlay_art_present(files: &mut Files, id: &str, override_material: Option<&str>, waiting: &mut bool) -> bool {
    let name = match override_material {
        Some(m) => crate::lui::materials::image(m),
        None => {
            let Some(name) = crate::weapons::catalog()
                .get(id)
                .filter(|d| !d.overlay_material.is_empty())
                .map(|d| crate::lui::materials::image(&d.overlay_material))
            else {
                return false;
            };
            name
        }
    };
    let ini = "Data\\INI\\MappedImages\\HandCreated\\ZHBO_BO2.ini";
    if files.is_missing(ini) {
        return false;
    }
    match files.get(ini) {
        Some(bytes) => String::from_utf8_lossy(&bytes)
            .lines()
            .any(|line| line.trim_start().strip_prefix("MappedImage ").is_some_and(|n| n.trim() == name)),
        None => {
            *waiting = true;
            false
        }
    }
}

/// Load step for `key`: `Ok(None)` while files are still coming, `Err` when the viewmodel can't be
/// built (logged once by the caller; the W3D path keeps drawing).
pub fn load(files: &mut Files, vm: i32, faction: u8, overlay_material_override: Option<&str>) -> Result<Option<Loaded>, String> {
    let path = zhbo_native::viewmodel_path(vm, faction as char);
    if files.is_missing(&path) {
        return Err(format!("{path} is not in the archive (reconvert the BO2 art)"));
    }
    let Some(bytes) = files.get(&path) else { return Ok(None) };
    let manifest = Viewmodel::from_bytes(&bytes).map_err(|e| format!("{path}: {e}"))?;
    let mut models = Vec::with_capacity(manifest.parts.len());
    let mut waiting = false;
    for p in &manifest.parts {
        if files.is_missing(&p.model) {
            return Err(format!("{path}: {} is not in the archive", p.model));
        }
        match files.get(&p.model) {
            Some(b) => models.push(Model::from_bytes(&b).map_err(|e| format!("{}: {e}", p.model))?),
            None => waiting = true,
        }
    }
    // The physical scope may only hand off to the screen-space overlay when the overlay art is in
    // the archive. The HUD's image fallback consults TheMappedImageCollection; the archive's
    // MappedImage INI is that collection's source. A material absent from the INI cannot draw, so
    // the base optic stays. The INI request may still be in flight: wait a frame.
    let overlay_art_present = overlay_art_present(files, &manifest.id, overlay_material_override, &mut waiting);
    if waiting {
        return Ok(None);
    }
    let parts: Vec<PartRef> = manifest.parts.iter().zip(&models).map(|(p, m)| PartRef { model: m, tag: p.tag.as_deref(), offset: p.offset, rigid: p.rigid }).collect();
    let (skeleton, problems) = Skeleton::compose(&parts, &manifest.markers, &manifest.hide);
    if !problems.is_empty() {
        return Err(format!("{path}: {}", problems.join("; ")));
    }
    let mesh = build_mesh(&skeleton, &models)?;
    // The aimed-half mesh: the base optic's surfaces are dropped at full ADS. Built once here so
    // the draw path only swaps handles. The mask's indices are the manifest's part order, which
    // the skeleton preserves.
    let aimed = visible_parts(&models, 1.0, overlay_art_present);
    let mesh_aimed = if aimed.iter().any(|v| !v) {
        Some(build_mesh_visible(&skeleton, &models, |i| aimed.get(i).copied().unwrap_or(true))?)
    } else {
        None
    };
    // The bytes are decoded now; keep the cache small.
    for p in &manifest.parts {
        files.forget(&p.model);
    }
    files.forget(&path);
    let aim = skeleton.bone(AIM_TAG);
    Ok(Some(Loaded { key: (vm, faction), manifest, skeleton, mesh, mesh_aimed, aim, anims: HashMap::new() }))
}

/// The eye's frame as a transform: X along `dir`, Y to the left, Z up, at `eye`.
pub fn eye_frame(eye: [f32; 3], dir: [f32; 3]) -> Mat4 {
    let norm = |v: [f32; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l > 1e-6 { v.map(|c| c / l) } else { [1.0, 0.0, 0.0] }
    };
    let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let x = norm(dir);
    let mut y = cross([0.0, 0.0, 1.0], x);
    if y.iter().map(|c| c * c).sum::<f32>() < 1e-6 {
        y = [0.0, 1.0, 0.0];
    }
    let y = norm(y);
    let z = cross(x, y);
    [x[0], x[1], x[2], 0.0, y[0], y[1], y[2], 0.0, z[0], z[1], z[2], 0.0, eye[0], eye[1], eye[2], 1.0]
}

/// Reusable pose buffers (the frame path allocates nothing once warm).
#[derive(Default)]
pub struct Posed {
    pub local: Vec<Xf>,
    pub model: Vec<Xf>,
    pub palette: Vec<Mat4>,
    /// Model to world: the eye's frame, the gun's pose, the art's scale — and the ADS slide only
    /// when the layer did not pose this frame (see [`Loaded::placement`]).
    pub root: Mat4,
}

impl Loaded {
    /// The clip to show for `clip`: its own, else the idle of its mode, else the idle (the W3D
    /// path's fallback, `ZhboViewmodel.cpp`). A dual-wield clip's id carries `INSTANCE_LEFT` in its
    /// high byte; the fallbacks keep that bit so the left gun falls back to its own idle.
    pub fn clip_path(&self, clip: i32) -> Option<&str> {
        use zhbo_native::viewmodel::INSTANCE_LEFT;
        let alt = crate::weapons::ALT_CLIPS;
        let instance = (clip as u16) & INSTANCE_LEFT;
        let base = (clip as u16) & !INSTANCE_LEFT;
        let mode_idle = if i32::from(base) >= alt { alt as u16 + crate::viewmodel::slot::IDLE as u16 } else { crate::viewmodel::slot::IDLE as u16 };
        [base, mode_idle, crate::viewmodel::slot::IDLE as u16].into_iter().find_map(|c| self.manifest.clip(c | instance))
    }

    /// The ADS layer's clip for `clip`'s mode: [`crate::viewmodel::slot::ADS_UP`] entering,
    /// [`crate::viewmodel::slot::ADS_DOWN`] leaving. Resolved from the manifest **directly**, never
    /// through [`Loaded::clip_path`]: that falls back to the mode's idle, and an idle keys every
    /// bone — overlaying it with `sample_over` would replace the action pose instead of layering
    /// on it. An alt mode's clips follow its
    /// weapon's by [`crate::weapons::ALT_CLIPS`], so its ADS clips are `ALT_CLIPS + 0x55/0x56`;
    /// a dual-wield pair's left instance has no ADS clip of its own (the base's track carries both
    /// guns), so the instance bit is masked, never added.
    pub fn ads_clip(&self, clip: i32, down: bool) -> Option<&str> {
        use crate::weapons::ALT_CLIPS;
        use zhbo_native::viewmodel::INSTANCE_LEFT;
        let base = (clip as u16) & !INSTANCE_LEFT;
        let mode = if i32::from(base) >= ALT_CLIPS { ALT_CLIPS } else { 0 };
        let slot = if down { crate::viewmodel::slot::ADS_DOWN } else { crate::viewmodel::slot::ADS_UP };
        self.manifest.clip((mode + slot) as u16)
    }

    /// Pose for `req`: the clip (if loaded) at its frame, then placement. `anim` is the base
    /// (right) clip; `left` is the dual-wield left instance's clip, sampled over it so the left
    /// arm, `tag_weapon1` and the `_lh` gun are driven by the `_left_*` set instead of the
    /// base clip (which never tracks them) leaving them at bind; `ads` is the ADS transition layer
    /// — its clip and frame — sampled over both, because it keys only `tag_torso`, the arms' rig
    /// root (BO2's second anim channel). Bones no clip names keep what the previous layer left them
    /// in (`zhbo_native::pose::sample_over`).
    pub fn pose(&self, req: &Request, anim: Option<&Anim>, left: Option<&Anim>, ads: Option<(&Anim, f32)>, out: &mut Posed) {
        let looping = crate::viewmodel::slot::loops(req.clip);
        let time = req.time * req.rate.max(0.0);
        match anim {
            Some(a) => {
                let frame = a.frame_at(time, looping);
                self.skeleton.sample(a, frame, &mut out.local);
            }
            None => {
                out.local.clear();
                out.local.extend(self.skeleton.bind_pose());
            }
        }
        if let Some(l) = left {
            let frame = l.frame_at(time, looping);
            self.skeleton.sample_over(l, frame, &mut out.local);
        }
        // The ADS layer last: a `tag_torso`-only track, so it adds the rig root over both halves
        // (the torso is not a left-gun bone; the order is free and ADS-last is the semantic one).
        let layer = ads.is_some();
        if let Some((a, frame)) = ads {
            self.skeleton.sample_over(a, frame, &mut out.local);
        }
        self.skeleton.model_space(&out.local, &mut out.model);
        out.root = self.placement(req, &out.model, layer);
        self.skeleton.palette(&out.model, &out.root, &mut out.palette);
    }

    /// The model-to-world matrix: eye frame × gun pose × scale. Aiming is the authored ADS clip's
    /// job — the rig root's hip→aimed translation (eye relief included) and rotation — so when the
    /// layer posed this frame the eye-frame slide is off: applying both double-corrects
    /// mid-transition and keeps the hip's depth forever. A weapon whose manifest has no ADS clip
    /// (or one still loading) keeps the slide, exactly as the W3D path does.
    pub fn placement(&self, req: &Request, model_space: &[Xf], layer: bool) -> Mat4 {
        let gun = match (self.aim.and_then(|i| model_space.get(i)), layer) {
            (Some(aim), false) => place::aligned(req.gun.0, *aim, req.ads),
            _ => req.gun.0,
        };
        place::root(req.eye, req.dir, gun)
    }

    /// A bone's world transform for the last pose (3x4 rows: the bridge's `Matrix3D`), for
    /// effects at `tag_flash`/`tag_brass`.
    pub fn bone_world(&self, posed: &Posed, name: &str) -> Option<[[f32; 4]; 3]> {
        let i = self.skeleton.bone(name)?;
        let m = mat4_mul(&posed.root, &mat4(*posed.model.get(i)?));
        Some([[m[0], m[4], m[8], m[12]], [m[1], m[5], m[9], m[13]], [m[2], m[6], m[10], m[14]]])
    }
}

/// The ADS layer's frame for the aim fraction: `ads · num_frames` entering, `(1 − ads) · num_frames`
/// leaving. `num_frames` is BO2's last frame index (`zhbo_native::anim::Anim`), so 1 lands the
/// authored aimed key. Stateless by design: the clip is compressed into the def's ADS time by the
/// fraction itself (`weapons::arsenal` moves `ads` linearly), so no second clock is kept. The
/// research's mp7 measurement — 63 % of the move at half — is this function at N/2.
pub fn ads_frame(ads: f32, down: bool, num_frames: u16) -> f32 {
    let k = ads.clamp(0.0, 1.0);
    (if down { 1.0 - k } else { k }) * num_frames as f32
}

#[cfg(test)]
mod tests;
