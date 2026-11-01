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
use std::collections::BTreeMap;


/// The magazine model of a gun's viewmodel: BO2 names them `t6_attach_mag_<weapon>_view` after a
/// part of the gun's own view model name (the zones don't link them for base weapons).
pub(crate) fn magazine(pool: &[&Zone], view_model: &str) -> Option<xmodel::Model> {
    const SKIP: &[&str] = &["t6", "wpn", "ar", "smg", "lmg", "mg", "pistol", "sniper", "shotty", "launch", "view", "base"];
    view_model
        .trim_end_matches("_view")
        .split('_')
        .rev()
        .filter(|t| t.len() >= 3 && !SKIP.contains(t))
        .find_map(|t| find_model(pool, &format!("t6_attach_mag_{t}_view")).ok())
}

/// A model as BO2's asset pool holds it over `pool` (highest priority first,
/// `zhbo_formats::weapon::MP_POOL`: `patch_mp`, `common_patch_mp`, `common_mp`): the first zone with
/// the model's geometry (some `common_mp` models are empty stubs that the patch replaces, such as
/// the frag grenade's view model; the Peacekeeper's are only in `common_patch_mp`), else the last
/// zone's.
pub(crate) fn find_model(pool: &[&Zone], name: &str) -> Result<xmodel::Model, String> {
    let (last, higher) = pool.split_last().ok_or("no zones")?;
    let patched = higher.iter().find_map(|z| xmodel::find(z, name).ok().filter(|m| !m.lods.is_empty()));
    patched.map_or_else(|| xmodel::find(last, name).map_err(|e| format!("{name}: {e}")), Ok)
}

/// An animation as the asset pool holds it over `pool` (highest priority first): the first zone
/// that has it (a patched weapon's clips can be `common_patch_mp`'s alone).
pub(crate) fn find_anim(pool: &[&Zone], name: &str) -> Option<xanim::Anim> {
    pool.iter().find_map(|z| xanim::find(z, name).ok())
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
    /// `attachViewModel[7]`, slot 0's ADS model (the M32's `_ads` sight): drawn only while aiming
    /// (G14 §3). The W3D path draws only the hip parts.
    pub ads_only: bool,
    /// The base part (an index into the same list) this ADS-only part replaces while aiming: slot
    /// 0's, when the weapon has one.
    pub ads_of: Option<usize>,
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
pub(crate) fn base_parts(pool: &[&Zone], w: &zhbo_formats::weapon::Weapon, c: &Combo, view: &str) -> Vec<BasePart> {
    let mut out = Vec::new();
    if w.is_gun() && !c.no_clip {
        let authored = authored_parts(w)
            .find(|(m, ..)| m.contains("attach_mag"))
            .and_then(|(m, _, offset, _)| find_model(pool, m).ok().map(|model| (model, offset)));
        if let Some((model, offset)) = authored {
            out.push(BasePart { model, tag: None, offset, magazine: true, optic: false, ads_only: false, ads_of: None });
        } else if let Some(model) = magazine(pool, view) {
            out.push(BasePart { model, tag: None, offset: crate::xf::IDENTITY, magazine: true, optic: false, ads_only: false, ads_of: None });
        }
    }
    if !c.no_base_attachment {
        // Slot 0's part, for slot 7 (its ADS model) to replace.
        let mut slot0 = None;
        for (name, tag, (rotation, offset), slot) in authored_parts(w) {
            if name.is_empty() || name.contains("attach_mag") {
                continue;
            }
            let Ok(model) = find_model(pool, name) else { continue };
            let optic = model.bones.iter().any(|b| b.name == "tag_scope" || b.name == "tag_sights");
            // `viewModelOffsetBaseAttachment` replaces this part's authored offset when the
            // combination sets `overrideBaseWeaponAttachmentOffsets` (the IR optics); the part
            // stays. It is in the same frame (the AS50's Y/Z match), so it replaces, never adds.
            // The authored rotation stays (BO2 overrides only the offset).
            let offset = (rotation, c.base_attachment_offset.unwrap_or(offset));
            let ads_only = slot == ADS_SLOT;
            if slot == 0 {
                slot0 = Some(out.len());
            }
            let ads_of = if ads_only { slot0 } else { None };
            out.push(BasePart { model, tag: (!tag.is_empty()).then(|| tag.to_string()), offset, magazine: false, optic, ads_only, ads_of });
        }
    }
    out
}

/// The raw parallel `attachViewModel` arrays as (model, tag, authored placement, slot): the
/// rotation (BO2 degrees as a quaternion), the offset (inches) and the `attachViewModel` slot BO2
/// reads the entry's role by (G14 §3).
fn authored_parts(w: &zhbo_formats::weapon::Weapon) -> impl Iterator<Item = (&str, &str, crate::xf::Xf, u8)> {
    let rotation = |i: usize| w.attach_view_rotations.get(i).copied().unwrap_or_default();
    w.attach_view_models
        .iter()
        .zip(&w.attach_view_offsets)
        .zip(&w.attach_view_slots)
        .enumerate()
        .map(move |(i, (((m, t), off), &slot))| (m.as_str(), t.as_str(), (crate::xf::from_angles(rotation(i)), *off), slot))
}

/// `attachViewModel[7]`: slot 0's ADS model. BO2 draws it only while aiming, and slot 0 then only
/// when it is empty (`FUN_00535170`, G14 §3).
const ADS_SLOT: u8 = 7;

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

/// One model an attachment adds, as [`attachment_parts`] lists it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct AttachmentModel<'a> {
    name: &'a str,
    /// The unique's offset and rotation for this model (scaled).
    xf: crate::xf::Xf,
    /// The attachment's point, or 0 for a model that is not its optic (the IR rail mount).
    point: u32,
    /// The unique's `viewModelADS`: drawn only while aiming.
    ads_only: bool,
    /// The model (an index into the same list) the ADS model replaces while aiming.
    ads_of: Option<usize>,
}

/// The models one attachment adds, in draw order, with their placement and whether each is the
/// attachment's optic. An IR optic's `viewModelAdditional` (the dual-band rail mount) comes first,
/// at the unique's own `viewModelAddOffsets`/`viewModelAddRotations` (as50: `[1, 0, 4.711]` under
/// the optic's `[1.045, 0, 5.568]`); only the single-attachment IR uniques carry one.
///
/// A unique's `viewModelADS` (the holo's and the rangefinder's `_ads`) follows its `viewModel`: BO2
/// draws it instead, on the same tag at the same offset and rotation, while aiming; the additional
/// model draws in both lists (`FUN_00535170`, G14 §3). It keeps the attachment's point: at full ADS
/// it is the sight the eye lines up with.
fn attachment_parts(u: &zhbo_formats::weapon::AttachmentUnique, pt: u32) -> Vec<AttachmentModel<'_>> {
    let xf = |rotation: [f32; 3], offset: [f32; 3]| character::scaled_xf((crate::xf::from_angles(rotation), offset));
    let part = |name, xf, point| AttachmentModel { name, xf, point, ads_only: false, ads_of: None };
    let mut out = Vec::new();
    if let Some(name) = u.view_model_additional.as_deref() {
        out.push(part(name, xf(u.view_add_rotation, u.view_add_offset), 0));
    }
    let hip = u.view_model.as_deref().map(|name| {
        out.push(part(name, xf(u.view_rotation, u.view_offset), pt));
        out.len() - 1
    });
    if let Some(name) = u.view_model_ads.as_deref() {
        out.push(AttachmentModel { ads_only: true, ads_of: hip, ..part(name, xf(u.view_rotation, u.view_offset), pt) });
    }
    out
}

/// A notetrack source: viewmodel, clip (BO2 slot or tablet clip) and the BO2 animation.
pub type Clip = (i32, i32, xanim::Anim);

/// One loaded attachment model of a combination, with where it goes and its ADS role
/// ([`AttachmentModel`] with the model found).
struct Extra {
    model: xmodel::Model,
    tag: Option<String>,
    xf: crate::xf::Xf,
    point: u32,
    ads_only: bool,
    /// The `extra` index of the model this ADS model replaces while aiming.
    ads_of: Option<usize>,
    /// The single unique's `weaponCamo` (G16).
    camo: Option<String>,
}

