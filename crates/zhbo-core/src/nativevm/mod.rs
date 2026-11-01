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
use std::sync::{Arc, Mutex};

use zhbo_native::anim::Anim;
use zhbo_native::math::{axis_angle, mat4, mat4_mul, qmul, Mat4, Xf};
use zhbo_native::model::{Blend, CamoLayer, Cull, Model, Vertex};
use zhbo_native::pose::{PartRef, Skeleton};
use zhbo_native::viewmodel::{Part, Viewmodel};

#[cfg(feature = "renderer")]
mod gpu;
pub mod camo;
pub mod ffi;
pub mod place;

use camo::LocalCamo;

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

/// A surface's lens reticle as the renderer takes it, its noise map by `handle`. Shared with the
/// third person.
#[cfg(feature = "renderer")]
pub(crate) fn native_reticle(r: &zhbo_native::model::Reticle, display: Option<&zhbo_native::display::RangeFinderDisplay>, handle: impl Fn(&Option<String>) -> u16) -> zhbo_gfx::runtime::NativeReticle {
    zhbo_gfx::runtime::NativeReticle {
        noise: handle(&r.noise),
        scale: r.scale,
        color: r.color,
        glow: r.glow,
        glow_falloff: r.glow_falloff,
        emissive: r.emissive,
        noise_scale: r.noise_scale,
        noise_amount: r.noise_amount,
        background: r.background,
        kind: match r.kind {
            zhbo_native::model::ReticleKind::Dynamic => zhbo_gfx::runtime::NativeReticleKind::Dynamic,
            zhbo_native::model::ReticleKind::TwoColors => zhbo_gfx::runtime::NativeReticleKind::TwoColors,
            zhbo_native::model::ReticleKind::Static => zhbo_gfx::runtime::NativeReticleKind::Static,
            zhbo_native::model::ReticleKind::Ui3d => zhbo_gfx::runtime::NativeReticleKind::Ui3d,
        },
        color_alt: r.color_alt,
        glow_alt: r.glow_alt,
        display: display.map(|d| zhbo_gfx::runtime::NativeDisplay { spotted: handle(&Some(d.spotted.clone())), atlas: handle(&Some(d.atlas.clone())), diamond_uv: d.diamond_uv, diamond_size: d.diamond_size }),
    }
}

/// A surface's MMS screen as the renderer takes it, its maps by `handle`. Shared with the third
/// person.
#[cfg(feature = "renderer")]
pub(crate) fn native_sonar(s: &zhbo_native::model::Sonar, handle: impl Fn(&Option<String>) -> u16) -> zhbo_gfx::runtime::NativeSonar {
    zhbo_gfx::runtime::NativeSonar { lookup: handle(&s.lookup), noise: handle(&s.noise), constants: s.constants() }
}

/// The texture files a surface's own technique reads besides its colour and packed maps (the
/// reticle's noise, the MMS screen's lookup and noise).
#[cfg(feature = "renderer")]
pub(crate) fn technique_files(s: &SurfaceSpec) -> impl Iterator<Item = &String> {
    let reticle = s.reticle.as_ref().and_then(|r| r.noise.as_ref());
    let sonar = s.sonar.as_ref().into_iter().flat_map(|so| [so.lookup.as_ref(), so.noise.as_ref()]).flatten();
    let display = s.display.as_ref().into_iter().flat_map(|d| [&d.spotted, &d.atlas]);
    reticle.into_iter().chain(sonar).chain(display)
}

/// Every texture file a surface draws with: its colour, packed maps, specular colour, camo layer
/// and its technique's own ([`technique_files`]). Shared with the third person.
#[cfg(feature = "renderer")]
pub(crate) fn surface_files(s: &SurfaceSpec) -> impl Iterator<Item = &String> {
    [&s.color, &s.maps, &s.specular, &s.camo].into_iter().flatten().chain(technique_files(s))
}

