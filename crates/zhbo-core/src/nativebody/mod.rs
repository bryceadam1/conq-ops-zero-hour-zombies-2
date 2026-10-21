//! The native third person (Track R7): the weapon in the General's hand, and (milestone 3) the
//! General's body, drawn through the renderer's native BO2 pass instead of the W3D conversion.
//!
//! **The data.** The converter writes a faction body manifest (`ZB<faction>.zbc`) and a
//! world-weapon manifest per converted weapon (`ZW<vm>.zbw`), `zhbo_native::thirdperson`. The body
//! is composed from the body model and the weapon's world model hung on the body's
//! `tag_weapon_right`; world attachments hang from the weapon's own bones. The manifests are
//! separate because the weapon changes with the General's loadout while the body does not.
//!
//! **The two modes.** In *weapon* mode (milestone 2) Zero Hour still draws the W3D body, so the
//! native pass draws only the weapon, placed by the engine's world-space `tag_weapon_right`
//! transform (`zhbo_native_body`'s `tag`, read from the drawable by the bridge). In *body* mode
//! (milestone 3) the body is native too: the weapon rides the body's own posed
//! `tag_weapon_right`, and the whole rig is placed at the General object's world transform.
//!
//! **Placement.** BO2 inches are scaled once, at the root: `root = transform × ART_SCALE`, so a
//! skinned vertex is `root × pose × inv_bind × vertex`. The tag transform comes from Zero Hour's
//! own units, so the same root puts the native gun exactly where the engine's W3D gun sits.
//!
//! Presentation only: nothing here feeds the sim, and the bridge keeps drawing the W3D model until
//! this says it drew.

use std::collections::HashMap;

use zhbo_native::math::{axis_angle, mat4, mat4_mul, qnorm, Mat4, MAT4_IDENTITY};
use zhbo_native::model::Model;
use zhbo_native::pose::{PartRef, Skeleton};
use zhbo_native::thirdperson::{state, Weapon};

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

/// A composed rig ready to pose. The body (milestone 3) is always the first part; the weapon hangs
/// from its `weapon_tag`.
pub struct Loaded {
    pub key: (u8, i32),
    pub weapon: WeaponAsset,
    pub skeleton: Skeleton,
    pub mesh: Mesh,
    pub anims: HashMap<String, AnimClip>,
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
    // Milestone 3's body part comes from the body manifest; until then the weapon stands alone.
    if waiting {
        return Ok(None);
    }
    let parts: Vec<PartRef> = manifest.parts.iter().zip(&models).map(|(p, m)| PartRef { model: m, tag: p.tag.as_deref(), offset: p.offset, rigid: p.rigid }).collect();
    let (skeleton, problems) = Skeleton::compose(&parts, &[], &manifest.hide);
    if !problems.is_empty() {
        return Err(format!("{path}: {}", problems.join("; ")));
    }
    let mesh = build_mesh(&skeleton, &models)?;
    for p in &manifest.parts {
        files.forget(&p.model);
    }
    files.forget(&path);
    Ok(Some(Loaded { key: (req.faction, req.vm), weapon: WeaponAsset { vm: req.vm, manifest, models }, skeleton, mesh, anims: HashMap::new() }))
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
