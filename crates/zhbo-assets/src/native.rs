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

/// The native skeleton of `m` (its bones, bind pose).
pub(crate) fn bones(m: &xmodel::Model) -> Vec<nm::Bone> {
    m.bones
        .iter()
        .map(|b| nm::Bone {
            name: b.name.clone(),
            parent: b.parent.map(|p| p as u32),
            local_rotation: b.local_rotation,
            local_position: b.local_position,
            rotation: b.rotation,
            position: b.position,
        })
        .collect()
}

/// `m` as a native model: the textures the W3D pass already wrote for its surfaces (`tex`, from
/// `textures::textures`), the packed maps and blends `looks` holds for them. Surfaces the W3D pass
/// leaves out (`looks.dropped`) are left out here too, except the ones only this pass can draw
/// (`looks.native_only`: the lens reticle, the clan tag, the emblem).
pub(crate) fn model(m: &xmodel::Model, tex: &[Option<String>], looks: &Looks) -> nm::Model {
    let mut out = nm::Model { name: m.name.clone(), bones: bones(m), mins: m.mins, maxs: m.maxs, radius: m.radius, ..Default::default() };
    for lod in &m.lods {
        let first = out.surfaces.len() as u32;
        for s in lod.first_surface..lod.first_surface + lod.surface_count {
            let Some(surface) = m.surfaces.get(s) else { continue };
            let texture = tex.get(s).cloned().flatten();
            if texture.as_ref().is_some_and(|t| looks.dropped.contains(t) && !looks.native_only.contains(t)) {
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
        emissive: crate::textures::self_lit(mat.map_or(w3d::Blend::Opaque, |m| m.blend()), texture, looks),
        color: texture.filter(|t| !looks.no_file.contains(*t)).map(|t| format!("Art\\Textures\\{t}")),
        maps: texture.and_then(|t| looks.maps.get(t)).map(|t| format!("Art\\Textures\\{t}")),
        specular: texture.and_then(|t| looks.specular.get(t)).map(|t| format!("Art\\Textures\\{t}")),
        sort_key: mat.map_or(0, |m| m.sort_key),
        // Surfaces BO2 leaves to code (no material) stay double-sided, as they drew.
        cull: mat.map_or(nm::Cull::None, |m| cull(m.cull())),
        camo: mat.map_or(nm::CamoLayer::None, camo_layer),
        reticle: texture.and_then(|t| looks.reticle.get(t)).cloned(),
        emblem: texture.is_some_and(|t| looks.emblem.contains(t)),
        sonar: texture.and_then(|t| looks.sonar.get(t)).cloned(),
        polygon_offset: mat.map_or(0, |m| m.polygon_offset()),
        lighting: mat.map_or_else(nm::Lighting::default, lighting),
    }
}

/// BO2's character skin technique (`sw4_3d_char_skin_*`), its cloth (`sw4_3d_char_cloth_*`) and
/// the IW lit family (`lit_*`, the guns and props).
const SKIN_TECHSET: &str = "_char_skin_";
const LIT_TECHSET: &str = "_lit_";

/// The scales `m`'s lit technique reads from its constants (the `lit_sun` pixel shaders' DXBC,
/// MODLOG "BO2's lit shading, from its DXBC"); any constant the material leaves out is BO2's
/// neutral 1.
/// * `sw4_3d_char_skin` (`pimp_shader_sw4_3d_char_skin_7f1d600`): the highlight times
///   `Specular_Amount`, the reflection times `Reflection_Amount`, the diffuse sees the normal
///   map's bump times `Diffuse_Normal_Height_Facing`.
/// * `lit_*` with an occlusion map (`..o0`, `pimp_shader_lprobe_bc900d1`): the highlight times
///   `colorDetailScale.z`; the map itself occludes (its red, in the colour map's alpha).
/// * `lit_*` with a specular map and no occlusion map (`pimp_shader_lprobe_a96c9c79`):
///   `occlusionAmount` stands in for the map: `.x` the highlight and the ambient, `.y` the
///   reflection. Without a specular map the shader has neither term and reads neither.
/// * `sw4_3d_char_cloth` reads none of them.
pub(crate) fn lighting(m: &xmodel::Material) -> nm::Lighting {
    let mut out = nm::Lighting::default();
    let k = |name: &str| m.constant(name);
    if m.techset.contains(SKIN_TECHSET) {
        out.specular = k("Specular_Amount").map_or(1.0, |v| v[0]);
        out.reflection = k("Reflection_Amount").map_or(1.0, |v| v[0]);
        out.diffuse_normal = k("Diffuse_Normal_Height_Facing").map_or(1.0, |v| v[0]);
    } else if m.techset.contains(LIT_TECHSET) {
        if m.image(xmodel::TS_OCCLUSION_MAP).is_some() {
            out.specular = k("colorDetailScale").map_or(1.0, |v| v[2]);
        } else if m.image(xmodel::TS_SPECULAR_MAP).is_some() {
            if let Some([x, y, _, _]) = k("occlusionAmount") {
                (out.specular, out.ambient, out.reflection) = (x, x, y);
            }
        }
    }
    // The blob refuses a non-finite value; BO2's neutral stands in for one.
    for v in [&mut out.specular, &mut out.reflection, &mut out.ambient, &mut out.diffuse_normal] {
        if !v.is_finite() {
            *v = 1.0;
        }
    }
    out
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
    /// The def's `weaponCamo`: the class camo's asset on the gun and its base parts (G16).
    pub gun_camo: Option<String>,
    /// The base parts (the magazine and the base attachment art), pushed after the gun and before
    /// the attachments.
    pub base_parts: Vec<BasePart<'a>>,
    /// The attachment models found, in order, with their tag, offset (inches), whether each is an
    /// optic, its ADS role and its single unique's `weaponCamo` (G16).
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
    /// The authored rotation and offset (inches) from the tag.
    pub offset: zhbo_native::math::Xf,
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
    /// The single unique's `WeaponCamo` the class camo paints on this model (`FUN_00529600`, G16).
    pub camo: Option<String>,
}

/// The left-hand companion of a dual-wield pair: its own model (bones suffixed `1`) and mount.
pub(crate) struct DualGun<'a> {
    pub gun: &'a nm::Model,
    /// The viewhands bone it hangs from (the plan's default `tag_knife_attach`).
    pub tag: &'a str,
    /// The `_lh` weapon's hide tags (its bones carry the `1` suffix).
    pub hide: &'a [String],
    /// The `_lh` def's `weaponCamo`.
    pub camo: Option<String>,
}

/// One manifest per faction for weapon `v`: the faction's arms, the gun on its mount tag
/// (`tag_weapon` for primaries, `tag_knife_attach` for off-hand equipment), the base parts on the
/// gun root (`j_gun`) at their authored rotation and offset (the magazine and, when the
/// combination does not replace it, the base attachment art; BO2 lists no second magazine — the
/// reload's spare on `tag_clip1` is the gun model's own geometry), each attachment on its `viewModelTag` (else the gun's
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
        // (model, tag, offset, rigid, ads_only, path, camo) in composition order. The camo is the
        // `WeaponCamo` BO2 resolves per DObj entry (`FUN_00529600`, G16): none on the arms, the
        // def's on the gun and its base parts, the single unique's on an attachment's models.
        type PartRow<'a> = (&'a nm::Model, Option<String>, zhbo_native::math::Xf, bool, bool, String, Option<String>);
        // (ADS part, the hip part it replaces) as composition indices: `Part::ads_swap`.
        let mut swaps: Vec<(usize, usize)> = Vec::new();
        let mut parts: Vec<PartRow> = vec![
            (arms, None, IDENTITY, false, false, arms_path, None),
            (gun, Some(v.gun_tag.into()), IDENTITY, false, false, gun_path.clone(), v.gun_camo.clone()),
        ];
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
            parts.push((bp.model, Some(tag), bp.offset, false, bp.ads_only, path.clone(), v.gun_camo.clone()));
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
            parts.push((model, tag, a.offset, false, a.ads_only, path.clone(), a.camo.clone()));
        }
        // The left gun is a second rig on the left palm, not a merge into the base hierarchy: its
        // `1`-suffixed bones append after every base part and hang from the mount tag.
        if let (Some(dual), Some(path)) = (&v.dual, &dual_path) {
            parts.push((dual.gun, Some(dual.tag.to_string()), IDENTITY, false, false, path.clone(), dual.camo.clone()));
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
                    camo: p.6.clone(),
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
            hit_boxes: Vec::new(),
            coll_surfs: Vec::new(),
            materials: vec![None],
        }
    }

    /// Each lit technique's scales come from the constants its shader reads, by the install's
    /// own materials: the SEAL arm's skin (`Specular_Amount` 1.5, `Reflection_Amount` 1,
    /// `Diffuse_Normal_Height_Facing` 0.8), a gun with an occlusion map (`colorDetailScale`
    /// 8, 8, 0.7), one without (`occlusionAmount` 0.5, 0.25), the cloth (none read), and a lit
    /// surface without a specular map (its shader reads no `occlusionAmount`).
    #[test]
    fn each_lit_technique_takes_the_scales_its_shader_reads() {
        let k = |name: &str, value: [f32; 4]| xmodel::Constant { name_hash: xmodel::sampler_hash(name), value };
        let map = |semantic: u8| xmodel::Texture { name_hash: 0, semantic, image: Some(xmodel::Image { name: "m".into(), hash: 0, stream_hash: 0, streamed_parts: 0, width: 1, height: 1 }) };
        let mat = |techset: &str, textures: Vec<xmodel::Texture>, constants: Vec<xmodel::Constant>| xmodel::Material { techset: techset.into(), textures, constants, ..Default::default() };
        let skin = mat("mc_sw4_3d_char_skin_j92387z3", vec![], vec![k("Reflection_Amount", [1.0, 0.0, 0.0, 1.0]), k("Diffuse_Normal_Height_Facing", [0.8, 0.0, 0.0, 1.0]), k("Specular_Amount", [1.5, 0.0, 0.0, 1.0])]);
        assert_eq!(lighting(&skin), nm::Lighting { specular: 1.5, reflection: 1.0, ambient: 1.0, diffuse_normal: 0.8 });
        let occluded = mat("mc_lit_sm_r0c0n0s0o0_3z86zq2z", vec![map(xmodel::TS_SPECULAR_MAP), map(xmodel::TS_OCCLUSION_MAP)], vec![k("colorDetailScale", [8.0, 8.0, 0.7, 0.0]), k("occlusionAmount", [0.5, 0.25, 1.0, 1.0])]);
        assert_eq!(lighting(&occluded), nm::Lighting { specular: 0.7, ..Default::default() }, "the map occludes; occlusionAmount is not read");
        let bare = mat("mc_lit_sm_r0c0n0s0_7j414q0z", vec![map(xmodel::TS_SPECULAR_MAP)], vec![k("colorDetailScale", [8.0, 8.0, 0.7, 0.0]), k("occlusionAmount", [0.5, 0.25, 1.0, 1.0])]);
        assert_eq!(lighting(&bare), nm::Lighting { specular: 0.5, reflection: 0.25, ambient: 0.5, diffuse_normal: 1.0 });
        let matte = mat("mc_lit_sm_r0c0n0_2z223015", vec![], vec![k("occlusionAmount", [0.5, 0.25, 1.0, 1.0])]);
        assert_eq!(lighting(&matte), nm::Lighting::default());
        let cloth = mat("mc_sw4_3d_char_cloth_4z8fq5wu", vec![map(xmodel::TS_SPECULAR_MAP)], vec![k("Specular_Amount", [3.0, 0.0, 0.0, 1.0])]);
        assert_eq!(lighting(&cloth), nm::Lighting::default());
        let broken = mat("mc_sw4_3d_char_skin_j92387z3", vec![], vec![k("Specular_Amount", [f32::INFINITY, 0.0, 0.0, 1.0])]);
        assert_eq!(lighting(&broken).specular, 1.0, "a non-finite constant falls back to BO2's neutral");
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
            textures: vec![xmodel::Texture { name_hash: 0, semantic: xmodel::TS_CAMO_MAP, image: Some(image(name)) }],
            ..Default::default()
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
        looks.specular.insert("gun.tga".into(), "gun_spec.tga".into());
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
        assert_eq!(back.materials[0].specular.as_deref(), Some("Art\\Textures\\gun_spec.tga"), "the spec colour reaches the blob");
        // A dropped texture drops its surface.
        looks.dropped.insert("gun.tga".into());
        assert!(model(&gun(), &[Some("gun.tga".into())], &looks).surfaces.is_empty());
    }

    /// R4: the material's `cullFace` carries into the blob; a surface BO2 left to code (no
    /// material) stays double-sided, as it drew before culling was read.
    #[test]
    fn surfaces_cull_as_their_materials_do() {
        let mut g = gun();
        g.materials = vec![Some(xmodel::Material { name: "mtl".into(), textures: vec![], state_bits: vec![None, None, Some([0x0000_C000, 0])], sort_key: 0, ..Default::default() })];
        assert_eq!(model(&g, &[None], &Looks::default()).materials[0].cull, nm::Cull::Front, "GFXS0_CULL_FRONT (bits 14-15 = 3)");
        assert_eq!(model(&gun(), &[None], &Looks::default()).materials[0].cull, nm::Cull::None);
    }

    /// The native pose agrees with the W3D rig (verified in game) bone by bone: the same weapon,
    /// arms and clip posed both ways, model space, at several frames.
    #[test]
    #[ignore = "reads this machine's Black Ops II install (ZHBO_BO2_DIR)"]
    fn the_sights_tritium_carries_bo2s_polygon_offset() {
        // The MP7's iron sights: BO2 draws their tritium dots and glows with
        // GFXS1_POLYGON_OFFSET_1 (loadBits[1] bits 4-5) so they win over the sight they lie on; the
        // gun's own surfaces have none. Without it the dots flickered on and off as the gun swayed.
        let bo2 = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let common = crate::paks::load_zone(&bo2, "common_mp").unwrap();
        let patch = crate::paks::load_zone(&bo2, "patch_mp").unwrap();
        let (weapons, _) = crate::paks::MpPool::load(&bo2).unwrap().weapons().unwrap();
        let w = weapons.iter().find(|w| w.name == "mp7_mp").unwrap();
        let gun = crate::viewmodels::find_model(&[&patch, &common], w.view_model.as_deref().unwrap()).unwrap();
        let looks = Looks::default();
        let (mut decals, mut solid) = (0, 0);
        for m in gun.materials.iter().flatten() {
            let record = material_record(Some(m), None, &looks);
            if m.name.contains("tritium") {
                assert_eq!(record.polygon_offset, 1, "{}", m.name);
                decals += 1;
            } else if record.blend == Blend::Opaque {
                assert_eq!(record.polygon_offset, 0, "{}", m.name);
                solid += 1;
            }
        }
        assert!(decals >= 2 && solid >= 1, "{decals} tritium, {solid} opaque materials");
    }

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
        let (weapons, _) = crate::paks::MpPool::load(&bo2).unwrap().weapons().unwrap();
        for (id, slot) in [("kard_mp", 1usize), ("kard_mp", 0x17), ("an94_mp", 1), ("an94_mp", 0x10)] {
            let w = weapons.iter().find(|w| w.name == id).unwrap();
            let gun = crate::viewmodels::find_model(&[&patch, &common], w.view_model.as_deref().unwrap()).unwrap();
            let mag = crate::viewmodels::magazine(&[&common], w.view_model.as_deref().unwrap());
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
            gun_camo: None,
            base_parts: Vec::new(),
            extra_models: vec![&g],
            attachments: vec![AttachmentPart { tag: None, offset: authored, optic: false, ads_only: false, ads_of: None, camo: None }],
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

    /// The magazine is one part on the gun root at its authored rotation and offset (the AN-94's
    /// −15° roll), even when the gun carries `tag_clip1`: the reload's spare magazine there is the
    /// gun model's own geometry, and BO2 lists no second part.
    #[test]
    fn the_magazine_is_one_part_at_its_authored_rotation() {
        let arms = nm::Model { name: "arms".into(), bones: vec![nm::Bone { name: "tag_weapon".into(), parent: None, local_rotation: QUAT_IDENTITY, local_position: [0.0; 3], rotation: QUAT_IDENTITY, position: [5.0, 0.0, 0.0] }], ..Default::default() };
        let mut g = gun();
        let roll = crate::xf::from_angles([0.0, 0.0, -15.0]);
        g.bones.push(Bone { name: "tag_clip1".into(), parent: Some(1), local_rotation: roll, local_position: [-14.0, -1.085, -1.5], rotation: roll, position: [-13.0, -1.085, -1.5] });
        let mut mag = gun();
        mag.name = "t6_attach_mag_test_view".into();
        mag.bones.truncate(1);
        mag.bones[0].name = "tag_clip".into();
        let nm_g = model(&g, &[None], &Looks::default());
        let nm_mag = model(&mag, &[None], &Looks::default());
        let authored = (roll, [4.473, -1.085, -1.5]);
        let v = Viewmodel {
            vm: 0,
            id: "test_mp",
            gun: &g,
            gun_tag: "tag_weapon",
            gun_camo: None,
            base_parts: vec![BasePart { model: &nm_mag, tag: None, offset: authored, optic: false, ads_only: false, ads_of: None }],
            extra_models: Vec::new(),
            attachments: Vec::new(),
            hide: &[],
            clips: &[],
            dual: None,
        };
        let mut out = BTreeMap::new();
        viewmodels(&mut out, &v, &nm_g, std::slice::from_ref(&nm_mag), &[], &[('A', arms)]);
        let manifest = zhbo_native::viewmodel::Viewmodel::from_bytes(out.get(&zhbo_native::viewmodel_path(0, 'A')).unwrap()).unwrap();
        let mags: Vec<_> = manifest.parts.iter().filter(|p| p.model.contains("t6_attach_mag_test_view")).collect();
        assert_eq!(mags.len(), 1, "one magazine part, no rigid copy: {:?}", manifest.parts);
        assert_eq!(mags[0].tag.as_deref(), Some("j_gun"));
        assert_eq!(mags[0].offset, authored, "the authored rotation and offset reach the manifest");
        assert!(manifest.parts.iter().all(|p| p.tag.as_deref() != Some("tag_clip1") && !p.rigid), "nothing hangs on tag_clip1");
    }

    /// Each manifest part carries the `WeaponCamo` BO2 paints it with (`FUN_00529600`, G16): none
    /// on the arms, the def's on the gun and its base parts (the magazine), the single unique's on an attachment's model.
    #[test]
    fn each_part_carries_its_own_camo_asset() {
        let arms = nm::Model { name: "arms".into(), bones: vec![nm::Bone { name: "tag_weapon".into(), parent: None, local_rotation: QUAT_IDENTITY, local_position: [0.0; 3], rotation: QUAT_IDENTITY, position: [5.0, 0.0, 0.0] }], ..Default::default() };
        let g = gun();
        let nm_g = model(&g, &[None], &Looks::default());
        let mut nm_mag = model(&g, &[None], &Looks::default());
        nm_mag.name = "t6_attach_mag_test_view".into();
        let mut nm_reflex = model(&g, &[None], &Looks::default());
        nm_reflex.name = "t6_attach_optic_reflex_view".into();
        let v = Viewmodel {
            vm: 0,
            id: "test_mp+reflex",
            gun: &g,
            gun_tag: "tag_weapon",
            gun_camo: Some("camo_test".into()),
            base_parts: vec![BasePart { model: &nm_mag, tag: None, offset: zhbo_native::math::IDENTITY, optic: false, ads_only: false, ads_of: None }],
            extra_models: vec![&g],
            attachments: vec![AttachmentPart { tag: None, offset: crate::xf::IDENTITY, optic: true, ads_only: false, ads_of: None, camo: Some("camo_reflex".into()) }],
            hide: &[],
            clips: &[],
            dual: None,
        };
        let mut out = BTreeMap::new();
        viewmodels(&mut out, &v, &nm_g, std::slice::from_ref(&nm_mag), &[nm_reflex], &[('A', arms)]);
        let manifest = zhbo_native::viewmodel::Viewmodel::from_bytes(out.get(&zhbo_native::viewmodel_path(0, 'A')).unwrap()).unwrap();
        let camos: Vec<Option<&str>> = manifest.parts.iter().map(|p| p.camo.as_deref()).collect();
        let last = camos.len() - 1;
        assert_eq!(camos[0], None, "the arms: {camos:?}");
        assert!(camos[1..last].iter().all(|c| *c == Some("camo_test")), "the gun and its base parts: {camos:?}");
        assert_eq!(camos[last], Some("camo_reflex"), "the optic wears its unique's camo: {camos:?}");
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