/// A composed surface as the renderer takes it, its textures by `handle`. Shared with the third
/// person, so the two native paths cannot drift (the third person once dropped the camo layer
/// here).
#[cfg(feature = "renderer")]
pub(crate) fn native_surface(s: &SurfaceSpec, handle: impl Fn(&Option<String>) -> u16) -> zhbo_gfx::runtime::NativeSurface {
    zhbo_gfx::runtime::NativeSurface {
        first_index: s.first_index,
        index_count: s.index_count,
        blend: native_blend(s.blend),
        emissive: s.emissive,
        color: handle(&s.color),
        maps: handle(&s.maps),
        specular: handle(&s.specular),
        camo: handle(&s.camo),
        camo_uv: s.camo_uv,
        sort_key: s.sort_key,
        cull: native_cull(s.cull),
        reticle: s.reticle.as_ref().map(|r| native_reticle(r, s.display.as_ref(), &handle)),
        sonar: s.sonar.as_ref().map(|so| native_sonar(so, &handle)),
    }
}

/// Whether the native viewmodel drew a lens reticle in its last frame: the HUD then leaves its
/// own 2D optic reticle out (BO2 has one reticle, on the lens). Presentation only; per thread,
/// as the viewmodel and the HUD are both driven from the game thread.
pub fn lens_reticle_drawn() -> bool {
    LENS_RETICLE.with(|d| d.get())
}

#[cfg_attr(not(feature = "renderer"), allow(dead_code))]
pub(crate) fn set_lens_reticle_drawn(drawn: bool) {
    LENS_RETICLE.with(|d| d.set(drawn));
}

thread_local! {
    static LENS_RETICLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
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
    /// A dual-wield weapon's left gun on its own clip (`INSTANCE_LEFT` set) and seconds into it,
    /// when the hands differ (one fires or reloads alone); `None` plays the base clip's left half
    /// at `time`.
    pub left: Option<(i32, f32)>,
}

/// A surface of the composed mesh, with its textures by archive path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceSpec {
    pub first_index: u32,
    pub index_count: u32,
    pub blend: Blend,
    pub emissive: bool,
    pub color: Option<String>,
    pub maps: Option<String>,
    /// BO2's specular colour map (`Material::specular`), or `None` for the maps' scalar level.
    pub specular: Option<String>,
    pub sort_key: u8,
    pub cull: Cull,
    /// BO2's material name (`mc/mtl_t6_wpn_...`): the key a style camo's material pairs match.
    pub material: String,
    /// The surface's semantic-22 camo layer default: which of a chosen pattern camo's two images
    /// it takes. Filled by `camo::apply`.
    pub camo_layer: CamoLayer,
    /// The chosen pattern camo's image path, or `None` for no camo.
    pub camo: Option<String>,
    /// The chosen camo's UV transform: offset.xy, scale, gain.
    pub camo_uv: [f32; 4],
    /// The manifest part (composition index) the surface came from: which `WeaponCamo` it wears
    /// (`Part::camo`, per part as BO2's `FUN_00529600`).
    pub part: u16,
    /// The lens reticle's projection and constants (`Material::reticle`): drawn by the pass's
    /// reticle technique, at infinity on the optic's axis, faded in by the aim.
    pub reticle: Option<zhbo_native::model::Reticle>,
    /// BO2's emblem surface (`Material::emblem`).
    pub emblem: bool,
    /// The MMS screen (`Material::sonar`): drawn by the pass's scanner technique.
    pub sonar: Option<zhbo_native::model::Sonar>,
    /// The target finder's display, on its ui3d reticle (`zhbo_native::display`).
    pub display: Option<zhbo_native::display::RangeFinderDisplay>,
}

fn is_ui3d(s: &SurfaceSpec) -> bool {
    s.reticle.as_ref().is_some_and(|r| r.kind == zhbo_native::model::ReticleKind::Ui3d)
}

/// The composed viewmodel's one mesh: every part's LOD 0 with its bones renumbered into the
/// composed skeleton, so one palette skins it all.
#[derive(Clone, Debug, Default, PartialEq)]
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

