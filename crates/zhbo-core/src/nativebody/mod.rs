//! The native third person (Track R7): the General's body and the weapon in his hand, drawn
//! through the renderer's native BO2 pass instead of the W3D conversion.
//!
//! **The data.** The converter writes a faction body manifest (`ZB<faction>.zbc`) and a
//! world-weapon manifest per converted weapon (`ZW<vm>.zbw`), `zhbo_native::thirdperson`. The body
//! manifest names the body's model blob and one animation blob per movement [`state`]; the weapon
//! manifest lists its `world_model` (the base part) and each world attachment. The manifests are
//! separate because the weapon changes with the General's loadout while the body does not; the core
//! composes the two at run time, the weapon hung on the body's `weapon_tag` (`tag_weapon_right`) and
//! its attachments on their own bones.
//!
//! **The two modes.** In *weapon* mode (milestone 2) Zero Hour still draws the W3D body, so the
//! native pass draws only the weapon, placed by the engine's world-space `tag_weapon_right`
//! transform (`zhbo_native_body`'s `tag`, read from the drawable by the bridge). In *body* mode
//! (milestone 3) the body is native too: the weapon rides the body's own posed `tag_weapon_right`,
//! and the whole rig is placed at the General object's world transform. The bridge hides the W3D
//! body when body mode draws.
//!
//! **The body clip.** Milestone 3 picks one clip per movement state from the sim's facts (stance,
//! gait, ADS, pistol stance): a body's own clip for the state, else the nearest stance/gait the
//! body has (`zhbo_native::thirdperson::state::fallbacks`). It deliberately does NOT drive BO2's
//! full transition graph: no per-state blend times, no upper-body layers (reload/throw/melee), no
//! additive flinch. The clip plays at the request's `time` and `rate`; the bridge supplies both
//! from the sim's movement state. Idle/walk/run/sprint/crouch/prone/ADS all loop; a one-shot clip
//! (none are requested today) would hold at its end.
//!
//! **Placement.** BO2 inches are scaled once, at the root: `root = transform × ART_SCALE`, so a
//! skinned vertex is `root × pose × inv_bind × vertex`. In weapon mode the transform is the
//! engine's own `tag_weapon_right` world matrix; in body mode it is the General object's world
//! transform (feet at `pos`, yawed about +Z). The tag transform comes from Zero Hour's own units,
//! so the same root puts the native gun exactly where the engine's W3D gun sits.
//!
//! Presentation only: nothing here feeds the sim, and the bridge keeps drawing the W3D model until
//! this says it drew.

use zhbo_native::math::{axis_angle, mat4, mat4_mul, qnorm, Mat4, IDENTITY, MAT4_IDENTITY};
use zhbo_native::model::Model;
use zhbo_native::pose::{PartRef, Skeleton};
use zhbo_native::thirdperson::{state, Body, Weapon};

use crate::nativevm::{build_mesh, Files, Mesh, Posed};

#[cfg(feature = "renderer")]
mod gpu;
pub mod ffi;

/// BO2 inches to world units: [`crate::nativevm::ART_SCALE`], one source with the sim.
pub const ART_SCALE: f32 = crate::nativevm::ART_SCALE;

/// What to draw this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Only the weapon, posed rigidly on the engine's `tag_weapon_right` (the W3D body draws).
    Weapon,
    /// The body and the weapon on the body's own tag (the W3D body is hidden by the bridge).
    Body,
}

/// One general's frame.
#[derive(Clone, Copy, Debug)]
pub struct Request {
    /// The converted weapon index (`WeaponDef::viewmodel`), or -1 for no weapon in hand.
    pub vm: i32,
    /// The faction letter of the body (`U`, `C`, `G`).
    pub faction: u8,
    pub mode: Mode,
    /// Which body clip, seconds into it and its playback rate (used by [`Mode::Body`]; the
    /// weapon-only mode is rigid).
    pub state: u16,
    pub time: f32,
    pub rate: f32,
    /// The General's feet and yaw about +Z (world units, radians).
    pub pos: [f32; 3],
    pub yaw: f32,
    /// The engine's `tag_weapon_right` world transform (3 rows of 4, translation in each row's
    /// fourth lane), for [`Mode::Weapon`].
    pub tag: Option<[[f32; 4]; 3]>,
}

/// A world weapon's manifest and the models it names, decoded.
pub struct WeaponAsset {
    pub vm: i32,
    pub manifest: Weapon,
    pub models: Vec<Model>,
}

/// A faction body's manifest, model and the clips it loaded (by state id; a clip the archive lacks
/// is absent, and [`Loaded::clip_for`] falls back to a nearer state).
pub struct BodyAsset {
    pub manifest: Body,
    pub model: Model,
    pub clips: Vec<(u16, AnimClip)>,
}

