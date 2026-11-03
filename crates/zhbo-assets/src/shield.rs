//! The Assault Shield's collision boxes, posed on the BO2 body, and its planted model.
//!
//! BO2 attaches the shield as a model with collision surfaces (`_riotshield.gsc`
//! `refreshshieldattachment`): `t6_wpn_shield_carry_world` on `tag_weapon_left` while the shield is
//! the weapon up, `t6_wpn_shield_stow_world` on `tag_stowed_back` while it is carried but not up. A
//! ray through them is a hit on location `riotshield` (rank 101 in the trace, above every body box).
//! The converter poses the body once and writes the bounds of each model's collision surfaces
//! (`XModel.collSurfs`) on its tag as a box in the body's model space: the held shield in the
//! shield-raise torso animation's last frame (the shield squarely in front), the stowed one in the
//! standing alert pose. Planted, the carry model plays `o_riot_stand_deploy` (the armour panel
//! slides up over the visor, the leg and spikes come out): the converter draws its last frame and
//! takes the deployed box from the surfaces where that frame puts their bones.

use crate::xf::{compose, Xf, IDENTITY};
use zhbo_core::weapons::{ShieldBox, ShieldGeometry};
use zhbo_formats::t6zone::Zone;
use zhbo_formats::xanim::{self, Anim};
use zhbo_formats::xmodel::{self, Model};

pub const CARRY_MODEL: &str = "t6_wpn_shield_carry_world";
pub const STOW_MODEL: &str = "t6_wpn_shield_stow_world";
/// The model a planted shield stands as (`level.deployedshieldmodel`: the carry model) and the
/// animation it plays when it goes down (`level.riotshielddeployanim`; a late joiner sees its last
/// frame, `setanimtime( %o_riot_stand_deploy, 1.0 )`).
pub const DEPLOYED_MODEL: &str = CARRY_MODEL;
pub const DEPLOY_ANIM: &str = "o_riot_stand_deploy";
pub const HELD_TAG: &str = "tag_weapon_left";
pub const STOWED_TAG: &str = "tag_stowed_back";
/// The torso animation that brings the shield up (its last frame holds it).
pub const HELD_POSE: &str = "pt_riotshield_stand_raise";
/// The standing pose the stowed shield is measured in.
pub const STOWED_POSE: &str = "pb_stand_alert";

/// The deployed shield's health in Zero Hour damage (the tuning pack's `riotshield_deployed_health`,
/// BO2's 100-HP scale times `BO2_TO_ZH_DAMAGE`): what its object tells Zero Hour's AI.
pub fn deployed_health_zh() -> f32 {
    zhbo_core::tuning::Tuning::default().riotshield_deployed_health * zhbo_core::units::BO2_TO_ZH_DAMAGE
}

/// The world transform of every bone of `body` posed by `anim` at `frame` (the preview's walk:
/// animated rotations replace, animated translations offset the bone's local position).
fn posed(body: &Model, anim: &Anim, frame: f32) -> Vec<(String, Xf)> {
    let mut world: Vec<Xf> = Vec::with_capacity(body.bones.len());
    for b in &body.bones {
        let track = anim.bones.iter().find(|t| t.name == b.name);
        let rot = track.and_then(|t| t.rotation_at(frame));
        let pos = track.and_then(|t| t.translation_at(frame));
        let xf = match b.parent {
            Some(p) => compose(world[p], (rot.unwrap_or(b.local_rotation), pos.map_or(b.local_position, |t| [0, 1, 2].map(|k| b.local_position[k] + t[k])))),
            None => (rot.unwrap_or(b.rotation), pos.unwrap_or(b.position)),
        };
        world.push(xf);
    }
    body.bones.iter().map(|b| b.name.clone()).zip(world).collect()
}

/// `model`'s bounds on `tag` of `body` posed by `anim`'s last frame.
fn box_on(body: &Model, anim: &Anim, tag: &str, model: &Model) -> Result<ShieldBox, String> {
    let frame = anim.num_frames.saturating_sub(1) as f32;
    let bones = posed(body, anim, frame);
    let tag_xf = bones.iter().find(|(n, _)| n == tag).map(|b| b.1).ok_or_else(|| format!("{} has no {tag}", body.name))?;
    // The attached model's root follows the tag (plus the animation's own track for it, if any).
    let root = model.bones.first().and_then(|r| anim.bones.iter().find(|t| t.name == r.name));
    let local = match root {
        Some(t) => (t.rotation_at(frame).unwrap_or(IDENTITY.0), t.translation_at(frame).unwrap_or(IDENTITY.1)),
        None => IDENTITY,
    };
    let (rot, pos) = compose(tag_xf, local);
    let (mins, maxs) = collision_bounds(model);
    Ok(ShieldBox { rot, pos, mins, maxs })
}

/// The bounds of `model`'s collision surfaces (what BO2's trace meets), or of the whole model when
/// it has none.
pub fn collision_bounds(model: &Model) -> ([f32; 3], [f32; 3]) {
    if model.coll_surfs.is_empty() {
        return (model.mins, model.maxs);
    }
    model.coll_surfs.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), c| {
        ([0, 1, 2].map(|k| lo[k].min(c.mins[k])), [0, 1, 2].map(|k| hi[k].max(c.maxs[k])))
    })
}