/// BO2's `cg_opticAttachmentViewmodelSwitch` (default 1.0, range 0..1: "Zoom factor to switch
/// Optic 1 ADS viewmodel"). The viewmodel's DObj is built with the ADS part list while the weapon
/// position fraction is at or above it (`FUN_0053a340` → `FUN_00535540` → `FUN_00535170`), and
/// rebuilt with the hip list the moment it drops below: the swap is instantaneous, at full ADS.
pub const OPTIC_VIEWMODEL_SWITCH: f32 = 1.0;

/// Which parts are drawn at `ads`. From [`OPTIC_VIEWMODEL_SWITCH`] up, BO2 draws the ADS part list
/// (G14 §3): each part that names an ADS model ([`Part::ads_swap`]) gives way to it, every other
/// part keeps its model — the scope included; below it the ADS-only models are hidden. The swap is
/// per part — a holo's `_ads` replaces the holo, never the magazine or another optic. A scope under
/// a 2D overlay is not dropped here: BO2 stops drawing the whole viewmodel while the reticle zoom
/// is up ([`gun_under_overlay`]). Only the first-person path uses this; the third-person
/// `nativebody` keeps `build_mesh`'s all-visible default.
pub fn visible_parts(parts: &[Part], ads: f32) -> Vec<bool> {
    let aimed = ads >= OPTIC_VIEWMODEL_SWITCH;
    parts.iter().map(|p| if aimed { p.ads_only || p.ads_swap.is_none() } else { !p.ads_only }).collect()
}

/// The viewmodel (`vm`) BO2 is not drawing this frame because its scope overlay's reticle zoom is
/// up (`FUN_005374c0` → `FUN_00536f30` with `drawGun = 0`), or −1. Set once per client frame from
/// [`crate::hud::state::HudLui::gun_under_overlay`]; read by the native draw.
static GUN_UNDER_OVERLAY: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

/// Record (or clear, with `None`) the viewmodel hidden under its scope overlay this frame.
pub fn set_gun_under_overlay(vm: Option<i32>) {
    GUN_UNDER_OVERLAY.store(vm.unwrap_or(-1), std::sync::atomic::Ordering::Relaxed);
}

/// Whether viewmodel `vm` is under its scope overlay this frame.
pub(crate) fn gun_under_overlay(vm: i32) -> bool {
    vm >= 0 && GUN_UNDER_OVERLAY.load(std::sync::atomic::Ordering::Relaxed) == vm
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
                specular: m.and_then(|m| m.specular.clone()),
                sort_key: m.map_or(0, |m| m.sort_key),
                cull: m.map_or(Cull::None, |m| m.cull),
                material: m.map_or_else(String::new, |m| m.name.clone()),
                camo_layer: m.map_or(CamoLayer::None, |m| m.camo),
                camo: None,
                camo_uv: [0.0, 0.0, 1.0, 1.0],
                part: i as u16,
                reticle: m.and_then(|m| m.reticle.clone()),
                emblem: m.is_some_and(|m| m.emblem),
                sonar: m.and_then(|m| m.sonar.clone()),
                display: None,
            });
        }
    }
    Ok(mesh)
}

/// A composed viewmodel, ready to pose.
pub struct Loaded {
    /// Viewmodel, faction, camo choice, reticle row.
    pub key: (i32, u8, u64, u32),
    /// The camo option applied to this mesh, or `None` for the stock finish.
    pub camo: Option<String>,
    pub manifest: Viewmodel,
    pub skeleton: Skeleton,
    pub mesh: Mesh,
    /// The ADS part list's mesh ([`visible_parts`] at the switch): what the first-person draw
    /// submits at full ADS. `None` when no part swaps (the hip mesh is also the aimed one).
    pub mesh_aimed: Option<Mesh>,
    /// The weapon's scope overlay art is in the archive, so the draw may stop under the overlay.
    pub overlay_art_present: bool,
    pub aim: Option<usize>,
    pub anims: HashMap<String, Anim>,
}