/// What a weapon's attachments change about its art: BO2's `WeaponAttachmentUnique` for each, the
/// animation set to use, the weapon's bones to hide, and the gunshot.
pub struct Combo<'a> {
    /// One per attachment: the unique BO2 draws for it in this combination — the single unique, or
    /// the combined child it resolves to (`Weapon::resolved_unique`, G14 §2/§3) — with the
    /// attachment's point and the attachment's own single unique, whose `weaponCamo` the part wears
    /// (`FUN_00529600`, G16: the camo pass reads the single, never the combined child — the
    /// `ir`+`vzoom` dual-band optic resolves its `ir` model through the combined child but wears
    /// `camo_ir`).
    pub parts: Vec<(&'a zhbo_formats::weapon::AttachmentUnique, u32, &'a zhbo_formats::weapon::AttachmentUnique)>,
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
    /// The combination's `eAttachment` kinds, in its order: the third-person parts walk their
    /// single uniques and resolve each through [`zhbo_formats::weapon::Weapon::resolved_unique`].
    pub kinds: Vec<u32>,
}

/// The default mount of a third-person part: `FUN_008ace30` takes it from its callers as
/// `DAT_0240749c`, the script string `FUN_00678440` registers for `tag_weapon` — the world models'
/// root bone (the viewmodel's default is `j_gun`, which no world model has).
pub const WORLD_DEFAULT_TAG: &str = "tag_weapon";

/// One third-person part after the world model: a base `attachWorldModel` slot or an attachment's
/// world model, with its tag (BO2's, or [`WORLD_DEFAULT_TAG`] when it names none) and placement
/// (inches, degrees).
#[derive(Clone, Debug, PartialEq)]
pub struct WorldPart {
    pub model: String,
    pub tag: String,
    pub offset: [f32; 3],
    pub rotation: [f32; 3],
    /// The `WeaponCamo` the class camo paints on it (`FUN_00529600`, G16): the def's on a base
    /// slot, the attachment's **single** unique's on its models (even when a combined unique
    /// supplies the model).
    pub camo: Option<String>,
}

/// BO2's third-person parts after the world model (`FUN_008ace30`, the conformance layer's
/// `resolve::world_parts`): the variant's `attachWorldModel` slots — slot 0 (the base attachment
/// art) dropped when a single unique disables it and moved by the first that overrides its
/// offsets, slot 6 (the clip) dropped when one disables the base clip — then, per attachment whose
/// single unique has a world model, the resolved unique's world model and its additional model.
pub fn world_parts(w: &zhbo_formats::weapon::Weapon, c: &Combo) -> Vec<WorldPart> {
    let singles = || c.kinds.iter().filter_map(|k| w.single_unique(*k));
    let no_base_attachment = singles().any(|u| u.disable_base_attachment);
    let no_base_clip = singles().any(|u| u.disable_base_clip);
    let moved = singles().find(|u| u.override_base_attachment_offsets).map(|u| u.world_base_attachment_offset);
    let mut out = Vec::new();
    for a in &w.attach_world_models {
        if (a.slot == 0 && no_base_attachment) || (a.slot == 6 && no_base_clip) {
            continue;
        }
        let offset = if a.slot == 0 { moved.unwrap_or(a.offset) } else { a.offset };
        let tag = a.tag.clone().unwrap_or_else(|| WORLD_DEFAULT_TAG.to_string());
        out.push(WorldPart { model: a.model.clone(), tag, offset, rotation: a.rotation, camo: w.weapon_camo.clone() });
    }
    for s in singles() {
        if s.world_model.is_none() {
            continue;
        }
        let r = w.resolved_unique(s.kind, &c.kinds).unwrap_or(s);
        let Some(model) = r.world_model.clone() else { continue };
        let tag = r.world_model_tag.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| WORLD_DEFAULT_TAG.to_string());
        out.push(WorldPart { model, tag: tag.clone(), offset: r.world_offset, rotation: r.world_rotation, camo: s.weapon_camo.clone() });
        if let Some(add) = r.world_model_additional.clone() {
            out.push(WorldPart { model: add, tag, offset: r.world_add_offset, rotation: r.world_add_rotation, camo: s.weapon_camo.clone() });
        }
    }
    out
}

/// The art of `w` with the attachments `shorts` (BO2 create-a-class names).
///
/// BO2 reads two different uniques per attachment (G14 §2): the drawn parts (§3) and the animation
/// table (§5) read the *resolved* unique — a combined child (`au_xm8_acog+grip`) when the
/// combination holds one of its partner kinds — while the flags, hide tags, base-art offset and
/// sounds (§4) read the *single* uniques only. A combined child's own flags and hide tags are
/// therefore never consulted, by BO2 or here.
pub fn combo<'a>(w: &'a zhbo_formats::weapon::Weapon, atts: &[zhbo_formats::weapon::Attachment], shorts: &[&str]) -> Combo<'a> {
    use crate::weapons::short_name;
    let mut combo_kinds = Vec::new();
    for short in shorts {
        let Some((kind, id)) = w.attachment_variants.iter().find(|(_, id)| short_name(id) == *short) else { continue };
        let point = atts.iter().find(|a| &a.name == id).map_or(0, |a| a.point);
        combo_kinds.push((*kind, point));
    }
    let kinds: Vec<u32> = combo_kinds.iter().map(|(k, _)| *k).collect();
    let singles: Vec<&zhbo_formats::weapon::AttachmentUnique> = kinds.iter().filter_map(|k| w.single_unique(*k)).collect();
    let resolved = |kind: u32| w.resolved_unique(kind, &kinds);
    let parts: Vec<(&zhbo_formats::weapon::AttachmentUnique, u32, &zhbo_formats::weapon::AttachmentUnique)> = combo_kinds
        .iter()
        .filter_map(|(k, point)| resolved(*k).map(|u| (u, *point, w.single_unique(*k).unwrap_or(u))))
        .collect();
    // The animations (G14 §5, `FUN_0052d540`/`FUN_008eddb0`): start from the variant's table; a
    // slot is a candidate only when one of the combination's SINGLE uniques sets its
    // `animationOverrides` bit (`FUN_008eb540`), and each candidate is resolved through five tiers
    // over the RESOLVED uniques' override bits. A set bit with an empty name overrides to empty
    // (BO2 then plays the idle); a unique's name without its bit is never read.
    let base = w.single_unique(0);
    let optic_kind = combo_kinds.iter().find(|(_, p)| *p == 1).map(|(k, _)| *k);
    // Tier 2 is an ALT-MODE rule (`FUN_008eddb0` tests the def's `inventoryType == ALTMODE` before
    // `FUN_008ecb00`): the parent of an ADS-only alt (`an94_mp+dualoptic`) is not an alt mode, so
    // its alt raise/drop and ADS slots resolve through the tiers like any other slot (the dual
    // optic's `rmr_alt_raise_up`/`rmr_top_ads_up`).
    let alt_mode = w.inventory == zhbo_formats::weapon::inventory::ALTMODE;
    let alt_ads_only = alt_mode
        && (w.alt_weapon_ads_only
            || combo_kinds.iter().any(|(kind, _)| atts.iter().find(|a| a.kind == *kind).is_some_and(|a| a.alt_weapon_ads_only)));
    let priority = |kind: u32| -> i32 {
        atts.iter().find(|a| a.kind == kind).map_or(0, |a| a.first_raise_priority)
    };
    let mut sorted = kinds.clone();
    sorted.sort_unstable();
    sorted.dedup();
    sorted.truncate(3);
    let overriding = |k: u32, s: usize| resolved(k).filter(|u| u.overrides_anim(s));
    let mut anims = w.anims.clone();
    for (s, slot) in anims.iter_mut().enumerate() {
        if !singles.iter().any(|u| u.overrides_anim(s)) {
            continue;
        }
        *slot = if s == 0x18 {
            // 1. First raise (`FUN_008eb720`): the first field's resolved unique takes it if it
            // overrides the slot; a later field needs a strictly higher `firstRaisePriority`.
            let mut best = None;
            let mut prio = 0;
            for (i, k) in sorted.iter().enumerate() {
                if let Some(u) = overriding(*k, s) {
                    let p = priority(*k);
                    if i == 0 || p > prio {
                        best = u.anims.get(s).cloned().flatten();
                        prio = p;
                    }
                }
            }
            best
        } else if alt_ads_only && [0x1A, 0x1B, 0x55, 0x56, 0x57].contains(&s) {
            // 2. Alt-mode ADS-only: these slots stay the variant's.
            continue;
        } else if let Some(u) = optic_kind.filter(|_| (0x3D..=0x40).contains(&s) || (0x55..=0x57).contains(&s)).and_then(|k| overriding(k, s)) {
            // 3. The aim slots: the mounted optic's resolved unique, if it overrides the slot.
            u.anims.get(s).cloned().flatten()
        } else if let Some(u) = sorted.iter().find_map(|k| overriding(*k, s)) {
            // 4. The first field kind's resolved unique that overrides it, ascending eAttachment.
            u.anims.get(s).cloned().flatten()
        } else {
            // 5. The base unique's own clip (not the variant's).
            base.and_then(|u| u.anims.get(s).cloned().flatten())
        };
    }
    // `FUN_0052d540`: after the merge an alt mode puts back its own alt raise, alt drop and ADS
    // down (slots 0x1A, 0x1B, 0x56), whatever the attachments override.
    if alt_mode && !singles.is_empty() {
        for s in [0x1A, 0x1B, 0x56] {
            if let (Some(slot), Some(own)) = (anims.get_mut(s), w.anims.get(s)) {
                *slot = own.clone();
            }
        }
    }
    // The hidden bones (`FUN_008ec1a0`, G14 §4), over the single uniques as `FUN_008eb360` lists
    // them (ascending kinds; a kind without a unique stands in as the base unique, except in the
    // third field, which ends the list): every tag a single hides is hidden, and a tag the variant
    // hides stays hidden only if every single lists it too.
    let hide = match base.filter(|_| w.has_default_attachment && !sorted.is_empty()) {
        None => w.hide_tags.clone(),
        Some(u0) => {
            let lists: Vec<&[String]> = sorted
                .iter()
                .enumerate()
                .map(|(i, k)| w.single_unique(*k).or(if i < 2 { Some(u0) } else { None }))
                .map_while(|u| u.map(|u| u.hide_tags.as_slice()))
                .collect();
            let mut out: Vec<String> = w.hide_tags.iter().filter(|t| lists.iter().all(|l| l.contains(t))).cloned().collect();
            for t in lists.iter().flat_map(|l| l.iter()) {
                if !w.hide_tags.contains(t) && !out.contains(t) {
                    out.push(t.clone());
                }
            }
            out
        }
    };
    // The shot follows BO2's override gate (`FUN_008ebe80`): only the uniques the base unique lets
    // override it, the silencer's first.
    let gated = |bit, field: fn(&zhbo_formats::weapon::AttachmentUnique) -> &Option<String>, own: &Option<String>| {
        crate::weapons::gated_sound(w, atts, &kinds, bit, field).unwrap_or_else(|| own.clone())
    };
    Combo {
        no_clip: singles.iter().any(|u| u.disable_base_clip),
        no_base_attachment: singles.iter().any(|u| u.disable_base_attachment),
        base_attachment_offset: singles.iter().find(|u| u.override_base_attachment_offsets).map(|u| u.base_attachment_offset),
        fire_sound_player: gated(crate::weapons::FIRE_SOUND_PLAYER_BIT, |u| &u.fire_sound_player, &w.fire_sound_player),
        fire_sound: gated(crate::weapons::FIRE_SOUND_BIT, |u| &u.fire_sound, &w.fire_sound),
        parts,
        anims,
        hide,
        kinds,
    }
}