/// The axis-aligned bounds of the box `mins..maxs` moved by `x`.
fn moved_bounds(x: Xf, mins: [f32; 3], maxs: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    (0..8).fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), i| {
        let corner = [0, 1, 2].map(|k| if i >> k & 1 == 0 { mins[k] } else { maxs[k] });
        let p = crate::xf::apply(x, corner);
        ([0, 1, 2].map(|k| lo[k].min(p[k])), [0, 1, 2].map(|k| hi[k].max(p[k])))
    })
}

/// `model` with every vertex moved into `anim`'s last frame (weighted skinning, as BO2 draws an
/// animated script model) and every collision surface with its bone, its bounds the posed bounds.
/// The bones keep their bind pose, so the result converts like any static model.
pub fn posed_model(model: &Model, anim: &Anim) -> Model {
    let frame = anim.num_frames.saturating_sub(1) as f32;
    let world = posed(model, anim, frame);
    let skin: Vec<Xf> = model.bones.iter().zip(&world).map(|(b, (_, w))| compose(*w, crate::xf::inverse((b.rotation, b.position)))).collect();
    let mut out = model.clone();
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    let lod0 = model.lods.first().map_or(0..0, |l| l.first_surface..l.first_surface + l.surface_count);
    for (i, s) in out.surfaces.iter_mut().enumerate() {
        for v in &mut s.vertices {
            let blend = |f: &dyn Fn(Xf) -> [f32; 3]| {
                v.weights.iter().fold([0.0f32; 3], |acc, &(bone, w)| {
                    let p = skin.get(bone as usize).map_or([0.0; 3], |&x| f(x));
                    [acc[0] + p[0] * w, acc[1] + p[1] * w, acc[2] + p[2] * w]
                })
            };
            let (p, n) = (v.position, v.normal);
            v.position = blend(&|x| crate::xf::apply(x, p));
            v.normal = blend(&|x| crate::xf::qrot(x.0, n));
            if lod0.contains(&i) {
                for k in 0..3 {
                    lo[k] = lo[k].min(v.position[k]);
                    hi[k] = hi[k].max(v.position[k]);
                }
            }
        }
    }
    if lo[0] <= hi[0] {
        (out.mins, out.maxs) = (lo, hi);
    }
    for c in &mut out.coll_surfs {
        if let Some(&x) = skin.get(c.bone) {
            (c.mins, c.maxs) = moved_bounds(x, c.mins, c.maxs);
        }
    }
    out
}

/// The deployed shield as BO2 shows it: the carry model standing in the deploy animation's last
/// frame (`_riotshield.csc` `riotshield_deploy_anim`: panel up, leg out, spikes down).
/// `pool` is BO2's asset pool, highest priority first.
pub fn deployed_model(pool: &[&Zone]) -> Result<Model, String> {
    let model = crate::viewmodels::find_model(pool, DEPLOYED_MODEL)?;
    let anim = crate::viewmodels::find_anim(pool, DEPLOY_ANIM).ok_or_else(|| format!("no {DEPLOY_ANIM}"))?;
    Ok(posed_model(&model, &anim))
}

/// The held and stowed boxes on `body` (any MP body: they share the player skeleton) and the
/// deployed box in the planted model's own frame.
pub fn geometry(common: &Zone, body: &Model) -> Result<ShieldGeometry, String> {
    let load_model = |n: &str| xmodel::find(common, n).map_err(|e| format!("{n}: {e}"));
    let load_anim = |n: &str| xanim::find(common, n).map_err(|e| format!("{n}: {e}"));
    let (mins, maxs) = collision_bounds(&deployed_model(&[common])?);
    Ok(ShieldGeometry {
        held: box_on(body, &load_anim(HELD_POSE)?, HELD_TAG, &load_model(CARRY_MODEL)?)?,
        stowed: box_on(body, &load_anim(STOWED_POSE)?, STOWED_TAG, &load_model(STOW_MODEL)?)?,
        deployed: ShieldBox { rot: IDENTITY.0, pos: IDENTITY.1, mins, maxs },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xf::qrot;

    /// Real install: the held shield stands in front of the body, facing forward, covering the
    /// torso and head; the stowed one hangs behind, facing back.
    #[test]
    #[ignore]
    fn the_shield_boxes_pose_in_front_and_behind() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let common = crate::generals::load_zone(&dir, "common_mp").unwrap();
        let f = &crate::generals::FACTIONS[0];
        let zone = crate::generals::load_zone(&dir, f.zone).unwrap();
        let body = xmodel::find(&zone, f.body).unwrap();
        let g = geometry(&common, &body).unwrap();
        let centre = |b: &ShieldBox| {
            let c = [0, 1, 2].map(|k| (b.mins[k] + b.maxs[k]) * 0.5);
            let r = qrot(b.rot, c);
            [b.pos[0] + r[0], b.pos[1] + r[1], b.pos[2] + r[2]]
        };
        let (held, stowed) = (centre(&g.held), centre(&g.stowed));
        let (held_fwd, stowed_fwd) = (qrot(g.held.rot, [1.0, 0.0, 0.0]), qrot(g.stowed.rot, [1.0, 0.0, 0.0]));
        println!("held {g:?}\n held centre {held:?} fwd {held_fwd:?}\n stowed centre {stowed:?} fwd {stowed_fwd:?}");
        assert!(held[0] > 8.0 && (30.0..60.0).contains(&held[2]), "held centre {held:?}");
        assert!(stowed[0] < 0.0 && (30.0..70.0).contains(&stowed[2]), "stowed centre {stowed:?}");
        assert!(held_fwd[0] > 0.7, "the held shield faces forward: {held_fwd:?}");
    }
}