/// The local player's reticle choice from the last client frame: the primary's viewmodel and the
/// `weaponoptions.csv` row its lens takes (0: the optic's stock reticle). Set by
/// `ffi::zhbo_client_frame`; read by the viewmodel draw.
static LOCAL_RETICLE: Mutex<Option<(i32, u32)>> = Mutex::new(None);

/// Record (or clear) the local player's reticle choice: `(viewmodel, row)`.
pub fn set_local_reticle(choice: Option<(i32, u32)>) {
    *LOCAL_RETICLE.lock().unwrap_or_else(|e| e.into_inner()) = choice;
}

/// The reticle row for viewmodel `vm` (0, the stock reticle, when the choice is another gun's).
pub(crate) fn local_reticle(vm: i32) -> u32 {
    LOCAL_RETICLE.lock().unwrap_or_else(|e| e.into_inner()).filter(|c| c.0 == vm).map_or(0, |c| c.1)
}

/// The `weaponoptions.csv` reticle row for a class's reticle option: `reticle_N` is row `N`, its
/// index in the reticle group. `FUN_0052a010` swaps the lens for row `(options >> 7) & 0x3f` and only
/// when it is not 0 (`FUN_005299a0`), so `reticle_0` (the default, `MPUI_RETICLE_DEFAULT`) is the
/// optic's stock row 0; the table's 38 rows per optic (0..37) are the group's 38 options. No
/// choice is row 0 too.
pub fn reticle_row(id: &str) -> u32 {
    id.strip_prefix("reticle_").and_then(|n| n.parse::<u32>().ok()).unwrap_or(0)
}

/// Swap each lens surface's material for reticle row `row` of its optic's set (the converter's
/// `weaponoptions.csv` table). A surface whose stock material has no set, or a row past the set,
/// keeps its stock reticle.
pub(crate) fn apply_reticle(mesh: &mut Mesh, set: &zhbo_native::reticles::Reticles, row: u32) {
    for s in mesh.surfaces.iter_mut().filter(|s| s.reticle.is_some()) {
        let Some(m) = set.option(&s.material, row as usize) else { continue };
        if m.reticle.is_none() {
            continue;
        }
        s.color = m.color.clone();
        s.reticle = m.reticle.clone();
        s.blend = m.blend;
        s.emissive = m.emissive;
        s.sort_key = m.sort_key;
        s.cull = m.cull;
    }
}

/// What the local player's optic displays show (the sim's `sim::optics`), with the presentation
/// clock their pulse runs on. Set by `ffi::zhbo_client_frame`; read by the viewmodel draw.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LocalOptics {
    /// Seconds of client frames (presentation only: the MMS pulse's phase).
    pub time: f32,
    /// The target finder's `weaponParam0.z`.
    pub alt: f32,
    /// The MMS's characters.
    pub blips: Vec<crate::sim::optics::Blip>,
}

static LOCAL_OPTICS: Mutex<Option<LocalOptics>> = Mutex::new(None);

/// Advance the displays' clock by `dt` and take the local player's optics (`None`: nothing to
/// show). The clock wraps at an hour to keep its precision.
pub fn update_local_optics(dt: f32, optics: Option<(f32, Vec<crate::sim::optics::Blip>)>) {
    let mut slot = LOCAL_OPTICS.lock().unwrap_or_else(|e| e.into_inner());
    let time = (slot.as_ref().map_or(0.0, |o| o.time) + dt.max(0.0)) % 3600.0;
    let (alt, blips) = optics.unwrap_or_default();
    *slot = Some(LocalOptics { time, alt, blips });
}

