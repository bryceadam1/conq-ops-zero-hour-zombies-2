//! Converts Black Ops II characters (body, weapon and animations) into Zero Hour W3D files.
//!
//! One hierarchy serves every body: BO2's MP bodies share the player skeleton, so the skeleton of
//! the first body plus the third-person rifle (its root hung on `tag_weapon_right`) becomes
//! `ZHBO_PLR_SKL`, and the animations are built once against it. Each body is skinned to that
//! hierarchy by bone name. Vertices go to their heaviest bone (W3D skins are one bone per vertex)
//! and are stored relative to that bone's bind transform, which is how the engine draws them.
//!
//! BO2 animation rotations replace a bone's local rotation and translations are added to its local
//! offset (a body's `j_mainroot` keys about -4" for a relaxed stance and -32" lying dead). W3D
//! channels are deltas from the hierarchy's base pose (`base * translate(t) * rotate(q)`), so each
//! key becomes `t = qb^-1 (p - pb)` and `q = qb^-1 q_anim`, with `p = pb + offset`. BO2 is in inches; everything is scaled into Zero Hour units here.

use crate::xf::{self, Xf};
use zhbo_formats::w3d::{self, Channel, Pivot, SkinMesh};
use zhbo_formats::xanim::Anim;
use zhbo_formats::xmodel::Model;

/// BO2 inches to Zero Hour world units: [`zhbo_native::ZH_PER_INCH`] (a 72" soldier becomes 12
/// units), the same factor the sim and the native viewmodel use. Stock ZH infantry mesh heads sit
/// at 14.3–16 units, taller than their 12–13 `GeometryHeight` collision cylinder; the sim's
/// documented 12-unit body is the reference, not the stylised head height.
pub const SCALE: f32 = zhbo_native::ART_SCALE;
pub const HIERARCHY: &str = "ZHBO_PLR_SKL";
/// The bone a third-person weapon hangs from.
pub const WEAPON_TAG: &str = "tag_weapon_right";

/// The shared hierarchy: its pivots and each pivot's bind transform in model space.
pub struct Rig {
    pub pivots: Vec<Pivot>,
    /// Bind transform per pivot (model space, scaled).
    pub bind: Vec<Xf>,
    /// Full BO2 bone name per pivot (W3D cuts pivot names to 15 characters).
    pub names: Vec<String>,
    /// The model's own local offset per pivot (scaled): BO2 animated translations are added to it.
    pub anim_base: Vec<[f32; 3]>,
}

pub fn scaled(p: [f32; 3]) -> [f32; 3] {
    p.map(|c| c * SCALE)
}

