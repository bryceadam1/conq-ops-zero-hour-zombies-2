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

/// The marker the native viewmodel lines up with the eye when aiming down the sights: BO2's
/// `tag_sights`, an optic's window, or the gun's front sight (the same choice the W3D path makes,
/// `viewmodels::weapon_viewmodels`). A name of its own, so it never collides with BO2's bones.
pub const AIM_TAG: &str = "tag_zhbo_aim";

fn blend(b: w3d::Blend) -> Blend {
    match b {
        w3d::Blend::Opaque => Blend::Opaque,
        w3d::Blend::AlphaTest => Blend::AlphaTest,
        w3d::Blend::Alpha => Blend::Alpha,
        w3d::Blend::Premultiplied => Blend::Premultiplied,
        w3d::Blend::Additive { alpha } => Blend::Additive { alpha },
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
            let b = mat.as_ref().map_or(Blend::Opaque, |mat| blend(mat.blend()));
            let material = nm::Material {
                name: mat.as_ref().map_or_else(String::new, |mat| mat.name.clone()),
                blend: b,
                emissive: matches!(b, Blend::Additive { .. }),
                color: texture.as_ref().map(|t| format!("Art\\Textures\\{t}")),
                maps: texture.as_ref().and_then(|t| looks.maps.get(t)).map(|t| format!("Art\\Textures\\{t}")),
                sort_key: mat.as_ref().map_or(0, |mat| mat.sort_key),
            };
            out.materials.push(material);
            let vertices = surface.vertices.iter().map(vertex).collect();
            let indices = surface.triangles.iter().flatten().copied().collect();
            out.surfaces.push(nm::Surface { material: out.materials.len() as u32 - 1, vertices, indices });
        }
        out.lods.push(nm::Lod { distance: lod.distance, first_surface: first, surface_count: out.surfaces.len() as u32 - first });
    }
    out
}

/// A vertex with its four heaviest influences, renormalized (BO2 skins with at most four). Some
/// of BO2's own vertices decode with non-finite values (a few viewhands' half-float weights and
/// unit vectors); the blob refuses those, so they become a neutral value here: the origin, an up
/// normal, a zero UV, no weight.
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
/// rule: an optic's window (`OPTIC_WINDOW` of the way up it, above its root), else BO2's own
/// `tag_sights`, else `tag_sights_on`, else the front sight found from the gun's shape. Returned
/// as a marker under the gun's root bone (`gun_root`).
pub(crate) fn aim_marker(skel: &Skeleton, gun: &xmodel::Model, optic: Option<(usize, &xmodel::Model)>) -> Option<Marker> {
    let gun_root = skel.parts.get(1)?.first;
    let bind = |i: usize| skel.bones[i].bind;
    let at = if let Some((part, optic)) = optic {
        let root = skel.parts.get(part)?.first;
        let mid = optic.mins[2] + (optic.maxs[2] - optic.mins[2]) * crate::generals::OPTIC_WINDOW;
        Some(apply(bind(root), [0.0, 0.0, mid]))
    } else {
        skel.bone("tag_sights").or_else(|| skel.bone("tag_sights_on")).map(|i| bind(i).1).or_else(|| {
            let weapon = skel.bone("tag_weapon").map_or(zhbo_native::math::IDENTITY, bind);
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
    /// The attachment models found, in order, with their tag, offset (inches) and whether each
    /// is an optic.
    pub extra_models: Vec<&'a xmodel::Model>,
    pub attachments: Vec<(Option<String>, crate::xf::Xf, bool)>,
    pub hide: &'a [String],
    pub clips: &'a [(u16, String)],
}

/// One manifest per faction for weapon `v`: the faction's arms, the gun on `tag_weapon`, the
/// magazine on `j_gun` (and its rigid copy on `tag_clip1`, where the reload brings the new one in),
/// each attachment on its `viewModelTag` (else the gun's root) at its view offset, and the aim
/// marker. Model blobs are written once each.
pub(crate) fn viewmodels(
    out: &mut BTreeMap<String, Vec<u8>>,
    v: &Viewmodel,
    gun: &nm::Model,
    mag: Option<&nm::Model>,
    extra: &[nm::Model],
    factions: &[(char, nm::Model)],
) {
    use zhbo_native::math::IDENTITY;
    use zhbo_native::viewmodel::{Part, Viewmodel as Manifest};
    let Some(gun_root) = gun.bones.first().map(|b| b.name.clone()) else { return };
    let gun_path = put_model(out, gun);
    let mag_path = mag.map(|m| put_model(out, m));
    let extra_paths: Vec<String> = extra.iter().map(|m| put_model(out, m)).collect();
    for (letter, arms) in factions {
        let arms_path = put_model(out, arms);
        // (model, tag, offset, rigid, path) in composition order.
        let mut parts: Vec<(&nm::Model, Option<String>, zhbo_native::math::Xf, bool, String)> =
            vec![(arms, None, IDENTITY, false, arms_path), (gun, Some("tag_weapon".into()), IDENTITY, false, gun_path.clone())];
        if let (Some(m), Some(p)) = (mag, &mag_path) {
            parts.push((m, Some("j_gun".into()), IDENTITY, false, p.clone()));
        }
        let has = |name: &str, parts: &[(&nm::Model, Option<String>, zhbo_native::math::Xf, bool, String)]| parts.iter().any(|p| p.0.bone(name).is_some());
        if let (Some(m), Some(p)) = (mag, &mag_path) {
            if has("tag_clip1", &parts) {
                parts.push((m, Some("tag_clip1".into()), IDENTITY, true, p.clone()));
            }
        }
        let mut optic = None;
        for (k, ((tag, offset, is_optic), (model, path))) in v.attachments.iter().zip(extra.iter().zip(&extra_paths)).enumerate() {
            let tag = tag.clone().filter(|t| has(t, &parts)).unwrap_or_else(|| gun_root.clone());
            if *is_optic {
                optic = Some((parts.len(), v.extra_models[k]));
            }
            parts.push((model, Some(tag), *offset, false, path.clone()));
        }
        let skel = skeleton(&parts.iter().map(|p| (p.0, p.1.as_deref(), p.2, p.3)).collect::<Vec<_>>());
        let markers = aim_marker(&skel, v.gun, optic).into_iter().collect();
        let manifest = Manifest {
            id: v.id.to_string(),
            faction: *letter as u8,
            parts: parts.iter().map(|p| Part { model: p.4.clone(), tag: p.1.clone(), offset: p.2, rigid: p.3 }).collect(),
            markers,
            hide: v.hide.to_vec(),
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
    fn converts_a_model_with_four_normalized_influences() {
        let mut looks = Looks::default();
        looks.maps.insert("gun.tga".into(), "gun_nsg.tga".into());
        let m = model(&gun(), &[Some("gun.tga".into())], &looks);
        let back = nm::Model::from_bytes(&m.to_bytes()).expect("a valid blob");
        assert_eq!(back.surfaces[0].indices, vec![0, 1, 2]);
        let w = back.surfaces[0].vertices[1].weights;
        assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5 && w[0] >= w[1], "{w:?}");
        assert_eq!(back.surfaces[0].vertices[2].weights, [1.0, 0.0, 0.0, 0.0], "no weights: the first bone");
        assert_eq!(back.materials[0].maps.as_deref(), Some("Art\\Textures\\gun_nsg.tga"));
        // A dropped texture drops its surface.
        looks.dropped.insert("gun.tga".into());
        assert!(model(&gun(), &[Some("gun.tga".into())], &looks).surfaces.is_empty());
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
}