#[cfg_attr(not(feature = "renderer"), allow(dead_code))]
pub(crate) fn local_optics() -> LocalOptics {
    LOCAL_OPTICS.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

/// The local player's camo choices from the last client frame: one per held weapon's viewmodel
/// (the primary's, the secondary's, the knife's), each with its own asset and option. Set by
/// `ffi::zhbo_client_frame`; read by the viewmodel draw for the viewmodel it is drawing.
static LOCAL_CAMO: Mutex<Vec<LocalCamo>> = Mutex::new(Vec::new());

/// Record the local player's camo choices (empty clears them). Called once per client frame.
pub fn set_local_camos(choices: Vec<LocalCamo>) {
    *LOCAL_CAMO.lock().unwrap_or_else(|e| e.into_inner()) = choices;
}

/// The choice that belongs to viewmodel `vm`, if any held weapon draws through it.
pub(crate) fn local_camo(vm: i32) -> Option<LocalCamo> {
    camo_for_vm(&LOCAL_CAMO.lock().unwrap_or_else(|e| e.into_inner()), vm)
}

/// The choice among `choices` that belongs to viewmodel `vm`.
pub(crate) fn camo_for_vm(choices: &[LocalCamo], vm: i32) -> Option<LocalCamo> {
    choices.iter().find(|c| c.vm == vm).cloned()
}

/// Each held weapon's own camo choice: `slots` pairs a weapon id with the class camo id it wears
/// (BO2's `_class.gsc` gives the primary `calcweaponoptions(class, 0)`, the sidearm `(class, 1)`
/// and the knife `(class, 2)`; the engine paints each from its own def's `weaponCamo`,
/// `FUN_00529600`).
///
/// * **Alt modes wear the parent's camo.** BO2's give (`FUN_00670460`, the `giveweapon` builtin's
///   engine half) walks the weapon's alt chain (`FUN_008ec9e0`, `gl_`/`sf_`/`dualoptic_`) and
///   stores the *same* weapon options on each alt's held-weapon entry; the viewmodel's camo pass
///   then reads the current weapon's options (`centity+0x268`, `FUN_00536f30`) and paints with
///   the alt's **own** def's `weaponCamo`. So each slot's alt chain joins with the slot's camo,
///   resolved through each alt's own def (an alt whose def names no camo asset draws stock).
/// * **The current weapon owns its viewmodel.** An alt mode draws through its parent's viewmodel
///   (a combination's alt shares it, `ClipBase` past the parent's clips), so `current` — the
///   weapon being held — decides the shared viewmodel first: the alt's own asset, or stock when
///   its def has none. Otherwise a viewmodel goes to the first slot that reaches it.
///
/// A weapon without a viewmodel, an unknown id and a stock choice contribute nothing.
pub fn class_camos(slots: &[(&str, &str)], current: &str) -> Vec<LocalCamo> {
    class_camos_in(|id| crate::weapons::catalog().get(id), slots, current)
}

/// [`class_camos`] over any definition lookup (the tests' own defs).
pub(crate) fn class_camos_in<'a>(
    def: impl Fn(&str) -> Option<&'a crate::weapons::WeaponDef>,
    slots: &[(&str, &str)],
    current: &str,
) -> Vec<LocalCamo> {
    // Every slot weapon, then its alt chain (stopping where it loops back), each with the slot's camo.
    let mut held: Vec<(&crate::weapons::WeaponDef, &str)> = Vec::new();
    for (weapon, camo) in slots {
        let mut next = def(weapon);
        while let Some(d) = next {
            if held.iter().any(|(h, _)| h.id == d.id) {
                break;
            }
            held.push((d, camo));
            next = (!d.alt.is_empty()).then(|| def(&d.alt)).flatten();
        }
    }
    held.sort_by_key(|(d, _)| d.id != current);
    let mut claimed: Vec<i32> = Vec::new();
    let mut out: Vec<LocalCamo> = Vec::new();
    for (d, camo) in held {
        if d.viewmodel < 0 || claimed.contains(&d.viewmodel) {
            continue;
        }
        claimed.push(d.viewmodel);
        out.extend(choice_for(d, camo));
    }
    out
}