impl Rig {
    /// Build from a base model (a body or a pair of arms) and models hung from its bones, e.g. a
    /// weapon on `tag_weapon_right`, or a viewmodel gun on `tag_weapon` with its magazine on `j_gun`.
    /// Attachments are added in order, so a later one may hang from an earlier one's bone.
    pub fn new(base: &Model, attachments: &[(&Model, &str)]) -> Rig {
        let mut rig = Rig {
            pivots: vec![Pivot { name: "ROOTTRANSFORM".into(), parent: None, translation: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0] }],
            bind: vec![xf::IDENTITY],
            names: vec!["ROOTTRANSFORM".into()],
            anim_base: vec![[0.0; 3]],
        };
        rig.add_bones(base, None);
        for (model, tag) in attachments {
            let tag = rig.pivot(tag);
            rig.add_bones(model, tag);
        }
        rig
    }

    /// Hang another model from pivot `tag` with its origin at `offset` (scaled units) in the tag's
    /// frame: BO2 attachments sit at an offset from the weapon.
    pub fn attach(&mut self, model: &Model, tag: usize, offset: Xf) {
        self.add_bones_at(model, Some(tag), offset);
    }

    /// Add a pivot with no mesh of its own: `name` under pivot `parent`, at `local` (scaled) in the
    /// parent's frame. Zero Hour finds effect positions by bone (a gun's muzzle).
    pub fn add_marker(&mut self, name: &str, parent: usize, local: Xf) {
        self.pivots.push(Pivot { name: name.to_string(), parent: Some(parent), translation: local.1, rotation: local.0 });
        self.bind.push(xf::compose(self.bind[parent], local));
        self.names.push(name.to_string());
        self.anim_base.push(local.1);
    }

    /// Put pivot `name` at `pos` (model space, scaled), keeping its rotation: an optic moves the
    /// line of sight (`tag_sights`) up to its own.
    pub fn move_pivot(&mut self, name: &str, pos: [f32; 3]) {
        let Some(i) = self.pivot(name) else { return };
        let parent = self.pivots[i].parent.unwrap_or(0);
        self.bind[i].1 = pos;
        let local = xf::compose(xf::inverse(self.bind[parent]), self.bind[i]);
        self.pivots[i].translation = local.1;
        self.pivots[i].rotation = local.0;
        self.anim_base[i] = local.1;
    }

    /// Append a model's bones. Root bones hang from pivot `attach` (with an identity offset) or
    /// from the root transform.
    fn add_bones(&mut self, model: &Model, attach: Option<usize>) {
        self.add_bones_at(model, attach, xf::IDENTITY);
    }

    fn add_bones_at(&mut self, model: &Model, attach: Option<usize>, offset: Xf) {
        let first = self.pivots.len();
        // An attached model's origin sits on its tag (plus `offset`) in the bind pose, so all of
        // its bones are placed relative to the tag.
        let origin = xf::compose(attach.map_or(xf::IDENTITY, |tag| self.bind[tag]), offset);
        for b in &model.bones {
            let world = xf::compose(origin, (b.rotation, scaled(b.position)));
            let parent = match b.parent {
                Some(p) => first + p,
                None => attach.unwrap_or(0),
            };
            let local = xf::compose(xf::inverse(self.bind[parent]), world);
            self.pivots.push(Pivot { name: b.name.clone(), parent: Some(parent), translation: local.1, rotation: local.0 });
            self.bind.push(world);
            self.names.push(b.name.clone());
            self.anim_base.push(if b.parent.is_some() { scaled(b.local_position) } else { [0.0; 3] });
        }
    }

    pub fn pivot(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    pub fn hierarchy_file(&self, name: &str) -> Vec<u8> {
        w3d::hierarchy(name, &self.pivots)
    }
}

/// Texture file name for a BO2 image name, e.g. `~-gc_usa_mp_fbi_vest_c` -> `zhbo_c_usa_mp_fbi_vest_c.tga`.
pub fn texture_file(image_name: &str) -> String {
    let clean: String = image_name
        .trim_start_matches(['~', '-', 'g'].as_ref())
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    let clean: String = clean.chars().take(40).collect();
    format!("zhbo_{clean}.tga")
}

/// Skin a model's LOD 0 to the rig. `attach` hangs the model's root bones from a pivot (for a weapon
/// that is part of the rig, pass `None`: its bones are found by name). `textures` gives each surface's
/// texture file, if any.
pub fn skin(rig: &Rig, model: &Model, mesh_prefix: &str, textures: &[Option<String>]) -> Vec<SkinMesh> {
    skin_hiding(rig, model, mesh_prefix, textures, &[])
}

