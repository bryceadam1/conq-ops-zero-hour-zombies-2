/*
** Conquer Ops - converter: BO2 models and animations as the mod's native blobs (zhbo-native), for the
** renderer's native BO2 pass (Track R). Written beside the W3D conversion, which stays the default.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/
use crate::textures::Looks;
use std::collections::BTreeMap;
use zhbo_formats::{w3d, xanim, xmodel};
use zhbo_native::math::{apply, compose, inverse, QUAT_IDENTITY};
use zhbo_native::model::{self as nm, Blend};
use zhbo_native::pose::{PartRef, Skeleton};
use zhbo_native::viewmodel::Marker;

/// The aim marker's name (placed by the same rule as the W3D path's `tag_sights`,
/// `viewmodels::weapon_viewmodels`).
pub use zhbo_native::AIM_TAG;

pub(crate) fn blend(b: w3d::Blend) -> Blend {
    match b {
        w3d::Blend::Opaque => Blend::Opaque,
        w3d::Blend::AlphaTest => Blend::AlphaTest,
        w3d::Blend::Alpha => Blend::Alpha,
        w3d::Blend::Premultiplied => Blend::Premultiplied,
        w3d::Blend::Additive { alpha } => Blend::Additive { alpha },
    }
}

pub(crate) fn cull(c: xmodel::Cull) -> nm::Cull {
    match c {
        xmodel::Cull::None => nm::Cull::None,
        xmodel::Cull::Back => nm::Cull::Back,
        xmodel::Cull::Front => nm::Cull::Front,
    }
}

/// `m` as a native model: the textures the W3D pass already wrote for its surfaces (`tex`, from
/// `textures::textures`), the packed maps and blends `looks` holds for them. Surfaces the W3D pass
/// leaves out (`looks.dropped`: reticles, clan tags) are left out here too.
pub(crate) fn model(m: &xmodel::Model, tex: &[Option<String>], looks: &Looks) -> nm::Model {
    let bones = m
        .bones
        .iter()
        .map(|b| nm::Bone {
            name: b.name.clone(),
            parent: b.parent.map(|p| p as u32),
            local_rotation: b.local_rotation,
            local_position: b.local_position,
            rotation: b.rotation,
            position: b.position,
        })
        .collect();
    let mut out = nm::Model { name: m.name.clone(), bones, mins: m.mins, maxs: m.maxs, radius: m.radius, ..Default::default() };
    for lod in &m.lods {
        let first = out.surfaces.len() as u32;
        for s in lod.first_surface..lod.first_surface + lod.surface_count {
            let Some(surface) = m.surfaces.get(s) else { continue };
            let texture = tex.get(s).cloned().flatten();
            if texture.as_ref().is_some_and(|t| looks.dropped.contains(t)) {
                continue;
            }
            let mat = m.materials.get(s).cloned().flatten();
            let material = material_record(mat.as_ref(), texture.as_deref(), looks);
            out.materials.push(material);
            let vertices = surface.vertices.iter().map(vertex).collect();
            let indices = surface.triangles.iter().flatten().copied().collect();
            out.surfaces.push(nm::Surface { material: out.materials.len() as u32 - 1, vertices, indices });
        }
        out.lods.push(nm::Lod { distance: lod.distance, first_surface: first, surface_count: out.surfaces.len() as u32 - first });
    }
    out
}

/// One BO2 material as the blob records it, with `texture` (the converted colour map's file name)
/// resolved by the caller. Shared by the model conversion and the camo art conversion.
pub(crate) fn material_record(mat: Option<&xmodel::Material>, texture: Option<&str>, looks: &Looks) -> nm::Material {
    let b = mat.map_or(Blend::Opaque, |m| blend(m.blend()));
    nm::Material {
        name: mat.map_or_else(String::new, |m| m.name.clone()),
        blend: b,
        emissive: matches!(b, Blend::Additive { .. }),
        color: texture.map(|t| format!("Art\\Textures\\{t}")),
        maps: texture.and_then(|t| looks.maps.get(t)).map(|t| format!("Art\\Textures\\{t}")),
        sort_key: mat.map_or(0, |m| m.sort_key),
        // Surfaces BO2 leaves to code (no material) stay double-sided, as they drew.
        cull: mat.map_or(nm::Cull::None, |m| cull(m.cull())),
        camo: mat.map_or(nm::CamoLayer::None, camo_layer),
    }
}

/// Which camo layer a surface carries, from the semantic-22 image's name (research §2.1).
pub(crate) fn camo_layer(m: &xmodel::Material) -> nm::CamoLayer {
    match m.image(xmodel::TS_CAMO_MAP).map(|i| i.name.as_str()) {
        Some("camo_off_solid") => nm::CamoLayer::Solid,
        Some("camo_off_pattern") => nm::CamoLayer::Pattern,
        Some("weapon_camo_neutral") => nm::CamoLayer::Neutral,
        _ => nm::CamoLayer::None,
    }
}

/// A vertex with its four heaviest influences, renormalized (BO2 skins with at most four). The
/// blob refuses non-finite values, so anything that ever decodes non-finite becomes a neutral
/// value here: the origin, an up normal, a zero UV, no weight.
fn vertex(v: &xmodel::Vertex) -> nm::Vertex {
    let finite = |a: &[f32]| a.iter().all(|c| c.is_finite());
    let mut w: Vec<(u16, f32)> = v.weights.iter().copied().filter(|x| x.1.is_finite() && x.1 > 0.0).collect();
    w.sort_by(|a, b| b.1.total_cmp(&a.1));
    w.truncate(4);
    let total: f32 = w.iter().map(|x| x.1).sum();
    let mut out = nm::Vertex {
        position: if finite(&v.position) { v.position } else { [0.0; 3] },
        normal: if finite(&v.normal) { v.normal } else { [0.0, 0.0, 1.0] },
        uv: if finite(&v.uv) { v.uv } else { [0.0; 2] },
        ..Default::default()
    };
    if w.is_empty() || total <= 1e-6 {
        out.weights[0] = 1.0;
        return out;
    }
    for (k, (bone, weight)) in w.into_iter().enumerate() {
        out.bones[k] = bone;
        out.weights[k] = weight / total;
    }
    out
}

pub(crate) fn anim(a: &xanim::Anim) -> zhbo_native::anim::Anim {
    use zhbo_native::anim::{Anim, BoneTrack, Note};
    Anim {
        name: a.name.clone(),
        num_frames: a.num_frames,
        frame_rate: a.frame_rate,
        looping: a.looping,
        delta: a.delta,
        asset_type: a.asset_type,
        bones: a.bones.iter().map(|b| BoneTrack { name: b.name.clone(), rotation: b.rotation.clone(), translation: b.translation.clone() }).collect(),
        notes: a.notifies.iter().map(|n| Note { name: n.name.clone(), time: n.time }).collect(),
    }
}

/// Where the aim marker goes (model space of the composed viewmodel, inches), by the W3D path's
/// rule: an optic's window (`OPTIC_WINDOW` of the way up it, above its root; the base scope or a
/// mounted attachment optic), else BO2's own `tag_sights`, else `tag_sights_on`, else the front
/// sight found from the gun's shape. Returned as a marker under the gun's root bone (`gun_root`).
/// A `_scope_view` optic instead reads its root bone, the scope's authored `tag_scope`.
/// The optic is `(part index, model mins, model maxs, model name)`.
pub(crate) fn aim_marker(skel: &Skeleton, gun: &xmodel::Model, optic: Option<(usize, [f32; 3], [f32; 3], &str)>) -> Option<Marker> {
    let gun_root = skel.parts.get(1)?.first;
    let bind = |i: usize| skel.bones[i].bind;
    let at = if let Some((part, mins, maxs, model)) = optic {
        if model.contains("_scope_view") {
            Some(bind(skel.parts.get(part)?.first).1)
        } else {
            let root = skel.parts.get(part)?.first;
            let mid = mins[2] + (maxs[2] - mins[2]) * crate::generals::OPTIC_WINDOW;
            Some(apply(bind(root), [0.0, 0.0, mid]))
        }
    } else {
        skel.bone("tag_sights").or_else(|| skel.bone("tag_sights_on")).map(|i| bind(i).1).or_else(|| {
            // The fallback sight anchor follows the gun's mount: the gun part's first bone
            // (`j_gun`) hangs from the part's mount tag (`tag_weapon` for primaries,
            // `tag_knife_attach` for off-hand equipment). A part with no tag hangs from the
            // base's origin, so fall back to `tag_weapon` only when the parent is absent.
            let mount = skel.bones.get(gun_root).and_then(|b| b.parent);
            let weapon = mount.map_or_else(|| skel.bone("tag_weapon").map_or(zhbo_native::math::IDENTITY, bind), bind);
            crate::generals::sight_point(gun).map(|p| apply(weapon, p))
        })
    }?;
    let local = compose(inverse(bind(gun_root)), (QUAT_IDENTITY, at));
    Some(Marker { name: AIM_TAG.into(), parent: skel.bones[gun_root].name.clone(), local })
}

/// Write `m` once under its native path.
pub(crate) fn put_model(out: &mut BTreeMap<String, Vec<u8>>, m: &nm::Model) -> String {
    let path = zhbo_native::model_path(&m.name);
    out.entry(path.clone()).or_insert_with(|| m.to_bytes());
    path
}

pub(crate) fn put_anim(out: &mut BTreeMap<String, Vec<u8>>, a: &xanim::Anim) -> String {
    let path = zhbo_native::anim_path(&a.name);
    out.entry(path.clone()).or_insert_with(|| anim(a).to_bytes());
    path
}

/// What `viewmodels::weapon_viewmodels` knows about one converted weapon's viewmodel.
pub(crate) struct Viewmodel<'a> {
    pub vm: i32,
    pub id: &'a str,
    pub gun: &'a xmodel::Model,
    /// The bone the gun hangs from: `tag_weapon` for primaries, `tag_knife_attach` for off-hand
    /// equipment (the left palm).
    pub gun_tag: &'a str,
    /// The base parts (the magazine and the base attachment art), pushed after the gun and before
    /// the attachments.
    pub base_parts: Vec<BasePart<'a>>,
    /// The attachment models found, in order, with their tag, offset (inches), whether each is an
    /// optic and its ADS role.
    pub extra_models: Vec<&'a xmodel::Model>,
    pub attachments: Vec<AttachmentPart>,
    pub hide: &'a [String],
    pub clips: &'a [(u16, String)],
    /// The left gun of a dual-wield pair: the `_lh` model as a second rig on the left palm.
    pub dual: Option<DualGun<'a>>,
}

/// One of a weapon's base `attachViewModel` parts: the magazine or the base attachment art (the
/// snipers' scope). It carries its own role flags so the composition and the aim marker stay
/// data-driven.
pub(crate) struct BasePart<'a> {
    pub model: &'a nm::Model,
    /// The authored tag; empty/absent hangs the part at the gun root (`j_gun`), like the magazine.
    pub tag: Option<String>,
    /// The authored offset, in inches.
    pub offset: zhbo_native::math::Xf,
    /// The magazine channel: keep its rigid copy on `tag_clip1` when the gun has one.
    pub magazine: bool,
    /// The model carries the weapon's sight: it drives the aim marker when no attachment optic is
    /// mounted.
    pub optic: bool,
    /// BO2's `attachViewModel[7]` (the M32's `_ads` sight): drawn only while aiming.
    pub ads_only: bool,
    /// The base part (an index into the same list) this ADS-only part replaces while aiming:
    /// slot 7 replaces slot 0 (G14 §3).
    pub ads_of: Option<usize>,
}

/// One attachment model of a viewmodel: where it hangs and its roles.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AttachmentPart {
    /// The unique's `viewModelTag`; `None` (or a bone the composition lacks) mounts at `j_gun`.
    pub tag: Option<String>,
    /// The unique's view offset and rotation, in inches.
    pub offset: crate::xf::Xf,
    /// The attachment's optic: it drives the aim marker.
    pub optic: bool,
    /// A unique's `viewModelADS`: drawn only while aiming.
    pub ads_only: bool,
    /// The attachment model (an index into the same list) this ADS-only model replaces while
    /// aiming: the unique's own `viewModel`.
    pub ads_of: Option<usize>,
}

/// The left-hand companion of a dual-wield pair: its own model (bones suffixed `1`) and mount.
pub(crate) struct DualGun<'a> {
    pub gun: &'a nm::Model,
    /// The viewhands bone it hangs from (the plan's default `tag_knife_attach`).
    pub tag: &'a str,
    /// The `_lh` weapon's hide tags (its bones carry the `1` suffix).
    pub hide: &'a [String],
}

/// One manifest per faction for weapon `v`: the faction's arms, the gun on its mount tag
/// (`tag_weapon` for primaries, `tag_knife_attach` for off-hand equipment), the base parts on the
/// gun root (`j_gun`) at their authored offsets (the magazine and, when the combination does not
/// replace it, the base attachment art — the magazine also keeps its rigid copy on `tag_clip1`,
/// where the reload brings the new one in), each attachment on its `viewModelTag` (else the gun's
/// root) at its view offset (an IR mount is a second part at its add offset), and the aim marker
/// (a base optic's window, else an attachment optic's,
/// else the gun's own sights). Model blobs are written once each.
pub(crate) fn viewmodels(
    out: &mut BTreeMap<String, Vec<u8>>,
    v: &Viewmodel,
    gun: &nm::Model,
    base: &[nm::Model],
    extra: &[nm::Model],
    factions: &[(char, nm::Model)],
) {
    use zhbo_native::math::IDENTITY;
    use zhbo_native::viewmodel::{Part, Viewmodel as Manifest};
    if gun.bones.is_empty() { return; }
    let gun_path = put_model(out, gun);
    let base_paths: Vec<String> = base.iter().map(|m| put_model(out, m)).collect();
    let extra_paths: Vec<String> = extra.iter().map(|m| put_model(out, m)).collect();
    let dual_path = v.dual.as_ref().map(|d| put_model(out, d.gun));
    for (letter, arms) in factions {
        let arms_path = put_model(out, arms);
        // (model, tag, offset, rigid, ads_only, path) in composition order.
        type PartRow<'a> = (&'a nm::Model, Option<String>, zhbo_native::math::Xf, bool, bool, String);
        // (ADS part, the hip part it replaces) as composition indices: `Part::ads_swap`.
        let mut swaps: Vec<(usize, usize)> = Vec::new();
        let mut parts: Vec<PartRow> =
            vec![(arms, None, IDENTITY, false, false, arms_path), (gun, Some(v.gun_tag.into()), IDENTITY, false, false, gun_path.clone())];
        let has = |name: &str, parts: &[PartRow]| parts.iter().any(|p| p.0.bone(name).is_some());
        // The base parts after the gun and before the attachments: `aim_marker` reads `parts[1]`
        // as the gun, and an attachment optic later overrides a base optic in `optic`.
        let mut optic = None;
        let mut base_at = Vec::with_capacity(v.base_parts.len());
        for (bp, path) in v.base_parts.iter().zip(&base_paths) {
            let tag = bp.tag.clone().filter(|t| has(t, &parts)).unwrap_or_else(|| "j_gun".into());
            // The last optic drives the marker: an ADS model after its hip model, so the eye lines
            // up with the sight drawn at full ADS.
            if bp.optic {
                optic = Some((parts.len(), bp.model.mins, bp.model.maxs, bp.model.name.as_str()));
            }
            if let Some(hip) = bp.ads_of.and_then(|k| base_at.get(k).copied()) {
                swaps.push((parts.len(), hip));
            }
            base_at.push(parts.len());
            parts.push((bp.model, Some(tag), bp.offset, false, bp.ads_only, path.clone()));
            if bp.magazine && has("tag_clip1", &parts) {
                parts.push((bp.model, Some("tag_clip1".into()), IDENTITY, true, false, path.clone()));
            }
        }
        let mut extra_at = Vec::with_capacity(v.attachments.len());
        for (k, (a, (model, path))) in v.attachments.iter().zip(extra.iter().zip(&extra_paths)).enumerate() {
            // A missing tag mounts at the gun's root (`j_gun`), BO2's default (G10): the authored
            // offset is gun-local and applies unchanged in `j_gun`'s frame. The gun composes before
            // its attachments, so `j_gun` holds; a gun blob without it hangs the part from the base
            // origin (`pose.rs`'s missing-tag rule).
            let tag = a.tag.clone().filter(|t| has(t, &parts)).or_else(|| has("j_gun", &parts).then(|| "j_gun".into()));
            if a.optic {
                optic = Some((parts.len(), v.extra_models[k].mins, v.extra_models[k].maxs, v.extra_models[k].name.as_str()));
            }
            if let Some(hip) = a.ads_of.and_then(|k| extra_at.get(k).copied()) {
                swaps.push((parts.len(), hip));
            }
            extra_at.push(parts.len());
            parts.push((model, tag, a.offset, false, a.ads_only, path.clone()));
        }
        // The left gun is a second rig on the left palm, not a merge into the base hierarchy: its
        // `1`-suffixed bones append after every base part and hang from the mount tag.
        if let (Some(dual), Some(path)) = (&v.dual, &dual_path) {
            parts.push((dual.gun, Some(dual.tag.to_string()), IDENTITY, false, false, path.clone()));
        }
        let skel = skeleton(&parts.iter().map(|p| (p.0, p.1.as_deref(), p.2, p.3)).collect::<Vec<_>>());
        let markers = aim_marker(&skel, v.gun, optic).into_iter().collect();
        let mut hide = v.hide.to_vec();
        if let Some(dual) = &v.dual {
            hide.extend(dual.hide.iter().cloned());
        }
        hide.sort();
        hide.dedup();
        let manifest = Manifest {
            id: v.id.to_string(),
            faction: *letter as u8,
            parts: parts
                .iter()
                .enumerate()
                .map(|(i, p)| Part {
                    model: p.5.clone(),
                    tag: p.1.clone(),
                    offset: p.2,
                    rigid: p.3,
                    ads_only: p.4,
                    ads_swap: swaps.iter().find(|s| s.1 == i).map(|s| s.0 as u16),
                })
                .collect(),
            markers,
            hide,
            clips: v.clips.to_vec(),
        };
        out.insert(zhbo_native::viewmodel_path(v.vm, *letter), manifest.to_bytes());
    }
}

/// Compose parts the way the core will, to place markers in the converter.
pub(crate) fn skeleton(parts: &[(&nm::Model, Option<&str>, zhbo_native::math::Xf, bool)]) -> Skeleton {
    let refs: Vec<PartRef> = parts.iter().map(|&(model, tag, offset, rigid)| PartRef { model, tag, offset, rigid }).collect();
    Skeleton::compose(&refs, &[], &[]).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use zhbo_formats::xmodel::{Bone, Lod, Surface, Vertex};

    fn gun() -> xmodel::Model {
        let bone = |name: &str, parent: Option<usize>, pos: [f32; 3]| Bone { name: name.into(), parent, local_rotation: QUAT_IDENTITY, local_position: pos, rotation: QUAT_IDENTITY, position: pos };
        let v = |p: [f32; 3], weights: Vec<(u16, f32)>| Vertex { position: p, normal: [0.0, 0.0, 1.0], uv: [0.0; 2], weights };
        xmodel::Model {
            name: "t6_wpn_test_view".into(),
            bones: vec![bone("tag_weapon", None, [0.0; 3]), bone("j_gun", Some(0), [1.0, 0.0, 0.0])],
            surfaces: vec![Surface {
                vertices: vec![
                    v([0.0, 0.0, 0.0], vec![(0, 1.0)]),
                    v([30.0, 0.0, 2.0], vec![(1, 0.2), (0, 0.6), (0, 0.1), (1, 0.05), (0, 0.05)]),
                    v([20.0, 0.0, 4.0], vec![]),
                ],
                triangles: vec![[0, 1, 2]],
            }],
            lods: vec![Lod { distance: 0.0, first_surface: 0, surface_count: 1 }],
            radius: 30.0,
            mins: [0.0; 3],
            maxs: [30.0, 0.0, 4.0],
            materials: vec![None],
        }
    }

    #[test]
    fn the_semantic_22_layer_classifies_the_surface() {
        let image = |name: &str| xmodel::Image {
            name: name.into(),
            hash: 0,
            stream_hash: 1,
            streamed_parts: 1,
            width: 8,
            height: 8,
        };
        let mat = |name: &str| xmodel::Material {
            name: "mtl".into(),
            textures: vec![xmodel::Texture { semantic: xmodel::TS_CAMO_MAP, image: Some(image(name)) }],
            state_bits: Vec::new(),
            sort_key: 0,
        };
        assert_eq!(camo_layer(&mat("camo_off_solid")), nm::CamoLayer::Solid);
        assert_eq!(camo_layer(&mat("camo_off_pattern")), nm::CamoLayer::Pattern);
        assert_eq!(camo_layer(&mat("weapon_camo_neutral")), nm::CamoLayer::Neutral);
        assert_eq!(camo_layer(&mat("t6_camo_atacs_pattern")), nm::CamoLayer::None);
    }

    #[test]
    fn converts_a_model_with_four_normalized_influences() {
        let mut looks = Looks::default();
        looks.maps.insert("gun.tga".into(), "gun_nsg.tga".into());
        let m = model(&gun(), &[Some("gun.tga".into())], &looks);
        let back = nm::Model::from_bytes(&m.to_bytes()).expect("a valid blob");
        assert_eq!(back.surfaces[0].indices, vec![0, 1, 2]);
        // The blob never holds a surface with no indices: an empty surface can never draw, and the
        // composer's hide rule is what empties a surface, never the converter.
        assert!(back.surfaces.iter().all(|s| !s.indices.is_empty()), "the converter wrote an empty surface");
        let w = back.surfaces[0].vertices[1].weights;
        assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5 && w[0] >= w[1], "{w:?}");
        assert_eq!(back.surfaces[0].vertices[2].weights, [1.0, 0.0, 0.0, 0.0], "no weights: the first bone");
        assert_eq!(back.materials[0].maps.as_deref(), Some("Art\\Textures\\gun_nsg.tga"));
        // A dropped texture drops its surface.
        looks.dropped.insert("gun.tga".into());
        assert!(model(&gun(), &[Some("gun.tga".into())], &looks).surfaces.is_empty());
    }

    /// R4: the material's `cullFace` carries into the blob; a surface BO2 left to code (no
    /// material) stays double-sided, as it drew before culling was read.
    #[test]
    fn surfaces_cull_as_their_materials_do() {
        let mut g = gun();
        g.materials = vec![Some(xmodel::Material { name: "mtl".into(), textures: vec![], state_bits: vec![None, None, Some([0x0000_C000, 0])], sort_key: 0 })];
        assert_eq!(model(&g, &[None], &Looks::default()).materials[0].cull, nm::Cull::Front, "GFXS0_CULL_FRONT (bits 14-15 = 3)");
        assert_eq!(model(&gun(), &[None], &Looks::default()).materials[0].cull, nm::Cull::None);
    }

    /// The native pose agrees with the W3D rig (verified in game) bone by bone: the same weapon,
    /// arms and clip posed both ways, model space, at several frames.
    #[test]
    #[ignore = "reads this machine's Black Ops II install (ZHBO_BO2_DIR)"]
    fn native_pose_matches_the_w3d_rig() {
        use crate::character::{animate, Rig, SCALE};
        use crate::xf;
        let bo2 = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let common = crate::paks::load_zone(&bo2, "common_mp").unwrap();
        let patch = crate::paks::load_zone(&bo2, "patch_mp").unwrap();
        let faction = crate::paks::load_zone(&bo2, crate::generals::FACTIONS[0].zone).unwrap();
        let hands = xmodel::find(&faction, crate::generals::FACTIONS[0].hands).unwrap();
        let weapons = zhbo_formats::weapon::all(&common).unwrap();
        for (id, slot) in [("kard_mp", 1usize), ("kard_mp", 0x17), ("an94_mp", 1), ("an94_mp", 0x10)] {
            let w = weapons.iter().find(|w| w.name == id).unwrap();
            let gun = crate::viewmodels::find_model(&patch, &common, w.view_model.as_deref().unwrap()).unwrap();
            let mag = crate::viewmodels::magazine(&common, w.view_model.as_deref().unwrap());
            let mut parts: Vec<(&xmodel::Model, &str)> = vec![(&gun, "tag_weapon")];
            if let Some(m) = &mag {
                parts.push((m, "j_gun"));
            }
            let rig = Rig::new(&hands, &parts);
            let Some(anim_name) = w.anims.get(slot).cloned().flatten() else { continue };
            let anim = xanim::find(&common, &anim_name).unwrap();
            let (_, channels) = animate(&rig, &anim);
            let looks = Looks::default();
            let nh = model(&hands, &vec![None; hands.surfaces.len()], &looks);
            let ng = model(&gun, &vec![None; gun.surfaces.len()], &looks);
            let nmag = mag.as_ref().map(|m| model(m, &vec![None; m.surfaces.len()], &looks));
            let mut np: Vec<(&nm::Model, Option<&str>, zhbo_native::math::Xf, bool)> = vec![(&nh, None, zhbo_native::math::IDENTITY, false), (&ng, Some("tag_weapon"), zhbo_native::math::IDENTITY, false)];
            if let Some(m) = &nmag {
                np.push((m, Some("j_gun"), zhbo_native::math::IDENTITY, false));
            }
            let skel = skeleton(&np);
            let na = super::anim(&anim);
            let mut worst = (0.0f32, String::new(), 0u16);
            for f in [0u16, anim.num_frames / 2, anim.num_frames] {
                // W3D: pivot local = base * channel delta, composed from ROOTTRANSFORM.
                let mut world = vec![xf::IDENTITY; rig.pivots.len()];
                for (i, p) in rig.pivots.iter().enumerate().skip(1) {
                    let mut local = (p.rotation, p.translation);
                    if let Some(ch) = channels.iter().find(|c| c.pivot as usize == i) {
                        let (dq, dt) = (ch.rotations[f as usize], ch.translations[f as usize]);
                        local = (xf::qnorm(xf::qmul(p.rotation, dq)), { let t = xf::qrot(p.rotation, dt); [p.translation[0] + t[0], p.translation[1] + t[1], p.translation[2] + t[2]] });
                    }
                    world[i] = xf::compose(world[p.parent.unwrap_or(0)], local);
                }
                let (mut local, mut ms) = (Vec::new(), Vec::new());
                skel.sample(&na, f as f32, &mut local);
                skel.model_space(&local, &mut ms);
                // Pivot k (skipping ROOTTRANSFORM) is native bone k - 1: same order, same names.
                for (k, b) in skel.bones.iter().enumerate() {
                    assert_eq!(rig.names[k + 1], b.name);
                    let n = ms[k].1.map(|c| c * SCALE);
                    let d = (0..3).map(|c| (n[c] - world[k + 1].1[c]).powi(2)).sum::<f32>().sqrt();
                    if d > worst.0 {
                        worst = (d, b.name.clone(), f);
                    }
                }
            }
            println!("{id} slot {slot:#x} {anim_name}: worst bone {} off by {:.4} units at frame {}", worst.1, worst.0, worst.2);
            assert!(worst.0 < 0.01, "{id} {anim_name}: {} off by {} at frame {}", worst.1, worst.0, worst.2);
        }
    }

    #[test]
    fn aim_marker_falls_back_to_the_front_sight() {
        let arms = nm::Model { name: "arms".into(), bones: vec![nm::Bone { name: "tag_weapon".into(), parent: None, local_rotation: QUAT_IDENTITY, local_position: [0.0; 3], rotation: QUAT_IDENTITY, position: [5.0, 0.0, 0.0] }], ..Default::default() };
        let g = gun();
        let gm = model(&g, &[None], &Looks::default());
        let skel = skeleton(&[(&arms, None, zhbo_native::math::IDENTITY, false), (&gm, Some("tag_weapon"), zhbo_native::math::IDENTITY, false)]);
        let m = aim_marker(&skel, &g, None).expect("a sight");
        assert_eq!(m.parent, "tag_weapon");
        // The front third's highest vertex (20, 0, 4) in the gun's frame, which sits at x = 5.
        assert!((m.local.1[0] - 20.0).abs() < 1e-4 && (m.local.1[2] - 4.0).abs() < 1e-4, "{:?}", m.local.1);
    }

    /// A `tag=None` attachment mounts at the gun's root (`j_gun`), BO2's default (G10), with its
    /// authored offset unchanged. The offset is gun-local and applies in `j_gun`'s frame.
    #[test]
    fn tag_none_attachment_resolves_to_j_gun() {
        let arms = nm::Model { name: "arms".into(), bones: vec![nm::Bone { name: "tag_weapon".into(), parent: None, local_rotation: QUAT_IDENTITY, local_position: [0.0; 3], rotation: QUAT_IDENTITY, position: [5.0, 0.0, 0.0] }], ..Default::default() };
        let g = gun();
        let nm_g = model(&g, &[None], &Looks::default());
        let nm_extra = model(&g, &[None], &Looks::default());
        let authored = (crate::xf::IDENTITY.0, [1.0, 2.0, 3.0]);
        let v = Viewmodel {
            vm: 0,
            id: "test_mp",
            gun: &g,
            gun_tag: "tag_weapon",
            base_parts: Vec::new(),
            extra_models: vec![&g],
            attachments: vec![AttachmentPart { tag: None, offset: authored, optic: false, ads_only: false, ads_of: None }],
            hide: &[],
            clips: &[],
            dual: None,
        };
        let mut out = BTreeMap::new();
        viewmodels(&mut out, &v, &nm_g, &[], &[nm_extra], &[('A', arms)]);
        let path = zhbo_native::viewmodel_path(0, 'A');
        let manifest = zhbo_native::viewmodel::Viewmodel::from_bytes(out.get(&path).unwrap()).unwrap();
        let part = manifest.parts.iter().find(|p| p.tag.as_deref() == Some("j_gun")).expect("the attachment mounts at j_gun");
        assert_eq!(part.offset, authored, "the authored offset is unchanged");
    }

    /// A `_scope_view` optic's marker reads the part's `tag_scope` bone, not the computed window.
    #[test]
    fn aim_marker_prefers_tag_scope_for_scope_view_optics() {
        let mut scope = gun();
        scope.name = "t6_wpn_sniper_test_scope_view".into();
        scope.bones[0].name = "tag_scope".into();
        scope.mins = [0.0; 3];
        scope.maxs = [30.0, 0.0, 4.0];
        let scope_nm = model(&scope, &[None], &Looks::default());
        let gun_model = gun();
        let gun_nm = model(&gun_model, &[None], &Looks::default());
        let arms = nm::Model {
            name: "arms".into(),
            bones: vec![nm::Bone { name: "tag_weapon".into(), parent: None, local_rotation: QUAT_IDENTITY, local_position: [0.0; 3], rotation: QUAT_IDENTITY, position: [0.0; 3] }],
            ..Default::default()
        };
        let skel = skeleton(&[
            (&arms, None, zhbo_native::math::IDENTITY, false),
            (&gun_nm, Some("tag_weapon"), zhbo_native::math::IDENTITY, false),
            (&scope_nm, Some("j_gun"), zhbo_native::math::IDENTITY, false),
        ]);
        let m = aim_marker(&skel, &gun_model, Some((2, scope.mins, scope.maxs, "t6_wpn_sniper_test_scope_view"))).expect("a sight");
        assert_eq!(m.parent, "tag_weapon");
        assert!((m.local.1[0] - 1.0).abs() < 1e-4, "{:?}", m.local.1);
        assert!((m.local.1[2] - 0.0).abs() < 1e-4, "{:?}", m.local.1);
    }

    /// A non-`_scope_view` optic (the M32's sight family) keeps the computed window point.
    #[test]
    fn aim_marker_keeps_the_window_for_a_non_scope_view_optic() {
        let mut sight = gun();
        sight.name = "t6_attach_optic_m32_view".into();
        sight.bones[0].name = "tag_scope".into();
        sight.mins = [0.0; 3];
        sight.maxs = [30.0, 0.0, 4.0];
        let sight_nm = model(&sight, &[None], &Looks::default());
        let gun_model = gun();
        let gun_nm = model(&gun_model, &[None], &Looks::default());
        let arms = nm::Model {
            name: "arms".into(),
            bones: vec![nm::Bone { name: "tag_weapon".into(), parent: None, local_rotation: QUAT_IDENTITY, local_position: [0.0; 3], rotation: QUAT_IDENTITY, position: [0.0; 3] }],
            ..Default::default()
        };
        let skel = skeleton(&[
            (&arms, None, zhbo_native::math::IDENTITY, false),
            (&gun_nm, Some("tag_weapon"), zhbo_native::math::IDENTITY, false),
            (&sight_nm, Some("j_gun"), zhbo_native::math::IDENTITY, false),
        ]);
        let m = aim_marker(&skel, &gun_model, Some((2, sight.mins, sight.maxs, "t6_attach_optic_m32_view"))).expect("a sight");
        assert_eq!(m.parent, "tag_weapon");
        assert!((m.local.1[0] - 1.0).abs() < 1e-4, "{:?}", m.local.1);
        assert!((m.local.1[2] - 2.88).abs() < 1e-3, "{:?}", m.local.1);
    }

}