/// The camo choice for a weapon definition, or `None` for the stock finish. `none`, `camo_none`
/// and an empty camo material are the stock finish.
pub fn choice_for(def: &crate::weapons::WeaponDef, camo: &str) -> Option<LocalCamo> {
    let off = camo.is_empty() || camo == "none" || camo == "camo_none";
    (!off && def.viewmodel >= 0 && !def.camo_material.is_empty()).then(|| LocalCamo {
        vm: def.viewmodel,
        asset: def.camo_material.clone(),
        option: camo.to_string(),
    })
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
pub fn load(files: &mut Files, vm: i32, faction: u8, overlay_material_override: Option<&str>, camo_choice: Option<(&str, &str)>, reticle_row: u32) -> Result<Option<Loaded>, String> {
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
    // The gun may only stop drawing under the screen-space overlay when the overlay art is in the
    // archive. The HUD's image fallback consults TheMappedImageCollection; the archive's
    // MappedImage INI is that collection's source. A material absent from the INI cannot draw, so
    // the gun keeps drawing rather than leaving neither sight. The INI request may still be in
    // flight: wait a frame.
    let overlay_art_present = overlay_art_present(files, &manifest.id, overlay_material_override, &mut waiting);
    if waiting {
        return Ok(None);
    }
    let parts: Vec<PartRef> = manifest.parts.iter().zip(&models).map(|(p, m)| PartRef { model: m, tag: p.tag.as_deref(), offset: p.offset, rigid: p.rigid }).collect();
    let (skeleton, problems) = Skeleton::compose(&parts, &manifest.markers, &manifest.hide);
    if !problems.is_empty() {
        return Err(format!("{path}: {}", problems.join("; ")));
    }
    // The hip half hides the ADS-only models (the holo's `_ads`, the M32's slot 7) — the
    // below-the-switch arm of `visible_parts`; without it both models of a pair would draw at hip.
    let hip = visible_parts(&manifest.parts, 0.0);
    let mut mesh = if hip.iter().any(|v| !v) {
        build_mesh_visible(&skeleton, &models, |i| hip.get(i).copied().unwrap_or(true))?
    } else {
        build_mesh(&skeleton, &models)?
    };
    // The aimed-half mesh: each paired part gives way to its ADS model at the switch. Built once so
    // the draw path only swaps handles. The mask's indices are the manifest's part order, which
    // the skeleton preserves.
    let aimed = visible_parts(&manifest.parts, OPTIC_VIEWMODEL_SWITCH);
    let mut mesh_aimed = if aimed.iter().any(|v| !v) {
        Some(build_mesh_visible(&skeleton, &models, |i| aimed.get(i).copied().unwrap_or(true))?)
    } else {
        None
    };
    // The class's camo, when this is the primary weapon: one option, painted per part with that
    // part's own `WeaponCamo` (the gun's on the gun and its base parts, an optic's own on the
    // optic — BO2's `FUN_00529600`). A blob that is not in the archive is not an error: those
    // parts draw their stock finish and the caller logs once when none applied.
    let mut applied = None;
    if let Some((_, option)) = camo_choice {
        let assets: std::collections::BTreeSet<&str> = manifest.parts.iter().filter_map(|p| p.camo.as_deref()).collect();
        for asset in assets {
            let Some(recipe) = camo::read(files, asset, &mut waiting)? else { continue };
            let wears = |part: u16| manifest.parts.get(part as usize).and_then(|p| p.camo.as_deref()) == Some(asset);
            camo::apply(&mut mesh, &recipe, option, wears);
            if let Some(m) = &mut mesh_aimed {
                camo::apply(m, &recipe, option, wears);
            }
            applied = Some(option.to_string());
        }
    }
    // The target finder's display (the converter's `zhbo_native::display`): the ui3d reticle
    // samples the frame BO2's code draws into the ui3d texture. An archive without it draws none.
    if mesh.surfaces.iter().any(is_ui3d) {
        let path = zhbo_native::display::display_path();
        if !files.is_missing(&path) {
            match files.get(&path) {
                Some(bytes) => {
                    let d = zhbo_native::display::RangeFinderDisplay::from_bytes(&bytes).map_err(|e| format!("{path}: {e}"))?;
                    for m in std::iter::once(&mut mesh).chain(mesh_aimed.as_mut()) {
                        for s in m.surfaces.iter_mut().filter(|s| is_ui3d(s)) {
                            s.color = Some(d.idle.clone());
                            s.display = Some(d.clone());
                        }
                    }
                }
                None => waiting = true,
            }
        }
    }
    // The emblem surfaces show the faction's Zero Hour logo (the converter's
    // `zhbo_native::emblem_path`); an archive without it keeps the material's own stand-in.
    if mesh.surfaces.iter().any(|s| s.emblem) {
        let path = zhbo_native::emblem_path(faction as char);
        if !files.is_missing(&path) {
            if files.get(&path).is_some() {
                for m in std::iter::once(&mut mesh).chain(mesh_aimed.as_mut()) {
                    for s in m.surfaces.iter_mut().filter(|s| s.emblem) {
                        s.color = Some(path.clone());
                    }
                }
            } else {
                waiting = true;
            }
        }
    }
    // The class's reticle: the lens surfaces take the chosen `weaponoptions.csv` row. An archive
    // without the table draws the stock reticle.
    if reticle_row != 0 && mesh.surfaces.iter().any(|s| s.reticle.is_some()) {
        let path = zhbo_native::reticles::reticles_path();
        if !files.is_missing(&path) {
            match files.get(&path) {
                Some(bytes) => {
                    let set = zhbo_native::reticles::Reticles::from_bytes(&bytes).map_err(|e| format!("{path}: {e}"))?;
                    apply_reticle(&mut mesh, &set, reticle_row);
                    if let Some(m) = &mut mesh_aimed {
                        apply_reticle(m, &set, reticle_row);
                    }
                }
                None => waiting = true,
            }
        }
    }
    if waiting {
        return Ok(None);
    }
    let camo_key = camo_choice.map_or(0, |(asset, option)| camo::key_of(asset, option));
    // The bytes are decoded now; keep the cache small.
    for p in &manifest.parts {
        files.forget(&p.model);
    }
    files.forget(&path);
    let aim = skeleton.bone(AIM_TAG);
    Ok(Some(Loaded { key: (vm, faction, camo_key, reticle_row), camo: applied, manifest, skeleton, mesh, mesh_aimed, overlay_art_present, aim, anims: HashMap::new() }))
}

/// The clip a manifest shows for `clip` ([`Loaded::clip_path`]'s rule, also read by the conformance
/// emitter): its own, else the idle of its mode, else the idle. A dual-wield clip's id carries
/// `INSTANCE_LEFT` in its high byte; the fallbacks keep that bit. The left gun's own slots (BO2's
/// `_LEFT` set, [`crate::viewmodel::slot::left_as_right`]) never fall back here: the converter
/// already applied BO2's registration rule to them (an empty `_LEFT` slot plays the left variant's
/// slot 1, `FUN_0052d3e0`), so one it left out is empty in BO2 too.
pub fn clip_for(manifest: &Viewmodel, clip: i32) -> Option<&str> {
    use zhbo_native::viewmodel::INSTANCE_LEFT;
    let alt = crate::weapons::ALT_CLIPS;
    let instance = (clip as u16) & INSTANCE_LEFT;
    let base = (clip as u16) & !INSTANCE_LEFT;
    if instance != 0 && (0x4e..=0x54).any(|s| crate::viewmodel::slot::left_as_right(s) == Some(i32::from(base))) {
        return manifest.clip(clip as u16);
    }
    let mode_idle = if i32::from(base) >= alt { alt as u16 + crate::viewmodel::slot::IDLE as u16 } else { crate::viewmodel::slot::IDLE as u16 };
    [base, mode_idle, crate::viewmodel::slot::IDLE as u16].into_iter().find_map(|c| manifest.clip(c | instance))
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
        clip_for(&self.manifest, clip)
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
            let frame = match req.left {
                Some((clip, t)) => l.frame_at(t * req.rate.max(0.0), crate::viewmodel::slot::loops(clip & !(zhbo_native::viewmodel::INSTANCE_LEFT as i32))),
                None => l.frame_at(time, looping),
            };
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