/// The animation table of an alt mode (`alt`, e.g. `gl_an94_mp`) toggled from a weapon carrying
/// `shorts`: BO2's alt weapon keeps its parent's attachment fields, so it merges them over its OWN
/// variant's uniques (`FUN_0052d540`), first dropping the attachment whose unique produced it
/// (`FUN_0052d470`, matched on the unique's `altWeaponIndex`) unless the alt is ADS-only
/// (`FUN_008ecb00`: the def's flag or any attachment's).
pub fn alt_anims(alt: &zhbo_formats::weapon::Weapon, atts: &[zhbo_formats::weapon::Attachment], shorts: &[&str]) -> Vec<Option<String>> {
    use crate::weapons::short_name;
    let kind_of = |short: &str| alt.attachment_variants.iter().find(|(_, id)| short_name(id) == short).map(|(k, _)| *k);
    let ads_only = alt.alt_weapon_ads_only
        || shorts.iter().filter_map(|s| kind_of(s)).any(|k| atts.iter().find(|a| a.kind == k).is_some_and(|a| a.alt_weapon_ads_only));
    let mut kept: Vec<&str> = shorts.to_vec();
    if !ads_only {
        let producer = kept.iter().position(|s| {
            kind_of(s).and_then(|k| alt.single_unique(k)).and_then(|u| u.alt_weapon.as_deref()) == Some(alt.name.as_str())
        });
        if let Some(i) = producer {
            kept.remove(i);
        }
    }
    combo(alt, atts, &kept).anims
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

/// The left gun of a dual-wield weapon, which the converter loads as a second rig: the def's own
/// `szDualWieldWeaponName` (`fnp45_dw_mp` names `fnp45_lh_mp`, G16). BO2's class never gives a
/// pistol with a `dw` attachment — the attachment switches the given weapon to the `_dw_mp` one —
/// so a single gun (and the `DWLEFTHAND` companion itself, which names none) has no pair.
pub(crate) fn left_gun<'a>(w: &zhbo_formats::weapon::Weapon, bo2: &'a [zhbo_formats::weapon::Weapon]) -> Option<&'a zhbo_formats::weapon::Weapon> {
    let name = w.dual_wield_weapon.as_deref()?;
    bo2.iter().find(|x| x.name == name && x.name != w.name)
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
    pool: &[&Zone],
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
        // The dual-wield pair: the gun plus the left gun its def names, as a second rig on the left
        // palm.
        let lh_w = left_gun(w, bo2);
        let lh_gun = lh_w.and_then(|l| l.view_model.as_deref()).and_then(|name| find_model(pool, name).ok());
        let dual = lh_w.filter(|_| lh_gun.is_some());
        let c = combo(w, atts, &shorts);
        let gun = find_model(pool, view).map_err(|e| format!("{id}: {e}"))?;
        // The base `attachViewModel` parts: the magazine (its authored offset; the name search is
        // the fallback) and the base attachment art (the snipers' scope), unless the combination
        // replaces that art (the ACOG) or the magazine (the dualclip).
        let base = base_parts(pool, w, &c, view);
        let parts: Vec<(&xmodel::Model, &str)> = vec![(&gun, mount_tag(w))];
        // The attachments' models, each with where it goes: an attachment's additional mount (the
        // IR rail) before its own model, both on the unique's tag. Each carries its single
        // unique's `weaponCamo`: BO2's camo pass reads the unique each DObj entry came from
        // (`FUN_00529600`, G16), so an optic wears its own camo asset, not the gun's.
        let mut extra: Vec<Extra> = Vec::new();
        for (u, pt, single) in &c.parts {
            // The list index of each of this unique's models in `extra`, for the ADS pairing.
            let mut at: Vec<Option<usize>> = Vec::new();
            for a in attachment_parts(u, *pt) {
                let Ok(model) = find_model(pool, a.name) else {
                    at.push(None);
                    continue;
                };
                at.push(Some(extra.len()));
                let ads_of = a.ads_of.and_then(|k| at.get(k).copied().flatten());
                extra.push(Extra { model, tag: u.view_model_tag.clone(), xf: a.xf, point: a.point, ads_only: a.ads_only, ads_of, camo: single.weapon_camo.clone() });
            }
        }
        let gun_tex = textures(&gun, paks, out, &mut looks);
        let base_tex: Vec<Vec<Option<String>>> = base.iter().map(|p| textures(&p.model, paks, out, &mut looks)).collect();
        let extra_tex: Vec<Vec<Option<String>>> = extra.iter().map(|e| textures(&e.model, paks, out, &mut looks)).collect();
        // The left gun of a dual-wield pair: its own textures and native blob, written once.
        let lh_tex = lh_gun.as_ref().map(|m| textures(m, paks, out, &mut looks));
        let native_lh = lh_gun.as_ref().zip(lh_tex.as_ref()).map(|(m, t)| native::model(m, t, &looks));
        // The native blobs (Track R): the same parts, unconverted, for the renderer's own BO2 pass.
        let native_gun = native::model(&gun, &gun_tex, &looks);
        let native_base: Vec<zhbo_native::model::Model> = base.iter().zip(&base_tex).map(|(p, t)| native::model(&p.model, t, &looks)).collect();
        let native_extra: Vec<zhbo_native::model::Model> = extra.iter().zip(&extra_tex).map(|(e, t)| native::model(&e.model, t, &looks)).collect();
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
                    for Extra { model, tag, xf, point: pt, .. } in extra.iter().filter(|e| !e.ads_only) {
                        let at = tag.as_deref().and_then(|t| rig.pivot(t)).unwrap_or(root);
                        rig.attach(model, at, *xf);
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
                for (k, (e, tex)) in extra.iter().zip(&extra_tex).enumerate() {
                    if e.ads_only {
                        continue;
                    }
                    meshes.extend(character::skin(rig, &e.model, &format!("A{k}"), tex));
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
        let alt_anims = (!alt_id.is_empty()).then(|| bo2.iter().find(|a| a.name == base_id(&alt_id))).flatten().map(|a| alt_anims(a, atts, &shorts));
        let mut sets = vec![(0usize, c.anims.clone())];
        if let Some(a) = alt_anims.clone() {
            sets.push((zhbo_core::weapons::ALT_CLIPS as usize, a));
        }
        let mut n = 0;
        let mut native_clips = Vec::new();
        for (base_slot, anims) in &sets {
            for (slot, anim_name) in anims.iter().enumerate() {
                let Some(anim_name) = anim_name else { continue };
                let anim = anim_cache.entry(anim_name.clone()).or_insert_with(|| find_anim(pool, anim_name));
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
        // BO2's `_LEFT` slots (0x4E-0x54, the left variant's table, G14 §5) map by what each plays
        // (`slot::left_as_right`: `LASTSHOT_LEFT` is the left gun's last shot even when it names
        // the fire clip); a shared clip (`pullout`, `sprint_*`) keeps its own slot.
        // An empty `_LEFT` slot registers the left variant's slot 1 (`FUN_0052d3e0`, G14 §5); when
        // that is empty too the slot stays empty (the runtime does not fall back for these ids).
        if let Some(lh_w) = dual {
            let registered = |s: usize| lh_w.anims.get(s).cloned().flatten().or_else(|| zhbo_core::viewmodel::slot::left_as_right(s).and_then(|_| lh_w.anims.get(1).cloned().flatten()));
            for left_slot in 0..lh_w.anims.len() {
                let Some(name) = registered(left_slot) else { continue };
                let name = &name;
                let right_slot = zhbo_core::viewmodel::slot::left_as_right(left_slot).map_or(left_slot, |r| r as usize);
                let anim = anim_cache.entry(name.clone()).or_insert_with(|| find_anim(pool, name));
                let Some(anim) = anim else { continue };
                native_clips.push(((right_slot as u16) | zhbo_native::viewmodel::INSTANCE_LEFT, native::put_anim(out, anim)));
                // The left gun's clips feed the native path and the sound events; the base W3D render
                // path's animation table stays as is.
                clips.push((vm, (right_slot as i32) | zhbo_native::viewmodel::INSTANCE_LEFT as i32, anim.clone()));
                n += 1;
            }
        }
        let attachments: Vec<native::AttachmentPart> = extra
            .iter()
            .map(|e| native::AttachmentPart {
                tag: e.tag.clone(),
                offset: (e.xf.0, e.xf.1.map(|c| c / character::SCALE)),
                optic: e.point == point::OPTIC as u32,
                ads_only: e.ads_only,
                ads_of: e.ads_of,
                camo: e.camo.clone(),
            })
            .collect();
        let base_native_parts: Vec<native::BasePart> = base
            .iter()
            .zip(&native_base)
            .map(|(p, m)| native::BasePart { model: m, tag: p.tag.clone(), offset: p.offset, optic: p.optic, ads_only: p.ads_only, ads_of: p.ads_of })
            .collect();
        let dual_tag = dual_mount();
        let dual_native = dual.zip(native_lh.as_ref()).map(|(w, m)| native::DualGun { gun: m, tag: dual_tag.as_str(), hide: &w.hide_tags, camo: w.weapon_camo.clone() });
        let vm_native = native::Viewmodel { vm, id, gun: &gun, gun_tag: mount_tag(w), gun_camo: w.weapon_camo.clone(), base_parts: base_native_parts, extra_models: extra.iter().map(|e| &e.model).collect(), attachments, hide: &c.hide, clips: &native_clips, dual: dual_native };
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

/// A world model loaded once: its parsed form, its textures and its native blob's path.
type WorldModel = (std::rc::Rc<xmodel::Model>, Vec<Option<String>>, String);

/// The third-person weapons: each converted weapon's BO2 world model with its base parts and its
/// attachments' (at BO2's world offsets, [`world_parts`]) as a native `.zbw` manifest for every id
/// in `converted`, and — for the first `base_count` (the base set) — rigid on the right hand as
/// `W<vm><k>` meshes on the soldier model, so the bridge shows the one in hand. Sets each base
/// definition's `world_meshes` (its alt mode shares them); a combination beyond the base set has
/// none, and the W3D body shows its base weapon's meshes (`Catalog::world_viewmodel`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn world_weapons(
    pool: &[&Zone],
    paks: &Paks,
    out: &mut BTreeMap<String, Vec<u8>>,
    rig: &Rig,
    bo2: &[zhbo_formats::weapon::Weapon],
    atts: &[zhbo_formats::weapon::Attachment],
    defs: &mut [zhbo_core::weapons::WeaponDef],
    converted: &[String],
    base_count: usize,
    looks: &mut Looks,
) -> (Vec<w3d::SkinMesh>, Vec<(usize, crate::xf::Xf)>) {
    let hand = rig.pivot(character::WEAPON_TAG).unwrap_or(0);
    let mut meshes = Vec::new();
    let mut flashes = Vec::new();
    // Each model once: its parsed form, its textures and its native blob's path. The ~20,000
    // legal combinations share a few hundred models, so a combination costs only its manifest.
    let mut cache: BTreeMap<String, Option<WorldModel>> = BTreeMap::new();
    let mut load = |name: &str, out: &mut BTreeMap<String, Vec<u8>>, looks: &mut Looks| {
        cache
            .entry(name.to_string())
            .or_insert_with(|| {
                let model = find_model(pool, name).ok()?;
                let tex = textures(&model, paks, out, looks);
                let path = crate::native::put_model(out, &crate::native::model(&model, &tex, looks));
                Some((std::rc::Rc::new(model), tex, path))
            })
            .clone()
    };
    for (vm, id) in converted.iter().enumerate() {
        let (base, shorts) = zhbo_core::attachments::split_id(id);
        let Some(w) = bo2.iter().find(|w| w.name == base) else { continue };
        // The Assault Shield is carried as the script's carry model on the left hand
        // (`_riotshield.gsc` `refreshshieldattachment`), not its pickup world model.
        let shield = crate::weapons::is_shield(w);
        let world_name = if shield { Some(crate::shield::CARRY_MODEL) } else { w.world_model.as_deref() };
        let Some((world, tex, world_path)) = world_name.and_then(|m| load(m, out, looks)) else { continue };
        let c = combo(w, atts, &shorts);
        let prefix = format!("W{vm:02}");
        let write_w3d = vm < base_count;
        if let Some(b) = world.bones.iter().find(|b| b.name == "tag_flash").filter(|_| write_w3d) {
            flashes.push((vm, character::scaled_xf((b.rotation, b.position))));
        }
        let mut mine = if write_w3d { character::skin_rigid(&world, hand, crate::xf::IDENTITY, &prefix, 0, &tex, &c.hide) } else { Vec::new() };
        // Track R7: the same world model and attachments as native blobs, composed at run time on
        // the General's `tag_weapon_right` (`zhbo_core::nativebody`). The native side takes BO2
        // inches, so its offsets stay unscaled (the W3D path's are scaled above).
        let mut native_parts = vec![zhbo_native::viewmodel::Part {
            model: world_path,
            tag: shield.then(|| crate::shield::HELD_TAG.to_string()),
            offset: crate::xf::IDENTITY,
            rigid: true,
            ads_only: false,
            camo: w.weapon_camo.clone(),
            ads_swap: None,
        }];
        // The base parts (the magazine, a sniper's scope) and the attachments, as BO2 mounts them.
        for u in world_parts(w, &c) {
            let Some((model, t, path)) = load(&u.model, out, looks) else { continue };
            // On the world model's own tag (`tag_weapon`, its root, by default), or its origin when
            // the model lacks the named bone.
            let tag = world.bones.iter().find(|b| b.name == u.tag);
            let base_xf = tag.map_or(crate::xf::IDENTITY, |b| character::scaled_xf((b.rotation, b.position)));
            let offset = crate::xf::compose(base_xf, character::scaled_xf((crate::xf::from_angles(u.rotation), u.offset)));
            if write_w3d {
                let first = mine.len();
                mine.extend(character::skin_rigid(&model, hand, offset, &prefix, first, &t, &[]));
            }
            let native_base = tag.map_or(crate::xf::IDENTITY, |b| (b.rotation, b.position));
            native_parts.push(zhbo_native::viewmodel::Part {
                model: path,
                // A tag the world model does not have hangs the attachment at its origin, as the
                // W3D path does; the native composition treats a missing tag the same way.
                tag: tag.map(|b| b.name.clone()),
                offset: crate::xf::compose(native_base, (crate::xf::from_angles(u.rotation), u.offset)),
                rigid: true,
                ads_only: false,
                camo: u.camo,
                ads_swap: None,
            });
        }
        // Carried but not up, the shield hangs on the back (`tag_stowed_back`, the stow model).
        if let Some(stow) = shield.then(|| find_model(pool, crate::shield::STOW_MODEL).ok()).flatten() {
            let t = textures(&stow, paks, out, looks);
            let part = zhbo_native::viewmodel::Part {
                model: crate::native::put_model(out, &crate::native::model(&stow, &t, looks)),
                tag: Some(crate::shield::STOWED_TAG.to_string()),
                offset: crate::xf::IDENTITY,
                rigid: true,
                ads_only: false,
                // The stowed shield wears the def's camo, as the held one does.
                camo: w.weapon_camo.clone(),
                ads_swap: None,
            };
            out.insert(zhbo_native::stowed_weapon_path(vm as i32), zhbo_native::thirdperson::Weapon { id: format!("{id}_stowed"), parts: vec![part], hide: Vec::new() }.to_bytes());
        }
        out.insert(
            zhbo_native::world_weapon_path(vm as i32),
            zhbo_native::thirdperson::Weapon { id: (*id).to_string(), parts: native_parts, hide: c.hide.clone() }.to_bytes(),
        );
        if !write_w3d {
            continue;
        }
        finish(&mut mine, looks);
        number(&mut mine, |k| crate::generals::world_mesh(vm as i32, k));
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
    pool: &[&Zone],
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
        if let Some(d) = defs.iter().find(|d| &d.id == id) {
            if !d.alt.is_empty() {
                ids.push(d.alt.clone());
            }
            // A base weapon's alt modes (the under-barrel launcher `gl_an94_mp`) launch their own
            // projectile; every combo carrying one shares it (`share_projectiles`).
            ids.extend(d.alt_modes.iter().map(|(_, alt)| alt.clone()).filter(|a| !ids_contains(converted, a)));
        }
    }
    ids.dedup();
    for (n, id) in ids.iter().enumerate() {
        let Some(w) = bo2.iter().find(|w| w.name == base_id(id)) else { continue };
        let Some(d) = defs.iter_mut().find(|d| &d.id == id) else { continue };
        let throws = d.class.is_offhand() || d.class == Class::Launcher || d.projectile_speed > 0.0;
        // The Assault Shield is set down as cover in the script's deployed model
        // (`_riotshield.gsc` `level.deployedshieldmodel`), not its pickup world model.
        let shield = d.class == Class::Shield;
        if !(throws || shield) || d.class == Class::Melee {
            continue;
        }
        let model_name = if shield { Some(crate::shield::CARRY_MODEL.to_string()) } else { w.projectile_model.clone().or_else(|| w.world_model.clone()) };
        let Some(model_name) = model_name else { continue };
        let Ok(model) = find_model(pool, &model_name) else { continue };
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
        // The deployed Assault Shield is cover Zero Hour's units can shoot: solid, attackable, a box
        // of its model's bounds (x is its facing). The engine never applies the damage — the
        // bridge's hook hands it to the core's wear-down (`zhbo_logic_shield_damage`) — so its
        // health only tells the AI what it is up against: the script's deployed health.
        let body = if shield {
            let k = character::SCALE;
            let half = |a: usize| (model.maxs[a] - model.mins[a]) * 0.5 * k;
            let health = crate::shield::deployed_health_zh();
            format!(
                "  KindOf = IMMOBILE\n  Body = ActiveBody ModuleTag_02\n    MaxHealth = {health:.1}\n    InitialHealth = {health:.1}\n  End\n{sight}  \
                 Geometry = Box\n  GeometryMajorRadius = {:.2}\n  GeometryMinorRadius = {:.2}\n  GeometryHeight = {:.2}\n  GeometryIsSmall = No\n",
                half(0),
                half(1),
                (model.maxs[2] - model.mins[2]) * k
            )
        } else {
            format!("  KindOf = NO_COLLIDE INERT IMMOBILE UNATTACKABLE CLICK_THROUGH\n  Body = InactiveBody ModuleTag_02\n  End\n{sight}  Geometry = Sphere\n  GeometryMajorRadius = 1.0\n  GeometryIsSmall = Yes\n")
        };
        ini.push_str(&format!(
            "Object {object}\n  Draw = W3DModelDraw ModuleTag_01\n    OkToChangeModelColor = Yes\n    DefaultConditionState\n      Model = {name}\n    End\n  End\n{body}End\n\n"
        ));
        d.projectile_object = object;
        models.insert(id.clone(), name.clone());
        made.push(id.as_str());
    }
    out.insert("Data\\INI\\Object\\ZHBO_BO2_Projectiles.ini".into(), ini.into_bytes());
    format!("projectiles {}", made.join(", "))
}

fn ids_contains(ids: &[String], id: &str) -> bool {
    ids.iter().any(|x| x == id)
}

/// A combo or alt-mode def fires its base weapon's BO2 `projectileModel`: give every def without a
/// projectile object the one its base (`base_id`) was converted with — the crossbow with an optic,
/// the under-barrel launcher of any combo (`gl_an94_mp~an94_mp+gl`).
pub(crate) fn share_projectiles(defs: &mut [zhbo_core::weapons::WeaponDef]) {
    let objects: BTreeMap<String, String> =
        defs.iter().filter(|d| !d.projectile_object.is_empty()).map(|d| (d.id.clone(), d.projectile_object.clone())).collect();
    for d in defs.iter_mut().filter(|d| d.projectile_object.is_empty()) {
        if let Some(o) = objects.get(base_id(&d.id)) {
            d.projectile_object = o.clone();
        }
    }
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
            .and_then(|i| paks.image(i.key()))
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
pub(crate) fn tablet_world(pool: &[&Zone], paks: &Paks, out: &mut BTreeMap<String, Vec<u8>>, rig: &Rig, looks: &mut Looks) -> Vec<w3d::SkinMesh> {
    let Ok(model) = find_model(pool, TABLET_WORLD) else { return Vec::new() };
    let Some(tag) = rig.pivot(TABLET_TAG).or_else(|| rig.pivot("tag_weapon_left")) else { return Vec::new() };
    let tex = textures(&model, paks, out, looks);
    let mut meshes = character::skin_rigid(&model, tag, crate::xf::IDENTITY, "TB", 0, &tex, &[]);
    finish(&mut meshes, looks);
    number(&mut meshes, crate::generals::tablet_mesh);
    meshes
}

/// Name a sub-object set `0..n` after [`finish`] dropped its unconvertible surfaces (the clan tag,
/// the emblem, reticles). The bridge shows a weapon by its count — `W<vm><k>` for `k` below
/// `WorldMeshes` — so a gap in the numbering left the surfaces after it never shown: the
/// third-person MP7 drew two of its four surfaces, see-through holes in the receiver and grip.
pub(crate) fn number(meshes: &mut [w3d::SkinMesh], name: impl Fn(usize) -> String) {
    for (k, m) in meshes.iter_mut().enumerate() {
        m.name = name(k);
    }
}

#[cfg(test)]
mod tests {
    /// A combo or an alt mode of a combo fires its base weapon's converted projectile.
    #[test]
    fn combos_share_their_base_weapons_projectile() {
        use zhbo_core::weapons::WeaponDef;
        let mut defs = vec![
            WeaponDef { id: "crossbow_mp".into(), projectile_object: "ZHBO_Proj_crossbow_mp".into(), ..Default::default() },
            WeaponDef { id: "crossbow_mp+acog".into(), ..Default::default() },
            WeaponDef { id: "gl_an94_mp".into(), projectile_object: "ZHBO_Proj_gl_an94_mp".into(), ..Default::default() },
            WeaponDef { id: "gl_an94_mp~an94_mp+gl".into(), ..Default::default() },
            WeaponDef { id: "an94_mp+gl".into(), ..Default::default() },
        ];
        super::share_projectiles(&mut defs);
        assert_eq!(defs[1].projectile_object, "ZHBO_Proj_crossbow_mp");
        assert_eq!(defs[3].projectile_object, "ZHBO_Proj_gl_an94_mp");
        assert_eq!(defs[4].projectile_object, "", "the rifle itself fires bullets");
    }

    use super::{attachment_parts, base_part_note, finish, left_gun, mount_tag, number, w3d, BasePart, Looks};
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
            hit_boxes: Vec::new(),
            materials: vec![Some(zhbo_formats::xmodel::Material { name: name.into(), textures: Vec::new(), state_bits: Vec::new(), sort_key: 0, ..Default::default() })],
        }
    }

    /// The third-person tear: `finish` drops the clan-tag/emblem surfaces in the middle of a world
    /// model, and the bridge shows `W<vm><k>` for `k` below the count. The kept meshes must be
    /// numbered `0..n` with no gap, or the ones after the gap are never shown.
    #[test]
    fn world_meshes_are_numbered_without_gaps_after_the_dropped_surfaces() {
        let mut looks = Looks::default();
        looks.dropped.insert("zhbo_clan_tag.tga".into());
        let mesh = |name: &str, t: &str| w3d::SkinMesh { name: name.into(), texture: Some(t.into()), ..Default::default() };
        let mut meshes = vec![mesh("W3000", "gun.tga"), mesh("W3001", "zhbo_clan_tag.tga"), mesh("W3002", "gun.tga"), mesh("W3003", "gun.tga")];
        finish(&mut meshes, &looks);
        number(&mut meshes, |k| crate::generals::world_mesh(30, k));
        let names: Vec<&str> = meshes.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["W3000", "W3001", "W3002"], "every kept mesh is below the count the bridge shows");
    }

    /// The third-person parts follow BO2's `FUN_008ace30`: the base `attachWorldModel` slots
    /// (slot 0 the base attachment art, slot 6 the clip) dropped by the single uniques' replacement
    /// flags and slot 0 moved by the overriding unique, then each attachment's world model and its
    /// additional model.
    #[test]
    fn the_world_parts_are_the_base_slots_then_the_attachments() {
        use zhbo_formats::weapon::AttachWorldModel;
        let slot = |slot: u32, model: &str| AttachWorldModel { slot, model: model.into(), tag: None, offset: [slot as f32, 0.0, 0.0], rotation: [0.0; 3] };
        let unique = |kind: u32, world: &str| AttachmentUnique { kind, world_model: Some(world.into()), world_model_tag: Some("tag_rail".into()), world_offset: [1.0, 2.0, 3.0], ..Default::default() };
        let mut ir = unique(2, "ir_world");
        ir.world_model_additional = Some("ir_mount_world".into());
        ir.override_base_attachment_offsets = true;
        ir.world_base_attachment_offset = [9.0, 0.0, 0.0];
        let mut ext = unique(3, "extclip_world");
        ext.disable_base_clip = true;
        let w = Weapon {
            attach_world_models: vec![slot(0, "scope_world"), slot(6, "mag_world")],
            attachment_variants: vec![(2, "ir".into()), (3, "extclip".into())],
            attachment_uniques: vec![ir, ext],
            ..Default::default()
        };
        let models = |shorts: &[&str]| super::world_parts(&w, &super::combo(&w, &[], shorts)).into_iter().map(|p| (p.model, p.offset[0])).collect::<Vec<_>>();
        assert_eq!(models(&[]), [("scope_world".to_string(), 0.0), ("mag_world".to_string(), 6.0)], "the bare weapon carries its base parts");
        assert_eq!(
            models(&["ir"]),
            [("scope_world".to_string(), 9.0), ("mag_world".to_string(), 6.0), ("ir_world".to_string(), 1.0), ("ir_mount_world".to_string(), 0.0)],
            "the IR moves the base scope and adds its mount"
        );
        assert_eq!(models(&["extclip"]), [("scope_world".to_string(), 0.0), ("extclip_world".to_string(), 1.0)], "the extended clip replaces slot 6");
        let tags: Vec<String> = super::world_parts(&w, &super::combo(&w, &[], &["ir"])).into_iter().map(|p| p.tag).collect();
        assert_eq!(tags, ["tag_weapon", "tag_weapon", "tag_rail", "tag_rail"], "an empty tag is the world default, `tag_weapon`");
    }

    /// A combination resolves each attachment through its single's child/sibling chain
    /// (`FUN_008eb690`): the first combined unique sharing an attachment with the others.
    #[test]
    fn the_resolved_unique_walks_the_childs_chain() {
        let single = AttachmentUnique { kind: 11, index: 11, child: 40, world_model: Some("grip_world".into()), ..Default::default() };
        let other = AttachmentUnique { kind: 11, index: 40, combined: 1 << 5, sibling: 41, ..Default::default() };
        let acog = AttachmentUnique { kind: 11, index: 41, combined: 1 << 2, world_model: Some("grip_acog_world".into()), ..Default::default() };
        let w = Weapon { attachment_uniques: vec![single.clone(), other, acog], ..Default::default() };
        assert_eq!(w.resolved_unique(11, &[11, 2]).unwrap().index, 41, "the acog+grip entry");
        assert_eq!(w.resolved_unique(11, &[11]).unwrap().index, 11, "alone: the single");
    }

    /// BO2's two replacement channels are separate fields: `disableBaseWeaponClip` suppresses the
    /// magazine, `disableBaseWeaponAttachment` the base attachment art (the AS50's ACOG). The flags
    /// are read from the single uniques only (`FUN_008ed820`/`FUN_008ed860`, G14 §3) — a combined
    /// child's own flags are never consulted — and never inferred from a name or a hide tag.
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

        // A combined child (`a`'s, sharing `b`) carrying the flag does not set it: BO2 reads the
        // single uniques for both channels. Its drawn part is still the child's.
        let a = AttachmentUnique { kind: 2, index: 2, child: 63, ..Default::default() };
        let b = AttachmentUnique { kind: 11, index: 11, ..Default::default() };
        let combined = AttachmentUnique { kind: 11, index: 63, combined: 0x804, disable_base_attachment: true, disable_base_clip: true, ..Default::default() };
        let pair = Weapon { attachment_variants: vec![(2, "a".into()), (11, "b".into())], attachment_uniques: vec![a, b, combined], ..Default::default() };
        let c = super::combo(&pair, &[], &["a", "b"]);
        assert!(!c.no_base_attachment, "the combined child's flag is not read");
        assert!(!c.no_clip, "the combined child's flag is not read");
        assert_eq!(c.parts.iter().map(|(u, _, _)| u.index).collect::<Vec<_>>(), [63, 11], "the drawn parts are the resolved uniques");
    }

    /// `viewModelOffsetBaseAttachment` applies only when the unique sets
    /// `overrideBaseWeaponAttachmentOffsets` (12 uniques in the catalog, all the four snipers' IR
    /// combinations) and is read from the first SINGLE unique that sets it (`FUN_008ed8f0`, G14 §3):
    /// a combined child's own offset is never consulted.
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
        // The swayreduc's `+ir` combined child carries its own offset; BO2 reads the single IR's.
        let combined = AttachmentUnique { kind: 14, index: 63, combined: 0x8004000, override_base_attachment_offsets: true, base_attachment_offset: [1.5, 0.0, 2.5], ..Default::default() };
        let sway = AttachmentUnique { kind: 27, index: 27, child: 63, ..Default::default() };
        let pair = weapon(vec![ir, sway, combined], vec![(14, "ir".into()), (27, "swayreduc".into())]);
        let c = super::combo(&pair, &[], &["swayreduc", "ir"]);
        assert_eq!(c.base_attachment_offset, Some([9.889, 0.0, 4.253]), "the single IR unique's offset");
        assert!(c.parts.iter().any(|(u, _, _)| u.index == 63), "the swayreduc draws its combined child");
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
        let (mount, at, point) = (parts[0].name, parts[0].xf, parts[0].point);
        assert_eq!((mount, point), ("t6_attach_optic_dualband_mount_view", 0), "the mount draws first and must not drive the aim marker");
        assert_eq!(at.1, crate::character::scaled([1.0, 0.0, 4.711]), "the mount rides its own add offset");
        let (optic, at, point) = (parts[1].name, parts[1].xf, parts[1].point);
        assert_eq!((optic, point), ("t6_attach_optic_dualband_view", 10), "the optic keeps the attachment's point");
        assert_eq!(at.1, crate::character::scaled([1.045, 0.0, 5.568]));
        assert_eq!(attachment_parts(&AttachmentUnique { view_model: Some("t6_attach_bcpu_view".into()), ..Default::default() }, 10).len(), 1, "no mount without viewModelAdditional");
    }

    /// A unique's `viewModelADS` is a second model of the same attachment: drawn only while
    /// aiming, instead of its `viewModel`, on the same tag at the same offset and rotation, and
    /// still the attachment's optic (G14 §3). The additional model is not paired: it draws in both
    /// lists.
    #[test]
    fn the_ads_model_pairs_with_its_view_model() {
        let holo = AttachmentUnique {
            view_model: Some("t6_attach_optic_holo_view".into()),
            view_model_ads: Some("t6_attach_optic_holo_ads".into()),
            view_offset: [2.0, 0.0, 4.5],
            view_rotation: [0.0, 1.0, 0.0],
            ..Default::default()
        };
        let parts = attachment_parts(&holo, 1);
        assert_eq!(parts.len(), 2, "{parts:?}");
        assert_eq!((parts[0].name, parts[0].ads_only, parts[0].ads_of), ("t6_attach_optic_holo_view", false, None));
        assert_eq!((parts[1].name, parts[1].ads_only, parts[1].ads_of), ("t6_attach_optic_holo_ads", true, Some(0)), "the ADS model replaces the view model");
        assert_eq!((parts[1].xf, parts[1].point), (parts[0].xf, 1), "the same placement and the optic's point");
        let mounted = AttachmentUnique { view_model_additional: Some("mount".into()), ..holo.clone() };
        let parts = attachment_parts(&mounted, 1);
        assert_eq!(parts.iter().map(|p| (p.name, p.ads_of)).collect::<Vec<_>>(), [("mount", None), ("t6_attach_optic_holo_view", None), ("t6_attach_optic_holo_ads", Some(1))]);
        let ads_alone = AttachmentUnique { view_model_ads: Some("x_ads".into()), ..Default::default() };
        let parts = attachment_parts(&ads_alone, 1);
        assert_eq!((parts.len(), parts[0].ads_only, parts[0].ads_of), (1, true, None), "an ADS model with no view model draws only while aiming");
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
            ads_of: None,
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

    /// A dual-wield weapon's left gun is the one its def names; a single gun and the left
    /// companion itself have none.
    #[test]
    fn the_left_gun_is_the_one_the_def_names() {
        let lh = Weapon { name: "fnp45_lh_mp".into(), inventory: inventory::DWLEFTHAND, ..Default::default() };
        let dw = Weapon { name: "fnp45_dw_mp".into(), dual_wield_weapon: Some("fnp45_lh_mp".into()), ..Default::default() };
        let base = Weapon { name: "fnp45_mp".into(), ..Default::default() };
        let all = vec![lh.clone(), dw.clone(), base.clone()];
        assert_eq!(left_gun(&dw, &all).map(|w| w.name.as_str()), Some("fnp45_lh_mp"));
        assert!(left_gun(&base, &all).is_none(), "the bare pistol is not a pair");
        assert!(left_gun(&lh, &all).is_none(), "the companion is the left half, not a pair");
    }

    /// The tests below author a unique's override bits from its named slots (BO2 sets the bit for
    /// every slot the unique overrides).
    fn with_bits(mut w: Weapon) -> Weapon {
        for u in &mut w.attachment_uniques {
            for (s, a) in u.anims.iter().enumerate() {
                if a.is_some() {
                    u.anim_overrides[s / 32] |= 1 << (s % 32);
                }
            }
        }
        w
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
        let w = with_bits(w);
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
        let mut w = Weapon {
            inventory: zhbo_formats::weapon::inventory::ALTMODE,
            attachment_variants: vec![(1, "gl".into())],
            attachment_uniques: vec![u],
            ..Default::default()
        };
        w.anims = vec![None; 0x58]; w.anims[0x1A] = Some("variant_alt_raise".into());
        let w = with_bits(w);
        let c = super::combo(&w, &[att], &["gl"]);
        assert_eq!(c.anims[0x1A].as_deref(), Some("variant_alt_raise"), "the alt-mode slot keeps the variant's clip");
    }

    /// An alt mode merges its parent's attachments over its own uniques, minus the one that produced
    /// it (`FUN_0052d470`); an ADS-only alt keeps the producer, and tier 2 keeps its own alt slots.
    #[test]
    fn the_alt_mode_merges_the_parents_attachments_minus_its_producer() {
        use zhbo_formats::weapon::inventory::ALTMODE;
        let gl = Attachment { name: "gl".into(), kind: 11, point: 2, ..Default::default() };
        let reflex = Attachment { name: "reflex".into(), kind: 20, point: 1, ..Default::default() };
        let mut gl_u = AttachmentUnique { kind: 11, alt_weapon: Some("gl_an94_mp".into()), ..Default::default() };
        gl_u.anims = vec![None; 0x58];
        gl_u.anims[0x01] = Some("gl_idle".into());
        let mut reflex_u = AttachmentUnique { kind: 20, ..Default::default() };
        reflex_u.anims = vec![None; 0x58];
        reflex_u.anims[0x55] = Some("reflex_ads_up".into());
        let mut alt = Weapon {
            name: "gl_an94_mp".into(),
            inventory: ALTMODE,
            attachment_variants: vec![(11, "gl".into()), (20, "reflex".into())],
            attachment_uniques: vec![gl_u, reflex_u],
            ..Default::default()
        };
        alt.anims = vec![None; 0x58];
        alt.anims[0x01] = Some("own_idle".into());
        let alt = with_bits(alt);
        let t = super::alt_anims(&alt, &[gl, reflex], &["gl", "reflex"]);
        assert_eq!(t[0x01].as_deref(), Some("own_idle"), "the producer's unique is dropped");
        assert_eq!(t[0x55].as_deref(), Some("reflex_ads_up"), "the parent's other attachment merges");

        // The dual optic: ADS-only, so its unique stays and the alt slots stay the variant's.
        let dualoptic = Attachment { name: "dualoptic".into(), kind: 3, point: 1, alt_weapon_ads_only: true, ..Default::default() };
        let mut u = AttachmentUnique { kind: 3, alt_weapon: Some("dualoptic_an94_mp".into()), ..Default::default() };
        u.anims = vec![None; 0x58];
        u.anims[0x1A] = Some("rmr_alt_raise_up".into());
        u.anims[0x3D] = Some("rmr_ads_fire".into());
        let mut alt = Weapon {
            name: "dualoptic_an94_mp".into(),
            inventory: ALTMODE,
            attachment_variants: vec![(3, "dualoptic".into())],
            attachment_uniques: vec![u],
            ..Default::default()
        };
        alt.anims = vec![None; 0x58];
        alt.anims[0x1A] = Some("rmr_alt_raise_down".into());
        let alt = with_bits(alt);
        let t = super::alt_anims(&alt, &[dualoptic], &["dualoptic"]);
        assert_eq!(t[0x1A].as_deref(), Some("rmr_alt_raise_down"));
        assert_eq!(t[0x3D].as_deref(), Some("rmr_ads_fire"), "the ADS-only producer is kept");
    }

    /// The dual optic's parent (`an94_mp+dualoptic`): its attachment is alt-ADS-only, but tier 2 is
    /// an alt-mode rule (`FUN_008eddb0` checks `inventoryType == ALTMODE` first), so the parent's
    /// alt raise and its ADS up/down/other-scope take the dual optic's clips, and the unique's
    /// empty alt drop overrides to empty (BO2 then plays the idle).
    #[test]
    fn the_dual_optic_parent_takes_the_hybrids_alt_and_ads_clips() {
        let att = Attachment { name: "dualoptic".into(), kind: 3, point: 1, alt_weapon_ads_only: true, ..Default::default() };
        let mut u = AttachmentUnique { kind: 3, ..Default::default() };
        u.anims = vec![None; 0x58];
        u.anims[0x1A] = Some("rmr_alt_raise_up".into());
        u.anims[0x55] = Some("rmr_top_ads_up".into());
        u.anims[0x56] = Some("rmr_top_ads_down".into());
        u.anims[0x57] = Some("rmr_bottom_ads_up".into());
        let mut w = Weapon {
            inventory: zhbo_formats::weapon::inventory::PRIMARY,
            attachment_variants: vec![(3, "dualoptic".into())],
            attachment_uniques: vec![u],
            ..Default::default()
        };
        w.anims = vec![None; 0x58];
        w.anims[0x1A] = Some("pullout".into());
        w.anims[0x1B] = Some("putaway".into());
        w.anims[0x55] = Some("ads_up".into());
        let mut w = with_bits(w);
        w.attachment_uniques[0].anim_overrides[0] |= 1 << 0x1B; // a set bit with an empty name
        let c = super::combo(&w, &[att], &["dualoptic"]);
        assert_eq!(c.anims[0x1A].as_deref(), Some("rmr_alt_raise_up"));
        assert_eq!(c.anims[0x1B], None, "the empty override wins over the variant's putaway");
        assert_eq!(c.anims[0x55].as_deref(), Some("rmr_top_ads_up"));
        assert_eq!(c.anims[0x56].as_deref(), Some("rmr_top_ads_down"));
        assert_eq!(c.anims[0x57].as_deref(), Some("rmr_bottom_ads_up"));
    }

    /// A launcher's parent (`an94_mp+gl`) takes the launcher unique's alt raise/drop; its alt mode
    /// (`gl_an94_mp`) puts its own 0x1A/0x1B/0x56 back after the merge (`FUN_0052d540`), even for
    /// an attachment that is not ADS-only.
    #[test]
    fn the_launcher_parent_takes_its_clips_and_the_alt_mode_restores_its_own() {
        let att = Attachment { name: "gl".into(), kind: 11, point: 2, ..Default::default() };
        let mut u = AttachmentUnique { kind: 11, ..Default::default() };
        u.anims = vec![None; 0x58];
        u.anims[0x1A] = Some("gl_alt_raise".into());
        u.anims[0x1B] = Some("gl_alt_drop".into());
        u.anims[0x56] = Some("gl_ads_down".into());
        u.anims[0x01] = Some("gl_idle".into());
        let mut w = Weapon { attachment_variants: vec![(11, "gl".into())], attachment_uniques: vec![u], ..Default::default() };
        w.anims = vec![None; 0x58];
        w.anims[0x1A] = Some("own_alt_raise".into());
        w.anims[0x1B] = Some("own_alt_drop".into());
        let parent = with_bits(w.clone());
        let c = super::combo(&parent, std::slice::from_ref(&att), &["gl"]);
        assert_eq!(c.anims[0x1A].as_deref(), Some("gl_alt_raise"));
        assert_eq!(c.anims[0x1B].as_deref(), Some("gl_alt_drop"));
        let alt = with_bits(Weapon { inventory: zhbo_formats::weapon::inventory::ALTMODE, ..w });
        let c = super::combo(&alt, &[att], &["gl"]);
        assert_eq!(c.anims[0x1A].as_deref(), Some("own_alt_raise"));
        assert_eq!(c.anims[0x1B].as_deref(), Some("own_alt_drop"));
        assert_eq!(c.anims[0x56], None, "the variant's own (empty) ADS down");
        assert_eq!(c.anims[0x01].as_deref(), Some("gl_idle"), "other slots still merge");
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
        let w = with_bits(w);
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
        let w = with_bits(w);
        let c = super::combo(&w, &[att2, att1], &["b", "a"]);
        assert_eq!(c.anims[0x01].as_deref(), Some("u1_idle"), "ascending kind 1 wins over kind 2");
    }

    /// G14 §5 priority 5: a candidate slot (a single unique overrides it) that no RESOLVED unique
    /// overrides takes the base unique's clip — here the single's combined child replaces it in
    /// the pair and does not override the slot.
    #[test]
    fn the_base_unique_is_the_final_fallback() {
        let (att1, att2) = (Attachment { kind: 1, ..Default::default() }, Attachment { kind: 2, ..Default::default() });
        let mut base = AttachmentUnique { kind: 0, ..Default::default() };
        base.anims = vec![None; 0x58]; base.anims[0x01] = Some("base_idle".into());
        let mut u = AttachmentUnique { kind: 1, index: 1, child: 63, ..Default::default() };
        u.anims = vec![None; 0x58]; u.anims[0x01] = Some("u_idle".into());
        u.anim_overrides[0] = 1 << 1;
        let child = AttachmentUnique { kind: 2, index: 63, combined: 6, anims: vec![None; 0x58], ..Default::default() };
        let mut w = Weapon { attachment_variants: vec![(1, "a".into()), (2, "b".into())], attachment_uniques: vec![base, u, child], ..Default::default() };
        w.anims = vec![None; 0x58]; w.anims[0x01] = Some("variant_idle".into());
        let c = super::combo(&w, &[att1.clone(), att2.clone()], &["a", "b"]);
        assert_eq!(c.anims[0x01].as_deref(), Some("base_idle"), "the base unique's clip, not the variant's");
        let c = super::combo(&w, &[att1], &["a"]);
        assert_eq!(c.anims[0x01].as_deref(), Some("u_idle"), "alone, the single overrides it");
    }

    /// Only the `animationOverrides` bits select: BO2 fills every slot of a unique's table (the
    /// an94's acog unique names the plain idle), and a name without its bit is never read; a bit
    /// with an empty name overrides to empty.
    #[test]
    fn only_the_override_bits_select_a_uniques_clip() {
        let (att1, att11) = (Attachment { kind: 1, ..Default::default() }, Attachment { kind: 11, ..Default::default() });
        let mut acog = AttachmentUnique { kind: 1, ..Default::default() };
        acog.anims = vec![None; 0x58]; acog.anims[0x01] = Some("an94_idle".into());
        let mut gl = AttachmentUnique { kind: 11, ..Default::default() };
        gl.anims = vec![None; 0x58]; gl.anims[0x01] = Some("an94_gl_idle".into());
        gl.anim_overrides[0] = 1 << 1 | 1 << 2;
        let mut w = Weapon { attachment_variants: vec![(1, "acog".into()), (11, "gl".into())], attachment_uniques: vec![acog, gl], ..Default::default() };
        w.anims = vec![None; 0x58]; w.anims[0x01] = Some("an94_idle".into()); w.anims[0x02] = Some("an94_fire".into());
        let c = super::combo(&w, &[att1, att11], &["acog", "gl"]);
        assert_eq!(c.anims[0x01].as_deref(), Some("an94_gl_idle"), "the acog's name without its bit is skipped");
        assert_eq!(c.anims[0x02], None, "the gl's bit with no name overrides to empty");
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
        let w = with_bits(w);
        let c = super::combo(&w, &[att], &["a"]);
        assert_eq!(c.anims[0x19].as_deref(), Some("variant_putaway_quick"), "the variant's own clip stands");
    }

    /// G14 §2/§5: a combined child is reached through its parent's `childLink`, and is picked when
    /// its mask shares another combo kind — not by its own `attachmentType` or an exact mask.
    #[test]
    fn the_linked_combined_child_participates_as_the_resolved_unique() {
        let att1 = Attachment { kind: 1, ..Default::default() };
        let att2 = Attachment { kind: 2, ..Default::default() };
        let att3 = Attachment { kind: 3, ..Default::default() };
        let mut u1 = AttachmentUnique { kind: 1, index: 1, child: 63, ..Default::default() };
        u1.anims = vec![None; 0x58]; u1.anims[0x01] = Some("u1_idle".into());
        let mut u2 = AttachmentUnique { kind: 2, index: 2, ..Default::default() };
        u2.anims = vec![None; 0x58]; u2.anims[0x01] = Some("u2_idle".into());
        let u3 = AttachmentUnique { kind: 3, index: 3, ..Default::default() };
        // The child's own attachmentType names the partner (2), as BO2 authors it.
        let mut child = AttachmentUnique { kind: 2, index: 63, combined: 6, ..Default::default() };
        child.anims = vec![None; 0x58]; child.anims[0x01] = Some("child_idle".into());
        let mut w = Weapon {
            attachment_variants: vec![(1, "a".into()), (2, "b".into()), (3, "c".into())],
            attachment_uniques: vec![u1, u2, u3, child],
            ..Default::default()
        };
        w.anims = vec![None; 0x58];
        let w = with_bits(w);
        let c = super::combo(&w, &[att1.clone(), att2.clone()], &["a", "b"]);
        assert_eq!(c.anims[0x01].as_deref(), Some("child_idle"), "kind 1 resolves to its linked child");
        let c = super::combo(&w, &[att1.clone(), att2, att3.clone()], &["a", "b", "c"]);
        assert_eq!(c.anims[0x01].as_deref(), Some("child_idle"), "a third kind does not block the overlap");
        let c = super::combo(&w, &[att1, att3], &["a", "c"]);
        assert_eq!(c.anims[0x01].as_deref(), Some("u1_idle"), "no partner: the single unique");
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
        let w = with_bits(w);
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
        let w = with_bits(w);
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
        let w = with_bits(w);
        let c = super::combo(&w, &[att], &["a"]);
        assert_eq!(c.anims[0x18].as_deref(), Some("variant_first_raise"), "no first-raise offer keeps the variant's clip");
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