/// [`skin`], leaving out triangles on the bones in `hide` (BO2's hide tags: the iron sights under
/// an optic, the magazine a fast mag replaces).
pub fn skin_hiding(rig: &Rig, model: &Model, mesh_prefix: &str, textures: &[Option<String>], hide: &[String]) -> Vec<SkinMesh> {
    let hidden: Vec<bool> = model.bones.iter().map(|b| hide.iter().any(|h| h == &b.name)).collect();
    // Map the model's bones to pivots by name; unknown bones fall back to their nearest known ancestor.
    let mut to_pivot = Vec::with_capacity(model.bones.len());
    for (i, b) in model.bones.iter().enumerate() {
        let p = rig.pivot(&b.name).or_else(|| b.parent.map(|p| to_pivot[p])).unwrap_or(0);
        debug_assert!(i == to_pivot.len());
        to_pivot.push(p);
    }
    // Some view models are bones only (the knife's base model hangs nothing of its own).
    let Some(lod) = model.lods.first() else { return Vec::new() };
    let mut meshes = Vec::new();
    for (k, s) in model.surfaces[lod.first_surface..lod.first_surface + lod.surface_count].iter().enumerate() {
        if s.vertices.is_empty() || s.triangles.is_empty() {
            continue;
        }
        let mut m = SkinMesh {
            name: format!("{mesh_prefix}{k:02}"),
            texture: textures.get(lod.first_surface + k).cloned().flatten(),
            ..Default::default()
        };
        for v in &s.vertices {
            let bone = v.weights.iter().copied().fold((0u16, -1.0f32), |a, w| if w.1 > a.1 { w } else { a }).0;
            let bone_index = bone as usize;
            let pivot = to_pivot.get(bone_index).copied().unwrap_or(0);
            // Relative to the bone's own bind pose in this model, so small differences between the
            // model's bind and the rig's base do not stretch the mesh.
            let bind = model.bones.get(bone_index).map_or(xf::IDENTITY, |b| (b.rotation, scaled(b.position)));
            let inv = xf::inverse(bind);
            m.positions.push(xf::apply(inv, scaled(v.position)));
            m.normals.push(xf::qrot(inv.0, v.normal));
            m.uvs.push(v.uv);
            m.bones.push(pivot as u16);
        }
        // BO2 and Zero Hour use opposite front-face winding: keep BO2's order and the engine culls
        // the outside of the model, so it looks inside-out.
        let heaviest = |v: usize| s.vertices[v].weights.iter().copied().fold((0u16, -1.0f32), |a, w| if w.1 > a.1 { w } else { a }).0 as usize;
        m.triangles = s
            .triangles
            .iter()
            .filter(|t| !t.iter().any(|&v| hidden.get(heaviest(v as usize)).copied().unwrap_or(false)))
            .map(|t| [t[0] as u32, t[2] as u32, t[1] as u32])
            .collect();
        if !m.triangles.is_empty() {
            meshes.push(m);
        }
    }
    meshes
}

/// Skin a rigid model (a third-person weapon or attachment) entirely to `pivot`, its origin at
/// `offset` (scaled) in the pivot's frame, leaving out triangles on the bones in `hide`.
pub fn skin_rigid(model: &Model, pivot: usize, offset: Xf, mesh_prefix: &str, first_index: usize, textures: &[Option<String>], hide: &[String]) -> Vec<SkinMesh> {
    let hidden: Vec<bool> = model.bones.iter().map(|b| hide.iter().any(|h| h == &b.name)).collect();
    let Some(lod) = model.lods.first() else { return Vec::new() };
    let mut meshes = Vec::new();
    for (k, s) in model.surfaces[lod.first_surface..lod.first_surface + lod.surface_count].iter().enumerate() {
        if s.vertices.is_empty() || s.triangles.is_empty() {
            continue;
        }
        let mut m = SkinMesh {
            name: format!("{mesh_prefix}{:02}", first_index + meshes.len()),
            texture: textures.get(lod.first_surface + k).cloned().flatten(),
            ..Default::default()
        };
        for v in &s.vertices {
            m.positions.push(xf::apply(offset, scaled(v.position)));
            m.normals.push(xf::qrot(offset.0, v.normal));
            m.uvs.push(v.uv);
            m.bones.push(pivot as u16);
        }
        let heaviest = |v: usize| s.vertices[v].weights.iter().copied().fold((0u16, -1.0f32), |a, w| if w.1 > a.1 { w } else { a }).0 as usize;
        m.triangles = s
            .triangles
            .iter()
            .filter(|t| !t.iter().any(|&v| hidden.get(heaviest(v as usize)).copied().unwrap_or(false)))
            .map(|t| [t[0] as u32, t[2] as u32, t[1] as u32])
            .collect();
        if !m.triangles.is_empty() {
            meshes.push(m);
        }
    }
    meshes
}

