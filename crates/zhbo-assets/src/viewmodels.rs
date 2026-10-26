/*
** Conquer Ops - converter: the viewmodels: BO2 weapon models rebuilt as Zero Hour W3D hierarchies, plus the tablets and the hammer.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/
use crate::character::{self, Rig};
use crate::generals::*;
use crate::images;
use crate::native;
use std::path::Path;
use crate::paks::{load_zone, Paks};
use crate::textures::{finish, textures, Looks};
use zhbo_formats::iwi::Rgba;
use zhbo_formats::{t6zone::Zone, w3d, xanim, xmodel};
use std::collections::{BTreeMap, BTreeSet};


/// The magazine model of a gun's viewmodel: BO2 names them `t6_attach_mag_<weapon>_view` after a
/// part of the gun's own view model name (the zones don't link them for base weapons).
pub(crate) fn magazine(common: &Zone, view_model: &str) -> Option<xmodel::Model> {
    const SKIP: &[&str] = &["t6", "wpn", "ar", "smg", "lmg", "mg", "pistol", "sniper", "shotty", "launch", "view", "base"];
    view_model
        .trim_end_matches("_view")
        .split('_')
        .rev()
        .filter(|t| t.len() >= 3 && !SKIP.contains(t))
        .find_map(|t| xmodel::find(common, &format!("t6_attach_mag_{t}_view")).ok())
}

/// A model as BO2 loads it: `patch_mp` over `common_mp` (some `common_mp` models are empty stubs
/// that the patch replaces, such as the frag grenade's view model).
pub(crate) fn find_model(patch: &Zone, common: &Zone, name: &str) -> Result<xmodel::Model, String> {
    let patched = xmodel::find(patch, name).ok().filter(|m| !m.lods.is_empty());
    patched.map_or_else(|| xmodel::find(common, name).map_err(|e| format!("{name}: {e}")), Ok)
}

/// One of a weapon's base `attachViewModel` parts: the magazine, or the base attachment art (the
/// snipers' scope, the M32's sight leaf) that BO2 composes before any attachment. The magazine and
/// the attachment art are separate replacement channels (`disableBaseWeaponClip` /
/// `disableBaseWeaponAttachment`).
pub(crate) struct BasePart {
    /// The model as loaded from the zones.
    pub model: xmodel::Model,
    /// The authored `attachViewModelTag`; empty/absent today (every base part hangs at the gun
    /// root, `j_gun`, and the authored offset places it).
    pub tag: Option<String>,
    /// The authored placement from the tag: `attachViewModelRotation` (BO2 degrees, as a
    /// quaternion) and `attachViewModelOffset` (inches, view frame). The AN-94's magazine is
    /// rolled −15°.
    pub offset: crate::xf::Xf,
    /// The magazine channel (`attach_mag`). The reload's spare magazine on `tag_clip1` is the gun
    /// model's own geometry, not a second part.
    pub magazine: bool,
    /// The model carries the weapon's sight (`tag_scope`/`tag_sights`): it drives `tag_sights`.
    pub optic: bool,
    /// The M32's `_ads` sight variant: drawn only at full ADS. The hip `_view` variant carries the
    /// same sight; the pair swaps at the ADS handoff. The W3D path draws only the hip sight.
    pub ads_only: bool,
}

/// The base parts of `w` for the combination `c`: the magazine (the `attach_mag` array entry, the
/// name search only as its fallback, gated by `is_gun` and `no_clip`) and, unless the combination
/// replaces the base attachment art, every other non-empty `attachViewModel` entry as an
/// attachment-channel part. The raw parallel arrays are authoritative (`Weapon::base_view_parts`
/// loses the offsets and appends the kind-0 unique's model). A combination that moves that art
/// (the IR optics) replaces the part's offset.
///
/// Only the seven weapons with a non-magazine entry have more than the magazine today (the four
/// snipers and the M32's three ids); everything else is unchanged.
pub(crate) fn base_parts(patch: &Zone, common: &Zone, w: &zhbo_formats::weapon::Weapon, c: &Combo, view: &str) -> Vec<BasePart> {
    let mut out = Vec::new();
    if w.is_gun() && !c.no_clip {
        let authored = authored_parts(w)
            .find(|(m, ..)| m.contains("attach_mag"))
            .and_then(|(m, _, offset)| find_model(patch, common, m).ok().map(|model| (model, offset)));
        if let Some((model, offset)) = authored {
            out.push(BasePart { model, tag: None, offset, magazine: true, optic: false, ads_only: false });
        } else if let Some(model) = magazine(common, view) {
            out.push(BasePart { model, tag: None, offset: crate::xf::IDENTITY, magazine: true, optic: false, ads_only: false });
        }
    }
    if !c.no_base_attachment {
        for (name, tag, (rotation, offset)) in authored_parts(w) {
            if name.is_empty() || name.contains("attach_mag") {
                continue;
            }
            let Ok(model) = find_model(patch, common, name) else { continue };
            if duplicate_sight(&out, &model) {
                continue;
            }
            let optic = model.bones.iter().any(|b| b.name == "tag_scope" || b.name == "tag_sights");
            // `viewModelOffsetBaseAttachment` replaces this part's authored offset when the
            // combination sets `overrideBaseWeaponAttachmentOffsets` (the IR optics); the part
            // stays. It is in the same frame (the AS50's Y/Z match), so it replaces, never adds.
            // The authored rotation stays (BO2 overrides only the offset).
            let offset = (rotation, c.base_attachment_offset.unwrap_or(offset));
            let ads_only = model.name.ends_with("_ads");
            out.push(BasePart { model, tag: (!tag.is_empty()).then(|| tag.to_string()), offset, magazine: false, optic, ads_only });
        }
    }
    out
}

/// The raw parallel `attachViewModel` arrays as (model, tag, authored placement): the rotation
/// (BO2 degrees as a quaternion) and the offset (inches).
fn authored_parts(w: &zhbo_formats::weapon::Weapon) -> impl Iterator<Item = (&str, &str, crate::xf::Xf)> {
    let rotation = |i: usize| w.attach_view_rotations.get(i).copied().unwrap_or_default();
    w.attach_view_models
        .iter()
        .zip(&w.attach_view_offsets)
        .enumerate()
        .map(move |(i, ((m, t), off))| (m.as_str(), t.as_str(), (crate::xf::from_angles(rotation(i)), *off)))
}

/// Whether `b` is the same sight as `a`: the same non-empty set of surface-material names and a
/// bounding box overlapping at IoU ≥ 0.9. The M32 lists its sight twice — `_view` (hip) and `_ads`
/// (aim) — as complete alternatives with the same lens and reticle, differing only in the housing
/// mesh; drawing both would double-draw the glass. The first entry (the hip sight) is kept.
fn same_sight(a: &xmodel::Model, b: &xmodel::Model) -> bool {
    let (ma, mb) = (material_names(a), material_names(b));
    if ma.is_empty() || ma != mb {
        return false;
    }
    let (mut intersection, mut union) = (1.0f32, 1.0f32);
    for k in 0..3 {
        let overlap = (a.maxs[k].min(b.maxs[k]) - a.mins[k].max(b.mins[k])).max(0.0);
        let span = (a.maxs[k].max(b.maxs[k]) - a.mins[k].min(b.mins[k])).max(0.0);
        intersection *= overlap;
        union *= span;
    }
    union > 0.0 && intersection / union >= 0.9
}

/// Whether `model` is a duplicate sight of an existing base part: the same material set and a
/// bounding box overlapping at IoU ≥ 0.9. The M32's `_view`/`_ads` pair is not a duplicate: the
/// two are the same sight at the same mount, but the ADS variant replaces the hip housing at full
/// ADS, so both must load.
fn duplicate_sight(out: &[BasePart], model: &xmodel::Model) -> bool {
    !model.name.ends_with("_ads")
        && !out.iter().any(|p| p.ads_only)
        && out.iter().any(|p| !p.magazine && same_sight(&p.model, model))
}

fn material_names(m: &xmodel::Model) -> BTreeSet<&str> {
    m.materials.iter().flatten().map(|m| m.name.as_str()).filter(|n| !n.is_empty()).collect()
}

/// The base-part clause of the conversion report: the count and every model's name, so a part that
/// silently stops loading is visible in the launcher log (the report used to be silent exactly
/// because the scope was never loaded).
fn base_part_note(base: &[BasePart]) -> String {
    if base.is_empty() {
        return String::new();
    }
    let names: Vec<&str> = base.iter().map(|p| p.model.name.as_str()).collect();
    format!(", {} base parts ({})", base.len(), names.join(", "))
}

/// The models one attachment adds, in draw order, with their placement and whether each is the
/// attachment's optic. An IR optic's `viewModelAdditional` (the dual-band rail mount) comes first,
/// at the unique's own `viewModelAddOffsets`/`viewModelAddRotations` (as50: `[1, 0, 4.711]` under
/// the optic's `[1.045, 0, 5.568]`); only the single-attachment IR uniques carry one.
fn attachment_parts(u: &zhbo_formats::weapon::AttachmentUnique, pt: u32) -> Vec<(&str, crate::xf::Xf, u32)> {
    let xf = |rotation: [f32; 3], offset: [f32; 3]| character::scaled_xf((crate::xf::from_angles(rotation), offset));
    let mut out = Vec::new();
    if let Some(name) = u.view_model_additional.as_deref() {
        out.push((name, xf(u.view_add_rotation, u.view_add_offset), 0));
    }
    if let Some(name) = u.view_model.as_deref() {
        out.push((name, xf(u.view_rotation, u.view_offset), pt));
    }
    out
}

/// A notetrack source: viewmodel, clip (BO2 slot or tablet clip) and the BO2 animation.
pub type Clip = (i32, i32, xanim::Anim);

/// What a weapon's attachments change about its art: BO2's `WeaponAttachmentUnique` for each, the
/// animation set to use, the weapon's bones to hide, and the gunshot.
pub struct Combo<'a> {
    /// One per attachment (the single-attachment entry), with the attachment's point.
    pub parts: Vec<(&'a zhbo_formats::weapon::AttachmentUnique, u32)>,
    pub anims: Vec<Option<String>>,
    pub hide: Vec<String>,
    /// An attachment replaces the weapon's own magazine.
    pub no_clip: bool,
    /// The combination replaces the base `attachViewModel` art (its scope), not the magazine. The
    /// two channels are BO2's own flags (`disableBaseWeaponClip` / `disableBaseWeaponAttachment`)
    /// and are independent.
    pub no_base_attachment: bool,
    /// The base attachment art's moved offset (`viewModelOffsetBaseAttachment`), when a unique sets
    /// `overrideBaseWeaponAttachmentOffsets`: moved, not removed. `None` keeps the authored one.
    pub base_attachment_offset: Option<[f32; 3]>,
    pub fire_sound_player: Option<String>,
    pub fire_sound: Option<String>,
}

/// The art of `w` with the attachments `shorts` (BO2 create-a-class names).
pub fn combo<'a>(w: &'a zhbo_formats::weapon::Weapon, atts: &[zhbo_formats::weapon::Attachment], shorts: &[&str]) -> Combo<'a> {
    use crate::weapons::short_name;
    let mut parts = Vec::new();
    let mut mask = 0u32;
    let mut combo_kinds = Vec::new();
    for short in shorts {
        let Some((kind, id)) = w.attachment_variants.iter().find(|(_, id)| short_name(id) == *short) else { continue };
        mask |= 1 << kind;
        let point = atts.iter().find(|a| &a.name == id).map_or(0, |a| a.point);
        combo_kinds.push((*kind, point));
        if let Some(u) = w.attachment_uniques.iter().find(|u| u.kind == *kind && u.combined == 0) {
            parts.push((u, point));
        }
    }
    // The animations: per-slot, five tiers (G14 §5). The base table is the variant's; each slot is
    // resolved independently. A unique's `None` is "no override", never an erase.
    let exact = (shorts.len() > 1).then(|| w.attachment_uniques.iter().find(|u| u.combined == mask)).flatten();
    let base = w.attachment_uniques.iter().find(|u| u.kind == 0 && u.combined == mask)
        .or_else(|| w.attachment_uniques.iter().find(|u| u.kind == 0 && u.combined == 0));
    let optic_kind = combo_kinds.iter().find(|(_, p)| *p == 1).map(|(k, _)| *k);
    let alt_ads_only = combo_kinds.iter().any(|(kind, _)| {
        atts.iter().find(|a| a.kind == *kind).map_or(false, |a| a.alt_weapon_ads_only)
    });
    let resolved = |kind: u32| -> Option<&'a zhbo_formats::weapon::AttachmentUnique> {
        w.attachment_uniques.iter().find(|u| u.kind == kind && u.combined == mask)
            .or_else(|| w.attachment_uniques.iter().find(|u| u.kind == kind && u.combined == 0))
    };
    let priority = |kind: u32| -> i32 {
        atts.iter().find(|a| a.kind == kind).map_or(0, |a| a.first_raise_priority)
    };
    let mut anims = w.anims.clone();
    for s in 0..anims.len() {
        // 1. First-raise slot 0x18: the resolved unique with the highest firstRaisePriority.
        if s == 0x18 {
            let best = combo_kinds.iter()
                .filter_map(|(k, _)| resolved(*k).map(|u| (u, priority(*k))))
                .filter(|(u, _)| u.anims.get(s).map_or(false, |a| a.is_some()))
                .fold(None, |acc: Option<(&zhbo_formats::weapon::AttachmentUnique, i32)>, (u, p)| {
                    match acc {
                        Some((_, ap)) if ap >= p => acc,
                        _ => Some((u, p)),
                    }
                });
            if let Some(a) = best.and_then(|(u, _)| u.anims.get(s).cloned()).flatten() {
                anims[s] = Some(a);
            }
            continue;
        }
        // 2. Alt-mode ADS-only: slots 0x1A, 0x1B, 0x55-0x57 stay the variant's.
        if alt_ads_only && [0x1A, 0x1B, 0x55, 0x56, 0x57].contains(&s) {
            continue;
        }
        // 3. Aim slots 0x3D-0x40 and 0x55-0x57: the optic's resolved unique.
        if [0x3D, 0x3E, 0x3F, 0x40, 0x55, 0x56, 0x57].contains(&s) {
            if let Some(k) = optic_kind {
                if let Some(u) = resolved(k) {
                    if let Some(a) = u.anims.get(s) {
                        anims[s] = a.clone();
                        continue;
                    }
                }
            }
        }
        // 4. Otherwise: the first field kind's resolved unique, ascending eAttachment order.
        let mut sorted: Vec<u32> = combo_kinds.iter().map(|(k, _)| *k).collect();
        sorted.sort();
        let mut found = false;
        for k in sorted {
            if let Some(u) = resolved(k) {
                if let Some(a) = u.anims.get(s) {
                    if a.is_some() {
                        anims[s] = a.clone();
                        found = true;
                        break;
                    }
                }
            }
        }
        if found { continue; }
        // 5. Base unique fallback: fills only a slot the variant left EMPTY (BO2 keeps the
        // variant's own clip for every populated slot — the bare an94's putaway_quick stands;
        // measured on the conformance rows).
        if anims[s].is_none() {
            if let Some(u) = base {
                if let Some(a) = u.anims.get(s) {
                    if a.is_some() {
                        anims[s] = a.clone();
                    }
                }
            }
        }
    }
    let mut hide = w.hide_tags.clone();
    for (u, _) in &parts {
        hide.extend(u.hide_tags.iter().cloned());
    }
    if let Some(u) = exact {
        hide.extend(u.hide_tags.iter().cloned());
    }
    hide.sort();
    hide.dedup();
    let sounds = parts.iter().map(|(u, _)| *u).find(|u| u.fire_sound_player.is_some() && u.fire_sound_player != w.fire_sound_player);
    Combo {
        no_clip: parts.iter().any(|(u, _)| u.disable_base_clip),
        // Read over both selection paths, as `hide` does: the single-attachment uniques and the
        // exact multi-attachment unique. Data-driven, never a name match or a hide tag.
        no_base_attachment: parts.iter().any(|(u, _)| u.disable_base_attachment)
            || exact.is_some_and(|u| u.disable_base_attachment),
        // Read over both selection paths like the flag above: the exact combination's unique is
        // the more specific entry and wins.
        base_attachment_offset: exact
            .filter(|u| u.override_base_attachment_offsets)
            .or_else(|| parts.iter().map(|(u, _)| *u).find(|u| u.override_base_attachment_offsets))
            .map(|u| u.base_attachment_offset),
        fire_sound_player: sounds.and_then(|u| u.fire_sound_player.clone()).or_else(|| w.fire_sound_player.clone()),
        fire_sound: sounds.and_then(|u| u.fire_sound.clone()).or_else(|| w.fire_sound.clone()),
        parts,
        anims,
        hide,
    }
}

/// The BO2 weapon a converted id stands for: `an94_mp+gl+reflex` and `gl_an94_mp~...` are
/// `an94_mp` and `gl_an94_mp`.
pub fn base_id(id: &str) -> &str {
    id.split('~').next().unwrap_or(id).split('+').next().unwrap_or(id)
}

/// The bone a weapon's viewmodel hangs from: the off-hand equipment clips key the raised item on
/// `tag_knife_attach` (a bone under the left wrist on the faction viewhands), while the primary
/// gun's `tag_weapon` anchor is what those same clips park or hide. Everything that is not
/// `OFFHAND` (1) or `ITEM` (2) — primaries, the melee knife, alt modes — stays on `tag_weapon`.
/// The raw `attachViewModelTag` array is empty for all thirteen off-hand items (the parser only
/// reads it when `attachViewModel` is non-empty), so the inventory type is the rule.
pub(crate) fn mount_tag(w: &zhbo_formats::weapon::Weapon) -> &'static str {
    use zhbo_formats::weapon::inventory;
    if w.inventory == inventory::OFFHAND || w.inventory == inventory::ITEM { "tag_knife_attach" } else { "tag_weapon" }
}

/// The left gun of a dual-wield pair hangs from `tag_weapon1`, the viewhands' second weapon mount
/// that the dual-wield left clip animates to the left gun's authored pose. The A/B sweep showed the
/// equipment mount (`tag_knife_attach`) rotated the gun off the view and the wrist (`j_wrist_le`)
/// left it behind the hand; `tag_weapon1` is the clip's own mount and keeps the barrel on the view.
/// The sweep can override it with `ZHBO_DW_MOUNT` (a dev-only knob, like `ZHBO_SWEEP_*`).
pub(crate) const DW_MOUNT: &str = "tag_weapon1";

pub(crate) fn dual_mount() -> String {
    std::env::var("ZHBO_DW_MOUNT").ok().filter(|m| !m.is_empty()).unwrap_or_else(|| DW_MOUNT.to_string())
}

/// Whether a converted weapon instance is the right/base half of a dual-wield pair, so the
/// converter loads the `_lh` companion as a second rig. The rule cascades:
/// 1. the weapon id carries the `_dw` suffix (`fnp45_dw_mp` and the other four pistols);
/// 2. the def's inventory is `DWLEFTHAND` (the `_lh` companion weapon def);
/// 3. the converted instance mounts the `dw` attachment (the base pistol + `au_<weapon>_dw`).
pub(crate) fn is_dual_wield(w: &zhbo_formats::weapon::Weapon, shorts: &[&str]) -> bool {
    use zhbo_formats::weapon::inventory;
    let base = w.name.strip_suffix("_mp").unwrap_or(&w.name);
    base.ends_with("_dw") || w.inventory == inventory::DWLEFTHAND || shorts.contains(&"dw")
}

/// The right-hand twin of a dual-wield left clip: `viewmodel_<w>_left_idle` -> `..._right_idle`.
/// Shared clips (`pullout`, `sprint_*`, ...) have no `_left_` and keep their name.
fn dual_clip_name(left: &str) -> String {
    left.split_once("_left_").map_or_else(|| left.to_string(), |(prefix, suffix)| format!("{prefix}_right_{suffix}"))
}

/// The first-person viewmodels of `converted` weapons: each faction's arms holding the weapon, its
/// base parts (magazine, scope/optic) and its attachments (placed where BO2 puts them, iron sights
/// hidden under an optic), with a W3D clip for every BO2 animation slot it has, and its alt mode's
/// clips after them. Sets each definition's `viewmodel` (and its alt mode's). Returns a summary and
/// the clips (for sounds).
///
/// The first `base_count` ids are the base set (`converted_base`, a prefix of `converted`): only
/// those emit the composed W3D viewmodel meshes and hierarchies. The remaining legal combinations
/// emit native manifests only; their W3D path falls back to the base weapon's mesh (the fallback's
/// deprecation direction), while the native path gets its own manifest per combination. The shared
/// `.zbm`/`.zba` blobs and textures are deduplicated by name, so a combination adds only its
/// manifests.
#[allow(clippy::too_many_arguments)]
pub(crate) fn weapon_viewmodels(
    patch: &Zone,
    common: &Zone,
    zones: &[Zone],
    paks: &Paks,
    out: &mut BTreeMap<String, Vec<u8>>,
    bo2: &[zhbo_formats::weapon::Weapon],
    atts: &[zhbo_formats::weapon::Attachment],
    defs: &mut [zhbo_core::weapons::WeaponDef],
    converted: &[String],
    base_count: usize,
) -> Result<(String, Vec<Clip>), String> {
    use zhbo_core::attachments::point;
    let mut looks = Looks::default();
    let mut made = Vec::new();
    let mut clips = Vec::new();
    let mut anim_cache: BTreeMap<String, Option<xanim::Anim>> = BTreeMap::new();
    for (vm, id) in converted.iter().enumerate() {
        let write_w3d = vm < base_count;
        let vm = vm as i32;
        let (base, shorts) = zhbo_core::attachments::split_id(id);
        let Some(w) = bo2.iter().find(|w| w.name == base) else { continue };
        let Some(view) = w.view_model.as_deref() else { continue };
        // The dual-wield pair: the base gun plus its `_lh` companion as a second rig on the left
        // palm. The five `_dw` pistols carry the suffix; the base + `dw` attachment path mounts the
        // same pair. The `_lh` def itself is the companion, not a pair: its DWLEFTHAND inventory
        // fires the rule, so its `_lh` suffix is excluded explicitly.
        let wbase = w.name.strip_suffix("_mp").unwrap_or(&w.name);
        let dual_pair = is_dual_wield(w, &shorts) && !wbase.ends_with("_lh");
        let lh_view = dual_pair.then(|| format!("{view}_lh"));
        let lh_w = lh_view.as_deref().and_then(|name| bo2.iter().find(|x| x.view_model.as_deref() == Some(name)));
        let lh_gun = lh_view.as_deref().and_then(|name| find_model(patch, common, name).ok());
        let dual = lh_w.filter(|_| lh_gun.is_some());
        let c = combo(w, atts, &shorts);
        let gun = find_model(patch, common, view).map_err(|e| format!("{id}: {e}"))?;
        // The base `attachViewModel` parts: the magazine (its authored offset; the name search is
        // the fallback) and the base attachment art (the snipers' scope), unless the combination
        // replaces that art (the ACOG) or the magazine (the dualclip).
        let base = base_parts(patch, common, w, &c, view);
        let parts: Vec<(&xmodel::Model, &str)> = vec![(&gun, mount_tag(w))];
        // The attachments' models, each with where it goes: an attachment's additional mount (the
        // IR rail) before its own model, both on the unique's tag.
        let mut extra: Vec<(xmodel::Model, Option<String>, crate::xf::Xf, u32)> = Vec::new();
        for (u, pt) in &c.parts {
            for (name, offset, point) in attachment_parts(u, *pt) {
                let Ok(model) = find_model(patch, common, name) else { continue };
                extra.push((model, u.view_model_tag.clone(), offset, point));
            }
        }
        let gun_tex = textures(&gun, paks, out, &mut looks);
        let base_tex: Vec<Vec<Option<String>>> = base.iter().map(|p| textures(&p.model, paks, out, &mut looks)).collect();
        let extra_tex: Vec<Vec<Option<String>>> = extra.iter().map(|(m, ..)| textures(m, paks, out, &mut looks)).collect();
        // The left gun of a dual-wield pair: its own textures and native blob, written once.
        let lh_tex = lh_gun.as_ref().map(|m| textures(m, paks, out, &mut looks));
        let native_lh = lh_gun.as_ref().zip(lh_tex.as_ref()).map(|(m, t)| native::model(m, t, &looks));
        // The native blobs (Track R): the same parts, unconverted, for the renderer's own BO2 pass.
        let native_gun = native::model(&gun, &gun_tex, &looks);
        let native_base: Vec<zhbo_native::model::Model> = base.iter().zip(&base_tex).map(|(p, t)| native::model(&p.model, t, &looks)).collect();
        let native_extra: Vec<zhbo_native::model::Model> = extra.iter().zip(&extra_tex).map(|((m, ..), t)| native::model(m, t, &looks)).collect();
        let mut native_factions: Vec<(char, zhbo_native::model::Model)> = Vec::new();
        let mut rig: Option<Rig> = None;
        for (f, zone) in FACTIONS.iter().zip(zones) {
            let hands = xmodel::find(zone, f.hands).map_err(|e| e.to_string())?;
            let hands_tex = textures(&hands, paks, out, &mut looks);
            native_factions.push((f.letter, native::model(&hands, &hands_tex, &looks)));
            if write_w3d {
                let rig = rig.get_or_insert_with(|| {
                    let mut rig = Rig::new(&hands, &parts);
                    let root = gun.bones.first().and_then(|b| rig.pivot(&b.name)).unwrap_or(0);
                    // The base parts ride the gun root at their authored `attachViewModelOffset` (the
                    // empty authored tag is `j_gun`), before the sight marker so the base optic can
                    // drive it.
                    for part in &base {
                        if part.ads_only {
                            continue;
                        }
                        let at = part.tag.as_deref().and_then(|t| rig.pivot(t)).unwrap_or(root);
                        rig.attach(&part.model, at, character::scaled_xf(part.offset));
                    }
                    // Aim through the base optic (the snipers' scope): the line of sight goes up to the
                    // middle of its glass. It creates the marker when the gun has none, and moves it
                    // when the gun already carries one; an attachment optic below still wins.
                    let base_optic = base
                        .iter()
                        .find(|p| p.optic)
                        .and_then(|p| p.model.bones.first().and_then(|b| rig.pivot(&b.name)).map(|pivot| (pivot, &p.model)));
                    if let Some((optic, model)) = base_optic {
                        let mid = (model.mins[2] + (model.maxs[2] - model.mins[2]) * OPTIC_WINDOW) * character::SCALE;
                        let pos = crate::xf::apply(rig.bind[optic], [0.0, 0.0, mid]);
                        if rig.pivot("tag_sights").is_some() {
                            rig.move_pivot("tag_sights", pos);
                        } else {
                            let local = crate::xf::compose(crate::xf::inverse(rig.bind[root]), (crate::xf::IDENTITY.0, pos));
                            rig.add_marker("tag_sights", root, local);
                        }
                    } else if rig.pivot("tag_sights").is_none() {
                        // Aiming lines `tag_sights` up with the eye; many BO2 viewmodels (every pistol)
                        // have none, so give them one where their sights are.
                        let at = rig.pivot("tag_sights_on").map(|p| rig.bind[p].1).or_else(|| {
                            let weapon = rig.pivot(mount_tag(w)).map_or(crate::xf::IDENTITY, |p| rig.bind[p]);
                            sight_point(&gun).map(|p| crate::xf::apply(weapon, character::scaled(p)))
                        });
                        if let Some(at) = at {
                            let local = crate::xf::compose(crate::xf::inverse(rig.bind[root]), (crate::xf::IDENTITY.0, at));
                            rig.add_marker("tag_sights", root, local);
                        }
                    }
                    for (model, tag, offset, pt) in &extra {
                        let at = tag.as_deref().and_then(|t| rig.pivot(t)).unwrap_or(root);
                        rig.attach(model, at, *offset);
                        // Aim through the optic: the line of sight goes up to the middle of its glass
                        // (the window sits in the upper part of the sight, above its mount).
                        if *pt == point::OPTIC as u32 {
                            if let Some(optic) = model.bones.first().and_then(|b| rig.pivot(&b.name)) {
                                let mid = (model.mins[2] + (model.maxs[2] - model.mins[2]) * OPTIC_WINDOW) * character::SCALE;
                                let pos = crate::xf::apply(rig.bind[optic], [0.0, 0.0, mid]);
                                rig.move_pivot("tag_sights", pos);
                            }
                        }
                    }
                    rig
                });
                let mut meshes = character::skin(rig, &hands, "H", &hands_tex);
                meshes.extend(character::skin_hiding(rig, &gun, "G", &gun_tex, &c.hide));
                // The base parts: the magazine keeps its `M` prefix (the reload's incoming magazine
                // on `tag_clip1` is the gun's own geometry); the attachment art (the scope) gets the
                // free `B` prefix.
                for (k, (part, tex)) in base.iter().zip(&base_tex).enumerate() {
                    if part.ads_only {
                        continue;
                    }
                    if part.magazine {
                        meshes.extend(character::skin(rig, &part.model, "M", tex));
                    } else {
                        meshes.extend(character::skin(rig, &part.model, &format!("B{k}"), tex));
                    }
                }
                for (k, ((model, ..), tex)) in extra.iter().zip(&extra_tex).enumerate() {
                    meshes.extend(character::skin(rig, model, &format!("A{k}"), tex));
                }
                finish(&mut meshes, &looks);
                let name = vm_model(vm, f.letter);
                out.insert(format!("Art\\W3D\\{name}.w3d"), w3d::skin_model(&name, &vm_hierarchy(vm), &meshes));
            }
        }
        // The W3D mesh output stays on the base set; a non-base combination's W3D path falls back
        // to its base weapon's mesh (the fallback's deprecation direction). Its native manifest is
        // still emitted below.
        let rig = if write_w3d { Some(rig.ok_or("no viewmodel arms")?) } else { None };
        let hierarchy = write_w3d.then(|| vm_hierarchy(vm));
        if let (Some(rig), Some(hierarchy)) = (&rig, &hierarchy) {
            out.insert(format!("Art\\W3D\\{hierarchy}.w3d"), rig.hierarchy_file(hierarchy));
        }
        // The weapon's clips, then its alt mode's after them on the same viewmodel.
        let alt_id = defs.iter().find(|d| &d.id == id).map(|d| d.alt.clone()).unwrap_or_default();
        let alt_anims = (!alt_id.is_empty()).then(|| bo2.iter().find(|a| a.name == base_id(&alt_id))).flatten().map(|a| a.anims.clone());
        let mut sets = vec![(0usize, c.anims.clone())];
        if let Some(a) = alt_anims.clone() {
            sets.push((zhbo_core::weapons::ALT_CLIPS as usize, a));
        }
        let mut n = 0;
        let mut native_clips = Vec::new();
        for (base_slot, anims) in &sets {
            for (slot, anim_name) in anims.iter().enumerate() {
                let Some(anim_name) = anim_name else { continue };
                let anim = anim_cache.entry(anim_name.clone()).or_insert_with(|| xanim::find(common, anim_name).ok());
                let Some(anim) = anim else { continue };
                if let (Some(rig), Some(hierarchy)) = (&rig, &hierarchy) {
                    let (frames, channels) = character::animate(rig, anim);
                    let fps = anim.frame_rate.round().max(1.0) as u32;
                    let clip = vm_anim(vm, base_slot + slot);
                    out.insert(format!("Art\\W3D\\{clip}.w3d"), w3d::animation(&clip, hierarchy, frames, fps, &channels));
                }
                native_clips.push(((base_slot + slot) as u16, native::put_anim(out, anim)));
                clips.push((vm, (base_slot + slot) as i32, anim.clone()));
                n += 1;
            }
        }
        // The left gun's clip table: its `_left_*` animations, keyed by the matching right slot
        // with the instance bit (`INSTANCE_LEFT`), so the native core samples both guns per request.
        // A shared clip (`pullout`, `sprint_*`) keeps its name and its slot; a clip with no name
        // twin falls back to its own slot index in the base's table.
        if let Some(lh_w) = dual {
            for (left_slot, name) in lh_w.anims.iter().enumerate() {
                let Some(name) = name else { continue };
                let right_name = dual_clip_name(name);
                let right_slot = w.anims.iter().position(|a| a.as_deref() == Some(right_name.as_str())).or_else(|| (left_slot < w.anims.len()).then_some(left_slot));
                let Some(right_slot) = right_slot else { continue };
                let anim = anim_cache.entry(name.clone()).or_insert_with(|| xanim::find(common, name).ok());
                let Some(anim) = anim else { continue };
                native_clips.push(((right_slot as u16) | zhbo_native::viewmodel::INSTANCE_LEFT, native::put_anim(out, anim)));
                // The left gun's clips feed the native path and the sound events; the base W3D render
                // path's animation table stays as is.
                clips.push((vm, (right_slot as i32) | zhbo_native::viewmodel::INSTANCE_LEFT as i32, anim.clone()));
                n += 1;
            }
        }
        let attachments: Vec<(Option<String>, crate::xf::Xf, bool)> =
            extra.iter().map(|(_, tag, offset, pt)| (tag.clone(), (offset.0, offset.1.map(|c| c / character::SCALE)), *pt == point::OPTIC as u32)).collect();
        let base_native_parts: Vec<native::BasePart> = base
            .iter()
            .zip(&native_base)
            .map(|(p, m)| native::BasePart { model: m, tag: p.tag.clone(), offset: p.offset, optic: p.optic, ads_only: p.ads_only })
            .collect();
        let dual_tag = dual_mount();
        let dual_native = dual.zip(native_lh.as_ref()).map(|(w, m)| native::DualGun { gun: m, tag: dual_tag.as_str(), hide: &w.hide_tags });
        let vm_native = native::Viewmodel { vm, id, gun: &gun, gun_tag: mount_tag(w), base_parts: base_native_parts, extra_models: extra.iter().map(|e| &e.0).collect(), attachments, hide: &c.hide, clips: &native_clips, dual: dual_native };
        native::viewmodels(out, &vm_native, &native_gun, &native_base, &native_extra, &native_factions);
        let mask = |a: &[Option<String>]| a.iter().enumerate().take(128).filter(|(_, x)| x.is_some()).fold(0u128, |m, (i, _)| m | (1u128 << i));
        if let Some(d) = defs.iter_mut().find(|d| &d.id == id) {
            d.viewmodel = vm;
            d.anims = mask(&c.anims);
        }
        if let (Some(a), Some(d)) = (alt_anims, defs.iter_mut().find(|d| d.id == alt_id)) {
            d.viewmodel = vm;
            d.clip_base = zhbo_core::weapons::ALT_CLIPS;
            d.anims = mask(&a);
        }
        made.push(format!(
            "{id} ({n} clips{}{}{})",
            if base.iter().any(|p| p.magazine) { ", magazine" } else { "" },
            base_part_note(&base),
            if extra.is_empty() { String::new() } else { format!(", {} attachment models", extra.len()) }
        ));
    }
    Ok((format!("viewmodels {} (surfaces left out: {})", made.join(", "), looks.dropped.into_iter().collect::<Vec<_>>().join(" ")), clips))
}

/// The third-person weapons on the soldier model: each converted weapon's BO2 world model and its
/// attachments' (at BO2's world offsets), rigid on the right hand as `W<vm><k>` meshes, so the bridge
/// shows the one in hand. Sets each definition's `world_meshes` (its alt mode shares them).
#[allow(clippy::too_many_arguments)]
pub(crate) fn world_weapons(
    patch: &Zone,
    common: &Zone,
    paks: &Paks,
    out: &mut BTreeMap<String, Vec<u8>>,
    rig: &Rig,
    bo2: &[zhbo_formats::weapon::Weapon],
    atts: &[zhbo_formats::weapon::Attachment],
    defs: &mut [zhbo_core::weapons::WeaponDef],
    converted: &[String],
    looks: &mut Looks,
) -> (Vec<w3d::SkinMesh>, Vec<(usize, crate::xf::Xf)>) {
    let hand = rig.pivot(character::WEAPON_TAG).unwrap_or(0);
    let mut meshes = Vec::new();
    let mut flashes = Vec::new();
    for (vm, id) in converted.iter().enumerate() {
        let (base, shorts) = zhbo_core::attachments::split_id(id);
        let Some(w) = bo2.iter().find(|w| w.name == base) else { continue };
        let Some(world) = w.world_model.as_deref().and_then(|m| find_model(patch, common, m).ok()) else { continue };
        let c = combo(w, atts, &shorts);
        let prefix = format!("W{vm:02}");
        if let Some(b) = world.bones.iter().find(|b| b.name == "tag_flash") {
            flashes.push((vm, character::scaled_xf((b.rotation, b.position))));
        }
        let tex = textures(&world, paks, out, looks);
        let mut mine = character::skin_rigid(&world, hand, crate::xf::IDENTITY, &prefix, 0, &tex, &c.hide);
        // Track R7: the same world model and attachments as native blobs, composed at run time on
        // the General's `tag_weapon_right` (`zhbo_core::nativebody`). The native side takes BO2
        // inches, so its offsets stay unscaled (the W3D path's are scaled above).
        let mut native_parts = vec![zhbo_native::viewmodel::Part {
            model: crate::native::put_model(out, &crate::native::model(&world, &tex, looks)),
            tag: None,
            offset: crate::xf::IDENTITY,
            rigid: true,
            ads_only: false,
        }];
        for (u, _) in &c.parts {
            let Some(model) = u.world_model.as_deref().and_then(|m| find_model(patch, common, m).ok()) else { continue };
            // From the world model's own tag, or its origin.
            let tag = u.world_model_tag.as_deref().and_then(|t| world.bones.iter().find(|b| b.name == t));
            let base_xf = tag.map_or(crate::xf::IDENTITY, |b| character::scaled_xf((b.rotation, b.position)));
            let offset = crate::xf::compose(base_xf, character::scaled_xf((crate::xf::from_angles(u.world_rotation), u.world_offset)));
            let t = textures(&model, paks, out, looks);
            let first = mine.len();
            mine.extend(character::skin_rigid(&model, hand, offset, &prefix, first, &t, &[]));
            let native_base = tag.map_or(crate::xf::IDENTITY, |b| (b.rotation, b.position));
            native_parts.push(zhbo_native::viewmodel::Part {
                model: crate::native::put_model(out, &crate::native::model(&model, &t, looks)),
                // A tag the world model does not have hangs the attachment at its origin, as the
                // W3D path does; the native composition treats a missing tag the same way.
                tag: u.world_model_tag.clone().filter(|n| world.bones.iter().any(|b| &b.name == n)),
                offset: crate::xf::compose(native_base, (crate::xf::from_angles(u.world_rotation), u.world_offset)),
                rigid: true,
                ads_only: false,
            });
        }
        out.insert(
            zhbo_native::world_weapon_path(vm as i32),
            zhbo_native::thirdperson::Weapon { id: (*id).to_string(), parts: native_parts, hide: c.hide.clone() }.to_bytes(),
        );
        finish(&mut mine, looks);
        let count = mine.len().min(99) as u8;
        for d in defs.iter_mut().filter(|d| &d.id == id || d.alt == *id && d.id.contains('~')) {
            d.world_meshes = count;
        }
        meshes.extend(mine);
    }
    (meshes, flashes)
}

/// Thrown and launched things for `converted` weapons: their BO2 projectile (or world) model as a
/// W3D model, and a Zero Hour object that shows it (`ZHBO_Proj_<id>`). Sets `projectile_object`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn projectiles(
    patch: &Zone,
    common: &Zone,
    paks: &Paks,
    out: &mut BTreeMap<String, Vec<u8>>,
    bo2: &[zhbo_formats::weapon::Weapon],
    defs: &mut [zhbo_core::weapons::WeaponDef],
    converted: &[String],
    models: &mut BTreeMap<String, String>,
) -> String {
    use zhbo_core::weapons::Class;
    let mut ini = String::from("; Generated by zhbo-assets: thrown, launched and placed BO2 weapons, moved by zhbo-core.\n\n");
    let mut looks = Looks::default();
    let mut made = Vec::new();
    // The converted weapons and their alt modes (an under-barrel launcher's grenade).
    let mut ids: Vec<String> = converted.to_vec();
    for id in converted {
        if let Some(alt) = defs.iter().find(|d| &d.id == id).map(|d| d.alt.clone()).filter(|a| !a.is_empty()) {
            ids.push(alt);
        }
    }
    for (n, id) in ids.iter().enumerate() {
        let Some(w) = bo2.iter().find(|w| w.name == base_id(id)) else { continue };
        let Some(d) = defs.iter_mut().find(|d| &d.id == id) else { continue };
        let throws = d.class.is_offhand() || d.class == Class::Launcher || d.projectile_speed > 0.0;
        if !throws || d.class == Class::Melee {
            continue;
        }
        let Some(model_name) = w.projectile_model.clone().or_else(|| w.world_model.clone()) else { continue };
        let Ok(model) = find_model(patch, common, &model_name) else { continue };
        let rig = Rig::new(&model, &[]);
        let tex = textures(&model, paks, out, &mut looks);
        let mut meshes = character::skin(&rig, &model, "P", &tex);
        finish(&mut meshes, &looks);
        let name = projectile_model(n as i32);
        let hierarchy = name.replace("_SKN", "_SKL");
        out.insert(format!("Art\\W3D\\{name}.w3d"), w3d::skin_model(&name, &hierarchy, &meshes));
        out.insert(format!("Art\\W3D\\{hierarchy}.w3d"), rig.hierarchy_file(&hierarchy));
        let object = projectile_object(&id.replace(['+', '~'], "_"));
        // A Sensor Grenade sees for its owner (and finds stealthed units).
        let sight = if base_id(id) == "sensor_grenade_mp" {
            let r = zhbo_core::equipment::SENSOR_RANGE;
            format!("  VisionRange = {r}\n  ShroudClearingRange = {r}\n  Behavior = StealthDetectorUpdate ModuleTag_03\n    DetectionRate = 500\n    DetectionRange = {r}\n  End\n")
        } else {
            String::new()
        };
        ini.push_str(&format!(
            "Object {object}\n  Draw = W3DModelDraw ModuleTag_01\n    OkToChangeModelColor = Yes\n    DefaultConditionState\n      Model = {name}\n    End\n  End\n  \
             KindOf = NO_COLLIDE INERT IMMOBILE UNATTACKABLE CLICK_THROUGH\n  Body = InactiveBody ModuleTag_02\n  End\n{sight}  \
             Geometry = Sphere\n  GeometryMajorRadius = 1.0\n  GeometryIsSmall = Yes\nEnd\n\n"
        ));
        d.projectile_object = object;
        models.insert(id.clone(), name.clone());
        made.push(id.as_str());
    }
    out.insert("Data\\INI\\Object\\ZHBO_BO2_Projectiles.ini".into(), ini.into_bytes());
    format!("projectiles {}", made.join(", "))
}

/// Each faction's arms holding the scorestreak tablet, with its lit screen.
pub(crate) fn tablets(common: &Zone, zones: &[Zone], paks: &Paks, out: &mut BTreeMap<String, Vec<u8>>, looks: &mut Looks) -> Result<(), String> {
    let tablet = xmodel::find(common, TABLET).map_err(|e| e.to_string())?;
    let mut tablet_tex = textures(&tablet, paks, out, looks);
    // The screen's material layers several images in a shader; draw one composite, self-lit.
    let screen_surface = tablet.materials.iter().position(|m| m.as_ref().is_some_and(|m| m.name.contains("screen")));
    if let Some(i) = screen_surface {
        if let Some(img) = tablet.materials[i].as_ref().and_then(|m| tablet_screen(m, paks)) {
            out.insert(format!("Art\\Textures\\{TABLET_SCREEN}"), images::tga_bytes(&img));
            tablet_tex[i] = Some(TABLET_SCREEN.to_string());
        }
    }
    let mut rig: Option<Rig> = None;
    for (f, zone) in FACTIONS.iter().zip(zones) {
        let hands = xmodel::find(zone, f.hands).map_err(|e| e.to_string())?;
        let rig = rig.get_or_insert_with(|| Rig::new(&hands, &[(&tablet, "tag_weapon")]));
        let hands_tex = textures(&hands, paks, out, looks);
        let mut meshes = character::skin(rig, &hands, "H", &hands_tex);
        let mut screen = character::skin(rig, &tablet, "T", &tablet_tex);
        for m in &mut screen {
            m.emissive = m.texture.as_deref() == Some(TABLET_SCREEN);
        }
        meshes.extend(screen);
        finish(&mut meshes, looks);
        let name = tablet_model(f.viewmodel);
        out.insert(format!("Art\\W3D\\{name}.w3d"), w3d::skin_model(&name, TABLET_HIERARCHY, &meshes));
    }
    let rig = rig.ok_or("no tablet arms")?;
    out.insert(format!("Art\\W3D\\{TABLET_HIERARCHY}.w3d"), rig.hierarchy_file(TABLET_HIERARCHY));
    for (w3d_name, bo2_name) in TABLET_ANIMS {
        let anim = xanim::find(common, bo2_name).map_err(|e| e.to_string())?;
        let (frames, channels) = character::animate(&rig, &anim);
        let fps = anim.frame_rate.round().max(1.0) as u32;
        out.insert(format!("Art\\W3D\\{w3d_name}.w3d"), w3d::animation(w3d_name, TABLET_HIERARCHY, frames, fps, &channels));
    }
    Ok(())
}

/// One faction's hammer viewmodel: its arms (`hands`, the rig's base, with the knife) and the
/// hammer in the knife's grip.
pub(crate) fn hammer_meshes(rig: &Rig, hands: &xmodel::Model, hammer: &xmodel::Model, hands_tex: &[Option<String>], hammer_tex: &[Option<String>]) -> Result<Vec<w3d::SkinMesh>, String> {
    let grip = rig.pivot("j_gun").ok_or("no j_gun on the knife")?;
    let mut meshes = character::skin(rig, hands, "H", hands_tex);
    meshes.extend(character::skin_rigid(hammer, grip, character::scaled_xf(HAMMER_GRIP), "M", 0, hammer_tex, &[]));
    Ok(meshes)
}

/// The first faction's hammer viewmodel as converted, untextured, with the clip `clip` (a W3D name
/// from [`HAMMER_ANIMS`]): to check it by eye (`zhbo-assets hammer`).
pub fn hammer_preview(bo2_dir: &Path, clip: &str) -> Result<(Rig, Vec<w3d::SkinMesh>, Vec<w3d::Channel>), String> {
    let common = load_zone(bo2_dir, "common_mp")?;
    let faction = load_zone(bo2_dir, FACTIONS[0].zone)?;
    let prop_zone = load_zone(bo2_dir, HAMMER_ZONE)?;
    let hammer = xmodel::find(&prop_zone, HAMMER).map_err(|e| format!("{HAMMER}: {e}"))?;
    let knife = xmodel::find(&common, KNIFE_VIEW).map_err(|e| format!("{KNIFE_VIEW}: {e}"))?;
    let hands = xmodel::find(&faction, FACTIONS[0].hands).map_err(|e| e.to_string())?;
    let rig = Rig::new(&hands, &[(&knife, "tag_weapon")]);
    let meshes = hammer_meshes(&rig, &hands, &hammer, &[], &[])?;
    let bo2_name = HAMMER_ANIMS.iter().find(|(w, _)| *w == clip).map(|(_, b)| *b).ok_or_else(|| format!("no hammer clip {clip}"))?;
    let anim = xanim::find(&common, bo2_name).map_err(|e| e.to_string())?;
    let (_, channels) = character::animate(&rig, &anim);
    Ok((rig, meshes, channels))
}

pub(crate) fn hammers(bo2_dir: &Path, common: &Zone, zones: &[Zone], paks: &Paks, out: &mut BTreeMap<String, Vec<u8>>, looks: &mut Looks) -> Result<(), String> {
    let prop_zone = load_zone(bo2_dir, HAMMER_ZONE)?;
    let hammer = xmodel::find(&prop_zone, HAMMER).map_err(|e| format!("{HAMMER}: {e}"))?;
    let knife = xmodel::find(common, KNIFE_VIEW).map_err(|e| format!("{KNIFE_VIEW}: {e}"))?;
    let hammer_tex = textures(&hammer, paks, out, looks);
    let mut rig: Option<Rig> = None;
    for (f, zone) in FACTIONS.iter().zip(zones) {
        let hands = xmodel::find(zone, f.hands).map_err(|e| e.to_string())?;
        let rig = rig.get_or_insert_with(|| Rig::new(&hands, &[(&knife, "tag_weapon")]));
        let hands_tex = textures(&hands, paks, out, looks);
        let mut meshes = hammer_meshes(rig, &hands, &hammer, &hands_tex, &hammer_tex)?;
        finish(&mut meshes, looks);
        let name = hammer_model(f.viewmodel);
        out.insert(format!("Art\\W3D\\{name}.w3d"), w3d::skin_model(&name, HAMMER_HIERARCHY, &meshes));
    }
    let rig = rig.ok_or("no hammer arms")?;
    out.insert(format!("Art\\W3D\\{HAMMER_HIERARCHY}.w3d"), rig.hierarchy_file(HAMMER_HIERARCHY));
    for (w3d_name, bo2_name) in HAMMER_ANIMS {
        let anim = xanim::find(common, bo2_name).map_err(|e| e.to_string())?;
        let (frames, channels) = character::animate(&rig, &anim);
        let fps = anim.frame_rate.round().max(1.0) as u32;
        out.insert(format!("Art\\W3D\\{w3d_name}.w3d"), w3d::animation(w3d_name, HAMMER_HIERARCHY, frames, fps, &channels));
    }
    Ok(())
}

/// The scorestreak screen as BO2 shows it: the background UI, the satellite panels over it, and the
/// targeting reticle over the panels.
pub(crate) fn tablet_screen(m: &xmodel::Material, paks: &Paks) -> Option<Rgba> {
    let layer = |part: &str| {
        m.textures
            .iter()
            .filter_map(|t| t.image.as_ref())
            .find(|i| i.name.contains(part))
            .and_then(|i| paks.image(i.stream_hash))
    };
    let mut screen = layer("background")?;
    if let Some(top) = layer("killstreaks_top") {
        let size = (screen.width, screen.height);
        over(&mut screen, &top, (0, 0), size);
    }
    if let Some(reticle) = layer("working_top") {
        let size = screen.height / 2;
        let at = (screen.width * 37 / 100 - size / 2, screen.height * 45 / 100 - size / 2);
        over(&mut screen, &reticle, at, (size, size));
    }
    Some(screen)
}

/// Alpha-blend `src`, scaled to `size`, over `dst` at `at`.
pub(crate) fn over(dst: &mut Rgba, src: &Rgba, at: (u32, u32), size: (u32, u32)) {
    for y in 0..size.1.min(dst.height.saturating_sub(at.1)) {
        for x in 0..size.0.min(dst.width.saturating_sub(at.0)) {
            let (sx, sy) = (x * src.width / size.0, y * src.height / size.1);
            let s = &src.pixels[((sy * src.width + sx) * 4) as usize..][..4];
            let a = s[3] as u32;
            let d = ((at.1 + y) * dst.width + at.0 + x) as usize * 4;
            for (c, &sc) in s.iter().take(3).enumerate() {
                let v = (sc as u32 * a + dst.pixels[d + c] as u32 * (255 - a)) / 255;
                dst.pixels[d + c] = v as u8;
            }
        }
    }
}

/// The tablet in the soldier's hand, as `TB<k>` meshes on BO2's `tag_inhand` (the bone its tablet
/// animations carry).
pub(crate) fn tablet_world(patch: &Zone, common: &Zone, paks: &Paks, out: &mut BTreeMap<String, Vec<u8>>, rig: &Rig, looks: &mut Looks) -> Vec<w3d::SkinMesh> {
    let Ok(model) = find_model(patch, common, TABLET_WORLD) else { return Vec::new() };
    let Some(tag) = rig.pivot(TABLET_TAG).or_else(|| rig.pivot("tag_weapon_left")) else { return Vec::new() };
    let tex = textures(&model, paks, out, looks);
    let mut meshes = character::skin_rigid(&model, tag, crate::xf::IDENTITY, "TB", 0, &tex, &[]);
    finish(&mut meshes, looks);
    meshes
}

#[cfg(test)]
mod tests {
    use super::{attachment_parts, base_part_note, duplicate_sight, is_dual_wield, mount_tag, same_sight, BasePart};
    use zhbo_formats::weapon::{inventory, Attachment, AttachmentUnique, Weapon};

    fn sight(name: &str, mins: [f32; 3], maxs: [f32; 3]) -> zhbo_formats::xmodel::Model {
        zhbo_formats::xmodel::Model {
            name: name.into(),
            bones: Vec::new(),
            surfaces: Vec::new(),
            lods: Vec::new(),
            radius: 0.0,
            mins,
            maxs,
            materials: vec![Some(zhbo_formats::xmodel::Material { name: name.into(), textures: Vec::new(), state_bits: Vec::new(), sort_key: 0 })],
        }
    }

    /// BO2's two replacement channels are separate fields: `disableBaseWeaponClip` suppresses the
    /// magazine, `disableBaseWeaponAttachment` the base attachment art (the AS50's ACOG). The flag
    /// is read over both selection paths — the single-attachment uniques and the exact
    /// multi-attachment unique — and never inferred from a name or a hide tag.
    #[test]
    fn the_replacement_flags_are_read_from_the_selection() {
        let unique = |kind: u32, clip: bool, attachment: bool| AttachmentUnique {
            kind,
            disable_base_clip: clip,
            disable_base_attachment: attachment,
            ..Default::default()
        };
        let acog = Weapon { attachment_variants: vec![(1, "acog".into())], attachment_uniques: vec![unique(1, false, true)], ..Default::default() };
        let c = super::combo(&acog, &[], &["acog"]);
        assert!(c.no_base_attachment, "the ACOG replaces the base scope");
        assert!(!c.no_clip, "it keeps the magazine (disableBaseWeaponClip is 0)");

        let dualclip = Weapon { attachment_variants: vec![(5, "dualclip".into())], attachment_uniques: vec![unique(5, true, false)], ..Default::default() };
        let c = super::combo(&dualclip, &[], &["dualclip"]);
        assert!(c.no_clip, "the dualclip replaces the magazine");
        assert!(!c.no_base_attachment, "the flags select disjoint sets: it keeps the base art");

        // The multi-attachment path: the flag lives on the exact `combined` unique, not on the
        // single-attachment entries (the mask is `1 << 2 | 1 << 11`).
        let combined = AttachmentUnique { kind: 0, combined: 0x804, disable_base_attachment: true, ..Default::default() };
        let pair = Weapon { attachment_variants: vec![(2, "a".into()), (11, "b".into())], attachment_uniques: vec![combined], ..Default::default() };
        let c = super::combo(&pair, &[], &["a", "b"]);
        assert!(c.no_base_attachment, "the exact combination's unique suppresses the base art");
        assert!(!c.no_clip);
    }

    /// `viewModelOffsetBaseAttachment` applies only when the unique sets
    /// `overrideBaseWeaponAttachmentOffsets` (12 uniques in the catalog, all the four snipers' IR
    /// combinations) and is read over both selection paths as `disableBaseWeaponAttachment` is:
    /// the exact multi-attachment unique is the more specific entry.
    #[test]
    fn the_base_attachment_override_is_read_from_the_selection() {
        let ir = AttachmentUnique { kind: 14, override_base_attachment_offsets: true, base_attachment_offset: [9.889, 0.0, 4.253], ..Default::default() };
        let weapon = |us: Vec<AttachmentUnique>, vs: Vec<(u32, String)>| Weapon { attachment_variants: vs, attachment_uniques: us, ..Default::default() };
        let one = weapon(vec![ir.clone()], vec![(14, "ir".into())]);
        assert_eq!(super::combo(&one, &[], &["ir"]).base_attachment_offset, Some([9.889, 0.0, 4.253]), "the IR unique moves the base scope");
        let acog = weapon(vec![AttachmentUnique { kind: 1, disable_base_attachment: true, ..Default::default() }], vec![(1, "acog".into())]);
        assert_eq!(super::combo(&acog, &[], &["acog"]).base_attachment_offset, None, "the ACOG replaces the base art, it does not move it");
        let vzoom = weapon(vec![AttachmentUnique { kind: 29, ..Default::default() }], vec![(29, "vzoom".into())]);
        assert_eq!(super::combo(&vzoom, &[], &["vzoom"]).base_attachment_offset, None, "the VZOOM leaves the base scope where it is");
        // The `+ir` combined unique is read too, and its offset wins over the single entry's.
        let combined = AttachmentUnique { kind: 14, combined: 0x8004000, override_base_attachment_offsets: true, base_attachment_offset: [1.5, 0.0, 2.5], ..Default::default() };
        let pair = weapon(vec![ir, AttachmentUnique { kind: 27, ..Default::default() }, combined], vec![(14, "ir".into()), (27, "swayreduc".into())]);
        assert_eq!(super::combo(&pair, &[], &["swayreduc", "ir"]).base_attachment_offset, Some([1.5, 0.0, 2.5]), "the exact combination's unique wins");
    }

    /// The IR optic's `viewModelAdditional` is a second model of the same attachment: the dual-band
    /// rail. It draws **before** the optic, on the unique's `viewModelTag`, at its own
    /// `viewModelAddOffsets`/`viewModelAddRotations` — under the optic, not at it (as50: `[1, 0,
    /// 4.711]` against the optic's `[1.045, 0, 5.568]`) — and is not the optic for the aim marker.
    #[test]
    fn the_additional_mount_precedes_the_optic_at_its_own_offset() {
        let ir = AttachmentUnique {
            view_model: Some("t6_attach_optic_dualband_view".into()),
            view_model_additional: Some("t6_attach_optic_dualband_mount_view".into()),
            view_offset: [1.045, 0.0, 5.568],
            view_add_offset: [1.0, 0.0, 4.711],
            ..Default::default()
        };
        let parts = attachment_parts(&ir, 10);
        assert_eq!(parts.len(), 2, "{parts:?}");
        let (mount, at, point) = parts[0];
        assert_eq!((mount, point), ("t6_attach_optic_dualband_mount_view", 0), "the mount draws first and must not drive the aim marker");
        assert_eq!(at.1, crate::character::scaled([1.0, 0.0, 4.711]), "the mount rides its own add offset");
        let (optic, at, point) = parts[1];
        assert_eq!((optic, point), ("t6_attach_optic_dualband_view", 10), "the optic keeps the attachment's point");
        assert_eq!(at.1, crate::character::scaled([1.045, 0.0, 5.568]));
        assert_eq!(attachment_parts(&AttachmentUnique { view_model: Some("t6_attach_bcpu_view".into()), ..Default::default() }, 10).len(), 1, "no mount without viewModelAdditional");
    }

    /// The M32 lists `_view` and `_ads` as two complete sights: the same non-empty material set and
    /// a near-identical bounding box. A genuinely different model (another material, or a distant
    /// box) is not the same sight.
    #[test]
    fn the_sight_variant_dedupe_collapses_the_m32_pair() {
        let view = sight("mc/mtl_t6_attach_optic_m32", [4.80, -0.80, 4.80], [8.54, 0.70, 6.84]);
        let ads = sight("mc/mtl_t6_attach_optic_m32", [4.79, -0.80, 4.78], [8.55, 0.70, 6.85]);
        assert!(same_sight(&view, &ads), "the same sight's hip and ADS variants collapse");
        let other_material = sight("mc/mtl_t6_attach_optic_other", [4.80, -0.80, 4.80], [8.54, 0.70, 6.84]);
        assert!(!same_sight(&view, &other_material), "a different material set is a different part");
        let elsewhere = sight("mc/mtl_t6_attach_optic_m32", [20.0, 0.0, 0.0], [24.0, 1.0, 2.0]);
        assert!(!same_sight(&view, &elsewhere), "a disjoint box is a different part");
    }

    /// The M32's `_view` and `_ads` sights are the same sight at the same mount. The ADS variant is
    /// not a duplicate: both load for the native manifest's ADS handoff. A true duplicate still
    /// dedupes, and the hip sight is not a duplicate when the ADS variant is present.
    #[test]
    fn the_m32_ads_sight_is_not_deduped() {
        let view = sight("t6_attach_optic_m32_view", [4.80, -0.80, 4.80], [8.54, 0.70, 6.84]);
        let ads = sight("t6_attach_optic_m32_ads", [4.79, -0.80, 4.78], [8.55, 0.70, 6.85]);
        let base = |model: zhbo_formats::xmodel::Model, ads_only: bool| BasePart { model, tag: None, offset: crate::xf::IDENTITY, magazine: false, optic: true, ads_only };
        let mut out = vec![base(view.clone(), false)];
        assert!(!duplicate_sight(&out, &ads), "the ADS variant is not a duplicate of the hip sight");
        assert!(duplicate_sight(&out, &view), "a true duplicate still dedupes");
        out.push(base(ads, true));
        assert!(!duplicate_sight(&out, &view), "the hip sight is not a duplicate when the ADS variant is present");
    }

    /// The conversion report names every base part, so a part that silently stops loading (the
    /// scope, before this change) is visible in the launcher log.
    #[test]
    fn the_conversion_report_names_the_base_parts() {
        let part = |name: &str, magazine: bool| BasePart {
            model: sight(name, [0.0; 3], [1.0; 3]),
            tag: None,
            offset: crate::xf::IDENTITY,
            magazine,
            optic: !magazine,
            ads_only: false,
        };
        let note = base_part_note(&[part("t6_attach_mag_xpr50_view", true), part("t6_wpn_sniper_xpr50_scope_view", false)]);
        assert!(note.contains("2 base parts"), "{note}");
        assert!(note.contains("t6_wpn_sniper_xpr50_scope_view"), "{note}");
        assert_eq!(base_part_note(&[]), "", "no base parts, no clause");
    }

    /// The mount follows the inventory type: `OFFHAND` and `ITEM` equipment hangs from the left
    /// palm's `tag_knife_attach`, while primaries, the melee knife, alt modes and dual-wield
    /// offhands stay on the primary gun's `tag_weapon` anchor (the tag those clips park or hide).
    #[test]
    fn the_mount_tag_follows_the_inventory_type() {
        for inv in [inventory::OFFHAND, inventory::ITEM] {
            let w = Weapon { inventory: inv, ..Default::default() };
            assert_eq!(mount_tag(&w), "tag_knife_attach", "inventory {inv} should hang from the left palm");
        }
        for inv in [inventory::PRIMARY, inventory::MELEE, inventory::ALTMODE, inventory::DWLEFTHAND] {
            let w = Weapon { inventory: inv, ..Default::default() };
            assert_eq!(mount_tag(&w), "tag_weapon", "inventory {inv} should stay on the gun anchor");
        }
    }

    /// The five DW pistols carry the `_dw` suffix; the left companion is the `DWLEFTHAND`
    /// inventory; a bare base pistol is not a pair.
    #[test]
    fn the_five_dw_pistols_are_dual_wield() {
        for name in ["fnp45_dw_mp", "fiveseven_dw_mp", "kard_dw_mp", "judge_dw_mp", "beretta93r_dw_mp"] {
            let w = Weapon { name: name.into(), ..Default::default() };
            assert!(is_dual_wield(&w, &[]), "{name} should be a dual-wield pair");
        }
        let lh = Weapon { name: "fnp45_lh_mp".into(), inventory: inventory::DWLEFTHAND, ..Default::default() };
        assert!(is_dual_wield(&lh, &[]), "the DWLEFTHAND companion is the left half");
        let base = Weapon { name: "fnp45_mp".into(), ..Default::default() };
        assert!(!is_dual_wield(&base, &[]), "the bare base pistol is not a pair");
    }

    /// G14 §5 priority 1: slot 0x18 takes the resolved unique whose attachment's
    /// `firstRaisePriority` is highest; first wins ties.
    #[test]
    fn the_first_raise_priority_wins_slot_0x18() {
        let att1 = Attachment { kind: 1, first_raise_priority: 10, ..Default::default() };
        let att2 = Attachment { kind: 2, first_raise_priority: 20, ..Default::default() };
        let att3 = Attachment { kind: 3, first_raise_priority: 20, ..Default::default() };
        let mut u1 = AttachmentUnique { kind: 1, ..Default::default() };
        u1.anims = vec![None; 0x58]; u1.anims[0x18] = Some("u1_first_raise".into());
        let mut u2 = AttachmentUnique { kind: 2, ..Default::default() };
        u2.anims = vec![None; 0x58]; u2.anims[0x18] = Some("u2_first_raise".into());
        let mut u3 = AttachmentUnique { kind: 3, ..Default::default() };
        u3.anims = vec![None; 0x58]; u3.anims[0x18] = Some("u3_first_raise".into());
        let mut w = Weapon {
            attachment_variants: vec![(1, "a".into()), (2, "b".into()), (3, "c".into())],
            attachment_uniques: vec![u1, u2, u3],
            ..Default::default()
        };
        w.anims = vec![None; 0x58];
        let c = super::combo(&w, &[att1, att2, att3], &["a", "b", "c"]);
        assert_eq!(c.anims[0x18].as_deref(), Some("u2_first_raise"), "the higher firstRaisePriority wins; the first of the ties wins");
    }

    /// G14 §5 priority 2: an alt-mode ADS-only weapon keeps the variant's clips for
    /// 0x1A/0x1B/0x55-0x57.
    #[test]
    fn the_alt_mode_ads_only_slots_stay_on_the_variant() {
        let att = Attachment { kind: 1, alt_weapon_ads_only: true, ..Default::default() };
        let mut u = AttachmentUnique { kind: 1, ..Default::default() };
        u.anims = vec![None; 0x58]; u.anims[0x1A] = Some("unique_alt_raise".into());
        let mut w = Weapon { attachment_variants: vec![(1, "gl".into())], attachment_uniques: vec![u], ..Default::default() };
        w.anims = vec![None; 0x58]; w.anims[0x1A] = Some("variant_alt_raise".into());
        let c = super::combo(&w, &[att], &["gl"]);
        assert_eq!(c.anims[0x1A].as_deref(), Some("variant_alt_raise"), "the alt-mode slot keeps the variant's clip");
    }

    /// G14 §5 priority 3: the aim slots take the mounted optic's resolved unique, not the grip's.
    #[test]
    fn the_optic_wins_the_aim_slots() {
        let optic_att = Attachment { name: "reflex".into(), kind: 20, point: 1, ..Default::default() };
        let grip_att = Attachment { name: "grip".into(), kind: 12, point: 2, ..Default::default() };
        let mut optic_u = AttachmentUnique { kind: 20, ..Default::default() };
        optic_u.anims = vec![None; 0x58]; optic_u.anims[0x3D] = Some("optic_ads_fire".into());
        let mut grip_u = AttachmentUnique { kind: 12, ..Default::default() };
        grip_u.anims = vec![None; 0x58];
        grip_u.anims[0x01] = Some("grip_idle".into());
        grip_u.anims[0x3D] = Some("grip_ads_fire".into());
        let mut w = Weapon {
            attachment_variants: vec![(20, "reflex".into()), (12, "grip".into())],
            attachment_uniques: vec![optic_u, grip_u],
            ..Default::default()
        };
        w.anims = vec![None; 0x58];
        let c = super::combo(&w, &[optic_att, grip_att], &["reflex", "grip"]);
        assert_eq!(c.anims[0x3D].as_deref(), Some("optic_ads_fire"), "the optic's unique wins the aim slot");
        assert_eq!(c.anims[0x01].as_deref(), Some("grip_idle"), "the hand-pose slot takes the grip's clip");
    }

    /// G14 §5 priority 4: the first field kind's resolved unique wins, ascending eAttachment order.
    #[test]
    fn the_attachment_order_picks_the_first_field_kind() {
        let att1 = Attachment { kind: 1, ..Default::default() };
        let att2 = Attachment { kind: 2, ..Default::default() };
        let mut u1 = AttachmentUnique { kind: 1, ..Default::default() };
        u1.anims = vec![None; 0x58]; u1.anims[0x01] = Some("u1_idle".into());
        let mut u2 = AttachmentUnique { kind: 2, ..Default::default() };
        u2.anims = vec![None; 0x58]; u2.anims[0x01] = Some("u2_idle".into());
        let mut w = Weapon {
            attachment_variants: vec![(2, "b".into()), (1, "a".into())],
            attachment_uniques: vec![u2, u1],
            ..Default::default()
        };
        w.anims = vec![None; 0x58];
        let c = super::combo(&w, &[att2, att1], &["b", "a"]);
        assert_eq!(c.anims[0x01].as_deref(), Some("u1_idle"), "ascending kind 1 wins over kind 2");
    }

    /// G14 §5 priority 5: the base unique's table fills a slot the variant left EMPTY.
    #[test]
    fn the_base_unique_is_the_final_fallback() {
        let att = Attachment { kind: 1, ..Default::default() };
        let mut base = AttachmentUnique { kind: 0, ..Default::default() };
        base.anims = vec![None; 0x58]; base.anims[0x01] = Some("base_idle".into());
        let u = AttachmentUnique { kind: 1, ..Default::default() };
        let mut w = Weapon { attachment_variants: vec![(1, "a".into())], attachment_uniques: vec![base, u], ..Default::default() };
        w.anims = vec![None; 0x58]; // the variant leaves 0x01 EMPTY: the base unique fills it.
        let c = super::combo(&w, &[att], &["a"]);
        assert_eq!(c.anims[0x01].as_deref(), Some("base_idle"), "the base unique fills the variant's empty slot");
    }

    /// G14 §5 priority 5 is a FALLBACK, not an override: the variant's populated slot survives
    /// the base unique (measured on the conformance rows: the bare an94 keeps its putaway_quick).
    #[test]
    fn the_variants_populated_slot_survives_the_base_unique() {
        let att = Attachment { kind: 1, ..Default::default() };
        let mut base = AttachmentUnique { kind: 0, ..Default::default() };
        base.anims = vec![None; 0x58]; base.anims[0x19] = Some("base_putaway".into());
        let u = AttachmentUnique { kind: 1, ..Default::default() };
        let mut w = Weapon { attachment_variants: vec![(1, "a".into())], attachment_uniques: vec![base, u], ..Default::default() };
        w.anims = vec![None; 0x58]; w.anims[0x19] = Some("variant_putaway_quick".into());
        let c = super::combo(&w, &[att], &["a"]);
        assert_eq!(c.anims[0x19].as_deref(), Some("variant_putaway_quick"), "the variant's own clip stands");
    }

    /// G14 §5: the exact-combined unique participates as the mask-matched resolved child.
    #[test]
    fn the_exact_combined_unique_participates_as_the_resolved_child() {
        let att1 = Attachment { kind: 1, ..Default::default() };
        let att2 = Attachment { kind: 2, ..Default::default() };
        let mut u1 = AttachmentUnique { kind: 1, ..Default::default() };
        u1.anims = vec![None; 0x58]; u1.anims[0x01] = Some("u1_idle".into());
        let mut u2 = AttachmentUnique { kind: 2, ..Default::default() };
        u2.anims = vec![None; 0x58]; u2.anims[0x01] = Some("u2_idle".into());
        let mut exact = AttachmentUnique { kind: 1, combined: 6, ..Default::default() };
        exact.anims = vec![None; 0x58]; exact.anims[0x01] = Some("exact_idle".into());
        let mut w = Weapon {
            attachment_variants: vec![(1, "a".into()), (2, "b".into())],
            attachment_uniques: vec![u1, u2, exact],
            ..Default::default()
        };
        w.anims = vec![None; 0x58];
        let c = super::combo(&w, &[att1, att2], &["a", "b"]);
        assert_eq!(c.anims[0x01].as_deref(), Some("exact_idle"), "the exact combined child participates");
    }

    /// A unique's `None` is "no override", never an erase: the base value survives.
    #[test]
    fn a_uniques_none_does_not_erase_the_base() {
        let att = Attachment { kind: 1, ..Default::default() };
        let mut base = AttachmentUnique { kind: 0, ..Default::default() };
        base.anims = vec![None; 0x58]; base.anims[0x01] = Some("base_idle".into());
        let mut u = AttachmentUnique { kind: 1, ..Default::default() };
        u.anims = vec![None; 0x58]; u.anims[0x01] = None;
        let mut w = Weapon { attachment_variants: vec![(1, "a".into())], attachment_uniques: vec![base, u], ..Default::default() };
        w.anims = vec![None; 0x58]; w.anims[0x01] = Some("variant_idle".into());
        let c = super::combo(&w, &[att], &["a"]);
        assert_eq!(c.anims[0x01].as_deref(), Some("variant_idle"), "the unique's None does not erase the base");
    }

    /// The MP7+grip single-attachment case: the grip's idle wins, as verified parity today.
    #[test]
    fn the_mp7_grip_idle_keeps_its_clip() {
        let att = Attachment { kind: 12, ..Default::default() };
        let mut u = AttachmentUnique { kind: 12, ..Default::default() };
        u.anims = vec![None; 0x58]; u.anims[0x01] = Some("viewmodel_mp7_grip_idle".into());
        let mut w = Weapon { name: "mp7_mp".into(), attachment_variants: vec![(12, "grip".into())], attachment_uniques: vec![u], ..Default::default() };
        w.anims = vec![None; 0x58]; w.anims[0x01] = Some("viewmodel_mp7_idle".into());
        let c = super::combo(&w, &[att], &["grip"]);
        assert_eq!(c.anims[0x01].as_deref(), Some("viewmodel_mp7_grip_idle"), "the grip's idle wins");
    }

    /// The erase guard: slot 0x18's tier-1 pass must not write `None` over the base table when
    /// no combo unique offers a first-raise clip. The variant's clip survives.
    #[test]
    fn no_first_raise_offer_keeps_the_variants_0x18() {
        let att = Attachment { kind: 1, first_raise_priority: 10, ..Default::default() };
        let mut u = AttachmentUnique { kind: 1, ..Default::default() };
        u.anims = vec![None; 0x58]; u.anims[0x18] = None;
        let mut w = Weapon { attachment_variants: vec![(1, "a".into())], attachment_uniques: vec![u], ..Default::default() };
        w.anims = vec![None; 0x58]; w.anims[0x18] = Some("variant_first_raise".into());
        let c = super::combo(&w, &[att], &["a"]);
        assert_eq!(c.anims[0x18].as_deref(), Some("variant_first_raise"), "no first-raise offer keeps the variant's clip");
    }

    /// The base pistol + the `dw` attachment is the composition path: the attachment the
    /// converter mounts (`au_<weapon>_dw`) turns the base weapon into a pair, without the `_dw`
    /// suffix on the weapon itself.
    #[test]
    fn the_base_pistol_with_the_dw_attachment_is_dual_wield() {
        let base = Weapon { name: "fnp45_mp".into(), attachment_variants: vec![(4, "au_fnp45_dw".into())], ..Default::default() };
        assert!(is_dual_wield(&base, &["dw"]), "the base + dw attachment should compose the left gun");
        assert!(!is_dual_wield(&base, &[]), "the bare base is not a pair");
    }

    /// The left gun's mount is `tag_weapon1`, the viewhands' second weapon mount. The dual-wield
    /// left clips animate it to the left gun's authored pose (`viewmodel_fnp45_dw_left_idle`'s
    /// `tag_weapon1` track); `tag_knife_attach` (the equipment palm) rotated the gun off the view,
    /// and `j_wrist_le` left it behind the hand.
    #[test]
    fn the_dual_wield_mount_is_the_clips_left_weapon_mount() {
        assert_eq!(super::DW_MOUNT, "tag_weapon1");
    }
}