/// A composed rig ready to pose. The body (body mode) is always the first part; the weapon hangs
/// from its `weapon_tag`.
pub struct Loaded {
    /// `(faction, vm, mode)`: the mode matters because a weapon-only rig has no body.
    pub key: (u8, i32, u8),
    pub weapon: WeaponAsset,
    pub body: Option<BodyAsset>,
    pub skeleton: Skeleton,
    pub mesh: Mesh,
}

/// A body clip, decoded (`zhbo_native::anim::Anim`).
pub type AnimClip = zhbo_native::anim::Anim;

/// Load step: `Ok(None)` while files are coming, `Err` when the rig can't be built (logged once by
/// the caller; the W3D path keeps drawing).
pub fn load(files: &mut Files, req: &Request) -> Result<Option<Loaded>, String> {
    let path = zhbo_native::world_weapon_path(req.vm);
    if files.is_missing(&path) {
        return Err(format!("{path} is not in the archive (reconvert the BO2 art)"));
    }
    let Some(bytes) = files.get(&path) else { return Ok(None) };
    let manifest = Weapon::from_bytes(&bytes).map_err(|e| format!("{path}: {e}"))?;
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
    if waiting {
        return Ok(None);
    }
    // Body mode also needs the faction's body: its manifest, its model and its clips. The weapon
    // hangs on the body's `weapon_tag`.
    let mut body = None;
    if req.mode == Mode::Body {
        let bpath = zhbo_native::body_path(req.faction as char);
        if files.is_missing(&bpath) {
            return Err(format!("{bpath} is not in the archive (reconvert the BO2 art)"));
        }
        let Some(bbytes) = files.get(&bpath) else { return Ok(None) };
        let bmanifest = Body::from_bytes(&bbytes).map_err(|e| format!("{bpath}: {e}"))?;
        if files.is_missing(&bmanifest.model) {
            return Err(format!("{bpath}: {} is not in the archive", bmanifest.model));
        }
        let Some(mbytes) = files.get(&bmanifest.model) else { return Ok(None) };
        let bmodel = Model::from_bytes(&mbytes).map_err(|e| format!("{}: {e}", bmanifest.model))?;
        // The clips: a state the archive lacks is absent (the fallback chain handles it).
        let mut clips = Vec::with_capacity(bmanifest.clips.len());
        for (state, cpath) in &bmanifest.clips {
            if files.is_missing(cpath) {
                continue;
            }
            match files.get(cpath) {
                Some(b) => clips.push((*state, AnimClip::from_bytes(&b).map_err(|e| format!("{cpath}: {e}"))?)),
                None => return Ok(None),
            }
        }
        for (_, cpath) in &bmanifest.clips {
            files.forget(cpath);
        }
        files.forget(&bmanifest.model);
        files.forget(&bpath);
        body = Some(BodyAsset { manifest: bmanifest, model: bmodel, clips });
    }
    // Compose: the body first (no tag), then the weapon's base part on the body's weapon_tag, then
    // its attachments on their own tags. Weapon-only mode hangs the base part from the model's
    // origin.
    let mut all_models: Vec<&Model> = Vec::new();
    let mut parts: Vec<PartRef> = Vec::new();
    match &body {
        Some(b) => {
            all_models.push(&b.model);
            parts.push(PartRef { model: &b.model, tag: None, offset: IDENTITY, rigid: false });
            // A part with no tag (the magazine: BO2's default `j_gun` is not a bone of the world
            // models) hangs from the world model's root, as in weapon mode — not the body's origin.
            let weapon_root = models.first().and_then(|m| m.bones.first()).map(|b| b.name.as_str());
            for (i, (p, m)) in manifest.parts.iter().zip(&models).enumerate() {
                let tag = if i == 0 { Some(b.manifest.weapon_tag.as_str()) } else { p.tag.as_deref().or(weapon_root) };
                all_models.push(m);
                parts.push(PartRef { model: m, tag, offset: p.offset, rigid: p.rigid });
            }
        }
        None => {
            for (p, m) in manifest.parts.iter().zip(&models) {
                all_models.push(m);
                parts.push(PartRef { model: m, tag: p.tag.as_deref(), offset: p.offset, rigid: p.rigid });
            }
        }
    }
    let (skeleton, problems) = Skeleton::compose(&parts, &[], &manifest.hide);
    if !problems.is_empty() {
        return Err(format!("{path}: {}", problems.join("; ")));
    }
    let mesh = build_mesh(&skeleton, all_models.into_iter())?;
    for p in &manifest.parts {
        files.forget(&p.model);
    }
    files.forget(&path);
    Ok(Some(Loaded { key: (req.faction, req.vm, req.mode as u8), weapon: WeaponAsset { vm: req.vm, manifest, models }, body, skeleton, mesh }))
}