/// Scale an offset (inches) into Zero Hour units.
pub fn scaled_xf(x: Xf) -> Xf {
    (x.0, scaled(x.1))
}

/// Convert an animation into W3D channels for the rig. Returns (frame count, channels).
pub fn animate(rig: &Rig, anim: &Anim) -> (u32, Vec<Channel>) {
    let frames = anim.num_frames as u32 + 1;
    let mut channels = Vec::new();
    for (pivot, name) in rig.names.iter().enumerate().skip(1) {
        let Some(track) = anim.bones.iter().find(|t| &t.name == name) else { continue };
        if track.rotation.is_none() && track.translation.is_none() {
            continue;
        }
        let base = &rig.pivots[pivot];
        let qb_inv = xf::qconj(base.rotation);
        let mut ch = Channel { pivot: pivot as u16, translations: Vec::new(), rotations: Vec::new() };
        for f in 0..frames {
            let frame = f as f32;
            // BO2 translations are offsets added to the model's local offset; rotations replace.
            let ab = rig.anim_base[pivot];
            let p = track.translation_at(frame).map_or(base.translation, |t| {
                let t = scaled(t);
                [ab[0] + t[0], ab[1] + t[1], ab[2] + t[2]]
            });
            let q = track.rotation_at(frame).unwrap_or(base.rotation);
            let d = [p[0] - base.translation[0], p[1] - base.translation[1], p[2] - base.translation[2]];
            ch.translations.push(xf::qrot(qb_inv, d));
            ch.rotations.push(xf::qnorm(xf::qmul(qb_inv, q)));
        }
        channels.push(ch);
    }
    (frames, channels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zhbo_formats::xanim::BoneTrack;
    use zhbo_formats::xmodel::{Bone, Lod, Surface, Vertex};

    fn bone(name: &str, parent: Option<usize>, pos: [f32; 3], rot: [f32; 4]) -> Bone {
        Bone { name: name.into(), parent, local_rotation: [0.0, 0.0, 0.0, 1.0], local_position: [0.0; 3], rotation: rot, position: pos }
    }

    fn two_bone_model() -> Model {
        let s = std::f32::consts::FRAC_1_SQRT_2;
        Model {
            name: "m".into(),
            bones: vec![bone("root", None, [0.0; 3], [0.0, 0.0, 0.0, 1.0]), {
                let mut b = bone("arm", Some(0), [72.0, 0.0, 0.0], [0.0, 0.0, s, s]);
                b.local_position = [72.0, 0.0, 0.0];
                b
            }],
            surfaces: vec![Surface {
                vertices: vec![
                    Vertex { position: [72.0, 0.0, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 0.0], weights: vec![(1, 1.0)] },
                    Vertex { position: [0.0, 0.0, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 0.0], weights: vec![(0, 0.7), (1, 0.3)] },
                    Vertex { position: [72.0, 72.0, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 0.0], weights: vec![(1, 1.0)] },
                ],
                triangles: vec![[0, 1, 2]],
            }],
            lods: vec![Lod { distance: 0.0, first_surface: 0, surface_count: 1 }],
            radius: 1.0,
            mins: [0.0; 3],
            maxs: [0.0; 3],
            materials: vec![None],
        }
    }

    #[test]
    fn converted_art_stands_at_the_sims_height() {
        // The world-space boundary: the W3D converter and the native viewmodel must use the sim's
        // factor, or a body drawn in third person (Track R7) is 9% shorter than its eye height.
        assert_eq!(SCALE, zhbo_core::units::ZH_PER_INCH);
        assert_eq!(SCALE, zhbo_native::ART_SCALE);
        assert!((72.0 * SCALE - 12.0).abs() < 1e-5, "a 72-inch soldier is 12 units");
        // A measured SEAL 6 body (`zhbo-assets model ... c_usa_mp_seal6_assault_fb`: 72.146").
        assert!((72.14641 * SCALE - zhbo_core::units::inches(72.14641)).abs() < 1e-5);
    }

    #[test]
    fn rig_reproduces_the_bind_pose() {
        let model = two_bone_model();
        let rig = Rig::new(&model, &[]);
        assert_eq!(rig.pivots.len(), 3);
        // Composing base transforms down the hierarchy gives back each bone's bind transform.
        let mut world = vec![xf::IDENTITY];
        for p in &rig.pivots[1..] {
            world.push(xf::compose(world[p.parent.unwrap()], (p.rotation, p.translation)));
        }
        for (w, b) in world.iter().zip(&rig.bind) {
            for k in 0..3 {
                assert!((w.1[k] - b.1[k]).abs() < 1e-4);
            }
        }
        // A skinned vertex drawn at bone * vertex lands back where BO2 put it, scaled.
        let meshes = skin(&rig, &model, "M", &[None]);
        let m = &meshes[0];
        assert_eq!(m.bones, vec![2, 1, 2], "heaviest bone wins");
        for (i, v) in model.surfaces[0].vertices.iter().enumerate() {
            let drawn = xf::apply(world[m.bones[i] as usize], m.positions[i]);
            for k in 0..3 {
                assert!((drawn[k] - v.position[k] * SCALE).abs() < 1e-4, "vertex {i}: {drawn:?}");
            }
        }
    }

    #[test]
    fn attached_models_follow_their_tag() {
        let body = two_bone_model();
        // A "gun" whose child bone sits 10" in front of its root.
        let mut gun = two_bone_model();
        gun.bones = vec![bone("g_root", None, [0.0; 3], [0.0, 0.0, 0.0, 1.0]), bone("g_tip", Some(0), [10.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0])];
        let rig = Rig::new(&body, &[(&gun, "arm")]);
        let arm = rig.pivot("arm").unwrap();
        let tip = rig.pivot("g_tip").unwrap();
        // The tip is 10" along the arm's (rotated) x axis from the arm, not at the origin.
        let expected = xf::apply(rig.bind[arm], [10.0 * SCALE, 0.0, 0.0]);
        for k in 0..3 {
            assert!((rig.bind[tip].1[k] - expected[k]).abs() < 1e-4, "{:?} vs {expected:?}", rig.bind[tip].1);
        }
    }

    #[test]
    fn animation_channels_are_deltas_from_the_base_pose() {
        let model = two_bone_model();
        let rig = Rig::new(&model, &[]);
        // An animation that holds the bind pose (bind rotation, no extra offset) gives identity deltas.
        let base = &rig.pivots[2];
        let anim = Anim {
            name: "a".into(),
            num_frames: 1,
            frame_rate: 30.0,
            looping: true,
            delta: false,
            asset_type: 0,
            bones: vec![BoneTrack {
                name: "arm".into(),
                rotation: Some(vec![(0, base.rotation)]),
                translation: Some(vec![(0, [0.0; 3])]),
            }],
            notifies: vec![],
        };
        let (frames, channels) = animate(&rig, &anim);
        assert_eq!(frames, 2);
        let ch = &channels[0];
        assert_eq!(ch.pivot, 2);
        for (t, q) in ch.translations.iter().zip(&ch.rotations) {
            assert!(t.iter().all(|c| c.abs() < 1e-4), "{t:?}");
            assert!((q[3].abs() - 1.0).abs() < 1e-4, "{q:?}");
        }
    }
}