impl Loaded {
    /// The body clip for `state`: its own, else the nearest stance/gait the body has. `None` in
    /// weapon-only mode (the gun is rigid) or when the body loaded no clip at all.
    pub fn clip_for(&self, state: u16) -> Option<&AnimClip> {
        let body = self.body.as_ref()?;
        zhbo_native::thirdperson::state::fallbacks(state).iter().find_map(|&s| body.clips.iter().find(|c| c.0 == s).map(|c| &c.1))
    }
}

/// The world matrix a 3x4 tag transform makes, then the art scale: `tag × scale`. The tag's rows
/// are W3D's row-vector `Matrix3D` (basis X, Y, Z, each with its translation lane); the renderer
/// takes column-major matrices.
pub fn root_from_tag(tag: &[[f32; 4]; 3]) -> Mat4 {
    let [r0, r1, r2] = *tag;
    let m: Mat4 = [
        r0[0], r1[0], r2[0], 0.0, //
        r0[1], r1[1], r2[1], 0.0, //
        r0[2], r1[2], r2[2], 0.0, //
        r0[3], r1[3], r2[3], 1.0,
    ];
    mat4_mul(&m, &scale())
}

/// The body's world matrix: its feet at `pos`, yawed about +Z (the engine's orientation: 0 = +X,
/// counter-clockwise), then the art scale.
pub fn root_from_object(pos: [f32; 3], yaw: f32) -> Mat4 {
    mat4_mul(&mat4((qnorm(axis_angle([0.0, 0.0, 1.0], yaw)), pos)), &scale())
}

fn scale() -> Mat4 {
    let mut s = MAT4_IDENTITY;
    s[0] = ART_SCALE;
    s[5] = ART_SCALE;
    s[10] = ART_SCALE;
    s
}

impl Loaded {
    /// Pose this frame and compute the palette.
    pub fn pose(&self, req: &Request, anim: Option<&AnimClip>, out: &mut Posed) {
        match (req.mode, anim) {
            (Mode::Body, Some(a)) => {
                let frame = a.frame_at(req.time * req.rate.max(0.0), true);
                self.skeleton.sample(a, frame, &mut out.local);
            }
            _ => {
                out.local.clear();
                out.local.extend(self.skeleton.bind_pose());
            }
        }
        self.skeleton.model_space(&out.local, &mut out.model);
        out.root = self.placement(req, &out.model);
        self.skeleton.palette(&out.model, &out.root, &mut out.palette);
    }

    /// The model-to-world matrix for this frame.
    pub fn placement(&self, req: &Request, _model_space: &[zhbo_native::math::Xf]) -> Mat4 {
        match req.mode {
            Mode::Weapon => req.tag.as_ref().map_or(MAT4_IDENTITY, root_from_tag),
            Mode::Body => root_from_object(req.pos, req.yaw),
        }
    }

    /// A bone's world transform for the last pose (3x4 rows, the bridge's `Matrix3D`).
    pub fn bone_world(&self, posed: &Posed, name: &str) -> Option<[[f32; 4]; 3]> {
        let i = self.skeleton.bone(name)?;
        let m = mat4_mul(&posed.root, &mat4(*posed.model.get(i)?));
        Some([[m[0], m[4], m[8], m[12]], [m[1], m[5], m[9], m[13]], [m[2], m[6], m[10], m[14]]])
    }
}

/// The body clip for the General's movement state: BO2's body animation for the stance and gait.
/// `run_speed` is the sim's own `g_speed` in world units/s (the walk/run split is a share of it),
/// so this stays a presentation rule with a single source for the speed.
pub fn state_of(stance: i32, speed: f32, sprinting: bool, ads: f32, sidearm: bool, moving: bool, run_speed: f32) -> u16 {
    let run = run_speed.max(1e-3);
    let moving = moving || speed > 0.2;
    match stance {
        2 => if moving { state::PRONE_CRAWL } else { state::PRONE_IDLE },
        1 => if moving { state::CROUCH_WALK } else if ads > 0.5 { state::CROUCH_ADS } else { state::CROUCH_IDLE },
        _ => {
            if ads > 0.5 {
                if sidearm { state::STAND_ADS_PISTOL } else { state::STAND_ADS }
            } else if sprinting && moving {
                state::STAND_SPRINT
            } else if speed > run * 0.6 {
                state::STAND_RUN
            } else if moving {
                state::STAND_WALK
            } else if sidearm {
                state::STAND_IDLE_PISTOL
            } else {
                state::STAND_IDLE
            }
        }
    }
}

#[cfg(test)]
mod tests;
