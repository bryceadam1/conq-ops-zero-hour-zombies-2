//! Black Ops II multiplayer weapons for Conquer Ops: which of BO2's weapons a player can carry, and
//! each one's definition turned into the core's [`WeaponDef`] (Zero Hour units, BO2's display name).
//!
//! The converted table (`Data\ConquerOps\weapons.ini`) holds every player weapon, every attachment
//! and the alt modes they add, so later features (picking up, buying,
//! swapping) can hand out any of them. Viewmodels are converted for the weapons in [`converted`]
//! (the base set plus every legal Create-a-Class combination; `ZHBO_CONVERT_ALL_COMBOS=0` limits it
//! to the base set), and sounds, projectile models and HUD icons for [`converted_base`] (the
//! General's loadout with its attachments, then every other weapon; `ZHBO_CONVERT_ALL_WEAPONS=0`
//! limits it to the loadout).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use zhbo_core::attachments::{self, AttachmentDef};
use zhbo_core::units::inches;
use zhbo_core::weapons::{CamoOption, Class, Effect, FireMode, GunKick, Kick, Loadout, NpcSound, OffhandClass, Penetrate, Place, Reticle, Specialties, Sway, Table, CycleTimes, Traits, ViewPos, WeapClass, WeapType, WeaponDef, Zoom, MELEE_DAMAGE};
#[cfg(debug_assertions)]
use zhbo_core::weapons::{format_table, parse_table};
use zhbo_formats::weapon::{self as bo2, Weapon};

use crate::manifest;


/// BO2's create-a-class name of an attachment asset: `silencer_smg` is the `silencer`, `rf_fast` the
/// `rf`.
pub fn short_name(attachment: &str) -> String {
    attachment.split('_').next().unwrap_or(attachment).to_string()
}

/// The converted event for BO2's `emptyFireSoundPlayer` alias. The shipped table carries five
/// aliases; anything else falls back to the generic click.
pub(crate) fn dry_fire_event(alias: Option<&str>) -> &'static str {
    match alias {
        Some("wpn_generic_dryfire_plr") => "ZHBO_BO2_DryFireGeneric",
        Some("wpn_gl_rifle_dryfire_plr") => "ZHBO_BO2_DryFireGlRifle",
        Some("wpn_crossbow_dryfire_plr") => "ZHBO_BO2_DryFireCrossbow",
        Some("wpn_rpg_dryfire_plr") => "ZHBO_BO2_DryFireRpg",
        Some("wpn_beretta93r_dryfire_plr") => "ZHBO_BO2_DryFireBeretta",
        _ => "ZHBO_BO2_DryFireGeneric",
    }
}

/// A combo's 2D ADS overlay (material, low-res material, reticle), BO2's way (`FUN_008ed570/5f0/670`,
/// G14 §4): each field independently is the first SINGLE unique's value (in the combo's ascending
/// `eAttachment` order; a kind without a unique stands in as the base unique) that differs from the
/// base unique's, else the weapon's own. An optic whose unique carries no overlay therefore clears
/// it (the physical sight stays), a non-optic leaves the base scope, and in `ir+vzoom` the IR
/// supplies the material while the VZOOM supplies the reticle the IR leaves at the base's. A
/// combined child (`au_as50_swayreduc+ir`) never supplies the overlay.
fn overlay_for(w: &Weapon, shorts: &[&str]) -> (String, String, u32) {
    let own = (w.overlay_material.clone(), w.overlay_material_low_res.clone(), w.overlay_reticle);
    let Some(base) = w.single_unique(0) else { return own };
    let kinds = combo_kinds(w, shorts);
    let singles: Vec<&bo2::AttachmentUnique> = kinds.iter().map(|k| w.single_unique(*k).unwrap_or(base)).collect();
    let first = |differs: &dyn Fn(&bo2::AttachmentUnique) -> bool| singles.iter().copied().find(|u| differs(u));
    (
        first(&|u| u.overlay_material != base.overlay_material).map_or(own.0, |u| u.overlay_material.clone()),
        first(&|u| u.overlay_material_low_res != base.overlay_material_low_res).map_or(own.1, |u| u.overlay_material_low_res.clone()),
        first(&|u| u.overlay_reticle != base.overlay_reticle).map_or(own.2, |u| u.overlay_reticle),
    )
}

/// The combo's `eAttachment` kinds as BO2's accessors walk them (`FUN_008eb300`): ascending, at
/// most three.
fn combo_kinds(w: &Weapon, shorts: &[&str]) -> Vec<u32> {
    let mut kinds: Vec<u32> = shorts
        .iter()
        .filter_map(|short| w.attachment_variants.iter().find(|(_, id)| short_name(id) == *short))
        .map(|(kind, _)| *kind)
        .collect();
    kinds.sort_unstable();
    kinds.dedup();
    kinds.truncate(3);
    kinds
}

/// The first of a combo's single uniques (its kinds ascending, each kind's own unique) whose field
/// differs from the base unique's, else `None` — BO2's `FUN_008ed2c0` (reload start add) and
/// `FUN_008ebdb0` (ADS depth of field) pattern (G14 §4). No base unique: `None` (the def's stands).
fn first_differing_unique<T: PartialEq>(w: &Weapon, shorts: &[&str], get: impl Fn(&bo2::AttachmentUnique) -> T) -> Option<T> {
    let base = get(w.attachment_uniques.iter().find(|u| u.kind == 0 && u.combined == 0)?);
    single_uniques(w, shorts).into_iter().map(get).find(|x| *x != base)
}

/// A combo's single uniques: each attachment kind's own unique, kinds ascending (`FUN_008eb360`).
fn single_uniques<'w>(w: &'w Weapon, shorts: &[&str]) -> Vec<&'w bo2::AttachmentUnique> {
    let mut kinds: Vec<u32> =
        shorts.iter().filter_map(|s| w.attachment_variants.iter().find(|(_, id)| short_name(id) == *s).map(|(k, _)| *k)).collect();
    kinds.sort_unstable();
    kinds.iter().filter_map(|k| w.attachment_uniques.iter().find(|u| u.kind == *k && u.combined == 0)).collect()
}

/// The unique-carried handling fields of a combo: the first raise (`FUN_008ed100`), the
/// segmented reload's rounds per cycle (`FUN_008ed250`) and start add, and the aimed depth of
/// field (each component on its own), over the def's own values.
fn apply_unique_fields(d: &mut WeaponDef, w: &Weapon) {
    let shorts: Vec<&str> = d.attachments.iter().map(String::as_str).collect();
    if let Some(t) = first_differing_unique(w, &shorts, |u| u.first_raise_time) {
        d.first_raise_time = t;
    }
    if let Some(n) = first_differing_unique(w, &shorts, |u| u.reload_ammo_add) {
        d.reload_ammo_add = n.max(0) as u16;
    }
    // `FUN_008ebc70`: the def's flag, or any single unique's unless the variant ignores them.
    if !d.traits.ignore_attachments && single_uniques(w, &shorts).iter().any(|u| u.segmented_reload) {
        d.traits.segmented_reload = true;
    }
    if let Some(n) = first_differing_unique(w, &shorts, |u| u.reload_start_add) {
        d.timing.reload_start_add = n.max(0) as u16;
    }
    if let Some(x) = first_differing_unique(w, &shorts, |u| u.ads_dof.0) {
        d.zoom.dof.0 = x;
    }
    if let Some(x) = first_differing_unique(w, &shorts, |u| u.ads_dof.1) {
        d.zoom.dof.1 = x;
    }
}

/// The alt-toggle drop time of the combination `shorts` on `w` (`FUN_008ed1e0`): for a weapon that
/// is not itself an alt mode, the first single unique — walked in `eAttachment` order, each kind's
/// unique at its own index of `attachmentUniques` (`FUN_008eb360`) — whose `iAltDropTime` differs
/// from the base unique's; `None` keeps the def's own. Select fire's unique carries the 500 ms the
/// toggle into `sf_*` takes.
fn alt_drop_for(w: &Weapon, shorts: &[&str]) -> Option<f32> {
    if w.inventory == bo2::inventory::ALTMODE {
        return None;
    }
    let mut kinds: Vec<u32> =
        shorts.iter().filter_map(|s| w.attachment_variants.iter().find(|(_, id)| short_name(id) == *s).map(|(k, _)| *k)).collect();
    kinds.sort_unstable();
    kinds.dedup();
    let unique = |index: u32| w.attachment_uniques.iter().find(|u| u.index == index);
    let base = unique(0)?.alt_drop_time;
    kinds.iter().take(3).filter_map(|&k| unique(k)).map(|u| u.alt_drop_time).find(|&t| t != base)
}

/// The attachment's metadata from BO2's attachment table: the full name and description keys'
/// localized text.
#[derive(Clone, Debug, Default)]
pub struct AttachmentMeta {
    pub full_name: String,
    pub description: String,
    /// Its create-a-class icon material (column 6, `cac_mods_red_dot`).
    pub image: String,
    /// The shorts it may be combined with: the `attachment` row's `compatibleAttachments`, parsed by
    /// the conformance rule's own reader (`zhbo_conformance::bo2::combos::table`, `FUN_008e70f0`:
    /// an unknown name sets nothing), in `eAttachment` order.
    pub compatible: Vec<String>,
}

/// BO2's attachment metadata from the player's `mp/attachmenttable.csv` (`patch_mp`): each short
/// name's display and description keys. Missing rows are omitted.
pub fn attachment_metadata(patch: &zhbo_formats::t6zone::Zone, names: &HashMap<String, String>) -> HashMap<String, AttachmentMeta> {
    let mut meta = HashMap::new();
    let Ok(Some(rows)) = zhbo_formats::stringtable::rows(patch, "mp/attachmenttable.csv") else { return meta };
    let compat = zhbo_conformance::bo2::combos::table(&rows);
    for row in rows {
        // Column 4 is the display name key, column 5 the short name, column 8 the description key.
        let (Some(short), Some(display_key), Some(desc_key)) = (row.get(4), row.get(3), row.get(7)) else { continue };
        // Exactly as BO2's strings have them (some descriptions end in spaces; the table quotes them).
        let full_name = names.get(display_key).cloned().unwrap_or_default();
        let description = names.get(desc_key).cloned().unwrap_or_default();
        let compatible = zhbo_conformance::bo2::combos::kind_of(short)
            .and_then(|k| compat.rows.get(&k))
            .map(|(_, mask)| (0..64).filter(|b| mask & (1u64 << b) != 0).filter_map(|b| zhbo_conformance::weapons::ATTACHMENTS.get(b)).map(|s| s.to_string()).collect())
            .unwrap_or_default();
        let image = row.get(6).map(|s| s.trim().to_string()).unwrap_or_default();
        meta.insert(short.clone(), AttachmentMeta { full_name, description, image, compatible });
    }
    meta
}

/// BO2's `defaultattachment`, the asset the stacking accessors compare each attachment's field
/// against (`DAT_0316fc84`, e.g. `FUN_008ed080`). It lives in `patch_mp`: `common_mp` carries only
/// a zeroed `,defaultattachment` reference stub, whose 0.0s are not the engine's values. This is
/// the one loader of it; the table builders take its result.
pub fn default_attachment(patch: &zhbo_formats::t6zone::Zone) -> Option<bo2::Attachment> {
    // zone-local: one named asset from the pool's top zone.
    bo2::attachments(patch).ok()?.into_iter().find(|a| a.name == DEFAULT_ATTACHMENT)
}

/// Each attachment's bar deltas on a weapon, keyed `(attachment short, weapon id)`.
pub type AttachmentBars = HashMap<(String, String), [f32; 4]>;

/// BO2's create-a-class bars from the player's `mp/attributestable.csv` (`patch_ui_mp`), as
/// `weaponattributelist.lua` reads them: `TableLookup(table, 1, ref, col)` for a weapon and
/// `TableLookup(table, 1, attachment, 9, weapon, col)` for an attachment's deltas on it. The
/// columns (0-based) are accuracy 2, damage 3, range 4 and 6 for the bar MP labels FIRE RATE (its
/// "mobility" bar); returned in the core's order (fire rate, damage, range, accuracy). Keys are
/// `<ref>_mp` (`an94_mp`), and `(<attachment>, <ref>_mp)` for the deltas.
pub fn attributes(patch_ui: &zhbo_formats::t6zone::Zone) -> (HashMap<String, [f32; 4]>, AttachmentBars) {
    let mut weapons = HashMap::new();
    let mut attachments = HashMap::new();
    let Ok(Some(rows)) = zhbo_formats::stringtable::rows(patch_ui, "mp/attributestable.csv") else { return (weapons, attachments) };
    for row in rows {
        let (Some(group), Some(name)) = (row.first(), row.get(1)) else { continue };
        if !group.starts_with("weapon_") {
            continue;
        }
        let num = |i: usize| row.get(i).and_then(|s| s.trim().parse::<f32>().ok()).unwrap_or(0.0);
        let bars = [num(6), num(3), num(4), num(2)];
        match row.get(9).map(|s| s.trim()).filter(|s| !s.is_empty()) {
            // A row keyed by a weapon too is an attachment's deltas on it (the Skorpion's
            // silencer sits under `weapon_smg`, so the second key decides, not the group).
            Some(weapon) => {
                attachments.entry((name.clone(), format!("{weapon}_mp"))).or_insert(bars);
            }
            None => {
                weapons.entry(format!("{name}_mp")).or_insert(bars);
            }
        }
    }
    (weapons, attachments)
}

/// Fill every base weapon's bars and its offered attachments' deltas from [`attributes`].
pub fn apply_attributes(table: &mut Table, weapons: &HashMap<String, [f32; 4]>, attachments: &AttachmentBars) {
    for d in table.weapons.iter_mut() {
        if let Some(a) = weapons.get(&d.id) {
            d.attributes = *a;
        }
        d.attachment_attributes =
            d.allowed.iter().filter_map(|(short, _)| attachments.get(&(short.clone(), d.id.clone())).map(|a| (short.clone(), *a))).collect();
    }
}

/// The core's definition of a BO2 attachment. `names` maps localize keys to text.
///
/// `default` is BO2's `defaultattachment` ([`default_attachment`]): an attachment field equal to
/// its value changes nothing in BO2's first/last-differing accessors (G14 §4), so it is carried as
/// `None` (a fixture without it compares against zeros).
pub fn attachment_def(a: &bo2::Attachment, default: &bo2::Attachment, names: &HashMap<String, String>, meta: &HashMap<String, AttachmentMeta>) -> AttachmentDef {
    let differs = |x: f32, base: f32| (x != base).then_some(x);
    let short = short_name(&a.name);
    let m = meta.get(&short).cloned().unwrap_or_default();
    let fire = a.fire_type.map(|f| match f {
        bo2::fire::FULLAUTO => FireMode::Auto,
        bo2::fire::STACKED => FireMode::Stacked,
        bo2::fire::BURST2 | bo2::fire::BURST3 | bo2::fire::BURST4 | bo2::fire::BURST5 => FireMode::Burst((f - bo2::fire::BURST2 + 2) as u8),
        _ => FireMode::Semi,
    });
    AttachmentDef {
        id: a.name.clone(),
        short: short.clone(),
        display_name: names.get(&a.display_name).map(|s| s.trim().to_string()).unwrap_or_else(|| short.clone()),
        full_name: m.full_name,
        description: m.description,
        image: m.image,
        point: a.point as u8,
        silenced: a.silenced,
        laser: a.laser_sight,
        mms_max_dist: a.mms_max_dist,
        fire,
        clip: if fire.is_some() { a.clip_size.max(0) as u16 } else { 0 },
        damage_range_scale: a.damage_range_scale,
        ads_fov: if a.ads_zoom_fov > 1.0 { a.ads_zoom_fov } else { 0.0 },
        ads_in_scale: a.ads_in_scale,
        ads_out_scale: a.ads_out_scale,
        // Kept only when it differs from `defaultattachment`'s: that is when BO2 lets it replace
        // the weapon's ADS walk (`FUN_008ed080`); 0 leaves the weapon's own.
        ads_move_scale: differs(a.ads_move_scale, default.ads_move_scale).unwrap_or(0.0),
        hip_spread_min_scale: a.hip_spread_min_scale,
        hip_spread_max_scale: a.hip_spread_max_scale,
        fire_time_scale: a.fire_time_scale,
        reload_scale: a.reload_scale,
        reload_empty_scale: a.reload_empty_scale,
        reload_add_scale: a.reload_add_scale,
        clip_scale: a.clip_size_scale,
        recoil_reduction: a.recoil_reduction,
        specialties: Specialties::from_words(a.perks.0, a.perks.1),
        kind: a.kind as u8,
        reload_quick_scale: (a.reload_quick_scale, a.reload_quick_empty_scale, a.reload_quick_add_scale),
        ads_idle_scale: a.ads_idle_amount_scale,
        hip_idle: differs(a.hip_idle_amount, default.hip_idle_amount),
        fov2: differs(a.ads_zoom_fov2, default.ads_zoom_fov2),
        fov3: differs(a.ads_zoom_fov3, default.ads_zoom_fov3),
        zoom_in_frac: differs(a.ads_zoom_in_frac, default.ads_zoom_in_frac),
        zoom_out_frac: differs(a.ads_zoom_out_frac, default.ads_zoom_out_frac),
        alt_scope_in_time: differs(a.alt_scope_ads_in_time, default.alt_scope_ads_in_time),
        alt_scope_out_time: differs(a.alt_scope_ads_out_time, default.alt_scope_ads_out_time),
        stack_fire: (a.stack_fire_spread, a.stack_fire_accuracy_decay),
        penetrate: Penetrate::from_bo2(a.penetrate_type),
        dual_mag: a.dual_mag,
        infrared: a.infrared,
        mms: a.mms_weapon,
        use_as_melee: a.use_as_melee,
        alt_ads_only: a.alt_weapon_ads_only,
        alt_disable_switching: a.alt_weapon_disable_switching,
        ads_kick_center_scale: a.ads_view_kick_center_scale,
        sway: a.sway_override.map(sway),
        ads_sway: a.ads_sway_override.map(sway),
        stand_move_f: differs(a.stand_move_f, default.stand_move_f),
        stand_rot_pitch: differs(a.stand_rot[0], default.stand_rot[0]),
        stand_rot_roll: differs(a.stand_rot[2], default.stand_rot[2]),
        strafe_rot_roll: differs(a.strafe_rot_r, default.strafe_rot_r),
        compatible: m.compatible,
    }
}

/// The name of BO2's `defaultattachment` (see [`default_attachment`]).
pub const DEFAULT_ATTACHMENT: &str = "defaultattachment";

/// BO2's perk rows from the player's `mp/statstable.csv` (`patch_mp`): each `specialty` row's
/// reference (column 3, `PERKS_LIGHTWEIGHT` -> `LIGHTWEIGHT`), its localized name (column 3's key)
/// and description (column 7's key) from `names`, and the specialties it grants (column 4,
/// `|`-separated). Rows with a specialty the core does not know are skipped (Ghidra G1's index table
/// names all 62).
pub fn perk_rows(patch: &zhbo_formats::t6zone::Zone, names: &HashMap<String, String>) -> Vec<zhbo_core::weapons::PerkRow> {
    let Ok(Some(rows)) = zhbo_formats::stringtable::rows(patch, "mp/statstable.csv") else { return Vec::new() };
    let loc = |k: Option<&String>| k.and_then(|k| names.get(k.trim())).cloned().unwrap_or_default();
    rows.iter()
        .filter(|r| r.get(2).is_some_and(|c| c == "specialty"))
        .filter_map(|r| {
            let stats = r.get(3)?.strip_prefix("PERKS_")?.to_string();
            let specialties = Specialties::parse(r.get(4)?).ok()?;
            (!specialties.is_empty()).then(|| zhbo_core::weapons::PerkRow { stats, name: loc(r.get(3)), description: loc(r.get(7)), specialties })
        })
        .collect()
}

/// BO2's bonus cards from the player's `mp/statstable.csv` (`patch_mp`): every `bonuscard` row the
/// class editor can charge (column 12 a cost, as the CaC lists filter), with its localized name
/// and description (`names`), image, loadout slot, sort key, index and unlock level
/// ([`zhbo_core::weapons::BonusCard`]).
pub fn bonus_card_rows(patch: &zhbo_formats::t6zone::Zone, names: &HashMap<String, String>) -> Vec<zhbo_core::weapons::BonusCard> {
    let Ok(Some(rows)) = zhbo_formats::stringtable::rows(patch, "mp/statstable.csv") else { return Vec::new() };
    let cell = |r: &Vec<String>, i: usize| r.get(i).map(|c| c.trim().to_string()).unwrap_or_default();
    let num = |r: &Vec<String>, i: usize| cell(r, i).parse::<i32>().ok();
    let loc = |k: String| names.get(&k).cloned().unwrap_or(k);
    rows.iter()
        .filter(|r| r.get(2).is_some_and(|c| c == "bonuscard"))
        .filter_map(|r| {
            let cost = num(r, 12).filter(|c| *c >= 0)?;
            Some(zhbo_core::weapons::BonusCard {
                reference: cell(r, 4),
                name: loc(cell(r, 3)),
                description: loc(cell(r, 7)),
                image: cell(r, 6),
                slot: cell(r, 13),
                cost,
                sort: num(r, 9).unwrap_or(0),
                index: num(r, 0).unwrap_or(0),
                unlock_level: num(r, 10).unwrap_or(0),
            })
        })
        .filter(|b| !b.reference.is_empty())
        .collect()
}

/// BO2's first 19 camo rows: `camo_none` plus 18 (`lua-dump/patch_mp/mp/attachmenttable.csv:32-50`),
/// the recipe list of a camo asset no player weapon wears. The class screen's tabs come from each
/// row's [`zhbo_core::weapons::OptionTab`].
pub const BASE_CAMO_OPTIONS: usize = 19;

/// BO2's reticle colours (`mp/attachmenttable.csv:68-74`: `reticle_color_1..7`).
pub const BASE_RETICLE_COLORS: usize = 7;

/// The option ids a `digital_unlock` milestone grants (`mp/statsmilestones1..4.csv`: column 4 the
/// type, column 9 the option, e.g. `camo_elite1`, `camo_ghost`, `reticle_31`). BO2's personalize
/// screens put these on the DLC tab (`AddCamos`: `GetChallengeForItemOption` then the milestone's
/// column 4).
fn digital_unlocks(patch: &zhbo_formats::t6zone::Zone) -> std::collections::HashSet<String> {
    (1..=4)
        .filter_map(|n| zhbo_formats::stringtable::rows(patch, &format!("mp/statsmilestones{n}.csv")).ok().flatten())
        .flat_map(|rows| rows.into_iter())
        .filter(|r| r.get(4).is_some_and(|t| t.trim() == "digital_unlock"))
        .filter_map(|r| r.get(9).map(|o| o.trim().to_string()).filter(|o| !o.is_empty()))
        .collect()
}

/// The `weaponoption` rows of one group in `mp/attachmenttable.csv` (`patch_mp`), in table order,
/// each with its personalize tab ([`zhbo_core::weapons::OptionTab::of`]).
fn weapon_options(patch: &zhbo_formats::t6zone::Zone, names: &HashMap<String, String>, group: &str, take: usize) -> Vec<CamoOption> {
    let Ok(Some(rows)) = zhbo_formats::stringtable::rows(patch, "mp/attachmenttable.csv") else { return Vec::new() };
    let unlocks = digital_unlocks(patch);
    rows.iter()
        .filter(|r| r.get(1).is_some_and(|c| c == group))
        .take(take)
        .filter_map(|r| {
            let id = r.get(4)?.trim();
            if id.is_empty() {
                return None;
            }
            let name = r.get(3).and_then(|k| names.get(k)).map(|s| s.trim().to_string()).unwrap_or_else(|| id.to_string());
            let image = r.get(6).cloned().unwrap_or_default();
            let sort = r.get(5).and_then(|s| s.trim().parse().ok()).unwrap_or(0);
            let mtx = r.get(16).is_some_and(|m| m.trim().starts_with("mtx_"));
            let tab = zhbo_core::weapons::OptionTab::of(mtx, sort, unlocks.contains(id));
            Some(CamoOption { id: id.to_string(), name, image, sort, tab, mtx })
        })
        .collect()
}

/// BO2's camo options: every `camo` row (`mp/attachmenttable.csv:32-129`), `camo_none` first.
/// Column 4 is the id, 3 the `MPUI_*` name, 6 the `menu_camo_*_pattern` thumbnail and 5 the sort.
pub fn camo_options(patch: &zhbo_formats::t6zone::Zone, names: &HashMap<String, String>) -> Vec<CamoOption> {
    weapon_options(patch, names, "camo", usize::MAX)
}

/// BO2's reticle options, every `reticle` row in the table's order: the base `reticle_0..10` and the
/// store rows `reticle_11..37` (`mtx_weapon*`, the DLC/DLC2 tabs). Each row's lens is its
/// `weaponoptions.csv` row and its grid art the optic's `<optic>_<N>` material, both in the base
/// game's zones (`patch_mp`, `patch_ui_mp`), so every row converts.
pub fn reticle_options(patch: &zhbo_formats::t6zone::Zone, names: &HashMap<String, String>) -> Vec<CamoOption> {
    weapon_options(patch, names, "reticle", usize::MAX)
}

/// BO2's reticle colours (`reticle_color_1..7`): the names are Red, Green, Blue, Purple, Teal,
/// Yellow, Orange (`MPUI_RETICLE_COLOR_*`).
pub fn reticle_color_options(patch: &zhbo_formats::t6zone::Zone, names: &HashMap<String, String>) -> Vec<CamoOption> {
    weapon_options(patch, names, "reticle_color", BASE_RETICLE_COLORS)
}

/// The option tables the converter feeds the weapon table: BO2's camo rows and the base
/// reticle/colour rows. Each weapon's camo asset is its own def's `weaponCamo` ([`to_def`]).
pub struct OptionTables<'a> {
    /// Who is a player weapon ([`Roster`]).
    pub roster: &'a Roster,
    pub camo: &'a [CamoOption],
    pub reticles: &'a [CamoOption],
    pub reticle_colors: &'a [CamoOption],
}

/// The whole table: every player weapon, the alt modes their attachments add, and every
/// attachment. `default_att` is [`default_attachment`].
pub fn table(bo2: &[Weapon], atts: &[bo2::Attachment], default_att: Option<&bo2::Attachment>, names: &HashMap<String, String>, roster: &Roster) -> Table {
    let opts = OptionTables { roster, camo: &[], reticles: &[], reticle_colors: &[] };
    table_with_metadata(bo2, atts, default_att, names, &HashMap::new(), &opts)
}

/// The whole table with attachment metadata (full name, description and attribute-table deltas)
/// and the camo/reticle/colour option tables.
pub fn table_with_metadata(
    bo2: &[Weapon],
    atts: &[bo2::Attachment],
    default_att: Option<&bo2::Attachment>,
    names: &HashMap<String, String>,
    meta: &HashMap<String, AttachmentMeta>,
    opts: &OptionTables,
) -> Table {
    let mut weapons: Vec<WeaponDef> = bo2.iter().filter(|w| opts.roster.is_player_weapon(w)).map(|w| to_def(w, names)).collect();
    // A sniper rifle is a `rifle` to its weapon def; the statstable group says it is a sniper.
    for d in &mut weapons {
        if d.class == Class::Rifle && opts.roster.item(&d.id).is_some_and(|i| i.group == "weapon_sniper") {
            d.class = Class::Sniper;
        }
    }
    // The weapons the roster's reach without being items: the alt modes their attachments add
    // and the grenades their projectiles become (`explosive_bolt_mp`).
    let mut extra: Vec<String> = Vec::new();
    for d in &weapons {
        extra.extend(d.alt_modes.iter().map(|(_, alt)| alt.clone()));
        extra.extend((!d.spawns.is_empty()).then(|| d.spawns.clone()));
    }
    for id in extra {
        if weapons.iter().any(|d| d.id == id) {
            continue;
        }
        if let Some(w) = bo2.iter().find(|w| w.name == id) {
            let mut d = to_def(w, names);
            if w.inventory == bo2::inventory::ALTMODE {
                // A launcher alt mode has its own grenades; select fire and a second optic share
                // the weapon's magazine.
                d.alt_own_ammo = d.class == Class::Launcher;
                // BO2 counts the alt launcher's grenades as rounds, not magazines: the start count
                // includes the loaded one, the max is the spare cap (`FUN_005e08d0`; G16).
                if d.alt_own_ammo {
                    d.start_reserve = (w.start_ammo - w.clip_size).max(0) as u16;
                    d.max_reserve = (w.max_ammo.max(0) as u16).max(d.start_reserve);
                }
            }
            weapons.push(d);
        }
    }
    let default = default_att.cloned().unwrap_or_default();
    let attachments = atts.iter().filter(|a| a.kind != 0 && !a.name.starts_with(',')).map(|a| attachment_def(a, &default, names, meta)).collect();
    for d in &mut weapons {
        // Every row in table order; the camo pass trims the DLC rows the install has no art for
        // (`camo::offer_what_converted`) and the class screen sorts them onto their tabs.
        d.camo = opts.camo.iter().map(|c| c.id.clone()).collect();
    }
    Table {
        weapons,
        attachments,
        camo: opts.camo.to_vec(),
        reticles: opts.reticles.to_vec(),
        reticle_colors: opts.reticle_colors.to_vec(),
        tablet_meshes: 0,
        perks: Vec::new(),
        bonus_cards: Vec::new(),
        ui_text: [ui_text(names), reticle_names(names)].concat(),
        weapon_groups: Vec::new(),
        class_caps: Vec::new(),
        hit_boxes: Vec::new(),
        shocks: Vec::new(),
        installed_dlc: Vec::new(),
    }
}

/// The localized UI strings the core draws ([`zhbo_core::weapons::UI_TEXT_KEYS`]), exactly as the
/// install's english zones have them; a key the install lacks is left out (the core falls back).
pub fn ui_text(names: &HashMap<String, String>) -> Vec<zhbo_core::weapons::UiText> {
    zhbo_core::weapons::UI_TEXT_KEYS
        .iter()
        .filter_map(|k| names.get(*k).map(|text| zhbo_core::weapons::UiText { key: k.to_string(), text: text.clone() }))
        .collect()
}

/// The reticle grid's cell names: every `MPUI_RETICLE_<OPTIC>_<N>` string the install has for the
/// custom-reticle optics ([`zhbo_core::weapons::reticle_name_key`]; BO2's `ReticlesGridButton.new`
/// localizes `"MPUI_RETICLE_" .. attachmentRef .. "_" .. index`), in key order.
pub fn reticle_names(names: &HashMap<String, String>) -> Vec<zhbo_core::weapons::UiText> {
    let prefixes: Vec<String> = zhbo_core::weapons::RETICLE_OPTICS.iter().map(|o| format!("MPUI_RETICLE_{}_", o.to_ascii_uppercase())).collect();
    let mut rows: Vec<zhbo_core::weapons::UiText> = names
        .iter()
        .filter(|(k, _)| prefixes.iter().any(|p| k.strip_prefix(p.as_str()).is_some_and(|n| n.parse::<u32>().is_ok())))
        .map(|(k, v)| zhbo_core::weapons::UiText { key: k.clone(), text: v.clone() })
        .collect();
    rows.sort_by(|a, b| a.key.cmp(&b.key));
    rows
}

/// Weapons with attachments, and their alt modes, that [`converted`] asks for: built from their base
/// definitions (the core's own rules) and added to the table, so their art can be set on them. An
/// alt mode for a converted weapon is `<alt>~<weapon>` (it shares the weapon's viewmodel). The
/// overlay fields follow the mounted optic: a replacement optic's unique supplies them, an empty
/// unique leaves an empty overlay, and no replacement optic keeps the base def's.
pub fn add_variants(table: &mut Table, converted: &[String], bo2: &[Weapon]) {
    for id in converted.iter().filter(|id| id.contains('+')) {
        if table.weapons.iter().any(|d| &d.id == id) {
            continue;
        }
        let Some((d, alt)) = build_variant(table, id, bo2) else { continue };
        // An id whose attachments do not all resolve on the weapon builds a combination already in
        // the table (`an94_mp+fmj+<unoffered>` is `an94_mp+fmj`): one entry per id, or the
        // catalog's later duplicate (no art, no sounds) shadows the converted one.
        if table.weapons.iter().any(|w| w.id == d.id) {
            continue;
        }
        table.weapons.extend(alt.filter(|a| !table.weapons.iter().any(|w| w.id == a.id)));
        table.weapons.push(d);
    }
}

/// One combination `id` and its alt mode (when an attachment adds one), derived from the table's
/// base defs by the core's attachment rule, with the overlay of the mounted optic, the alt drop and
/// the unique-carried handling fields.
fn build_variant(table: &Table, id: &str, bo2: &[Weapon]) -> Option<(WeaponDef, Option<WeaponDef>)> {
    let (base_id, shorts) = attachments::split_id(id);
    let base = table.weapons.iter().find(|d| d.id == base_id)?;
    let atts = attachments::resolve(base, &shorts, |a| table.attachments.iter().find(|x| x.id == a));
    let mut d = attachments::apply(base, &atts);
    let combo_shorts: Vec<&str> = d.attachments.iter().map(String::as_str).collect();
    if let Some(w) = bo2.iter().find(|w| w.name == base_id) {
        (d.overlay_material, d.overlay_material_low_res, d.overlay_reticle) = overlay_for(w, &combo_shorts);
    }
    if let Some(t) = bo2.iter().find(|w| w.name == base_id).and_then(|w| alt_drop_for(w, &combo_shorts)) {
        d.alt_drop_time = t;
    }
    if let Some(w) = bo2.iter().find(|w| w.name == base_id) {
        apply_unique_fields(&mut d, w);
    }
    let mut alt_out = None;
    if let Some((short, alt_id)) = base.alt_modes.iter().find(|(short, _)| d.attachments.contains(short)) {
        if let Some(alt_base) = table.weapons.iter().find(|w| &w.id == alt_id) {
            // Select fire and the second optic keep the weapon's other attachments by name, but
            // BO2 resolves them against the alt's own (empty) table: none changes its numbers
            // (`attachments::alt_mode`, G14 `FUN_008ec9e0`).
            let others: Vec<&str> = atts.iter().filter(|a| &a.short != short).map(|a| a.short.as_str()).collect();
            let mut alt = if alt_base.alt_own_ammo {
                alt_base.clone()
            } else {
                attachments::alt_mode(alt_base, &others, |a| table.attachments.iter().find(|x| x.id == a))
            };
            let alt_shorts: Vec<&str> = alt.attachments.iter().map(String::as_str).collect();
            if let Some(w) = bo2.iter().find(|w| &w.name == alt_id) {
                (alt.overlay_material, alt.overlay_material_low_res, alt.overlay_reticle) = overlay_for(w, &alt_shorts);
                apply_unique_fields(&mut alt, w);
            }
            alt.id = format!("{alt_id}~{}", d.id);
            alt.alt = d.id.clone();
            alt.alt_own_ammo = alt_base.alt_own_ammo;
            alt.display_name = alt_base.display_name.clone();
            d.alt = alt.id.clone();
            alt_out = Some(alt);
        }
    }
    Some((d, alt_out))
}

/// The combinations [`add_variants`] built before the converter filled the base defs' sounds and
/// FX (`sounds_and_fx`, the effects pass): re-derive each one from its now-converted base by the
/// same rule and take its converted assets ([`WeaponDef::take_converted_assets`]), so a combination
/// plays its own weapon's shot, flash and brass — and the silencer swap — exactly as a combination
/// the runtime builds (`Catalog::get`) would. Everything else on the entry (its viewmodel, clips,
/// overlay) is left as the art passes set it.
pub fn inherit_converted_assets(table: &mut Table, bo2: &[Weapon]) {
    let ids: Vec<String> = table.weapons.iter().filter(|d| d.id.contains('+') && !d.id.contains('~')).map(|d| d.id.clone()).collect();
    for id in ids {
        let Some((fresh, fresh_alt)) = build_variant(table, &id, bo2) else { continue };
        for fresh in std::iter::once(fresh).chain(fresh_alt) {
            if let Some(d) = table.weapons.iter_mut().find(|d| d.id == fresh.id) {
                d.take_converted_assets(&fresh);
            }
        }
    }
}

/// Mark the Create-a-Class items: each `mp/statstable.csv` weapon item with a non-negative
/// allocation cost (`zhbo_conformance::bo2::combos::items`, the same parse the enumerator and the
/// conformance expected side use) gives its variant's def the item's slot (column 13) and group
/// (column 2, the editor's category: [`WeaponDef::cac_class`]). A weapon no item names stays off
/// the class editor's lists (the `_dw` pistols, whose rows cost -1, are what a pistol with the
/// `dw` attachment gives; the War Machine is a pickup only).
pub fn set_cac_slots(table: &mut Table, patch: &zhbo_formats::t6zone::Zone, names: &HashMap<String, String>) {
    let Ok(Some(statstable)) = zhbo_formats::stringtable::rows(patch, "mp/statstable.csv") else { return };
    let items = zhbo_conformance::bo2::combos::items(&statstable);
    for d in &mut table.weapons {
        if let Some(item) = items.iter().find(|i| i.variant == d.id) {
            d.cac_slot = item.slot.clone();
            d.cac_group = item.group.clone();
            // Column 7's localized description, exactly as BO2's string table has it.
            let row = statstable.iter().find(|r| r.get(4).is_some_and(|c| *c == item.reference));
            d.description = row.and_then(|r| r.get(7)).and_then(|k| names.get(k.trim())).cloned().unwrap_or_default();
            // Column 3's localized name: the menus' (`GetItemName`), not the HUD's `displayName`.
            d.cac_name = row.and_then(|r| r.get(3)).and_then(|k| names.get(k.trim())).map(|s| s.trim().to_string()).unwrap_or_default();
            d.cac_sort = row.and_then(|r| r.get(9)).and_then(|c| c.trim().parse().ok()).unwrap_or(0);
            d.cac_index = item.index;
            // Column 15 names the DLC that carries the item (`dlc1`, the Peacekeeper's); its
            // unlock hint is `MENU_<DLC>_REQUIRED_HINT` (`GetUnlockStringForItemIndex`,
            // `codbase.lua:3741-3770`). Shown only if the item is locked ([`lock_items_without_art`]).
            let dlc = row.and_then(|r| r.get(15)).map(|c| c.trim().to_ascii_uppercase()).filter(|c| !c.is_empty());
            d.cac_lock_hint = dlc.and_then(|dlc| names.get(&format!("MENU_{dlc}_REQUIRED_HINT"))).map(|s| s.trim().to_string()).unwrap_or_default();
            order_attachments(d, item);
        }
    }
}

/// Lock the Create-a-Class items the player cannot have: BO2 asks `Engine.HasDLCForItem`
/// (`codbase.lua:3744`) of an item whose statstable column 15 names a DLC (`dlc1`, the
/// Peacekeeper's Revolution) — `dlc_installed` answers it from the install (the DLC's
/// `zone/all/<dlc>_load_mp.ff`) — and an item whose view model's art does not convert (`has_art`)
/// is locked too. Without Revolution BO2 lists the Peacekeeper locked (`c-primary/007`): data, not a
/// list of DLC weapons. A locked item stays listed, cannot be picked, and a stored class drops it
/// (`zhbo_core::classrepair`).
pub fn lock_items_without_art(table: &mut Table, bo2: &[Weapon], statstable: &[Vec<String>], has_art: impl Fn(&Weapon) -> bool, dlc_installed: impl Fn(&str) -> bool) {
    let dlc_of = |id: &str| {
        let reference = id.strip_suffix("_mp")?;
        statstable.iter().find(|r| r.get(4).is_some_and(|c| c == reference)).and_then(|r| r.get(15)).map(|c| c.trim().to_string()).filter(|c| !c.is_empty())
    };
    for d in table.weapons.iter_mut().filter(|d| !d.cac_slot.is_empty()) {
        if let Some(w) = bo2.iter().find(|w| w.name == d.id) {
            d.cac_locked = !has_art(w) || dlc_of(&d.id).is_some_and(|dlc| !dlc_installed(&dlc));
        }
    }
}

/// Whether `model`'s art converts: at least half of its materials' colour maps have pixels
/// (`resolve`). A gun whose own textures are stubs the install cannot fill fails even when a
/// shared part (a sight's glow) resolves.
pub fn model_art_converts(model: &zhbo_formats::xmodel::Model, resolve: impl Fn(&zhbo_formats::xmodel::Image) -> bool) -> bool {
    let colour: Vec<&zhbo_formats::xmodel::Image> = model.materials.iter().flatten().filter_map(crate::textures::colour_image).collect();
    let found = colour.iter().filter(|i| resolve(i)).count();
    colour.is_empty() || found * 2 >= colour.len()
}

/// Put a base weapon's offered attachments in BO2's attachment-grid order: the item's attachment
/// numbers, which follow its statstable column 8 (G17's `FUN_008cdfb0`;
/// `cacattachmentsmenu.lua:858-879` adds one button per number, `1 .. GetNumAttachments - 1`).
/// An offered attachment the column does not list keeps its place after the listed ones.
fn order_attachments(d: &mut WeaponDef, item: &zhbo_conformance::bo2::combos::Item) {
    let listed: Vec<&str> = zhbo_conformance::bo2::combos::listed(item).into_iter().map(|k| zhbo_conformance::weapons::ATTACHMENTS[k]).collect();
    d.allowed.sort_by_key(|(short, _)| listed.iter().position(|l| l == short).unwrap_or(listed.len()));
}

/// Where the weapon table goes in the archive (the bridge hands it to the core).
pub use zhbo_core::names::WEAPON_TABLE_PATH as TABLE_PATH;

/// The shellshocks the mod plays ([`zhbo_core::shellshock::names::ALL`]), from their
/// `shock/<name>.shock` files in the weapon pool (highest priority first), as the table's
/// `Shock` blocks. A missing file is an error: the core's shocks would silently do nothing. A
/// `soundLoop` alias none of the pool's sound banks holds is dropped: BO2 finds no alias and plays
/// nothing (MP has `chr_flashbang_tinnitus_loop`, not the concussion's `chr_tinitus_loop`).
pub fn shocks(pool: &[&zhbo_formats::t6zone::Zone]) -> Result<Vec<zhbo_core::shellshock::ShockDef>, String> {
    let banks: Vec<zhbo_formats::sndalias::Bank> = pool.iter().filter_map(|z| zhbo_formats::sndalias::banks(z).ok()).flatten().collect();
    zhbo_core::shellshock::names::ALL
        .iter()
        .map(|name| {
            let s = zhbo_formats::shock::find(pool, name).ok_or(format!("{}: not in the weapon pool", zhbo_formats::shock::path(name)))?;
            let mut d = shock_def(&s);
            if !banks.iter().any(|b| b.alias(&d.sound_loop).is_some()) {
                d.sound_loop.clear();
            }
            Ok(d)
        })
        .collect()
}

/// A parsed shock file as the table's `Shock` block. `bg_shock_sound` 0 plays no loop.
pub fn shock_def(s: &zhbo_formats::shock::Shock) -> zhbo_core::shellshock::ShockDef {
    zhbo_core::shellshock::ShockDef {
        name: s.name.clone(),
        flashed: s.screen_type == zhbo_formats::shock::ScreenType::Flashed,
        blur: s.blur_blend_time,
        blur_fade: s.blur_fade_time,
        white_fade: s.flash_white_fade_time,
        shot_fade: s.flash_shot_fade_time,
        look_control: s.look_control,
        max_pitch_speed: s.look_max_pitch_speed,
        max_yaw_speed: s.look_max_yaw_speed,
        sensitivity: s.look_sensitivity_scale,
        look_fade: s.look_fade_time,
        movement: s.movement,
        kick_period: s.view_kick_period,
        kick_radius: s.view_kick_radius,
        sound_fade_in: s.sound_fade_in_time,
        sound_fade_out: s.sound_fade_out_time,
        sound_mod_end: s.sound_mod_end_delay,
        sound_loop: if s.sound { s.sound_loop.clone() } else { String::new() },
    }
}

/// BO2's localized strings for the display names: `zone/<language>/<lang>_code_post_gfx_mp.ff`
/// with `<lang>_patch_mp.ff` over it. Empty if the language zones can't be read.
pub fn names(bo2_dir: &std::path::Path) -> HashMap<String, String> {
    zhbo_formats::localize::install_strings(bo2_dir, &["code_post_gfx_mp", "patch_mp"])
}

/// Sound events and Zero Hour FX weapons for the converted weapons. Each gun gets its own player
/// gunshot (BO2's `_plr` alias with its tail and low end) and third-person gunshot, played by its
/// own FX weapon (`ZHBO_FX_<id>`); the loadout's offhands supply the throw, pin and blast sounds and
/// the class melee weapon the global swing. Each melee weapon gets its own swing. Sets each
/// definition's `fire_sound`, `melee_swipe_sound` and `fx_weapon`; returns the events and the FX weapon
/// INI.
pub fn sounds_and_fx(bo2: &[Weapon], atts: &[bo2::Attachment], defs: &mut [WeaponDef], converted: &[String]) -> (Vec<crate::sfx::DynSfx>, String) {
    use crate::sfx::{DynSfx, Kind};
    let mut events = Vec::new();
    let mut ini = String::from("; Generated by zhbo-assets: each BO2 gun's third-person shot (zhbo-core plays these).\n\n");
    // A shot is its alias: the low end and tail BO2 starts with it are the alias's own
    // `secondaryName` chain, which the sound conversion follows (`sfx::secondary_chain`).
    let layers = |alias: &str| vec![alias.to_string()];
    let l = Loadout::default();
    // Every def derived from a parsed weapon carries its own empty-fire alias, not only the
    // converted ones: dual-optic and alt table variants share their base weapon's field. Set it
    // for all defs here so the shipped table and the conformance read-back both see it; the
    // synthesized variants inherit it when `attachments::apply` clones their base def.
    // MP melee is the class's melee weapon whatever is held (`_class.gsc:586`: every class gets
    // `knife_mp`): a gun's swing and hit are `knife_mp`'s; a melee weapon or riot shield swings
    // itself (its own fields, from `to_def`).
    let class_melee = bo2.iter().find(|w| w.name == CLASS_MELEE);
    for d in defs.iter_mut() {
        let base = crate::generals::base_id(&d.id);
        if let Some(w) = bo2.iter().find(|w| w.name == base) {
            d.empty_fire_sound = dry_fire_event(w.empty_fire_sound_player.as_deref()).to_string();
            if let Some(m) = class_melee.filter(|_| !matches!(w.weap_type, bo2::kind::MELEE | bo2::kind::RIOTSHIELD)) {
                d.melee_swipe_sound = def_sound(&m.melee_swipe_sound_player);
                d.melee_hit_sound = def_sound(&m.melee_hit_sound);
                // Others hear the knife's own third-person swing and hit.
                set_npc_sound(d, NpcSound::MeleeSwipe, &m.melee_swipe_sound);
                set_npc_sound(d, NpcSound::MeleeHit, &m.melee_hit_sound);
            }
        }
    }
    // The converted weapons and their alt modes: the one a combination toggles to, and every alt
    // weapon a base's attachments add (`gl_an94_mp`), so the combinations derived from them later
    // (`inherit_converted_assets`) find their sounds.
    let ids = with_alt_modes(defs, converted);
    for id in &ids {
        let base = crate::generals::base_id(id);
        let Some(w) = bo2.iter().find(|w| w.name == base) else { continue };
        let Some(d) = defs.iter_mut().find(|d| &d.id == id) else { continue };
        // Attachments can change the shot (a suppressor).
        let (_, shorts) = attachments::split_id(id.split('~').next().unwrap_or(id));
        let c = crate::generals::combo(w, atts, &shorts);
        // Zero Hour names: no `+` or `~`.
        let key = id.replace(['+', '~'], "_");
        if d.class.is_gun() {
            if let Some(plr) = &c.fire_sound_player {
                let event = format!("ZHBO_BO2_Fire_{key}");
                events.push(DynSfx { event: event.clone(), layers: layers(plr), kind: Kind::Player, limit: 3 });
                d.fire_sound = event;
            }
            // A dual-wield pair's left gun shoots with its own def's player sound (`_plr_lh`).
            if let Some(plr) = bo2.iter().find(|l| Some(l.name.as_str()) == w.dual_wield_weapon.as_deref()).and_then(|l| l.fire_sound_player.as_ref()) {
                let event = format!("ZHBO_BO2_FireLeft_{key}");
                events.push(DynSfx { event: event.clone(), layers: layers(plr), kind: Kind::Player, limit: 3 });
                d.left_fire_sound = event;
            }
            let npc_event = zhbo_core::names::npc_fire_event(&key);
            if let Some(npc) = &c.fire_sound {
                events.push(DynSfx { event: npc_event.clone(), layers: layers(npc), kind: Kind::Remote, limit: 3 });
            }
            let fx = zhbo_core::names::fx_weapon(&key);
            let launcher = d.class == Class::Launcher;
            ini.push_str(&format!(
                "Weapon {fx}\n  PrimaryDamage           = 0.0\n  PrimaryDamageRadius     = 0.0\n  AttackRange             = 1000.0\n  \
                 DamageType              = SMALL_ARMS\n  DeathType               = NORMAL\n  WeaponSpeed             = 999999.0\n  \
                 ProjectileObject        = NONE\n  FireFX                  = {}\n  FireSound               = NONE\n  \
                 RadiusDamageAffects     = ENEMIES\n  DelayBetweenShots       = 0\n  ClipSize                = 0\nEnd\n\n",
                if launcher { "WeaponFX_RocketBuggyMissileFire" } else if d.silenced { "NONE" } else { "WeaponFX_GenericMachineGunFire" }
            ));
            d.fx_weapon = fx;
        }
        // The suppressor's own shot and flash: BO2's `AttachmentUnique` for the weapon's silenced
        // attachment variant. The base def carries them; `attachments::apply` swaps a combo's
        // sound/fx to them when the combo mounts the silencer. Each sound only where BO2's gate lets
        // the silencer's unique override it (`FUN_008ebe80`).
        if let Some(sil_kind) = w.attachment_variants.iter().map(|(kind, _)| *kind).find(|kind| atts.iter().any(|a| a.kind == *kind && a.silenced)) {
            let sil = |bit, field: fn(&bo2::AttachmentUnique) -> &Option<String>| gated_sound(w, atts, &[sil_kind], bit, field);
            // The fire string's opening (`null_sound` on the burst guns). Overridden to nothing is
            // BO2's empty alias: Zero Hour's `NONE`, so the swap still happens (a blank field
            // means "no override").
            if let Some(start) = sil(FIRE_START_SOUND_PLAYER_BIT, |u| &u.fire_start_sound_player) {
                d.silenced_fire_start_sound = start.as_ref().map_or_else(|| zhbo_core::sounds::NO_SOUND.to_string(), |a| zhbo_core::names::def_sound_event(a));
            }
            let key_sil = format!("{key}_silenced");
            if let Some(Some(plr)) = sil(FIRE_SOUND_PLAYER_BIT, |u| &u.fire_sound_player) {
                let event = format!("ZHBO_BO2_Fire_{key_sil}");
                events.push(DynSfx { event: event.clone(), layers: layers(&plr), kind: Kind::Player, limit: 3 });
                d.silenced_fire_sound = event;
            }
            if let Some(Some(npc)) = sil(FIRE_SOUND_BIT, |u| &u.fire_sound) {
                let event = zhbo_core::names::npc_fire_event(&key_sil);
                events.push(DynSfx { event: event.clone(), layers: layers(&npc), kind: Kind::Remote, limit: 3 });
                d.silenced_fire_sound_npc = event;
            }
            let fx_sil = zhbo_core::names::fx_weapon(&key_sil);
            ini.push_str(&format!(
                "Weapon {fx_sil}\n  PrimaryDamage           = 0.0\n  PrimaryDamageRadius     = 0.0\n  AttackRange             = 1000.0\n  \
                 DamageType              = SMALL_ARMS\n  DeathType               = NORMAL\n  WeaponSpeed             = 999999.0\n  \
                 ProjectileObject        = NONE\n  FireFX                  = NONE\n  FireSound               = NONE\n  \
                 RadiusDamageAffects     = ENEMIES\n  DelayBetweenShots       = 0\n  ClipSize                = 0\nEnd\n\n",
            ));
            d.silenced_fx_weapon = fx_sil;
        }
        // Every other attachment whose unique the gate lets override the player's shot or the fire
        // string's opening (the SIG556/XM8 select fire: the full shot each round, no burst
        // opening), in `eAttachment` order; `attachments::apply` takes the first one mounted.
        if d.class.is_gun() {
            let mut others: Vec<(u32, String)> = w
                .attachment_variants
                .iter()
                .filter(|(kind, _)| !atts.iter().any(|a| a.kind == *kind && a.silenced))
                .map(|(kind, id)| (*kind, short_name(id)))
                .collect();
            others.sort();
            others.dedup();
            d.attachment_fire_sounds.clear();
            d.attachment_fire_start_sounds.clear();
            for (kind, short) in others {
                if let Some(Some(plr)) = gated_sound(w, atts, &[kind], FIRE_SOUND_PLAYER_BIT, |u| &u.fire_sound_player) {
                    if Some(&plr) != w.fire_sound_player.as_ref() {
                        let event = format!("ZHBO_BO2_Fire_{key}_{short}");
                        events.push(DynSfx { event: event.clone(), layers: layers(&plr), kind: Kind::Player, limit: 3 });
                        d.attachment_fire_sounds.push((short.clone(), event));
                    }
                }
                if let Some(start) = gated_sound(w, atts, &[kind], FIRE_START_SOUND_PLAYER_BIT, |u| &u.fire_start_sound_player) {
                    if start != w.fire_start_sound_player {
                        let event = start.as_ref().map_or_else(|| zhbo_core::sounds::NO_SOUND.to_string(), |a| zhbo_core::names::def_sound_event(a));
                        d.attachment_fire_start_sounds.push((short, event));
                    }
                }
            }
        }
        let general = |event: &str, alias: &Option<String>, kind: Kind| alias.as_ref().map(|a| DynSfx { event: event.into(), layers: vec![a.clone()], kind, limit: 2 });
        if *id == l.lethal {
            events.extend(general(zhbo_core::sounds::THROW, &w.fire_sound_player, Kind::Player));
            events.extend(general(zhbo_core::sounds::EXPLOSION, &w.projectile_explosion_sound, Kind::World));
        }
        // The Assault Shield's own cues (`sounds::SHIELD_SOUNDS`): heard where they happen.
        if d.class == Class::Shield {
            for &(event, alias) in zhbo_core::sounds::SHIELD_SOUNDS {
                events.push(DynSfx { event: event.into(), layers: vec![alias.into()], kind: Kind::World, limit: 2 });
            }
        }
        if id == zhbo_core::weapons::CLASS_MELEE {
            events.extend(general(zhbo_core::sounds::MELEE_SWING, &w.melee_swipe_sound_player, Kind::Player));
        }
    }
    // The def-level sounds (raise, putaway, ADS zoom, melee, fire start/last, pull-back, blast):
    // one event per BO2 alias any def names, shared by every def that names it.
    let mut def_events = BTreeSet::new();
    for d in defs.iter() {
        for (event, heard) in d.def_sound_events() {
            if let Some(alias) = zhbo_core::names::def_sound_alias(event) {
                if def_events.insert(event.to_string()) {
                    let kind = match heard {
                        zhbo_core::weapons::Heard::Player => Kind::Player,
                        zhbo_core::weapons::Heard::World => Kind::World,
                        zhbo_core::weapons::Heard::Remote => Kind::Remote,
                    };
                    events.push(DynSfx { event: event.to_string(), layers: vec![alias.to_string()], kind, limit: 2 });
                }
            }
        }
    }
    (events, ini)
}

/// The melee weapon every MP class carries (`_class.gsc:586`), whose swing a held gun plays.
const CLASS_MELEE: &str = "knife_mp";

/// `soundOverrides` bits of the overridable sounds the mod plays (G14 §4: bit `i` <-> unique field
/// `0x100 + 4i`): the third-person shot, the player's shot, the fire string's opening.
pub(crate) const FIRE_SOUND_BIT: u32 = 1 << 0;
pub(crate) const FIRE_SOUND_PLAYER_BIT: u32 = 1 << 1;
pub(crate) const FIRE_START_SOUND_PLAYER_BIT: u32 = 1 << 8;

/// BO2's override gate for one sound with the attachment `kinds` mounted (`FUN_008ebe80`): only when
/// the base unique sets `bit`, the silencer's unique if mounted and it sets the bit too, else the
/// first other single unique in `eAttachment` order that sets it — that unique's field (`Some(None)`:
/// overridden to nothing); `None` when the def's own sound stands. The silencer is the attachment
/// whose `silenced` flag is set (BO2's kind 23).
pub(crate) fn gated_sound(
    w: &Weapon,
    atts: &[bo2::Attachment],
    kinds: &[u32],
    bit: u32,
    field: fn(&bo2::AttachmentUnique) -> &Option<String>,
) -> Option<Option<String>> {
    let single = |k: u32| w.attachment_uniques.iter().find(|u| u.kind == k && u.combined == 0);
    if single(0)?.sound_overrides & bit == 0 {
        return None;
    }
    let mut kinds: Vec<u32> = kinds.iter().copied().filter(|k| *k != 0).collect();
    kinds.sort_unstable();
    let silencer = kinds.iter().copied().find(|k| atts.iter().any(|a| a.kind == *k && a.silenced));
    silencer.into_iter().chain(kinds).filter_map(single).find(|u| u.sound_overrides & bit != 0).map(|u| field(u).clone())
}

/// `ids` and the alt modes they reach: each one's current alt (`d.alt`), every alt weapon its
/// attachments add (`alt_modes`) and the grenade its projectile becomes (`spawns`), each once, in
/// first-seen order.
pub fn with_alt_modes(defs: &[WeaponDef], ids: &[String]) -> Vec<String> {
    let mut out: Vec<String> = ids.to_vec();
    for id in ids {
        let Some(d) = defs.iter().find(|d| &d.id == id) else { continue };
        let alts = std::iter::once(&d.alt).chain(d.alt_modes.iter().map(|(_, alt)| alt)).chain(std::iter::once(&d.spawns));
        for alt in alts.filter(|a| !a.is_empty()) {
            if !out.contains(alt) {
                out.push(alt.clone());
            }
        }
    }
    out
}

/// Who BO2 hands a weapon to: its Create-a-Class roster. A variant is a player weapon when
///
/// * `mp/statstable.csv` lists it as an item (group `weapon_*`, allocation cost not negative:
///   `zhbo_conformance::bo2::combos::items`, `_bot_loadout.gsc:506-509`);
/// * it is the dual-wield form `R_dw_mp` of an item `R` whose attachments (column 8) include `dw`
///   (G16: the `dw` attachment switches to the underscore form; those rows cost -1);
/// * or `_class.gsc` gives it to every class itself: `knife_mp` (`:586`) and `knife_held_mp` (`:682`).
///
/// No weapon id is special-cased: the War Machine and the minigun (cost -1) and `defaultweapon_mp`
/// (no row) fall out of the data.
#[derive(Clone, Debug, Default)]
pub struct Roster {
    /// The items by variant (`an94_mp`), or `None` when no statstable was read (synthetic
    /// fixtures): then every weapon with a view model is taken.
    items: Option<HashMap<String, zhbo_conformance::bo2::combos::Item>>,
}

impl Roster {
    /// The roster of the player's `patch_mp` statstable (empty when it has none).
    pub fn read(patch: &zhbo_formats::t6zone::Zone) -> Roster {
        let Ok(Some(statstable)) = zhbo_formats::stringtable::rows(patch, "mp/statstable.csv") else { return Roster { items: Some(HashMap::new()) } };
        Roster { items: Some(zhbo_conformance::bo2::combos::items(&statstable).into_iter().map(|i| (i.variant.clone(), i)).collect()) }
    }

    /// An install with an empty statstable: only the knives `_class.gsc` gives.
    pub fn read_none() -> Roster {
        Roster { items: Some(HashMap::new()) }
    }

    /// For synthetic fixtures without a statstable: every weapon with a view model.
    pub fn any() -> Roster {
        Roster { items: None }
    }

    /// The Create-a-Class item a variant is, if any.
    pub fn item(&self, variant: &str) -> Option<&zhbo_conformance::bo2::combos::Item> {
        self.items.as_ref()?.get(variant)
    }

    pub fn is_player_weapon(&self, w: &Weapon) -> bool {
        use zhbo_core::weapons::{CLASS_MELEE, KNIFE_HELD};
        if w.name == CLASS_MELEE || w.name == KNIFE_HELD {
            return true;
        }
        let Some(items) = &self.items else { return w.view_model.is_some() };
        if items.contains_key(&w.name) {
            return true;
        }
        w.name.strip_suffix("_dw_mp").is_some_and(|r| items.get(&format!("{r}_mp")).is_some_and(|i| i.attachments.iter().any(|a| a == "dw")))
    }
}

/// BO2's riot shield: weapType `riotshield` in the primary inventory (`riotshield_mp`).
pub fn is_shield(w: &Weapon) -> bool {
    w.weap_type == bo2::kind::RIOTSHIELD && w.inventory == bo2::inventory::PRIMARY && w.view_model.is_some()
}

/// Whether the converted weapon `id` is a riot shield in BO2's data.
pub fn is_shield_id(id: &str, all: &[Weapon]) -> bool {
    all.iter().any(|w| w.name == id && is_shield(w))
}

/// Per attachment (`eAttachment` order), the location multipliers its single unique sets
/// differently from the base unique's — what `FUN_008ed780` takes over the def's. Empty when the
/// weapon has no base unique (the def's stand alone).
fn attachment_location_multipliers(w: &Weapon) -> Vec<(String, Vec<(u8, f32)>)> {
    let single = |k: u32| w.attachment_uniques.iter().find(|u| u.kind == k && u.combined == 0);
    let Some(u0) = single(0) else { return Vec::new() };
    let mut kinds: Vec<(u32, String)> = w.attachment_variants.iter().map(|(k, id)| (*k, short_name(id))).collect();
    kinds.sort();
    kinds.dedup();
    kinds
        .into_iter()
        .filter_map(|(k, short)| {
            let u = single(k)?;
            let differs: Vec<(u8, f32)> = u
                .location_multipliers
                .iter()
                .enumerate()
                .filter(|(p, m)| u0.location_multipliers.get(*p) != Some(*m))
                .map(|(p, m)| (p as u8, *m))
                .collect();
            (!differs.is_empty()).then_some((short, differs))
        })
        .collect()
}

fn class_of(w: &Weapon) -> Class {
    // An offhand's class is its BO2 `offhandSlot`: the slot picks the button (`FUN_008de050`) and
    // the specialty that speeds its states (G3: slot 3 runs at `perk_weapEquipmentUseMultiplier`).
    // The Create-a-Class slot it is listed under is the statstable's (`cac_slot`), a separate
    // thing: the Sensor Grenade is a `specialgrenade` item on the equipment slot.
    match w.offhand_slot {
        bo2::offhand_slot::EQUIPMENT => return Class::Equipment,
        bo2::offhand_slot::TACTICAL => return Class::Tactical,
        bo2::offhand_slot::LETHAL => return Class::Lethal,
        _ => {}
    }
    if w.weap_type == bo2::kind::MELEE {
        return Class::Melee;
    }
    if is_shield(w) {
        return Class::Shield;
    }
    match w.class {
        bo2::class::MG => Class::Lmg,
        bo2::class::SMG => Class::Smg,
        bo2::class::SPREAD => Class::Shotgun,
        bo2::class::PISTOL | bo2::class::PISTOL_SPREAD => Class::Pistol,
        bo2::class::ROCKETLAUNCHER | bo2::class::GRENADE => Class::Launcher,
        bo2::class::RIFLE if w.weap_type == bo2::kind::PROJECTILE => Class::Special,
        bo2::class::RIFLE => Class::Rifle,
        _ => Class::Special,
    }
}

fn effect_of(w: &Weapon) -> Effect {
    if let Some(e) = manifest::named_effect(&w.name) {
        return e;
    }
    match w.weap_type {
        bo2::kind::PROJECTILE | bo2::kind::GRENADE | bo2::kind::MINE => Effect::Explode,
        _ => Effect::None,
    }
}

fn fire_mode(w: &Weapon) -> FireMode {
    match w.fire_type {
        bo2::fire::FULLAUTO => FireMode::Auto,
        bo2::fire::STACKED => FireMode::Stacked,
        bo2::fire::MINIGUN => FireMode::Minigun,
        bo2::fire::BURST2 | bo2::fire::BURST3 | bo2::fire::BURST4 | bo2::fire::BURST5 => FireMode::Burst(w.burst_count() as u8),
        _ => FireMode::Semi,
    }
}

fn kick(k: bo2::Kick) -> Kick {
    Kick { pitch: k.pitch, yaw: k.yaw }
}

fn gun_kick(k: bo2::GunKick) -> GunKick {
    GunKick {
        pitch: k.pitch,
        yaw: k.yaw,
        accel: k.accel,
        speed_max: k.speed_max,
        speed_decay: k.speed_decay,
        static_decay: k.static_decay,
        reduced_bullets: k.reduced_bullets as f32,
        reduced_percent: k.reduced_percent,
    }
}

fn sway(s: bo2::Sway) -> Sway {
    Sway { max_angle: s.max_angle, lerp_speed: s.lerp_speed, pitch_scale: s.pitch_scale, yaw_scale: s.yaw_scale, horiz_scale: s.horiz_scale, vert_scale: s.vert_scale }
}

fn view_pos(p: bo2::ViewPos) -> ViewPos {
    ViewPos {
        stand_move: p.stand_move,
        stand_rot: p.stand_rot,
        ducked_ofs: p.ducked_ofs,
        ducked_move: p.ducked_move,
        ducked_rot: p.ducked_rot,
        prone_ofs: p.prone_ofs,
        prone_move: p.prone_move,
        prone_rot: p.prone_rot,
        strafe_move: p.strafe_move,
        strafe_rot: p.strafe_rot,
        sprint_ofs: p.sprint_ofs,
        sprint_rot: p.sprint_rot,
        sprint_bob: p.sprint_bob,
        move_rate: p.move_rate,
        prone_move_rate: p.prone_move_rate,
        rot_rate: p.rot_rate,
        prone_rot_rate: p.prone_rot_rate,
        stand_min_speed: p.stand_min_speed,
        ducked_min_speed: p.ducked_min_speed,
        prone_min_speed: p.prone_min_speed,
        ads_aim_pitch: p.ads_aim_pitch,
        ads_bob_factor: p.ads_bob_factor,
        sprint_cycle_scale: p.sprint_cycle_scale,
    }
}

fn reticle(r: &bo2::Reticle) -> Reticle {
    Reticle {
        center: r.center.clone().unwrap_or_default(),
        side: r.side.clone().unwrap_or_default(),
        center_size: r.center_size as f32,
        side_size: r.side_size as f32,
        min_ofs: r.min_ofs as f32,
        side_pos: r.side_pos,
        ads_in_frac: r.ads_in_frac,
        ads_out_frac: r.ads_out_frac,
        keep_when_ads: r.keep_when_ads,
        color_change: r.color_change,
    }
}

/// The core definition for a BO2 weapon. `names` maps localize keys to text.
///
/// In debug builds the result is round-tripped through the table schema (format then parse), so
/// a schema line that `to_def` does not write — or one whose value does not survive the text
/// form — fails here instead of silently drifting (refactor plan 1.4 / 3.2).
/// A def-level BO2 sound as its converted event (`zhbo_core::names::def_sound_event`); empty when
/// BO2 names none. [`sounds_and_fx`] registers one event per alias any def uses.
fn def_sound(alias: &Option<String>) -> String {
    alias.as_deref().map(zhbo_core::names::def_sound_event).unwrap_or_default()
}

/// BO2 weapon `w`'s third-person def-level sounds (`WeaponDef::npc_sounds`): each moment's field
/// without `Player` (`raiseSound`, `meleeSwipeSound`, ...; `meleeHitSound` has one alias for
/// everyone), as positional events.
fn npc_sounds(w: &Weapon) -> Vec<(String, String)> {
    use zhbo_core::weapons::NpcSound;
    NpcSound::ALL
        .into_iter()
        .filter_map(|m| {
            let alias = match m {
                NpcSound::Raise => &w.raise_sound,
                NpcSound::FirstRaise => &w.first_raise_sound,
                NpcSound::Putaway => &w.putaway_sound,
                NpcSound::MeleeSwipe => &w.melee_swipe_sound,
                NpcSound::MeleeHit => &w.melee_hit_sound,
                NpcSound::Pullback => &w.pullback_sound,
                NpcSound::EmptyFire => &w.empty_fire_sound,
            };
            alias.as_deref().map(|a| (m.key().to_string(), zhbo_core::names::npc_def_sound_event(a)))
        })
        .collect()
}

/// Set (or clear, for no alias) one moment of `d.npc_sounds`.
fn set_npc_sound(d: &mut WeaponDef, m: zhbo_core::weapons::NpcSound, alias: &Option<String>) {
    d.npc_sounds.retain(|(k, _)| k != m.key());
    if let Some(a) = alias {
        d.npc_sounds.push((m.key().to_string(), zhbo_core::names::npc_def_sound_event(a)));
    }
}

pub fn to_def(w: &Weapon, names: &HashMap<String, String>) -> WeaponDef {
    let class = class_of(w);
    let clip = w.clip_size.max(0) as u16;
    let (start_reserve, max_reserve) = if class.is_offhand() {
        // Offhands: how many you spawn with, and carry at most.
        (w.start_ammo.max(1) as u16, w.max_ammo.max(w.start_ammo).max(1) as u16)
    } else if w.ammo_clip_relative || (w.start_ammo > 0 && w.start_ammo < w.clip_size) {
        // BO2 multiplayer counts gun ammo in magazines. `iStartAmmo` is what a spawn tops up to,
        // the loaded magazine included (`FUN_005e0910`); `iMaxAmmo` is the stock cap, the spare
        // rounds beside the magazine (`FUN_005e08d0`; G16): the AN-94's 4/8 is 30 + 90 at spawn
        // and 240 spare at most.
        (((w.start_ammo - 1).max(0) * w.clip_size) as u16, (w.max_ammo.max(0) * w.clip_size) as u16)
    } else {
        // The same in rounds: the crossbow's start 6 is 3 loaded + 3 spare, its max 3 spare.
        ((w.start_ammo - w.clip_size).max(0) as u16, w.max_ammo.max(0) as u16)
    };
    // The HUD's name as BO2 renders `displayName`: its localized text exactly (the Tac-45 Dual
    // Wield's ends in a space; the table quotes such a value, `table::quote`), else the reference
    // itself — what the engine shows for a key its string tables lack (the Concussion's
    // `WEAPON_CONCUSSION_GRENADE`, the Bouncing Betty's `BOUNCING_BETTY`); `knife_mp` names none.
    // The menus name an item by its statstable name instead (`cac_name`, `menu_name`).
    let display_name = names.get(&w.display_name).cloned().unwrap_or_else(|| w.display_name.clone());
    let effect = effect_of(w);
    // What sticks and then waits (a fuse, or landing as equipment): BO2's `stickiness` `all` or
    // `all_orient`. An impact weapon's strike is its damage (the ballistic knife lies where it
    // struck, spent), so it never waits.
    let sticky = matches!(w.stickiness, bo2::stickiness::ALL | bo2::stickiness::ALL_ORIENT) && effect != Effect::Impact;
    let mut damage = [0.0; 6];
    let mut damage_range = [0.0; 6];
    for i in 0..6 {
        damage[i] = w.damage[i] as f32;
        // BO2's own inches, its "no falloff" 999999 included (the curve's last range is also a
        // spread weapon's pellet range, `Bullet_Fire`).
        damage_range[i] = w.damage_range[i].max(0.0);
    }
    let d = WeaponDef {
        id: w.name.clone(),
        equipment_kind: manifest::equipment_kind_of(&w.name),
        display_name,
        class,
        weap_class: WeapClass::from_bo2(w.class),
        // The third-person animation set needs the player animation script: `thirdperson::assign_sets`.
        body_anims: 0,
        torso_anims: 0,
        fire: fire_mode(w),
        // BO2's own times, unclamped: the knife's `iFireTime` and the offhands' ADS times are 0 (they
        // never fire a cycle or aim; the sim's divisions guard their zero).
        fire_time: w.fire_time,
        burst_delay: w.burst_delay,
        spin_up_time: w.spin_up_time,
        pellets: w.shot_count.max(0) as u16,
        clip,
        start_reserve,
        max_reserve: max_reserve.max(start_reserve),
        reload_time: w.reload_time,
        // Raw: 0 (the offhands, the shield) reloads over the normal time (`arsenal::magazine_times`).
        reload_empty_time: w.reload_empty_time,
        reload_add_time: w.reload_add_time,
        // Raw: 0 means the rounds go in at the reload add time (the sim's fallback, `FUN_008dec90`).
        reload_empty_add_time: w.reload_empty_add_time,
        reload_start_time: w.reload_start_time,
        reload_end_time: w.reload_end_time,
        reload_ammo_add: w.reload_ammo_add.max(0) as u16,
        raise_time: w.raise_time,
        first_raise_time: w.first_raise_time,
        drop_time: w.drop_time,
        empty_raise_time: w.empty_raise_time,
        empty_drop_time: w.empty_drop_time,
        quick_raise_time: if w.quick_raise_time > 0.0 { w.quick_raise_time } else { w.raise_time * 0.6 },
        quick_drop_time: if w.quick_drop_time > 0.0 { w.quick_drop_time } else { w.drop_time * 0.6 },
        alt_raise_time: w.alt_raise_time,
        alt_drop_time: w.alt_drop_time,
        sprint_out_time: w.sprint_out_time,
        ads_in_time: w.ads_in_time,
        ads_out_time: w.ads_out_time,
        ads_fov: w.ads_zoom_fov,
        damage,
        damage_range,
        location_multipliers: std::array::from_fn(|i| w.location_multipliers.get(i).copied().unwrap_or(1.0)),
        attachment_location_multipliers: attachment_location_multipliers(w),
        max_range: inches(w.max_range),
        hip_spread: [w.hip_spread_stand, w.hip_spread_ducked, w.hip_spread_prone],
        spread_decay: w.hip_spread_decay,
        spread_fire_add: w.hip_spread_fire_add,
        spread_move_add: w.hip_spread_move_add,
        spread_turn_add: w.hip_spread_turn_add,
        spread_stance_decay: w.hip_spread_stance_decay,
        ads_spread: w.ads_spread,
        ads_kick: kick(w.ads_view_kick),
        hip_kick: kick(w.hip_view_kick),
        move_scale: if w.move_speed_scale > 0.0 { w.move_speed_scale } else { 1.0 },
        // BO2's adsMoveSpeedScale as the def carries it (rifles 1, LMGs 0.8, SMGs, pistols and
        // shotguns 2): `FUN_008ed080` takes it when above 0, else 1, and clamps nothing. The walk's
        // own aimed scale (`AdsWalkScale`, 0.4) is the engine's, applied by the sim.
        ads_move_scale: if w.ads_move_speed_scale > 0.0 { w.ads_move_speed_scale } else { 1.0 },
        sprint_duration_scale: if w.sprint_duration_scale > 0.0 { w.sprint_duration_scale } else { 1.0 },
        melee_damage: if w.melee_damage > 0 { w.melee_damage as f32 } else { MELEE_DAMAGE },
        melee_time: if w.melee_time > 0.0 { w.melee_time } else { 0.6 },
        melee_delay: w.melee_delay.max(0.0),
        melee_charge_range: inches(w.melee_charge_range.max(0.0)),
        melee_charge_time: w.melee_charge_time.max(0.0),
        melee_charge_delay: w.melee_charge_delay.max(0.0),
        effect,
        fuse_time: w.fuse_time,
        cook: w.cook_off_hold,
        detonate_delay: w.detonate_delay.max(0.0),
        has_detonator: w.has_detonator,
        hold_use: w.is_hold_use_grenade,
        hold_cancelable: w.offhand_hold_is_cancelable,
        plantable: w.plantable,
        explode_on_ground: w.explode_on_ground,
        timed_detonation: w.timed_detonation,
        laser_sensor: w.laser_sensor,
        aims: w.aim_down_sight,
        inventory: zhbo_core::weapons::Inventory::from_bo2(w.inventory),
        explosion_radius: inches(w.explosion_radius as f32),
        explosion_radius_min: inches(w.explosion_radius_min as f32),
        explosion_inner_damage: w.explosion_inner_damage as f32,
        explosion_outer_damage: w.explosion_outer_damage as f32,
        projectile_speed: inches(w.projectile_speed as f32),
        projectile_speed_up: inches(w.projectile_speed_up as f32),
        projectile_speed_relative_up: inches(w.projectile_speed_relative_up as f32),
        projectile_speed_forward: inches(w.projectile_speed_forward as f32),
        throw_back: w.throw_back,
        projectile_gravity: w.class != bo2::class::ROCKETLAUNCHER,
        impact_explode: w.projectile_impact_explode,
        // BO2's impact bucket: which `FxImpactTable` entry this weapon's hits use (R8d).
        impact_type: w.impact_type as i32,
        sticky,
        // Only the `bolt` explosion type reads the name (t6mp `FUN_00608c10`).
        spawns: if w.proj_explosion == bo2::proj_explosion::BOLT { w.spawned_grenade_weapon.clone().unwrap_or_default() } else { String::new() },
        fx_weapon: String::new(),
        view_flash_fx: String::new(),
        view_shell_fx: String::new(),
        view_last_shot_shell_fx: String::new(),
        // The def's own `projExplosionEffect` only (set by the effects pass): BO2 plays the impact
        // table's cell for the `impactType` and surface first (`impacts.txt`), and the sim falls
        // back to a stand-in only when neither exists.
        explosion_fx: String::new(),
        // The smoke screen keeps Zero Hour's cloud until the effects pass names BO2's.
        screen_fx: if effect == Effect::Smoke { "FX_OverlordSmoke".to_string() } else { String::new() },
        projectile_object: String::new(),
        // A gun's shot is its own `ZHBO_BO2_Fire_<id>` mix (`sounds_and_fx`); an offhand's throw is
        // its def-level alias.
        fire_sound: if class.is_offhand() { def_sound(&w.fire_sound_player) } else { String::new() },
        empty_fire_sound: String::new(),
        left_fire_sound: String::new(),
        // Set by `sounds_and_fx` (a converted event).
        silenced_fire_sound: String::new(),
        // What others hear when it is thrown or set down (`fireSound`; `client` plays it at the
        // thrower for every other player).
        npc_fire_sound: if class.is_offhand() { def_sound(&w.fire_sound) } else { String::new() },
        // A gun's third-person opening and last round (`FUN_005320e0` reads both off the def, so
        // no attachment changes them): the client plays them at another player's shot.
        npc_fire_start_sound: if class.is_offhand() { String::new() } else { def_sound(&w.fire_start_sound) },
        npc_fire_last_sound: if class.is_offhand() { String::new() } else { def_sound(&w.fire_last_sound) },
        silenced_fire_sound_npc: String::new(),
        silenced_fx_weapon: String::new(),
        silenced_view_flash_fx: String::new(),
        explode_sound: def_sound(&w.projectile_explosion_sound),
        // BO2's `bounceSound[surface]` for each of the mod's slots (event 0x47); the converter
        // makes the cells the install has no alias for `NONE` (`generals`).
        bounce_sounds: zhbo_core::impacts::Surface::ALL
            .into_iter()
            .filter_map(|s| w.bounce_sounds.get(crate::impacts::bo2_surface(s)).cloned().flatten().map(|a| (s.name().to_string(), zhbo_core::names::def_sound_event(&a))))
            .collect(),
        bounce: zhbo_core::impacts::Surface::ALL
            .into_iter()
            .filter_map(|s| {
                let i = crate::impacts::bo2_surface(s);
                Some((s.name().to_string(), *w.parallel_bounce.get(i)?, *w.perpendicular_bounce.get(i)?))
            })
            .collect(),
        activate_dist: inches(w.projectile_activate_dist.max(0) as f32),
        dud_fx: String::new(),
        dud_sound: def_sound(&w.projectile_dud_sound),
        raise_sound: def_sound(&w.raise_sound_player),
        first_raise_sound: def_sound(&w.first_raise_sound_player),
        alt_switch_sound: def_sound(&w.alt_switch_sound_player),
        putaway_sound: def_sound(&w.putaway_sound_player),
        ads_zoom_sound: def_sound(&w.ads_zoom_sound),
        melee_swipe_sound: def_sound(&w.melee_swipe_sound_player),
        melee_hit_sound: def_sound(&w.melee_hit_sound),
        fire_start_sound: def_sound(&w.fire_start_sound_player),
        fire_last_sound: def_sound(&w.fire_last_sound_player),
        pullback_sound: def_sound(&w.pullback_sound_player),
        npc_sounds: npc_sounds(w),
        silenced_fire_start_sound: String::new(),
        attachment_fire_sounds: Vec::new(),
        attachment_fire_start_sounds: Vec::new(),
        viewmodel: -1,
        anims: w.anims.iter().enumerate().take(128).filter(|(_, a)| a.is_some()).fold(0u128, |m, (i, _)| m | (1u128 << i)),
        clip_base: 0,
        world_meshes: 0,
        world_weapon: -1,
        attachments: Vec::new(),
        allowed: w.attachment_variants.iter().map(|(_, id)| (short_name(id), id.clone())).collect(),
        // Filled from the attribute table by `apply_attributes`.
        attributes: [0.0; 4],
        attachment_attributes: Vec::new(),
        alt_modes: w
            .attachment_uniques
            .iter()
            .filter(|u| u.combined == 0)
            .filter_map(|u| {
                let alt = u.alt_weapon.clone()?;
                let (_, att) = w.attachment_variants.iter().find(|(k, _)| *k == u.kind)?;
                Some((short_name(att), alt))
            })
            .collect(),
        alt: if w.inventory == bo2::inventory::ALTMODE { w.alt_weapon.clone().unwrap_or_default() } else { String::new() },
        alt_own_ammo: false,
        // Set from the statstable by `set_cac_slots` (the table is not this weapon's data).
        cac_slot: String::new(),
        cac_group: String::new(),
        cac_name: String::new(),
        cac_locked: false,
        cac_lock_hint: String::new(),
        description: String::new(),
        cac_sort: 0,
        cac_index: 0,
        dual_wield_weapon: w.dual_wield_weapon.clone().unwrap_or_default(),
        silenced: w.silenced,
        laser: false,
        target_finder: false,
        mms_max_dist: w.mms_max_dist,
        // The weapon's own ADS recoil reduction (`fAdsRecoilReductionRate`/`Limit`: the HAMR and its
        // dual-optic alt carry 0.03/0.15); a grip-type attachment replaces it (`attachments::apply`).
        recoil_reduction: (w.recoil.0, w.recoil.1),
        place: manifest::place_of(&w.name),
        specialties: Specialties::from_words(w.perks.0, w.perks.1),
        view_kick_min: w.view_kick_min,
        view_kick_center: w.view_kick_center,
        ads_kick_center_scale: 1.0,
        kick_center_stance: w.view_kick_center_stance,
        ads_gun_kick: gun_kick(w.ads_gun_kick),
        hip_gun_kick: gun_kick(w.hip_gun_kick),
        gun_max: w.gun_max,
        recoil_return_rate: w.recoil.2,
        hip_sway: sway(w.hip_sway),
        ads_sway: sway(w.ads_sway),
        view_pos: view_pos(w.view_pos),
        reticle: reticle(&w.reticle),
        camo: Vec::new(),
        // BO2's own pointer (`WeaponDef.weaponCamo`, G16): the INSAS's is `camo_msmc`, the AS50's
        // `camo_xpr50` — never derived from the weapon's name.
        camo_material: w.weapon_camo.clone().unwrap_or_default(),
        overlay_material: w.overlay_material.clone(),
        overlay_material_low_res: w.overlay_material_low_res.clone(),
        overlay_reticle: w.overlay_reticle,
        // The def's own, shared by every combo (`FUN_004b4c70` reads it off the WeaponDef).
        overlay_size: w.overlay_size,
        timing: CycleTimes {
            last_fire_time: w.last_fire_time,
            fire_delay: w.fire_delay,
            rechamber_time: w.rechamber_time,
            rechamber_bolt_time: w.rechamber_bolt_time,
            hold_fire_time: w.hold_fire_time,
            shots_before_rechamber: w.shots_before_rechamber.max(0) as u16,
            bolt_action: w.bolt_action,
            reload_quick_time: w.reload_quick_time,
            reload_quick_empty_time: w.reload_quick_empty_time,
            reload_quick_add_time: w.reload_quick_add_time,
            reload_start_add: w.reload_start_add.max(0) as u16,
            spin_down_time: w.spin_down_time,
            detonate_time: w.detonate_time,
            sprint_in_time: w.sprint_in_time,
        },
        zoom: Zoom {
            fov2: w.ads_zoom_fov2,
            fov3: w.ads_zoom_fov3,
            in_frac: w.ads_zoom_in_frac,
            out_frac: w.ads_zoom_out_frac,
            alt_scope_in_time: w.alt_scope_ads_in_time,
            alt_scope_out_time: w.alt_scope_ads_out_time,
            dof: w.ads_dof,
            overlay_alpha_scale: w.overlay_alpha_scale,
            idle_amount: (w.ads_idle_amount, w.hip_idle_amount),
            idle_speed: (w.ads_idle_speed, w.hip_idle_speed),
            idle_stance: (w.idle_crouch_factor, w.idle_prone_factor),
            hold_breath: w.hold_breath_to_steady,
            anti_quickscope: zhbo_core::weapons::AntiQuickScope {
                enabled: w.anti_quickscope,
                time: w.anti_quickscope_time,
                scale: w.anti_quickscope_scale,
                spread_multiplier: w.anti_quickscope_spread_multiplier,
                spread_max: w.anti_quickscope_spread_max,
                sway_factor: w.anti_quickscope_sway_factor,
            },
        },
        traits: Traits {
            clip_only: w.clip_only,
            dual_mag: w.dual_mag,
            infrared: w.infrared,
            mms: w.mms_weapon,
            // `FUN_008da950`: the def's bayonet counts as using the weapon to melee.
            use_as_melee: w.use_as_melee || w.has_bayonet,
            rifle_bullet: w.rifle_bullet,
            alt_ads_only: w.alt_weapon_ads_only,
            alt_disable_switching: w.alt_weapon_disable_switching,
            ignore_attachments: w.ignore_attachments,
            stack_fire: (w.stack_fire_spread, w.stack_fire_accuracy_decay),
            min_player_damage: w.min_player_damage as f32,
            penetrate: Penetrate::from_bo2(w.penetrate_type),
            offhand: OffhandClass::from_bo2(w.offhand_class),
            weap_type: WeapType::from_bo2(w.weap_type),
            segmented_reload: w.segmented_reload,
            unlimited_ammo: w.unlimited_ammo,
        },
        guided_missile_type: w.guided_missile_type,
        low_ammo_threshold: w.low_ammo_warning_threshold,
        no_partial_reload: w.no_partial_reload,
        ammo_counter: w.ammo_counter_clip != 0,
        // The variant's own action-slot material (`WeaponVariantDef.dpadIcon`): an alt mode carries
        // it (`gl_an94_mp`: `hud_gl_select`), the weapon it toggles from does not.
        dpad_icon: w.dpad_icon.clone().unwrap_or_default(),
        hud_icon: w.hud_icon.clone().unwrap_or_default(),
        kill_icon: zhbo_core::weapons::KillIcon { material: w.kill_icon.clone().unwrap_or_default(), ratio: w.kill_icon_ratio as u8, flip: w.flip_kill_icon },
        indicator: zhbo_core::weapons::GrenadeIndicator {
            proj_explosion: w.proj_explosion as u8,
            show: w.show_indicator,
            radius: w.indicator_radius as f32,
            icon: w.indicator_icon.clone().unwrap_or_default(),
        },
        // Set by `generals::files` once the effects are converted (`world_shot_fx`).
        world_shell_fx_weapon: String::new(),
        world_last_shot_shell_fx_weapon: String::new(),
        tracer_fx_weapon: String::new(),
        enemy_tracer_fx_weapon: String::new(),
        tracer_interval: 0,
        // Posed on the body by `shield::geometry` once the zones are open (`generals::files`).
        shield: None,
        drop: zhbo_core::weapons::DropRules { carried_killstreak: w.carried_killstreak, avoid_drop_cleanup: w.avoid_drop_cleanup, blocks_prone: w.blocks_prone },
        // Set by `props::drops` for the weapons that lie on the ground.
        drop_object: String::new(),
        // Set by `props::projectiles` from the `projectileModel` (its bounds).
        projectile_bounds: [0.0; 6],
        hud_icon_ratio: w.hud_icon_ratio.min(u8::MAX as u32) as u8,
        pickup_sound: def_sound(&w.pickup_sound_player),
        ammo_pickup_sound: def_sound(&w.ammo_pickup_sound_player),
        npc_pickup_sound: def_sound(&w.pickup_sound),
        npc_ammo_pickup_sound: def_sound(&w.ammo_pickup_sound),
    };
    #[cfg(debug_assertions)]
    {
        let text = format_table(std::slice::from_ref(&d));
        match parse_table(&text) {
            Ok(back) => debug_assert_eq!(back.as_slice(), std::slice::from_ref(&d), "to_def's output must round-trip through the weapon table schema"),
            Err(e) => panic!("to_def's output does not parse back ({}): {}", w.name, e),
        }
    }
    d
}

/// Zero Hour units always hit; per-shot RTS damage keeps BO2's ratios between weapons, scaled so a
/// 40-damage rifle round (the AN-94's) hits for the 15 the General's rifle always had.
const RTS_DAMAGE: f32 = 15.0 / 40.0;

/// How far the overhead General engages with a gun of `class` (Zero Hour units, edge to edge).
fn rts_range(class: Class) -> f32 {
    match class {
        Class::Sniper => 300.0,
        Class::Shotgun => 100.0,
        Class::Pistol => 125.0,
        _ => 175.0,
    }
}

/// One RTS shot's damage: BO2's first damage point (every pellet of a shotgun), or the blast of a
/// launcher's rocket, in [`RTS_DAMAGE`]'s scale.
fn rts_damage(d: &WeaponDef) -> f32 {
    if d.class == Class::Launcher && d.explosion_inner_damage > 0.0 {
        return d.explosion_inner_damage * RTS_DAMAGE;
    }
    d.damage[0] * RTS_DAMAGE * d.rounds_per_shot() as f32
}

/// The General's Zero Hour weapons under RTS control (`ZHBO_GeneralRifle`, `_Pistol`, `_Grenade`)
/// with the loadout's BO2 numbers: (weapon, key, value) to set in `ZHBO_Weapons.ini`. These are
/// the stand-ins a General holds before his class lands; [`rts_weapons_ini`] writes the ones a
/// class swaps in.
pub fn rts_weapon_edits(defs: &[WeaponDef]) -> Vec<(String, String, String)> {
    let l = Loadout::default();
    let find = |id: &str| defs.iter().find(|d| d.id == id);
    let mut out = Vec::new();
    let mut set = |w: &str, k: &str, v: String| out.push((w.to_string(), k.to_string(), v));
    for (weapon, id) in [("ZHBO_GeneralRifle", &l.primary), ("ZHBO_GeneralPistol", &l.secondary)] {
        let Some(d) = find(id) else { continue };
        set(weapon, "PrimaryDamage", format!("{:.1}", rts_damage(d)));
        set(weapon, "AttackRange", format!("{:.1}", rts_range(d.class)));
        set(weapon, "DelayBetweenShots", format!("{:.0}", d.fire_time * 1000.0));
        set(weapon, "ClipSize", d.clip.max(1).to_string());
        set(weapon, "ClipReloadTime", format!("{:.0}", d.reload_time * 1000.0));
        let key = id.replace(['+', '~'], "_");
        set(weapon, "FireSound", zhbo_core::names::npc_fire_event(&key));
    }
    // The third slot only stands in for the throw (zhbo-core throws the BO2 grenade).
    set("ZHBO_GeneralGrenade", "FireSound", zhbo_core::sounds::THROW.to_string());
    out
}

/// How far a thrown offhand flies at the speed zhbo-core throws it with (`Sim::rts_update`): the
/// farthest throw, at 45 degrees.
pub fn throw_range(d: &WeaponDef) -> f32 {
    let speed = if d.projectile_speed > 0.0 { d.projectile_speed } else { inches(800.0) };
    speed * speed / zhbo_core::projectiles::gravity()
}

/// The Zero Hour weapon of every player weapon the overhead General can hold
/// (`zhbo_core::names::rts_weapon`), which the bridge swaps into his slots as his class and his
/// orders change (`zhbo_core::sim::rts`):
///
/// * a **gun** fires with its BO2 fire rate, magazine and reload, hits for [`rts_damage`] out to
///   [`rts_range`], and sounds and flashes as BO2's does to others (`flashes` by weapon id;
///   `has_event` says which sound events were converted);
/// * an **offhand** is an order fired once: a throw reaches as far as the grenade flies
///   ([`throw_range`]) but never into its own blast, and a planted item walks him up to the spot
///   (`PLANT_DISTANCE`). zhbo-core throws or plants the BO2 item itself; Zero Hour only aims, so
///   these carry a token of damage (Zero Hour won't attack with a weapon that does none);
/// * the **knife** reaches `MELEE_RANGE`, edge to edge as Zero Hour measures it.
///
/// Attachments and alt modes share their base weapon's, so only base weapons get one, once each.
pub fn rts_weapons_ini(defs: &[WeaponDef], flashes: &BTreeMap<String, String>, has_event: &dyn Fn(&str) -> bool) -> String {
    let mut ini = String::from("; The overhead General's weapons, one per BO2 player weapon (zhbo-assets weapons::rts_weapons_ini).\n\n");
    let mut seen = BTreeSet::new();
    for d in defs.iter().filter(|d| !d.id.contains(['+', '~'])) {
        let name = zhbo_core::names::rts_weapon(&d.id);
        if !seen.insert(name.clone()) {
            continue;
        }
        let body = if d.class.is_gun() {
            let launcher = d.class == Class::Launcher;
            let npc = zhbo_core::names::npc_fire_event(&d.id.replace(['+', '~'], "_"));
            let flash = flashes.get(&d.id).cloned().unwrap_or_else(|| {
                if launcher { "WeaponFX_RocketBuggyMissileFire" } else if d.silenced { "NONE" } else { "WeaponFX_GenericMachineGunFire" }.to_string()
            });
            format!(
                "  PrimaryDamage           = {:.1}\n  PrimaryDamageRadius     = {:.1}\n  AttackRange             = {:.1}\n  \
                 DamageType              = {}\n  DeathType               = {}\n  WeaponSpeed             = 999999.0\n  \
                 ProjectileObject        = NONE\n  FireFX                  = {flash}\n  FireSound               = {}\n  \
                 RadiusDamageAffects     = ENEMIES NEUTRALS\n  DelayBetweenShots       = {:.0}\n  ClipSize                = {}\n  \
                 ClipReloadTime          = {:.0}\n",
                rts_damage(d),
                if launcher { d.explosion_radius } else { 0.0 },
                rts_range(d.class),
                if launcher { "EXPLOSION" } else { "SMALL_ARMS" },
                if launcher { "EXPLODED" } else { "NORMAL" },
                if has_event(&npc) { npc.as_str() } else { "NONE" },
                d.fire_time * 1000.0,
                d.clip.max(1),
                d.reload_time * 1000.0,
            )
        } else if d.class.is_offhand() {
            let throw = d.place == Place::Throw;
            let (range, min) = if throw {
                (throw_range(d), if d.effect == Effect::Explode { d.explosion_radius } else { 0.0 })
            } else {
                (zhbo_core::sim::PLANT_DISTANCE, 0.0)
            };
            // The order's cursor ring (RadiusCursorType ATTACK_SCATTER_AREA) is the blast. Zero
            // Hour's scattered token shot lands anywhere in it; zhbo-core aims at the point itself.
            let ring = if throw { d.explosion_radius } else { 0.0 };
            let sound = if throw && has_event(zhbo_core::sounds::THROW) { zhbo_core::sounds::THROW } else { "NONE" };
            format!(
                "  PrimaryDamage           = 0.01\n  PrimaryDamageRadius     = 0.0\n  AttackRange             = {range:.1}\n  \
                 MinimumAttackRange      = {min:.1}\n  ScatterRadius           = {ring:.1}\n  \
                 DamageType              = SMALL_ARMS\n  DeathType               = NORMAL\n  \
                 WeaponSpeed             = 999999.0\n  ProjectileObject        = NONE\n  RadiusDamageAffects     = ENEMIES NEUTRALS\n  \
                 FireSound               = {sound}\n  PreAttackDelay          = 450\n  PreAttackType           = PER_SHOT\n  \
                 DelayBetweenShots       = 0\n  ClipSize                = 1\n  ClipReloadTime          = 1500\n  AutoReloadsClip         = Yes\n"
            )
        } else {
            let swing = zhbo_core::sounds::some(d.melee_swipe_sound.as_str()).unwrap_or(zhbo_core::sounds::MELEE_SWING);
            let sound = if has_event(swing) { swing } else { "NONE" };
            format!(
                "  PrimaryDamage           = 0.01\n  PrimaryDamageRadius     = 0.0\n  AttackRange             = {:.1}\n  \
                 DamageType              = MELEE\n  DeathType               = NORMAL\n  WeaponSpeed             = 999999.0\n  \
                 ProjectileObject        = NONE\n  RadiusDamageAffects     = ENEMIES NEUTRALS\n  FireSound               = {sound}\n  \
                 DelayBetweenShots       = {:.0}\n  ClipSize                = 1\n  ClipReloadTime          = 0\n",
                zhbo_core::weapons::MELEE_RANGE,
                d.melee_time.max(0.0) * 1000.0,
            )
        };
        ini.push_str(&format!("Weapon {name}\n{body}End\n\n"));
    }
    ini
}

/// The ids whose art is converted for the base weapons only: the loadout's (with its attachments)
/// first, then every other player weapon unless `ZHBO_CONVERT_ALL_WEAPONS=0`. Used for HUD icons,
/// FX and sound tables, where a combo's entry is its base weapon's.
pub fn converted_base(all: &[Weapon], roster: &Roster) -> Vec<String> {
    let l = Loadout::default();
    let mut ids = vec![l.primary, l.secondary, l.lethal, l.tactical, l.melee, zhbo_core::weapons::CLASS_MELEE.to_string()];
    // Every weapon's art, so any of them can be picked up, bought or swapped to; `=0` converts only
    // the loadout's (quicker while developing).
    if !std::env::var("ZHBO_CONVERT_ALL_WEAPONS").is_ok_and(|v| v == "0") {
        for w in all.iter().filter(|w| roster.is_player_weapon(w)) {
            if !ids.contains(&w.name) {
                ids.push(w.name.clone());
            }
        }
    }
    ids
}

/// The attachment kinds an item lists that its variant has, mirroring
/// `zhbo_conformance::bo2::combos::offered` over the parsed weapon (the conformance crate reads the
/// raw structs; the converter's parsed `Weapon` carries the same `attachment_variants`). This is
/// the one place the converter bridges the conformance rule to its own type; the install test
/// `conformance_the_enumerator_matches_the_expected_rows` pins the two together.
fn offered_kinds(item: &zhbo_conformance::bo2::combos::Item, w: &Weapon) -> Vec<usize> {
    let mut kinds: Vec<usize> = zhbo_conformance::bo2::combos::listed(item)
        .into_iter()
        .filter(|&k| w.attachment_variants.iter().any(|(kind, _)| *kind as usize == k))
        .collect();
    kinds.sort_unstable();
    kinds.dedup();
    kinds
}

/// Every legal combo id a class can carry, from BO2's own rules (`zhbo_conformance::bo2::combos`):
/// each Create-a-Class item's variant with every attachment set its slot allows. The base weapon
/// itself is first (the empty combination).
pub fn all_legal_combos(patch: &zhbo_formats::t6zone::Zone, all: &[Weapon]) -> Vec<String> {
    let Ok(Some(statstable)) = zhbo_formats::stringtable::rows(patch, "mp/statstable.csv") else { return Vec::new() };
    let Ok(Some(attachmenttable)) = zhbo_formats::stringtable::rows(patch, "mp/attachmenttable.csv") else { return Vec::new() };
    let items = zhbo_conformance::bo2::combos::items(&statstable);
    let t = zhbo_conformance::bo2::combos::table(&attachmenttable);
    let mut ids = Vec::new();
    for item in items.iter() {
        let Some(w) = all.iter().find(|w| w.name == item.variant) else { continue };
        let kinds = offered_kinds(item, w);
        for combo in zhbo_conformance::bo2::combos::combos(item, &kinds, &t) {
            // The weapon the class gets (`combos::given`, G16): a pistol with Dual Wield is the
            // separate `<ref>_dw_mp` weapon, not the pistol with a `dw` part.
            let (name, key) = zhbo_conformance::bo2::combos::given(item, &combo);
            if !all.iter().any(|x| x.name == name) {
                continue;
            }
            let id = if key.is_empty() { name } else { format!("{name}+{key}") };
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    ids
}

/// The ids whose art is converted: the base set, plus every legal combination unless
/// `ZHBO_CONVERT_ALL_COMBOS=0` (on by default, so a locally built archive covers every legal combo;
/// developers set it to `0` for a quicker base-only build).
pub fn converted(patch: &zhbo_formats::t6zone::Zone, all: &[Weapon]) -> Vec<String> {
    let mut ids = converted_base(all, &Roster::read(patch));
    if !matches!(std::env::var("ZHBO_CONVERT_ALL_COMBOS").as_deref(), Ok("0")) {
        for id in all_legal_combos(patch, all) {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// An attachment field equal to `defaultattachment`'s changes nothing (`None`); the real
    /// default's zoom levels are 1.0, so a silencer's authored 1.0 is no change while an optic's
    /// 35 is.
    #[test]
    fn attachment_fields_equal_to_the_default_attachment_are_no_change() {
        let default = bo2::Attachment { name: DEFAULT_ATTACHMENT.into(), ads_zoom_fov2: 1.0, ads_zoom_fov3: 1.0, ..Default::default() };
        let silencer = bo2::Attachment { name: "silencer".into(), kind: 23, ads_zoom_fov2: 1.0, ads_zoom_fov3: 1.0, ..Default::default() };
        let acog = bo2::Attachment { name: "acog".into(), kind: 1, ads_zoom_fov2: 35.0, ads_zoom_fov3: 35.0, ads_zoom_in_frac: 0.5, ..Default::default() };
        let s = attachment_def(&silencer, &default, &HashMap::new(), &HashMap::new());
        assert_eq!((s.fov2, s.fov3, s.zoom_in_frac, s.kind), (None, None, None, 23));
        let a = attachment_def(&acog, &default, &HashMap::new(), &HashMap::new());
        assert_eq!((a.fov2, a.fov3, a.zoom_in_frac), (Some(35.0), Some(35.0), Some(0.5)));
    }

    /// The reload start add and the ADS depth of field come from the first single unique that
    /// differs from the base unique (`FUN_008ed2c0`, `FUN_008ebdb0`), each DOF component alone.
    #[test]
    fn unique_fields_take_the_first_single_differing_from_the_base() {
        let u = |kind: u32, start_add: i32, dof: (f32, f32)| bo2::AttachmentUnique { kind, reload_start_add: start_add, ads_dof: dof, ..Default::default() };
        let w = Weapon {
            name: "an94_mp".into(),
            attachment_variants: vec![(1, "acog".into()), (6, "dualclip".into()), (23, "silencer".into())],
            attachment_uniques: vec![u(0, 0, (2.2, 6.4)), u(1, 0, (3.4, 3.9)), u(6, 2, (2.2, 5.0)), u(23, 0, (2.2, 6.4))],
            ..Default::default()
        };
        let mut d = WeaponDef { attachments: vec!["dualclip".into(), "silencer".into()], ..Default::default() };
        d.zoom.dof = (2.2, 6.4);
        apply_unique_fields(&mut d, &w);
        assert_eq!(d.timing.reload_start_add, 2);
        assert_eq!(d.zoom.dof, (2.2, 5.0), "start: none differs; end: the dualclip's");
        let mut d = WeaponDef { attachments: vec!["acog".into(), "dualclip".into()], ..Default::default() };
        apply_unique_fields(&mut d, &w);
        assert_eq!(d.zoom.dof, (3.4, 3.9), "kind 1 is walked first");
    }

    /// The first raise and the segmented reload's rounds per cycle come from the first single
    /// unique differing from the base unique (`FUN_008ed100`, `FUN_008ed250`): the 870's dual
    /// magazine loads two shells, the INSAS grip raises first in its own time.
    #[test]
    fn first_raise_and_reload_ammo_add_take_the_first_differing_unique() {
        let u = |kind: u32, first: f32, add: i32| bo2::AttachmentUnique { kind, first_raise_time: first, reload_ammo_add: add, ..Default::default() };
        let w = Weapon {
            name: "870mcs_mp".into(),
            attachment_variants: vec![(4, "extbarrel".into()), (6, "dualclip".into()), (10, "grip".into())],
            attachment_uniques: vec![u(0, 1.0, 1), u(4, 1.0, 1), u(6, 1.0, 2), u(10, 1.2, 1)],
            ..Default::default()
        };
        let mut d = WeaponDef { attachments: vec!["extbarrel".into(), "dualclip".into(), "grip".into()], first_raise_time: 1.0, reload_ammo_add: 1, ..Default::default() };
        apply_unique_fields(&mut d, &w);
        assert_eq!((d.first_raise_time, d.reload_ammo_add), (1.2, 2));
        let mut d = WeaponDef { attachments: vec!["extbarrel".into()], first_raise_time: 0.9, reload_ammo_add: 1, ..Default::default() };
        apply_unique_fields(&mut d, &w);
        assert_eq!((d.first_raise_time, d.reload_ammo_add), (0.9, 1), "no unique differs: the def's own");
    }

    /// A combination reloads segmented when the def or any of its single uniques says so, unless
    /// the variant ignores its attachments (`FUN_008ebc70`).
    #[test]
    fn segmented_reload_is_the_def_or_any_single_unique() {
        let u = |kind: u32, seg: bool| bo2::AttachmentUnique { kind, segmented_reload: seg, ..Default::default() };
        let w = Weapon {
            name: "x_mp".into(),
            attachment_variants: vec![(4, "extbarrel".into()), (6, "dualclip".into())],
            attachment_uniques: vec![u(0, false), u(4, false), u(6, true)],
            ..Default::default()
        };
        let combo = |shorts: &[&str], ignore: bool| {
            let mut d = WeaponDef { attachments: shorts.iter().map(|s| s.to_string()).collect(), ..Default::default() };
            d.traits.ignore_attachments = ignore;
            apply_unique_fields(&mut d, &w);
            d.traits.segmented_reload
        };
        assert!(combo(&["extbarrel", "dualclip"], false));
        assert!(!combo(&["extbarrel"], false));
        assert!(!combo(&["dualclip"], true), "the variant ignores its attachments");
    }

    /// A dual-wield pair's left gun shoots with its own def's player sound (`fireSoundPlayer` of the
    /// `szDualWieldWeaponName` def, `wpn_<x>_fire_plr_lh`); a single gun has none.
    #[test]
    fn the_left_gun_gets_its_own_shot() {
        let dw = Weapon {
            name: "fnp45_dw_mp".into(),
            class: bo2::class::PISTOL,
            fire_sound_player: Some("wpn_fnp45_fire_plr_rh".into()),
            dual_wield_weapon: Some("fnp45_lh_mp".into()),
            ..Default::default()
        };
        let lh = Weapon { name: "fnp45_lh_mp".into(), class: bo2::class::PISTOL, fire_sound_player: Some("wpn_fnp45_fire_plr_lh".into()), ..Default::default() };
        let single = Weapon { name: "fnp45_mp".into(), class: bo2::class::PISTOL, fire_sound_player: Some("wpn_fnp45_fire_plr".into()), ..Default::default() };
        let mk = |id: &str| WeaponDef { id: id.into(), class: Class::Pistol, ..Default::default() };
        let mut defs = vec![mk("fnp45_dw_mp"), mk("fnp45_mp")];
        let (events, _) = sounds_and_fx(&[dw, lh, single], &[], &mut defs, &["fnp45_dw_mp".into(), "fnp45_mp".into()]);
        assert_eq!(defs[0].left_fire_sound, "ZHBO_BO2_FireLeft_fnp45_dw_mp");
        assert!(events.iter().any(|e| e.event == "ZHBO_BO2_FireLeft_fnp45_dw_mp" && e.layers[0] == "wpn_fnp45_fire_plr_lh"));
        assert_eq!(defs[1].left_fire_sound, "");
    }

    #[test]
    fn the_ui_text_carries_the_installs_strings_for_the_cores_keys() {
        let names: HashMap<String, String> = [("MPUI_SECOND_SPECIAL_GRENADE_CAPS", "SECOND TACTICAL"), ("MPUI_UNUSED", "X")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let rows = ui_text(&names);
        assert_eq!(rows, vec![zhbo_core::weapons::UiText { key: "MPUI_SECOND_SPECIAL_GRENADE_CAPS".into(), text: "SECOND TACTICAL".into() }]);
    }

    #[test]
    fn the_attachment_grid_follows_the_items_column_8() {
        // Column 8's tokens are cut at their first `_` (`silencer_smg` is `silencer`); the grid
        // walks them in that order, not by name.
        let item = zhbo_conformance::bo2::combos::Item {
            reference: "an94".into(),
            variant: "an94_mp".into(),
            group: "weapon_assault".into(),
            slot: "primary".into(),
            cost: 1,
            attachments: vec!["reflex".into(), "fastads".into(), "silencer_smg".into(), "acog".into()],
            index: 1,
        };
        let mut d = WeaponDef { id: "an94_mp".into(), ..Default::default() };
        d.allowed = ["acog", "extra", "fastads", "reflex", "silencer"].iter().map(|s| (s.to_string(), format!("{s}_x"))).collect();
        order_attachments(&mut d, &item);
        let shorts: Vec<&str> = d.allowed.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(shorts, ["reflex", "fastads", "silencer", "acog", "extra"]);
    }

    #[test]
    fn maps_bo2_types_to_classes() {
        let rifle = Weapon { name: "an94_mp".into(), class: bo2::class::RIFLE, view_model: Some("t6_wpn_ar_an94_view".into()), ..Default::default() };
        assert_eq!(class_of(&rifle), Class::Rifle);
        // A sniper is a `rifle` to its def; the statstable group (`weapon_sniper`) makes it a sniper.
        let sniper = Weapon { name: "dsr50_mp".into(), class: bo2::class::RIFLE, view_model: Some("t6_wpn_sniper_dsr50_view".into()), ..Default::default() };
        assert_eq!(class_of(&sniper), Class::Rifle);
        let item = |reference: &str, group: &str| zhbo_conformance::bo2::combos::Item { reference: reference.into(), variant: format!("{reference}_mp"), group: group.into(), slot: "primary".into(), cost: 1, attachments: Vec::new(), index: 0 };
        let roster = Roster { items: Some([("dsr50_mp".to_string(), item("dsr50", "weapon_sniper")), ("an94_mp".to_string(), item("an94", "weapon_assault"))].into_iter().collect()) };
        let opts = OptionTables { roster: &roster, camo: &[], reticles: &[], reticle_colors: &[] };
        let t = table_with_metadata(&[rifle.clone(), sniper.clone()], &[], None, &HashMap::new(), &HashMap::new(), &opts);
        let class = |id: &str| t.weapons.iter().find(|d| d.id == id).map(|d| d.class);
        assert_eq!((class("an94_mp"), class("dsr50_mp")), (Some(Class::Rifle), Some(Class::Sniper)));
        // Not on the roster: not a player weapon (the minigun's cost is -1; `defaultweapon_mp` has no row).
        assert!(!roster.is_player_weapon(&Weapon { name: "minigun_mp".into(), view_model: Some("v".into()), ..Default::default() }));
        let dw_roster = Roster { items: Some([("fiveseven_mp".to_string(), zhbo_conformance::bo2::combos::Item { attachments: vec!["dw".into()], ..item("fiveseven", "weapon_pistol") })].into_iter().collect()) };
        assert!(dw_roster.is_player_weapon(&Weapon { name: "fiveseven_dw_mp".into(), ..Default::default() }), "the dual-wield form of an item that lists dw");
        let frag = Weapon { name: "frag_grenade_mp".into(), weap_type: bo2::kind::GRENADE, offhand_slot: bo2::offhand_slot::LETHAL, ..Default::default() };
        assert_eq!((class_of(&frag), effect_of(&frag)), (Class::Lethal, Effect::Explode));
        let conc = Weapon { name: "concussion_grenade_mp".into(), weap_type: bo2::kind::GRENADE, offhand_slot: bo2::offhand_slot::TACTICAL, ..Default::default() };
        assert_eq!((class_of(&conc), effect_of(&conc)), (Class::Tactical, Effect::Stun));
    }

    #[test]
    fn an_offhands_class_is_its_bo2_offhand_slot() {
        // The claymore (a `primarygrenade` CaC item) and the Sensor Grenade (`specialgrenade`) are
        // both on BO2's equipment slot, so both run at Fast Equipment Use's rate.
        for name in ["claymore_mp", "sensor_grenade_mp", "trophy_system_mp"] {
            let w = Weapon { name: name.into(), weap_type: bo2::kind::GRENADE, offhand_slot: bo2::offhand_slot::EQUIPMENT, ..Default::default() };
            assert_eq!(class_of(&w), Class::Equipment, "{name}");
        }
        let flash = Weapon { name: "flash_grenade_mp".into(), weap_type: bo2::kind::GRENADE, offhand_slot: bo2::offhand_slot::TACTICAL, ..Default::default() };
        assert_eq!(class_of(&flash), Class::Tactical);
    }

    /// A `bolt` explosion spawns its `szSpawnedGrenadeWeaponName` (the crossbow's
    /// `explosive_bolt_mp`), which joins the table beside its weapon; no other explosion type reads
    /// the name. `stickiness` all/all_orient sticks, unless the strike is the damage.
    #[test]
    fn a_bolt_spawns_its_grenade_and_stickiness_is_bo2s() {
        use bo2::{proj_explosion, stickiness};
        let crossbow = Weapon {
            name: "crossbow_mp".into(),
            weap_type: bo2::kind::PROJECTILE,
            view_model: Some("v".into()),
            proj_explosion: proj_explosion::BOLT,
            stickiness: stickiness::ALL,
            spawned_grenade_weapon: Some("explosive_bolt_mp".into()),
            ..Default::default()
        };
        let bolt = Weapon { name: "explosive_bolt_mp".into(), weap_type: bo2::kind::GRENADE, offhand_slot: bo2::offhand_slot::LETHAL, stickiness: stickiness::ALL, fuse_time: 1.0, ..Default::default() };
        let d = to_def(&crossbow, &HashMap::new());
        assert_eq!(d.spawns, "explosive_bolt_mp");
        let text = zhbo_core::weapons::format_table(std::slice::from_ref(&d));
        assert_eq!(zhbo_core::weapons::parse_table(&text).unwrap()[0].spawns, "explosive_bolt_mp", "round trip");
        let rocket = Weapon { proj_explosion: proj_explosion::ROCKET, ..crossbow.clone() };
        assert_eq!(to_def(&rocket, &HashMap::new()).spawns, "", "only a bolt reads the name");
        let table = table(&[crossbow.clone(), bolt.clone()], &[], None, &HashMap::new(), &Roster::any());
        let spawned = table.weapons.iter().find(|w| w.id == "explosive_bolt_mp").expect("the spawned grenade is in the table");
        assert!(spawned.sticky && spawned.fuse_time == 1.0 && spawned.effect == Effect::Explode);
        // The ballistic knife sticks in BO2 too, but its strike is the damage: it never waits.
        let knife = Weapon { name: "knife_ballistic_mp".into(), weap_type: bo2::kind::PROJECTILE, stickiness: stickiness::ALL, ..Default::default() };
        assert!(!to_def(&knife, &HashMap::new()).sticky);
        let c4 = Weapon { name: "satchel_charge_mp".into(), weap_type: bo2::kind::GRENADE, stickiness: stickiness::ALL_ORIENT, ..Default::default() };
        assert!(to_def(&c4, &HashMap::new()).sticky);
        let claymore = Weapon { name: "claymore_mp".into(), weap_type: bo2::kind::GRENADE, stickiness: stickiness::GROUND_WITH_YAW, ..Default::default() };
        assert!(!to_def(&claymore, &HashMap::new()).sticky);
    }

    #[test]
    fn the_def_carries_bo2s_inventory_type_and_combinations_inherit_it() {
        use zhbo_core::weapons::Inventory;
        let claymore = Weapon { name: "claymore_mp".into(), weap_type: bo2::kind::GRENADE, offhand_slot: bo2::offhand_slot::EQUIPMENT, inventory: bo2::inventory::ITEM, ..Default::default() };
        let d = to_def(&claymore, &HashMap::new());
        assert_eq!(d.inventory, Inventory::Item);
        let text = zhbo_core::weapons::format_table(std::slice::from_ref(&d));
        assert_eq!(zhbo_core::weapons::parse_table(&text).unwrap()[0].inventory, Inventory::Item, "round trip");
        let gl = Weapon { name: "gl_an94_mp".into(), inventory: bo2::inventory::ALTMODE, ..Default::default() };
        assert_eq!(to_def(&gl, &HashMap::new()).inventory, Inventory::AltMode);
    }

    #[test]
    fn dry_fire_events_cover_the_five_bo2_aliases() {
        assert_eq!(dry_fire_event(Some("wpn_generic_dryfire_plr")), "ZHBO_BO2_DryFireGeneric");
        assert_eq!(dry_fire_event(Some("wpn_gl_rifle_dryfire_plr")), "ZHBO_BO2_DryFireGlRifle");
        assert_eq!(dry_fire_event(Some("wpn_crossbow_dryfire_plr")), "ZHBO_BO2_DryFireCrossbow");
        assert_eq!(dry_fire_event(Some("wpn_rpg_dryfire_plr")), "ZHBO_BO2_DryFireRpg");
        assert_eq!(dry_fire_event(Some("wpn_beretta93r_dryfire_plr")), "ZHBO_BO2_DryFireBeretta");
        assert_eq!(dry_fire_event(None), "ZHBO_BO2_DryFireGeneric");
    }

    /// An attachment's ADS walk is kept only when it differs from `defaultattachment`'s (the
    /// install's: 1.0 everywhere but the stalker variants), so the generic 1.0 never replaces a
    /// weapon's own value (`FUN_008ed080`); the zeroed `,defaultattachment` stub is not the default.
    #[test]
    fn the_ads_move_override_is_what_differs_from_defaultattachment() {
        let default = bo2::Attachment { name: "defaultattachment".into(), ads_move_scale: 1.0, ..Default::default() };
        let att = |name: &str, kind: u32, ads_move_scale: f32| bo2::Attachment { name: name.into(), kind, ads_move_scale, ..Default::default() };
        let names = HashMap::new();
        let meta = HashMap::new();
        assert_eq!(attachment_def(&att("dualclip", 2, 1.0), &default, &names, &meta).ads_move_scale, 0.0);
        assert_eq!(attachment_def(&att("stalker_smg", 25, 2.55), &default, &names, &meta).ads_move_scale, 2.55);
        // The whole table drops the stub and keeps the comparison.
        let atts = [att(",defaultattachment", 0, 0.0), att("dualclip", 2, 1.0), att("stalker_lmg", 25, 1.8)];
        let t = table(&[], &atts, Some(&default), &names, &Roster::any());
        let by = |id: &str| t.attachments.iter().find(|a| a.id == id).unwrap().ads_move_scale;
        assert_eq!(t.attachments.len(), 2);
        assert_eq!((by("dualclip"), by("stalker_lmg")), (0.0, 1.8));
    }

    #[test]
    fn clip_relative_ammo_counts_magazines() {
        let w = Weapon { name: "an94_mp".into(), clip_size: 30, start_ammo: 4, max_ammo: 8, ammo_clip_relative: false, fire_time: 0.096, ..Default::default() };
        let d = to_def(&w, &HashMap::new());
        assert_eq!((d.clip, d.start_reserve, d.max_reserve), (30, 90, 240));
        assert_eq!(d.display_name, "", "no displayName, no HUD name (BO2's knife_mp)");
    }

    /// The HUD name is BO2's rendering of `displayName`: its text, else the reference itself.
    #[test]
    fn the_display_name_is_the_localized_reference_or_the_reference() {
        let names: HashMap<String, String> = [("WEAPON_AN94".to_string(), "AN-94".to_string())].into();
        let named = |dn: &str| to_def(&Weapon { name: "x_mp".into(), display_name: dn.into(), clip_size: 1, ..Default::default() }, &names).display_name;
        assert_eq!(named("WEAPON_AN94"), "AN-94");
        assert_eq!(named("BOUNCING_BETTY"), "BOUNCING_BETTY", "a key no string table holds shows as itself");
    }

    /// `iMaxAmmo` is the spare cap beside the magazine, never minus one (`FUN_005e08d0`; G16): the
    /// crossbow (start 6, clip 3, max 3, in rounds) starts and caps at 3 spare; the SMAW's 1/1/1
    /// keeps one spare rocket at most (it was 0); an LMG's 3/4 magazines of 100 are 200 spare at
    /// spawn and 400 at most.
    #[test]
    fn max_ammo_is_the_spare_cap_beside_the_magazine() {
        let rounds = |name: &str, clip_size, start_ammo, max_ammo| {
            let w = Weapon { name: name.into(), clip_size, start_ammo, max_ammo, fire_time: 0.1, ..Default::default() };
            let d = to_def(&w, &HashMap::new());
            (d.clip, d.start_reserve, d.max_reserve)
        };
        assert_eq!(rounds("crossbow_mp", 3, 6, 3), (3, 3, 3));
        assert_eq!(rounds("smaw_mp", 1, 1, 1), (1, 0, 1));
        let lmg = Weapon { name: "lsat_mp".into(), clip_size: 100, start_ammo: 3, max_ammo: 4, ammo_clip_relative: true, fire_time: 0.1, ..Default::default() };
        let d = to_def(&lmg, &HashMap::new());
        assert_eq!((d.clip, d.start_reserve, d.max_reserve), (100, 200, 400));
    }

    /// `bounceSound` is per BO2 surface; the def carries the mod's slots' cells by name, and the dud
    /// fields with the activation distance in ZH units.
    #[test]
    fn bounce_cells_map_by_surface_and_the_dud_fields_carry() {
        let mut bounce = vec![None; 32];
        bounce[zhbo_formats::fx::surface::CONCRETE] = Some("wpn_grenade_bounce_concrete".to_string());
        bounce[zhbo_formats::fx::surface::FLESH] = Some("wpn_grenade_bounce_flesh".to_string());
        bounce[zhbo_formats::fx::surface::BRICK] = Some("wpn_grenade_bounce_brick".to_string());
        let w = Weapon {
            name: "gl_test_mp".into(),
            clip_size: 1,
            fire_time: 0.1,
            bounce_sounds: bounce,
            projectile_activate_dist: 300,
            projectile_dud_sound: Some("wpn_grenade_bounce_flesh".into()),
            ..Default::default()
        };
        let d = to_def(&w, &HashMap::new());
        use zhbo_core::impacts::Surface;
        assert_eq!(d.bounce_sound(Surface::Concrete), "ZHBO_BO2_Snd_wpn_grenade_bounce_concrete");
        assert_eq!(d.bounce_sound(Surface::Flesh), "ZHBO_BO2_Snd_wpn_grenade_bounce_flesh");
        assert_eq!(d.bounce_sound(Surface::Dirt), "");
        assert_eq!(d.bounce_sounds.len(), 2, "brick is no mod slot");
        assert_eq!(d.activate_dist, zhbo_core::units::inches(300.0));
        assert_eq!(d.dud_sound, "ZHBO_BO2_Snd_wpn_grenade_bounce_flesh");
        assert!(d.def_sound_events().any(|(e, heard)| e == d.dud_sound && heard == zhbo_core::weapons::Heard::World));
        // A combination takes them from its base like every converted asset.
        let mut combo = zhbo_core::weapons::WeaponDef::default();
        combo.take_converted_assets(&d);
        assert_eq!((combo.bounce_sounds.clone(), combo.dud_sound.clone()), (d.bounce_sounds.clone(), d.dud_sound.clone()));
    }

    /// The regression pin: a replacement optic's unique supplies the combo's overlay, and an empty
    /// unique means NO overlay — the base unique's material is never inherited. FMJ leaves the base
    /// scope, so the base def's fields stand.
    #[test]
    fn the_synthesized_combos_overlay_follows_the_mounted_optic() {
        let base = bo2::AttachmentUnique {
            name: "au_as50_none".into(),
            kind: 0,
            overlay_material: "scope_overlay_xpr50".into(),
            overlay_material_low_res: "scope_overlay_xpr50_low_res".into(),
            overlay_reticle: 1,
            ..Default::default()
        };
        let acog = bo2::AttachmentUnique { name: "au_as50_acog".into(), kind: 1, ..Default::default() };
        // A non-optic unique carries the weapon's overlay as authored (no difference from the base
        // unique), so it leaves the base scope.
        let fmj = bo2::AttachmentUnique { name: "au_as50_fmj".into(), kind: 10, ..base.clone() };
        let w = Weapon {
            name: "as50_mp".into(),
            clip_size: 5,
            start_ammo: 1,
            max_ammo: 2,
            fire_time: 0.1,
            overlay_material: "scope_overlay_xpr50".into(),
            overlay_material_low_res: "scope_overlay_xpr50_low_res".into(),
            overlay_reticle: 1,
            attachment_variants: vec![(1, "acog".into()), (10, "fmj".into())],
            attachment_uniques: vec![base, acog, fmj],
            ..Default::default()
        };
        let mut table = Table {
            weapons: vec![to_def(&w, &HashMap::new())],
            attachments: vec![
                AttachmentDef { id: "acog".into(), short: "acog".into(), point: 1, ..Default::default() },
                AttachmentDef { id: "fmj".into(), short: "fmj".into(), ..Default::default() },
            ],
            ..Default::default()
        };
        add_variants(&mut table, &["as50_mp+acog".into(), "as50_mp+fmj".into()], std::slice::from_ref(&w));
        let def = |id: &str| table.weapons.iter().find(|d| d.id == id).unwrap_or_else(|| panic!("no {id}"));
        assert_eq!(def("as50_mp").overlay_material, "scope_overlay_xpr50");
        assert!(def("as50_mp+acog").overlay_material.is_empty(), "the ACOG's unique has no overlay");
        assert_eq!(def("as50_mp+fmj").overlay_material, "scope_overlay_xpr50", "FMJ leaves the base scope");
    }

    /// The combinations are built before the converter fills the base defs' sounds and FX; the
    /// inheritance pass must hand every combination (and its alt mode) its base's assets, with the
    /// silencer's swap, and leave its art alone.
    #[test]
    fn combinations_inherit_the_converted_sounds_and_fx_with_the_silencer_swap() {
        let w = Weapon { name: "an94_mp".into(), clip_size: 30, fire_time: 0.1, ..Default::default() };
        let gl = Weapon { name: "gl_an94_mp".into(), clip_size: 1, fire_time: 0.1, ..Default::default() };
        let mut base = to_def(&w, &HashMap::new());
        base.alt_modes = vec![("gl".into(), "gl_an94_mp".into())];
        base.allowed = ["silencer", "grip", "gl"].map(|s| (s.to_string(), s.to_string())).to_vec();
        let mut alt = to_def(&gl, &HashMap::new());
        alt.alt_own_ammo = true;
        let mut table = Table {
            weapons: vec![base, alt],
            attachments: vec![
                // The silencer and the grip pair (combo-legality: both lists name the other).
                AttachmentDef { id: "silencer".into(), short: "silencer".into(), point: 2, silenced: true, compatible: vec!["grip".into()], ..Default::default() },
                AttachmentDef { id: "grip".into(), short: "grip".into(), point: 3, compatible: vec!["silencer".into()], ..Default::default() },
                AttachmentDef { id: "gl".into(), short: "gl".into(), point: 3, ..Default::default() },
            ],
            ..Default::default()
        };
        let ids: Vec<String> = ["an94_mp+grip", "an94_mp+grip+silencer", "an94_mp+gl"].map(String::from).to_vec();
        add_variants(&mut table, &ids, &[w.clone(), gl.clone()]);
        // The art pass marks a combination's own viewmodel; inheritance must keep it.
        table.weapons.iter_mut().find(|d| d.id == "an94_mp+grip").unwrap().viewmodel = 7;
        // The sound and FX passes fill the bases only.
        let b = table.weapons.iter_mut().find(|d| d.id == "an94_mp").unwrap();
        b.fire_sound = "ZHBO_BO2_Fire_an94_mp".into();
        b.fx_weapon = "ZHBO_FX_an94_mp".into();
        b.view_flash_fx = "ZHBO_BO2FX_fx_muz_ar_flash_1p".into();
        b.view_shell_fx = "ZHBO_BO2FX_fx_rifle".into();
        b.silenced_fire_sound = "ZHBO_BO2_Fire_an94_mp_silenced".into();
        b.silenced_fx_weapon = "ZHBO_FX_an94_mp_silenced".into();
        b.silenced_view_flash_fx = "ZHBO_BO2FX_fx_muz_silencer_md".into();
        table.weapons.iter_mut().find(|d| d.id == "gl_an94_mp").unwrap().explosion_fx = "ZHBO_BO2FX_gl".into();
        let def = |t: &Table, id: &str| t.weapons.iter().find(|d| d.id == id).unwrap_or_else(|| panic!("no {id}")).clone();
        assert!(def(&table, "an94_mp+grip").fire_sound.is_empty(), "the precondition: built before the passes");
        inherit_converted_assets(&mut table, &[w, gl]);
        let grip = def(&table, "an94_mp+grip");
        assert_eq!((grip.fire_sound.as_str(), grip.fx_weapon.as_str()), ("ZHBO_BO2_Fire_an94_mp", "ZHBO_FX_an94_mp"));
        assert_eq!((grip.view_flash_fx.as_str(), grip.view_shell_fx.as_str()), ("ZHBO_BO2FX_fx_muz_ar_flash_1p", "ZHBO_BO2FX_fx_rifle"));
        assert_eq!(grip.viewmodel, 7, "the combination's art is untouched");
        let sil = def(&table, "an94_mp+grip+silencer");
        assert_eq!(sil.fire_sound, "ZHBO_BO2_Fire_an94_mp_silenced");
        assert_eq!(sil.fx_weapon, "ZHBO_FX_an94_mp_silenced");
        assert_eq!(sil.view_flash_fx, "ZHBO_BO2FX_fx_muz_silencer_md");
        assert_eq!(def(&table, "gl_an94_mp~an94_mp+gl").explosion_fx, "ZHBO_BO2FX_gl", "the alt mode inherits from its alt weapon");
    }

    /// BO2's gate (`FUN_008ebe80`): a unique replaces a sound only when both the base unique and it
    /// set the bit — to its alias, or to nothing; the silencer's unique first, then `eAttachment`
    /// order.
    #[test]
    fn sound_overrides_go_through_both_bits_silencer_first() {
        let u = |kind, bits, start: Option<&str>| bo2::AttachmentUnique { kind, sound_overrides: bits, fire_start_sound_player: start.map(String::from), ..Default::default() };
        let w = |u0_bits, sil_bits, start| Weapon { attachment_uniques: vec![u(0, u0_bits, None), u(23, sil_bits, start)], ..Default::default() };
        let silencer = || bo2::Attachment { name: "silencer".into(), kind: 23, silenced: true, ..Default::default() };
        let atts = [silencer()];
        let bit = FIRE_START_SOUND_PLAYER_BIT;
        let start: fn(&bo2::AttachmentUnique) -> &Option<String> = |u| &u.fire_start_sound_player;
        assert_eq!(gated_sound(&w(bit, bit, Some("null_sound")), &atts, &[23], bit, start), Some(Some("null_sound".into())));
        assert_eq!(gated_sound(&w(bit, bit, None), &atts, &[23], bit, start), Some(None), "overridden to nothing");
        assert_eq!(gated_sound(&w(0, bit, Some("x")), &atts, &[23], bit, start), None, "the base unique gates it");
        assert_eq!(gated_sound(&w(bit, 0, Some("x")), &atts, &[23], bit, start), None);
        // Select fire (22) and the silencer (23) both override: the silencer wins even though it
        // sorts later; without its bit the select fire's override stands.
        let both = |sil_bits| Weapon { attachment_uniques: vec![u(0, bit, None), u(22, bit, Some("sf")), u(23, sil_bits, Some("sil"))], ..Default::default() };
        assert_eq!(gated_sound(&both(bit), &atts, &[22, 23], bit, start), Some(Some("sil".into())));
        assert_eq!(gated_sound(&both(0), &atts, &[23, 22], bit, start), Some(Some("sf".into())));
        // Two non-silencer overrides: the lower `eAttachment` kind, whatever the mount order.
        let two = Weapon { attachment_uniques: vec![u(0, bit, None), u(22, bit, Some("sf")), u(11, bit, Some("gl"))], ..Default::default() };
        assert_eq!(gated_sound(&two, &[], &[22, 11], bit, start), Some(Some("gl".into())));
    }

    /// The SIG556's select fire (BO2's data, G14 §4): its unique overrides the player's shot (the
    /// burst gun's own is only the low-end alias) and the burst opening (`null_sound`); the base
    /// def lists both for `attachments::apply`, and the silencer still wins on a combo with both.
    #[test]
    fn select_fire_overrides_the_shot_and_the_burst_opening() {
        let all = FIRE_SOUND_PLAYER_BIT | FIRE_START_SOUND_PLAYER_BIT;
        let u = |kind, plr: &str, start: &str| bo2::AttachmentUnique {
            kind,
            sound_overrides: all,
            fire_sound_player: Some(plr.into()),
            fire_start_sound_player: Some(start.into()),
            ..Default::default()
        };
        let w = Weapon {
            name: "sig556_mp".into(),
            class: bo2::class::RIFLE,
            fire_sound_player: Some("wpn_sig556_LFE".into()),
            fire_start_sound_player: Some("wpn_sig556_fire_plr_burst".into()),
            attachment_variants: vec![(22, "sf".into()), (23, "silencer".into())],
            attachment_uniques: vec![
                bo2::AttachmentUnique { kind: 0, sound_overrides: all, ..Default::default() },
                u(22, "wpn_sig556_fire_plr", "null_sound"),
                u(23, "wpn_sig556_silencer_fire_plr", "null_sound"),
            ],
            ..Default::default()
        };
        let atts = [
            bo2::Attachment { name: "sf".into(), kind: 22, ..Default::default() },
            bo2::Attachment { name: "silencer".into(), kind: 23, silenced: true, ..Default::default() },
        ];
        let mut defs = vec![to_def(&w, &HashMap::new())];
        let (events, _) = sounds_and_fx(std::slice::from_ref(&w), &atts, &mut defs, &["sig556_mp".into()]);
        let d = &defs[0];
        assert_eq!(d.attachment_fire_sounds, vec![("sf".to_string(), "ZHBO_BO2_Fire_sig556_mp_sf".to_string())]);
        assert_eq!(d.attachment_fire_start_sounds, vec![("sf".to_string(), zhbo_core::names::def_sound_event("null_sound"))]);
        let sf_shot = events.iter().find(|e| e.event == "ZHBO_BO2_Fire_sig556_mp_sf").expect("the select fire's shot event");
        assert_eq!(sf_shot.layers, vec!["wpn_sig556_fire_plr".to_string()], "the alias; its LFE and tail are its secondary chain");
        // Select fire and the silencer pair (combo-legality: both lists name the other).
        let att = |short: &str, silenced, with: &str| zhbo_core::attachments::AttachmentDef { short: short.into(), silenced, point: 0, compatible: vec![with.into()], ..Default::default() };
        let (sf, sil) = (att("sf", false, "silencer"), att("silencer", true, "sf"));
        let combo = zhbo_core::attachments::apply(d, &[&sf]);
        assert_eq!((combo.fire_sound.as_str(), combo.fire_start_sound.as_str()), ("ZHBO_BO2_Fire_sig556_mp_sf", d.attachment_fire_start_sounds[0].1.as_str()));
        assert!(combo.attachment_fire_sounds.is_empty(), "a combination is not a base");
        let both = zhbo_core::attachments::apply(d, &[&sf, &sil]);
        assert_eq!(both.fire_sound, "ZHBO_BO2_Fire_sig556_mp_silenced", "the silencer wins");
    }

    /// A held gun swings and lands with the class knife's sounds; a melee weapon with its own.
    #[test]
    fn guns_melee_with_the_class_knifes_sounds() {
        let knife = Weapon { name: CLASS_MELEE.into(), weap_type: bo2::kind::MELEE, melee_swipe_sound_player: Some("wpn_melee_whoosh_plr".into()), melee_hit_sound: Some("wpn_melee_knife_hit_body".into()), ..Default::default() };
        let gun = Weapon { name: "an94_mp".into(), class: bo2::class::RIFLE, melee_swipe_sound_player: Some("wpn_com_knife_whoosh_plr".into()), ..Default::default() };
        let held = Weapon { name: "knife_held_mp".into(), weap_type: bo2::kind::MELEE, melee_swipe_sound_player: Some("wpn_com_knife_whoosh_plr".into()), ..Default::default() };
        let mut defs: Vec<WeaponDef> = [&gun, &held].iter().map(|w| to_def(w, &HashMap::new())).collect();
        sounds_and_fx(&[knife, gun, held], &[], &mut defs, &[]);
        assert_eq!(defs[0].melee_swipe_sound, zhbo_core::names::def_sound_event("wpn_melee_whoosh_plr"));
        assert_eq!(defs[0].melee_hit_sound, zhbo_core::names::def_sound_event("wpn_melee_knife_hit_body"));
        assert_eq!(defs[1].melee_swipe_sound, zhbo_core::names::def_sound_event("wpn_com_knife_whoosh_plr"), "a melee weapon swings itself");
    }

    /// Others hear the third-person fields (BO2's without `Player`): a weapon's raise, putaway
    /// and pin are its own, a gun swings and lands with the class knife's third-person swing and
    /// hit, and each is a positional event of its own even when BO2 names one alias for both (the
    /// shield's swing).
    #[test]
    fn others_hear_the_third_person_def_sounds() {
        use zhbo_core::weapons::NpcSound;
        let npc = zhbo_core::names::npc_def_sound_event;
        let knife = Weapon { name: CLASS_MELEE.into(), weap_type: bo2::kind::MELEE, melee_swipe_sound: Some("wpn_melee_whoosh_npc".into()), melee_hit_sound: Some("wpn_melee_knife_hit_body".into()), ..Default::default() };
        let gun = Weapon { name: "an94_mp".into(), class: bo2::class::RIFLE, raise_sound: Some("fly_generic_raise_npc".into()), first_raise_sound: Some("fly_generic_first_raise_npc".into()), putaway_sound: Some("fly_generic_down_npc".into()), melee_swipe_sound: Some("wpn_tac_knife_whoosh_npc".into()), ..Default::default() };
        let shield = Weapon { name: "riotshield_mp".into(), weap_type: bo2::kind::RIOTSHIELD, melee_swipe_sound: Some("fly_riotshield_melee_swing".into()), melee_swipe_sound_player: Some("fly_riotshield_melee_swing".into()), ..Default::default() };
        let semtex = Weapon { name: "sticky_grenade_mp".into(), pullback_sound: Some("wpn_semtex_charge".into()), ..Default::default() };
        let mut defs: Vec<WeaponDef> = [&gun, &shield, &semtex].iter().map(|w| to_def(w, &HashMap::new())).collect();
        let (events, _) = sounds_and_fx(&[knife, gun, shield, semtex], &[], &mut defs, &[]);
        let (g, sh, se) = (&defs[0], &defs[1], &defs[2]);
        assert_eq!(g.npc_sound(NpcSound::Raise), npc("fly_generic_raise_npc"));
        assert_eq!(g.npc_sound(NpcSound::FirstRaise), npc("fly_generic_first_raise_npc"));
        assert_eq!(g.npc_sound(NpcSound::Putaway), npc("fly_generic_down_npc"));
        assert_eq!(g.npc_sound(NpcSound::MeleeSwipe), npc("wpn_melee_whoosh_npc"), "the class knife's, not the gun's own field");
        assert_eq!(g.npc_sound(NpcSound::MeleeHit), npc("wpn_melee_knife_hit_body"));
        assert_eq!(sh.npc_sound(NpcSound::MeleeSwipe), npc("fly_riotshield_melee_swing"), "the shield swings itself");
        assert_ne!(sh.npc_sound(NpcSound::MeleeSwipe), sh.melee_swipe_sound, "apart from its owner's 2D event");
        assert_eq!(se.npc_sound(NpcSound::Pullback), npc("wpn_semtex_charge"));
        assert_eq!(se.npc_sound(NpcSound::Raise), "", "no field, no sound");
        // Each converts once, positional.
        for e in [npc("fly_generic_raise_npc"), npc("fly_riotshield_melee_swing"), npc("wpn_semtex_charge"), npc("wpn_melee_whoosh_npc")] {
            let made: Vec<_> = events.iter().filter(|x| x.event == e).collect();
            assert_eq!(made.len(), 1, "{e}");
            assert_eq!(made[0].kind, crate::sfx::Kind::World, "{e}");
        }
        assert!(events.iter().any(|x| x.event == zhbo_core::names::def_sound_event("fly_riotshield_melee_swing") && x.kind == crate::sfx::Kind::Player), "the owner's own stays 2D");
    }

    /// The launcher alt mode carries its `altSwitchSoundPlayer` (EV_WEAPON_ALT); a rifle has none.
    #[test]
    fn an_alt_mode_carries_its_alt_switch_sound() {
        let gl = Weapon { name: "gl_an94_mp".into(), alt_switch_sound_player: Some("wpn_weap_raise_plr".into()), ..Default::default() };
        let rifle = Weapon { name: "an94_mp".into(), class: bo2::class::RIFLE, ..Default::default() };
        let defs: Vec<WeaponDef> = [&gl, &rifle].iter().map(|w| to_def(w, &HashMap::new())).collect();
        assert_eq!(defs[0].alt_switch_sound, zhbo_core::names::def_sound_event("wpn_weap_raise_plr"));
        assert!(defs[1].alt_switch_sound.is_empty());
        assert!(defs[0].def_sound_events().any(|(e, _)| e == defs[0].alt_switch_sound), "its event is registered with the others");
    }

    #[test]
    fn with_alt_modes_adds_each_reachable_alt_once() {
        let mut a = WeaponDef { id: "an94_mp".into(), ..Default::default() };
        a.alt_modes = vec![("gl".into(), "gl_an94_mp".into()), ("sf".into(), "sf_an94_mp".into())];
        let mut c = WeaponDef { id: "an94_mp+gl".into(), alt: "gl_an94_mp~an94_mp+gl".into(), ..Default::default() };
        c.alt_modes = vec![("gl".into(), "gl_an94_mp".into())];
        let ids = with_alt_modes(&[a, c], &["an94_mp".into(), "an94_mp+gl".into()]);
        assert_eq!(ids, ["an94_mp", "an94_mp+gl", "gl_an94_mp", "sf_an94_mp", "gl_an94_mp~an94_mp+gl"]);
    }

    #[test]
    fn a_combos_alt_drop_is_its_first_unique_that_differs_from_the_base() {
        // FUN_008ed1e0 (hk416's data): base unique and def 16 ms, select fire's unique 500 ms, the
        // launcher's 16 ms. A combined unique past the singles never stands in for a kind.
        let u = |index: u32, kind: u32, ms: f32, combined: u32| bo2::AttachmentUnique { index, kind, combined, alt_drop_time: ms / 1000.0, ..Default::default() };
        let w = Weapon {
            name: "hk416_mp".into(),
            clip_size: 30,
            start_ammo: 1,
            max_ammo: 2,
            fire_time: 0.1,
            alt_raise_time: 0.016,
            alt_drop_time: 0.016,
            attachment_variants: vec![(1, "acog".into()), (10, "gl".into()), (20, "sf".into())],
            attachment_uniques: vec![u(0, 0, 16.0, 0), u(1, 1, 16.0, 0), u(10, 10, 16.0, 0), u(20, 20, 500.0, 0), u(60, 1, 900.0, 0x2)],
            ..Default::default()
        };
        assert_eq!(alt_drop_for(&w, &["acog", "sf"]), Some(0.5));
        assert_eq!(alt_drop_for(&w, &["gl"]), None, "the launcher's unique keeps the def's");
        assert_eq!(alt_drop_for(&w, &["acog"]), None, "slot 60's combined unique is not the ACOG's single");
        let alt = Weapon { inventory: bo2::inventory::ALTMODE, ..w.clone() };
        assert_eq!(alt_drop_for(&alt, &["sf"]), None, "an alt mode always takes its own");
        let mut table = Table {
            weapons: vec![to_def(&w, &HashMap::new())],
            // Each pairs with the others (combo-legality: both compatibility lists name the other).
            attachments: ["acog", "gl", "sf"]
                .iter()
                .map(|s| AttachmentDef { id: s.to_string(), short: s.to_string(), compatible: ["acog", "gl", "sf"].iter().filter(|o| *o != s).map(|o| o.to_string()).collect(), ..Default::default() })
                .collect(),
            ..Default::default()
        };
        add_variants(&mut table, &["hk416_mp+acog+sf".into(), "hk416_mp+gl".into()], std::slice::from_ref(&w));
        let def = |id: &str| table.weapons.iter().find(|d| d.id == id).unwrap_or_else(|| panic!("no {id}"));
        assert_eq!((def("hk416_mp").alt_raise_time, def("hk416_mp").alt_drop_time), (0.016, 0.016));
        assert_eq!(def("hk416_mp+acog+sf").alt_drop_time, 0.5);
        assert_eq!(def("hk416_mp+acog+sf").alt_raise_time, 0.016, "no attachment scales the alt raise");
        assert_eq!(def("hk416_mp+gl").alt_drop_time, 0.016);
    }

    #[test]
    fn a_replacement_optic_carries_its_own_overlay() {
        let base = bo2::AttachmentUnique {
            name: "au_as50_none".into(),
            kind: 0,
            overlay_material: "scope_overlay_xpr50".into(),
            overlay_reticle: 1,
            ..Default::default()
        };
        let acog = bo2::AttachmentUnique {
            name: "au_as50_acog".into(),
            kind: 1,
            overlay_material: "scope_overlay_acog".into(),
            overlay_material_low_res: "scope_overlay_acog_low_res".into(),
            overlay_reticle: 2,
            ..Default::default()
        };
        let w = Weapon {
            name: "as50_mp".into(),
            clip_size: 5,
            start_ammo: 1,
            max_ammo: 2,
            fire_time: 0.1,
            overlay_material: "scope_overlay_xpr50".into(),
            overlay_reticle: 1,
            attachment_variants: vec![(1, "acog".into())],
            attachment_uniques: vec![base, acog],
            ..Default::default()
        };
        let mut table = Table {
            weapons: vec![to_def(&w, &HashMap::new())],
            attachments: vec![AttachmentDef { id: "acog".into(), short: "acog".into(), point: 1, ..Default::default() }],
            ..Default::default()
        };
        add_variants(&mut table, &["as50_mp+acog".into()], std::slice::from_ref(&w));
        let d = table.weapons.iter().find(|d| d.id == "as50_mp+acog").expect("the ACOG combo");
        assert_eq!(d.overlay_material, "scope_overlay_acog");
        assert_eq!(d.overlay_material_low_res, "scope_overlay_acog_low_res");
        assert_eq!(d.overlay_reticle, 2);
    }

    /// G14 §4: each overlay field is the first single unique differing from the base unique, field
    /// by field — in `ir+vzoom` the IR supplies the material and the VZOOM the reticle the IR leaves
    /// at the base's (the conformance rows `hamr_mp ir+vzoom`: `scope_overlay_envg_ir`, crosshair).
    #[test]
    fn each_overlay_field_is_the_first_differing_single() {
        let base = bo2::AttachmentUnique { name: "au_hamr_none".into(), kind: 0, ..Default::default() };
        let ir = bo2::AttachmentUnique { name: "au_hamr_ir".into(), kind: 14, overlay_material: "scope_overlay_envg_ir".into(), ..Default::default() };
        let vzoom = bo2::AttachmentUnique { name: "au_hamr_vzoom".into(), kind: 29, overlay_material: "scope_overlay_specter".into(), overlay_reticle: 1, ..Default::default() };
        let w = Weapon {
            name: "hamr_mp".into(),
            attachment_variants: vec![(14, "ir".into()), (29, "vzoom".into()), (10, "fmj".into())],
            attachment_uniques: vec![base, ir, vzoom],
            ..Default::default()
        };
        assert_eq!(super::overlay_for(&w, &["vzoom", "ir"]), ("scope_overlay_envg_ir".to_string(), String::new(), 1));
        assert_eq!(super::overlay_for(&w, &["vzoom"]), ("scope_overlay_specter".to_string(), String::new(), 1));
        assert_eq!(super::overlay_for(&w, &["fmj"]), (String::new(), String::new(), 0), "a kind without a unique stands in as the base: the weapon's own");
    }

    #[test]
    fn a_combined_unique_never_supplies_the_overlay() {
        let base = bo2::AttachmentUnique {
            name: "au_as50_none".into(),
            kind: 0,
            overlay_material: "scope_overlay_xpr50".into(),
            overlay_reticle: 1,
            ..Default::default()
        };
        let ir = bo2::AttachmentUnique {
            name: "au_as50_ir".into(),
            kind: 14,
            overlay_material: "scope_overlay_ir".into(),
            overlay_reticle: 2,
            ..Default::default()
        };
        let sway = bo2::AttachmentUnique { name: "au_as50_swayreduc".into(), kind: 27, ..Default::default() };
        let combined = bo2::AttachmentUnique {
            name: "au_as50_swayreduc+ir".into(),
            kind: 14,
            combined: 0x8004000,
            overlay_material: "scope_overlay_ir_combined".into(),
            overlay_reticle: 3,
            ..Default::default()
        };
        let w = Weapon {
            name: "as50_mp".into(),
            clip_size: 5,
            start_ammo: 1,
            max_ammo: 2,
            fire_time: 0.1,
            overlay_material: "scope_overlay_xpr50".into(),
            overlay_reticle: 1,
            attachment_variants: vec![(14, "ir".into()), (27, "swayreduc".into())],
            attachment_uniques: vec![base, ir, sway, combined],
            ..Default::default()
        };
        let mut table = Table {
            weapons: vec![to_def(&w, &HashMap::new())],
            attachments: vec![
                // The zoom window: each field is the first attachment (kind order: ir 14, swayreduc
                // 27) differing from `defaultattachment` (`attachment_def` keeps only those; the
                // core's `apply_handling` takes the first). Default (0.5, 0.05): the IR's in 0.016
                // differs, its out does not; the swayreduc's out 0.4 does.
                AttachmentDef { id: "ir".into(), short: "ir".into(), point: 1, kind: 14, zoom_in_frac: Some(0.016), compatible: vec!["swayreduc".into()], ..Default::default() },
                AttachmentDef { id: "swayreduc".into(), short: "swayreduc".into(), kind: 27, zoom_out_frac: Some(0.4), compatible: vec!["ir".into()], ..Default::default() },
            ],
            ..Default::default()
        };
        add_variants(&mut table, &["as50_mp+swayreduc+ir".into()], std::slice::from_ref(&w));
        let d = table.weapons.iter().find(|d| d.id == "as50_mp+ir+swayreduc").expect("the combined combo");
        assert_eq!((d.zoom.in_frac, d.zoom.out_frac), (0.016, 0.4), "in: the IR differs first; out: the IR matches the default, the swayreduc differs");
        // G14 §4: the overlay accessors read the single uniques; the combined child is only ever
        // read for the drawn parts and the animations.
        assert_eq!(d.overlay_material, "scope_overlay_ir", "the IR's single unique supplies the overlay");
        assert_eq!(d.overlay_reticle, 2);
    }

    /// The converter's built rows must equal the conformance expected set: the mechanical
    /// agreement between what the converter makes and BO2's own CaC rows. The expected side has a
    /// row per weapon a class can be given (`combos::given`: an item's combination, or a pistol's
    /// separate `_dw_mp` weapon), per such row's alt mode (`gl_an94_mp` under `an94_mp+gl`, its
    /// key the combination's), and per always-given melee weapon. So "ours" is the same three
    /// things as the converter builds them: [`all_legal_combos`], the alt defs [`add_variants`]
    /// adds for them (`<alt>~<combo>`), and the melee weapons every class has (the held knife and the
    /// class melee weapon `knife_mp`). The variant's `_mp` suffix is
    /// stripped so `an94_mp+reflex` compares to the conformance `an94+reflex`.
    /// This is a parity-gap detector, not a unit test: it lives in the conformance family's
    /// red-by-design set (the blocking suite runs `--skip conformance`).
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn conformance_the_enumerator_matches_the_expected_rows() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        // The converter's own source: BO2's weapon pool (`MpPool`), whose `common_patch_mp` alone
        // defines the Peacekeeper and the patch's attachments.
        let mp = crate::paks::MpPool::load(&dir).unwrap();
        let (all, atts) = mp.weapons().unwrap();
        let patch = mp.patch;
        let strip = |id: &str| -> String {
            let (base, rest) = id.split_once('+').map(|(b, r)| (b, Some(r))).unwrap_or((id, None));
            let base = base.strip_suffix("_mp").unwrap_or(base);
            match rest {
                Some(r) => format!("{base}+{r}"),
                None => base.to_string(),
            }
        };
        let combos = all_legal_combos(&patch, &all);
        // The table as the converter builds it: the attachments carry BO2's compatibility, so
        // `apply` keeps every legal combination whole.
        let meta = attachment_metadata(&patch, &HashMap::new());
        let opts = OptionTables { roster: &crate::weapons::Roster::read(&patch), camo: &[], reticles: &[], reticle_colors: &[] };
        let mut built = table_with_metadata(&all, &atts, default_attachment(&patch).as_ref(), &HashMap::new(), &meta, &opts);
        add_variants(&mut built, &combos, &all);
        let mut ours: std::collections::BTreeSet<String> = combos.iter().map(|id| strip(id)).collect();
        for d in &built.weapons {
            // An alt mode `<alt>~<combo>` is the alt variant's row under the combination's key.
            let Some((alt, combo)) = d.id.split_once('~') else { continue };
            if combos.iter().any(|c| c == combo) {
                let key = combo.split_once('+').map_or("", |(_, k)| k);
                ours.insert(strip(&format!("{alt}+{key}")));
            }
        }
        ours.insert(strip(&Loadout::default().melee));
        ours.insert(strip(zhbo_core::weapons::CLASS_MELEE));
        let expected = zhbo_conformance::bo2::expected(&dir).unwrap().0;
        let theirs: std::collections::BTreeSet<String> = expected
            .iter()
            .map(|r| {
                let base = r.key.0.strip_suffix("_mp").unwrap_or(r.key.0.as_str());
                if r.key.1.is_empty() { base.to_string() } else { format!("{base}+{}", r.key.1) }
            })
            .collect();
        let only_ours: Vec<&String> = ours.difference(&theirs).collect();
        let only_theirs: Vec<&String> = theirs.difference(&ours).collect();
        assert!(only_ours.is_empty() && only_theirs.is_empty(), "the converter's rows must equal the conformance expected set: only ours {only_ours:?}, only theirs {only_theirs:?}");
    }

    /// The converted attachments carry BO2's compatibility so the core's pairing rule
    /// (`attachments::compatible`, what the editor, the class repair and the catalog decide with)
    /// agrees with the conformance rule (`bo2::combos::Table::compatible`, `FUN_008e6b20`) on
    /// every pair of attachment kinds the install defines — one parse, no hand-copied list.
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_converted_compatibility_matches_bo2s_rule() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let mp = crate::paks::MpPool::load(&dir).unwrap();
        let (_, atts) = mp.weapons().unwrap();
        let patch = mp.patch;
        let meta = attachment_metadata(&patch, &HashMap::new());
        let default = default_attachment(&patch).unwrap_or_default();
        let defs: Vec<AttachmentDef> = atts.iter().filter(|a| a.kind != 0 && !a.name.starts_with(',')).map(|a| attachment_def(a, &default, &HashMap::new(), &meta)).collect();
        let rows = zhbo_formats::stringtable::rows(&patch, "mp/attachmenttable.csv").unwrap().unwrap();
        let t = zhbo_conformance::bo2::combos::table(&rows);
        let kind = |d: &AttachmentDef| zhbo_conformance::bo2::combos::kind_of(&d.short).expect("a known kind");
        let mut checked = 0;
        for a in &defs {
            for b in defs.iter().filter(|b| b.short != a.short) {
                assert_eq!(zhbo_core::attachments::compatible(a, b), t.compatible(kind(a), kind(b)), "{} + {}", a.id, b.id);
                checked += 1;
            }
        }
        assert!(checked > 1000, "only {checked} pairs");
        // The pairs this item was about: BO2 forbids dualoptic+gl and fmj+mms (the mod used to
        // allow them: different attach points) and allows fastads+stalker (one point, gunperk).
        let by = |s: &str| defs.iter().find(|d| d.short == s).unwrap();
        assert!(!zhbo_core::attachments::compatible(by("dualoptic"), by("gl")));
        assert!(!zhbo_core::attachments::compatible(by("fmj"), by("mms")));
        assert!(zhbo_core::attachments::compatible(by("fastads"), by("stalker")));
    }

    /// The full set includes every statstable-backed base id and at least one combination per base
    /// weapon that has attachments. A base id the Create-a-Class enumerator does not offer (the
    /// loadout's melee `knife_held_mp`, a table row rather than a CaC item) is excluded by the same
    /// `items` filter the enumerator uses. The plan's §7.2 direction: every legal combo is in the
    /// full `converted()` set.
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn every_cac_weapon_has_bo2s_attribute_bars() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let patch = crate::generals::load_zone(&dir, crate::fx::ZONE).unwrap();
        let patch_ui = crate::generals::load_zone(&dir, "patch_ui_mp").unwrap();
        let (all, _) = crate::paks::MpPool::load(&dir).unwrap().weapons().unwrap();
        let (bars, att_bars) = attributes(&patch_ui);
        // The AN-94's row and the reflex's row keyed by it (`mp/attributestable.csv`).
        assert_eq!(bars["an94_mp"], [60.0, 60.0, 55.0, 50.0]);
        assert_eq!(att_bars[&("reflex".to_string(), "an94_mp".to_string())], [0.0, 0.0, 5.0, 15.0]);
        // Every create-a-class gun (the statstable's items) resolves to a row.
        let statstable = zhbo_formats::stringtable::rows(&patch, "mp/statstable.csv").unwrap().unwrap();
        let guns: std::collections::BTreeSet<String> = zhbo_conformance::bo2::combos::items(&statstable)
            .into_iter()
            .map(|i| i.variant)
            .filter(|v| all.iter().any(|w| &w.name == v && !w.attachment_variants.is_empty()))
            .collect();
        let missing: Vec<&String> = guns.iter().filter(|g| !bars.contains_key(*g)).collect();
        assert!(missing.is_empty(), "no attribute row for {missing:?}");
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_legal_combo_set_covers_every_base_weapon() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let mp = crate::paks::MpPool::load(&dir).unwrap();
        let (all, _) = mp.weapons().unwrap();
        let patch = mp.patch;
        let base = converted_base(&all, &Roster::read(&patch));
        let full = all_legal_combos(&patch, &all);
        // The base set's statstable-backed ids: the enumerator's `items` are the only CaC bases.
        let statstable = zhbo_formats::stringtable::rows(&patch, "mp/statstable.csv").unwrap().unwrap();
        let backed: std::collections::BTreeSet<String> = zhbo_conformance::bo2::combos::items(&statstable)
            .into_iter()
            .map(|i| i.variant)
            .filter(|v| all.iter().any(|w| &w.name == v))
            .collect();
        for id in &base {
            let base_weapon = crate::generals::base_id(id);
            if backed.contains(base_weapon) {
                assert!(full.contains(id), "the full set must include the base id {id}");
            }
        }
        assert!(full.len() > base.len(), "the full set must have combinations beyond the base weapons");
        // The plan's §7.2 direction: every legal combo is in `converted()` (the full set).
        let converted = converted(&patch, &all);
        for id in &full {
            assert!(converted.contains(id), "the full set must include the legal combo {id}");
        }
    }

    /// The alt mode keeps the combination's attachment fields, but BO2 resolves them against the
    /// alt variant's own table (`FUN_008ec9e0`, `FUN_008eb300`; G14): `sf_mp7_mp` offers none, so
    /// `mp7_mp+extclip+rf+sf`'s select fire keeps its own clip, fire time and laser while the
    /// weapon takes the extended mag, rapid fire and laser. The names still ride along (the parts
    /// it draws are the parent's).
    #[test]
    fn an_alt_mode_takes_none_of_the_weapons_attachment_numbers() {
        let sf_unique = bo2::AttachmentUnique { name: "au_mp7_sf".into(), kind: 22, alt_weapon: Some("sf_mp7_mp".into()), ..Default::default() };
        let w = Weapon {
            name: "mp7_mp".into(),
            clip_size: 40,
            start_ammo: 4,
            max_ammo: 8,
            fire_time: 0.08,
            attachment_variants: vec![(6, "extclip".into()), (21, "rf".into()), (22, "sf".into()), (14, "steadyaim".into())],
            attachment_uniques: vec![sf_unique],
            ..Default::default()
        };
        let sf = Weapon { name: "sf_mp7_mp".into(), clip_size: 40, start_ammo: 4, max_ammo: 8, fire_time: 0.064, inventory: bo2::inventory::ALTMODE, ..Default::default() };
        let mut table = Table {
            weapons: vec![to_def(&w, &HashMap::new()), to_def(&sf, &HashMap::new())],
            attachments: vec![
                AttachmentDef { id: "extclip".into(), short: "extclip".into(), clip_scale: 1.35, ..Default::default() },
                AttachmentDef { id: "rf".into(), short: "rf".into(), fire_time_scale: 0.75, ..Default::default() },
                AttachmentDef { id: "sf".into(), short: "sf".into(), ..Default::default() },
                AttachmentDef { id: "steadyaim".into(), short: "steadyaim".into(), laser: true, hip_spread_min_scale: 0.65, ..Default::default() },
            ],
            ..Default::default()
        };
        // Each pairs with the others (combo-legality: both compatibility lists name the other).
        let shorts: Vec<String> = table.attachments.iter().map(|a| a.short.clone()).collect();
        for a in &mut table.attachments {
            a.compatible = shorts.iter().filter(|s| **s != a.short).cloned().collect();
        }
        let own = table.weapons[1].clone();
        add_variants(&mut table, &["mp7_mp+extclip+rf+sf".into(), "mp7_mp+sf+steadyaim".into()], &[w, sf]);
        let d = table.weapons.iter().find(|d| d.id == "mp7_mp+extclip+rf+sf").expect("the combo");
        assert_eq!(d.clip, 54);
        assert!((d.fire_time - 0.06).abs() < 1e-6, "{}", d.fire_time);
        let alt = table.weapons.iter().find(|a| a.id == d.alt).expect("the alt mode");
        assert_eq!(alt.id, "sf_mp7_mp~mp7_mp+extclip+rf+sf");
        assert_eq!((alt.clip, alt.fire_time, alt.reload_time, alt.start_reserve, alt.max_reserve), (own.clip, own.fire_time, own.reload_time, own.start_reserve, own.max_reserve));
        assert_eq!(alt.attachments, vec!["extclip", "rf"]);
        let d = table.weapons.iter().find(|d| d.id == "mp7_mp+sf+steadyaim").expect("the combo");
        assert!(d.laser);
        let alt = table.weapons.iter().find(|a| a.id == d.alt).expect("the alt mode");
        assert!(!alt.laser && alt.hip_spread == own.hip_spread && alt.specialties == own.specialties);
    }

    /// Each melee weapon swings with its own BO2 sound; the class melee weapon's (`knife_mp`) is the
    /// global swing a gun's melee plays (`sounds::melee_swing_for`).
    #[test]
    fn each_melee_weapon_gets_its_own_swing_and_the_class_melee_the_global_one() {
        let knife = |name: &str, alias: &str| Weapon { name: name.into(), weap_type: bo2::kind::MELEE, melee_swipe_sound_player: Some(alias.into()), ..Default::default() };
        let bo2 = [knife("knife_held_mp", "wpn_com_knife_whoosh_plr"), knife(zhbo_core::weapons::CLASS_MELEE, "wpn_melee_whoosh_plr")];
        let mut defs: Vec<WeaponDef> = bo2.iter().map(|w| to_def(w, &HashMap::new())).collect();
        let ids: Vec<String> = defs.iter().map(|d| d.id.clone()).collect();
        let (events, _) = sounds_and_fx(&bo2, &[], &mut defs, &ids);
        let layers = |e: &str| events.iter().find(|x| x.event == e).map(|x| x.layers.clone());
        // A melee weapon swings itself: its own def-level sound event (`names::def_sound_event`).
        assert_eq!(defs[0].melee_swipe_sound, zhbo_core::names::def_sound_event("wpn_com_knife_whoosh_plr"), "the held knife's own");
        assert_eq!(defs[1].melee_swipe_sound, zhbo_core::names::def_sound_event("wpn_melee_whoosh_plr"), "the class melee weapon's own");
        assert_eq!(layers(zhbo_core::sounds::MELEE_SWING), Some(vec!["wpn_melee_whoosh_plr".to_string()]), "the global swing");
        assert!(bo2.iter().all(|w| Roster::read_none().is_player_weapon(w)), "both knives are converted, whatever the statstable");
    }

    #[test]
    fn an_alt_mode_carries_its_own_overlay() {
        let base = bo2::AttachmentUnique {
            name: "au_as50_none".into(),
            kind: 0,
            overlay_material: "scope_overlay_xpr50".into(),
            overlay_reticle: 1,
            ..Default::default()
        };
        let fmj = bo2::AttachmentUnique {
            name: "au_as50_fmj".into(),
            kind: 10,
            alt_weapon: Some("sf_as50_mp".into()),
            ..Default::default()
        };
        let acog = bo2::AttachmentUnique { name: "au_as50_acog".into(), kind: 1, ..Default::default() };
        let w = Weapon {
            name: "as50_mp".into(),
            clip_size: 5,
            start_ammo: 1,
            max_ammo: 2,
            fire_time: 0.1,
            overlay_material: "scope_overlay_xpr50".into(),
            overlay_reticle: 1,
            attachment_variants: vec![(1, "acog".into()), (10, "fmj".into())],
            attachment_uniques: vec![base, fmj, acog],
            ..Default::default()
        };
        let sf_base = bo2::AttachmentUnique {
            name: "au_sf_as50_none".into(),
            kind: 0,
            overlay_material: "scope_overlay_sf_base".into(),
            overlay_reticle: 5,
            ..Default::default()
        };
        let sf_acog = bo2::AttachmentUnique {
            name: "au_sf_as50_acog".into(),
            kind: 1,
            overlay_material: "scope_overlay_sf_acog".into(),
            overlay_reticle: 4,
            ..Default::default()
        };
        let sf = Weapon {
            name: "sf_as50_mp".into(),
            clip_size: 5,
            start_ammo: 1,
            max_ammo: 2,
            fire_time: 0.1,
            inventory: bo2::inventory::ALTMODE,
            overlay_material: "scope_overlay_sf_base".into(),
            overlay_reticle: 5,
            attachment_variants: vec![(1, "acog".into())],
            attachment_uniques: vec![sf_base, sf_acog],
            ..Default::default()
        };
        let mut table = Table {
            weapons: vec![to_def(&w, &HashMap::new()), to_def(&sf, &HashMap::new())],
            attachments: vec![
                AttachmentDef { id: "acog".into(), short: "acog".into(), point: 1, compatible: vec!["fmj".into()], ..Default::default() },
                AttachmentDef { id: "fmj".into(), short: "fmj".into(), compatible: vec!["acog".into()], ..Default::default() },
            ],
            ..Default::default()
        };
        add_variants(&mut table, &["as50_mp+acog+fmj".into()], &[w.clone(), sf.clone()]);
        let d = table.weapons.iter().find(|d| d.id == "as50_mp+acog+fmj").expect("the base combo");
        assert!(d.overlay_material.is_empty(), "the base combo's ACOG has no overlay");
        let alt = table.weapons.iter().find(|a| !d.alt.is_empty() && a.id == d.alt).expect("the alt mode");
        assert_eq!(alt.overlay_material, "scope_overlay_sf_acog", "the alt base's ACOG unique");
        assert_eq!(alt.overlay_reticle, 4);
    }

    #[test]
    fn the_camo_material_is_the_defs_own_weapon_camo() {
        let w = Weapon { name: "insas_mp".into(), class: bo2::class::RIFLE, weapon_camo: Some("camo_msmc".into()), ..Default::default() };
        assert_eq!(to_def(&w, &HashMap::new()).camo_material, "camo_msmc");
        let none = Weapon { name: "mtar_mp".into(), class: bo2::class::RIFLE, ..Default::default() };
        assert_eq!(to_def(&none, &HashMap::new()).camo_material, "");
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_shellshocks_convert_and_the_stand_ins_match_them() {
        // Every shock the core plays is in the weapon pool, and the stand-in table's `Shock`
        // blocks (zhbo-core's BUILTIN) are the install's values.
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let crate::paks::MpPool { patch, common_patch, common } = crate::paks::MpPool::load(&dir).unwrap();
        let shocks = shocks(&[&patch, &common_patch, &common]).unwrap();
        assert_eq!(shocks.len(), zhbo_core::shellshock::names::ALL.len());
        let stand_in = zhbo_core::weapons::parse_full_table(zhbo_core::weapons::BUILTIN).unwrap().shocks;
        assert_eq!(stand_in, shocks);
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_smoke_concussion_and_emp_grenades_explode_on_the_ground() {
        // `bExplodeOnGround` (t6mp `FUN_0060a480`): these wait for the ground; the frag and the
        // flashbang go off in the air on their fuse. The Willy Pete's fuse is 10 ms.
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let (all, _) = crate::paks::MpPool::load(&dir).unwrap().weapons().unwrap();
        let def = |name: &str| to_def(all.iter().find(|w| w.name == name).expect(name), &HashMap::new());
        for name in ["willy_pete_mp", "concussion_grenade_mp", "emp_grenade_mp"] {
            assert!(def(name).explode_on_ground, "{name}");
        }
        for name in ["frag_grenade_mp", "flash_grenade_mp", "sticky_grenade_mp", "sensor_grenade_mp"] {
            assert!(!def(name).explode_on_ground, "{name}");
        }
        assert!(def("willy_pete_mp").fuse_time < 0.05);
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_install_says_who_aims_whose_fuse_is_timed_and_which_is_the_laser_sensor() {
        // `aimDownSight` (t6mp `FUN_008e4470`): every gun aims but a dual-wield pair, the Ballistic
        // Knife and the launcher modes; the Assault Shield, knives and offhands never do.
        // `timedDetonation` (`FUN_00607c00`): the thrown grenades and the Sensor Grenade, no
        // planted item. `isLaserSensor`: the Sensor Grenade alone. The stand-in table agrees.
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let (all, _) = crate::paks::MpPool::load(&dir).unwrap().weapons().unwrap();
        let def = |name: &str| to_def(all.iter().find(|w| w.name == name).expect(name), &HashMap::new());
        let stand_in = zhbo_core::weapons::parse_full_table(zhbo_core::weapons::BUILTIN).unwrap();
        for d in &stand_in.weapons {
            if let Some(w) = all.iter().find(|w| w.name == d.id) {
                assert_eq!(d.aims, w.aim_down_sight, "{}: the stand-in's Aims", d.id);
            }
        }
        for name in ["an94_mp", "dsr50_mp", "fiveseven_mp", "smaw_mp", "crossbow_mp", "dualoptic_an94_mp", "minigun_mp"] {
            assert!(def(name).aims, "{name}");
        }
        for name in ["riotshield_mp", "knife_ballistic_mp", "gl_an94_mp", "fiveseven_dw_mp", "knife_held_mp", "frag_grenade_mp", "trophy_system_mp"] {
            assert!(!def(name).aims, "{name}");
        }
        for name in ["frag_grenade_mp", "sticky_grenade_mp", "flash_grenade_mp", "concussion_grenade_mp", "emp_grenade_mp", "willy_pete_mp", "sensor_grenade_mp"] {
            assert!(def(name).timed_detonation, "{name}");
        }
        for name in ["satchel_charge_mp", "claymore_mp", "bouncingbetty_mp", "trophy_system_mp", "tactical_insertion_mp", "proximity_grenade_mp", "pda_hack_mp"] {
            assert!(!def(name).timed_detonation, "{name}");
        }
        let sensors: Vec<&str> = all.iter().filter(|w| w.laser_sensor).map(|w| w.name.as_str()).collect();
        assert_eq!(sensors, ["sensor_grenade_mp"]);
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn only_the_xpr_carries_the_anti_quickscope_and_snipers_hold_breath() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let (all, _) = crate::paks::MpPool::load(&dir).unwrap().weapons().unwrap();
        let aqs: Vec<&str> = all.iter().filter(|w| w.anti_quickscope).map(|w| w.name.as_str()).collect();
        assert_eq!(aqs, ["as50_mp"], "bAntiQuickScope");
        let xpr = all.iter().find(|w| w.name == "as50_mp").unwrap();
        assert_eq!((xpr.anti_quickscope_time, xpr.anti_quickscope_scale, xpr.anti_quickscope_sway_factor), (0.1, 4.0, 20.0));
        for sniper in ["dsr50_mp", "ballista_mp", "svu_mp", "as50_mp"] {
            let w = all.iter().find(|w| w.name == sniper).unwrap();
            assert!(w.hold_breath_to_steady && w.ads_idle_amount > 0.0 && w.ads_idle_speed > 0.0, "{sniper}");
        }
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_installs_camo_options_are_bo2s_table() {
        let bo2_dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let patch = crate::generals::load_zone(&bo2_dir, crate::fx::ZONE).unwrap();
        let options = camo_options(&patch, &names(&bo2_dir));
        assert!(options.len() >= 19, "{} camo options", options.len());
        assert_eq!(options[0].id, "camo_none");
        assert_eq!(options[0].name, "None");
        let multicam = options.iter().find(|c| c.id == "camo_multicam").expect("camo_multicam");
        assert_eq!(multicam.image, "menu_camo_multicam_pattern");
        assert_eq!(multicam.sort, 0, "the base rows carry the table order, not a DLC sort key");
        let common_patch = crate::generals::load_zone(&bo2_dir, "common_patch_mp").unwrap();
        let sets: HashSet<String> = bo2::camo_all(&common_patch).unwrap().into_iter().map(|c| c.name).collect();
        assert_eq!(sets.len(), 48, "common_patch_mp's camo_<weapon> assets");
        // Every camo asset a def or a unique names is one of those sets (G16's per-part camo).
        let (bo2w, _) = crate::paks::MpPool::load(&bo2_dir).unwrap().weapons().unwrap();
        let camo_of = |id: &str| bo2w.iter().find(|w| w.name == id).and_then(|w| w.weapon_camo.clone());
        assert_eq!(camo_of("an94_mp").as_deref(), Some("camo_an94"));
        assert_eq!(camo_of("saiga12_mp").as_deref(), Some("camo_saiga"));
        assert_eq!(camo_of("insas_mp").as_deref(), Some("camo_msmc"));
        let an94 = bo2w.iter().find(|w| w.name == "an94_mp").unwrap();
        let reflex = an94.attachment_uniques.iter().find(|u| u.combined == 0 && u.name.ends_with("_reflex")).map(|u| u.weapon_camo.clone());
        assert_eq!(reflex.flatten().as_deref(), Some("camo_reflex"), "the reflex unique's own camo");
        let named: BTreeSet<String> = bo2w.iter().flat_map(|w| w.weapon_camo.iter().chain(w.attachment_uniques.iter().flat_map(|u| u.weapon_camo.iter()))).cloned().collect();
        let missing: Vec<&String> = named.iter().filter(|n| !sets.contains(*n)).collect();
        println!("camo assets named by defs/uniques ({}): {named:?}; not in common_patch_mp: {missing:?}", named.len());
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_installs_weapon_groups_are_the_built_in_order() {
        // `cacweaponslot.lua`'s weapon-group order, read as the converter reads it, is the
        // stand-in table's, and the editor's primary categories follow it.
        let bo2_dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let patch = crate::generals::load_zone(&bo2_dir, crate::fx::ZONE).unwrap();
        let patch_ui = crate::generals::load_zone(&bo2_dir, "patch_ui_mp").unwrap();
        let groups = zhbo_conformance::bo2::cac::weapon_groups(&patch, &patch_ui).unwrap();
        let built_in = zhbo_core::weapons::parse_full_table(zhbo_core::weapons::BUILTIN).unwrap().weapon_groups;
        assert_eq!(groups, built_in);
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_installs_class_caps_are_the_built_in_caps() {
        // The caps the core reads from data, as `cacutility.lua` declares them, are the stand-ins.
        let bo2_dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let patch = crate::generals::load_zone(&bo2_dir, crate::fx::ZONE).unwrap();
        let patch_ui = crate::generals::load_zone(&bo2_dir, "patch_ui_mp").unwrap();
        let caps = zhbo_conformance::bo2::cac::class_caps(&patch, &patch_ui).unwrap();
        let built_in = zhbo_core::weapons::parse_full_table(zhbo_core::weapons::BUILTIN).unwrap().class_caps;
        for k in zhbo_core::classes::DATA_CAPS {
            let stand_in = built_in.iter().find(|(n, _)| n == k).map(|(_, v)| v.to_string());
            assert_eq!(caps.get(k).cloned(), stand_in, "{k}");
        }
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_installs_ui_text_is_the_built_in_stand_ins() {
        // Every key the core draws is in the install's english zones, and the stand-ins are its
        // strings exactly (`MPUI_SECOND_SPECIAL_GRENADE_CAPS` is `SECOND TACTICAL`).
        let bo2_dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let rows = ui_text(&names(&bo2_dir));
        assert_eq!(rows.len(), zhbo_core::weapons::UI_TEXT_KEYS.len(), "{rows:?}");
        let built_in = zhbo_core::weapons::parse_full_table(zhbo_core::weapons::BUILTIN).unwrap().ui_text;
        assert_eq!(rows, built_in);
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn every_optic_names_every_reticle() {
        // BO2's reticle cells are named `MPUI_RETICLE_<OPTIC>_<N>` (`ReticlesGridButton.new`), the
        // table's own name column being `MPUI_RETICLE_DEFAULT` ("Dot") on every row: the
        // converter carries each optic's name for each row, so no cell falls back to "Dot".
        let bo2_dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let names = names(&bo2_dir);
        let patch = crate::generals::load_zone(&bo2_dir, crate::fx::ZONE).unwrap();
        let rows = reticle_options(&patch, &names);
        assert!(rows.iter().all(|r| r.name == "Dot"), "the table's column 3 is MPUI_RETICLE_DEFAULT throughout");
        let text = reticle_names(&names);
        for optic in zhbo_core::weapons::RETICLE_OPTICS {
            for r in &rows {
                let key = zhbo_core::weapons::reticle_name_key(optic, &r.id).unwrap();
                let name = text.iter().find(|t| t.key == key).unwrap_or_else(|| panic!("{key} is not in the install"));
                assert!(!name.text.trim().is_empty(), "{key}");
            }
        }
        assert_eq!(text.iter().find(|t| t.key == "MPUI_RETICLE_REFLEX_3").map(|t| t.text.as_str()), Some("Target Drop"));
        assert_eq!(text.iter().find(|t| t.key == "MPUI_RETICLE_DUALOPTIC_1").map(|t| t.text.as_str()), Some("Plus, Precision"));
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_installs_bonus_cards_are_the_built_in_stand_ins() {
        // The stand-in table's eight cards are the install's rows, field for field (so a test
        // build's wildcard grid is BO2's), and every card is one the editor implements.
        let bo2_dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let patch = crate::generals::load_zone(&bo2_dir, crate::fx::ZONE).unwrap();
        let cards = bonus_card_rows(&patch, &names(&bo2_dir));
        let built_in = zhbo_core::weapons::parse_full_table(zhbo_core::weapons::BUILTIN).unwrap().bonus_cards;
        assert_eq!(cards, built_in);
        for c in &cards {
            assert!(zhbo_core::classmenu::Wildcard::by_reference(&c.reference).is_some(), "{} has no editor rule", c.reference);
        }
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_installs_reticle_options_are_bo2s_rows() {
        let bo2_dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let patch = crate::generals::load_zone(&bo2_dir, crate::fx::ZONE).unwrap();
        let names = names(&bo2_dir);
        let reticles = reticle_options(&patch, &names);
        assert_eq!(reticles.len(), 38, "reticle_0..37");
        assert_eq!(reticles.first().map(|r| r.id.as_str()), Some("reticle_0"));
        // The store rows sit on the DLC tabs, the base eleven on the base tab.
        assert!(reticles[..11].iter().all(|r| r.tab == zhbo_core::weapons::OptionTab::Base), "{reticles:?}");
        assert!(reticles[11..].iter().all(|r| r.tab != zhbo_core::weapons::OptionTab::Base), "{reticles:?}");
        assert!(reticles.iter().all(|r| r.name == "Dot"), "{reticles:?}");
        let colors = reticle_color_options(&patch, &names);
        assert_eq!(
            colors.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["Red", "Green", "Blue", "Purple", "Teal", "Yellow", "Orange"]
        );
    }

    #[test]
    fn the_weapon_camo_list_starts_as_every_row_in_table_order() {
        use zhbo_core::weapons::OptionTab;
        let camo: Vec<CamoOption> = (0..25)
            .map(|i| CamoOption { id: format!("camo_{i}"), tab: if i < 19 { OptionTab::Base } else { OptionTab::Dlc }, ..Default::default() })
            .collect();
        let w = Weapon {
            name: "an94_mp".into(),
            class: bo2::class::RIFLE,
            view_model: Some("t6_wpn_ar_an94_view".into()),
            ..Default::default()
        };
        let opts = OptionTables { roster: &crate::weapons::Roster::any(), camo: &camo, reticles: &[], reticle_colors: &[] };
        let table = table_with_metadata(&[w], &[], None, &HashMap::new(), &HashMap::new(), &opts);
        assert_eq!(table.weapons.len(), 1);
        let ids: Vec<String> = camo.iter().map(|c| c.id.clone()).collect();
        assert_eq!(table.weapons[0].camo, ids, "every row, in the table's order");
        assert_eq!(table.camo.len(), 25, "the table keeps every row for the DLC tabs");
        // The camo pass then keeps the base rows and the DLC rows with converted art.
        let mut table = table;
        table.weapons[0].camo_material = "camo_an94".into();
        let with_art = BTreeMap::from([("camo_an94".to_string(), BTreeSet::from(["camo_20".to_string()]))]);
        crate::camo::offer_what_converted(&mut table, &with_art);
        let want: Vec<String> = (0..19).chain([20]).map(|i| format!("camo_{i}")).collect();
        assert_eq!(table.weapons[0].camo, want, "base rows stay; only DLC rows with art join them");
    }

    #[test]
    fn the_option_tabs_follow_addcamos() {
        use zhbo_core::weapons::OptionTab;
        // `caccamomenu.lua`'s `AddCamos`: challenge rows on the base tab, `digital_unlock` rows and
        // store rows without a DLC2 key on DLC, store rows with one on DLC2.
        assert_eq!(OptionTab::of(false, 0, false), OptionTab::Base, "camo_atacs");
        assert_eq!(OptionTab::of(false, 0, true), OptionTab::Dlc, "camo_elite1 / camo_ghost");
        assert_eq!(OptionTab::of(true, 0, false), OptionTab::Dlc, "camo_mtp");
        assert_eq!(OptionTab::of(true, 2, false), OptionTab::Dlc2, "camo_dragon (column 5 = 2)");
    }

    /// The stand-in table's weapons name the install's `hudIcon` (the class buttons and the
    /// offhand counters draw it), and every alt mode that names one names its gun's (the select
    /// fire and the hybrid optic do; a launcher alt names none).
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_built_in_hud_icons_are_the_installs() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let (all, _) = crate::paks::MpPool::load(&dir).unwrap().weapons().unwrap();
        let icon = |id: &str| all.iter().find(|w| w.name == id).and_then(|w| w.hud_icon.clone()).unwrap_or_default();
        for d in zhbo_core::weapons::parse_full_table(zhbo_core::weapons::BUILTIN).unwrap().weapons {
            if all.iter().any(|w| w.name == d.id) {
                assert_eq!(d.hud_icon, icon(&d.id), "{}", d.id);
            }
        }
        for w in all.iter().filter(|w| w.inventory == bo2::inventory::ALTMODE && w.hud_icon.is_some()) {
            let parent = all.iter().find(|p| p.alt_weapon.as_deref() == Some(w.name.as_str()) || p.attachment_uniques.iter().any(|u| u.alt_weapon.as_deref() == Some(w.name.as_str())));
            if let Some(p) = parent {
                assert_eq!(w.hud_icon, p.hud_icon, "{} names {}'s icon", w.name, p.name);
            }
        }
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_installs_perk_rows_are_the_built_in_rows_and_weapons_carry_their_bits() {
        use zhbo_core::weapons::{Perk, Specialty};
        let bo2 = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let patch = crate::generals::load_zone(&bo2, crate::fx::ZONE).unwrap();
        let rows = perk_rows(&patch, &names(&bo2));
        let built_in = zhbo_core::weapons::parse_full_table(zhbo_core::weapons::BUILTIN).unwrap().perks;
        for perk in Perk::ALL {
            let d = perk.def();
            let row = rows.iter().find(|r| r.stats == d.stats).unwrap_or_else(|| panic!("statstable has {}", d.stats));
            let stand_in = built_in.iter().find(|r| r.stats == d.stats).unwrap_or_else(|| panic!("the built-in table has {}", d.stats));
            assert_eq!(row, stand_in, "{}: the built-in row (name, description, set) is the install's", d.id);
        }
        // In this install only FMJ grants specialties through the data (`armorpiercing`,
        // `bulletpenetration`); fast mag and quickdraw work through their reload/ADS time scales.
        let (_, atts) = crate::paks::MpPool::load(&bo2).unwrap().weapons().unwrap();
        let granting: Vec<(String, Specialties)> =
            atts.iter().map(|a| (a.name.clone(), Specialties::from_words(a.perks.0, a.perks.1))).filter(|(_, s)| !s.is_empty()).collect();
        assert!(granting.iter().any(|(n, s)| n == "fmj" && s.has(Specialty::BulletPenetration) && s.has(Specialty::ArmorPiercing)), "{granting:?}");
    }

    /// The `Weapon` block named `name` in `ini`, and one key's value in it.
    fn block<'a>(ini: &'a str, name: &str) -> &'a str {
        let at = ini.find(&format!("Weapon {name}\n")).unwrap_or_else(|| panic!("no {name} in\n{ini}"));
        &ini[at..at + ini[at..].find("\nEnd").unwrap()]
    }
    fn value<'a>(block: &'a str, key: &str) -> &'a str {
        let line = block.lines().find(|l| l.trim_start().starts_with(key)).unwrap_or_else(|| panic!("no {key} in\n{block}"));
        line.split('=').nth(1).unwrap().trim()
    }

    #[test]
    fn every_base_weapon_gets_the_overhead_generals_weapon() {
        let rifle = WeaponDef { id: "an94_mp".into(), class: Class::Rifle, clip: 30, fire_time: 0.1, reload_time: 2.5, damage: [40.0; 6], ..Default::default() };
        let variant = WeaponDef { id: "an94_mp+gl".into(), clip: 99, ..rifle.clone() };
        let frag = WeaponDef {
            id: "frag_grenade_mp".into(),
            class: Class::Lethal,
            effect: Effect::Explode,
            place: Place::Throw,
            explosion_radius: 40.0,
            projectile_speed: inches(900.0),
            ..Default::default()
        };
        let claymore = WeaponDef { id: "claymore_mp".into(), class: Class::Equipment, place: Place::Plant, ..Default::default() };
        let knife = WeaponDef { id: "knife_held_mp".into(), class: Class::Melee, melee_time: 0.5, ..Default::default() };
        let defs = [rifle, variant, frag.clone(), claymore, knife];
        let flashes = BTreeMap::from([("an94_mp".to_string(), "ZHBO_BO2FX_Flash".to_string())]);
        let ini = rts_weapons_ini(&defs, &flashes, &|e| e == zhbo_core::sounds::THROW);
        assert_eq!(ini.matches("\nWeapon ").count() + usize::from(ini.starts_with("Weapon ")), 4, "attachments share their base weapon's:\n{ini}");

        let gun = block(&ini, "ZHBO_GeneralWeapon_an94_mp");
        assert_eq!((value(gun, "ClipSize"), value(gun, "DelayBetweenShots"), value(gun, "ClipReloadTime")), ("30", "100", "2500"), "the base weapon's numbers");
        assert_eq!(value(gun, "FireFX"), "ZHBO_BO2FX_Flash");
        assert_eq!(value(gun, "FireSound"), "NONE", "no converted NPC sound, no event");
        assert_eq!(value(gun, "PrimaryDamage"), format!("{:.1}", 40.0 * RTS_DAMAGE));

        let throw = block(&ini, "ZHBO_GeneralWeapon_frag_grenade_mp");
        assert_eq!(value(throw, "AttackRange"), format!("{:.1}", throw_range(&frag)));
        assert_eq!(value(throw, "MinimumAttackRange"), "40.0", "never into its own blast");
        assert_eq!(value(throw, "ScatterRadius"), "40.0", "the order's cursor ring is the blast");
        assert_eq!(value(throw, "FireSound"), zhbo_core::sounds::THROW);
        let plant = block(&ini, "ZHBO_GeneralWeapon_claymore_mp");
        assert_eq!(value(plant, "AttackRange"), format!("{:.1}", zhbo_core::sim::PLANT_DISTANCE));
        let melee = block(&ini, "ZHBO_GeneralWeapon_knife_held_mp");
        assert_eq!((value(melee, "AttackRange"), value(melee, "DamageType")), (format!("{:.1}", zhbo_core::weapons::MELEE_RANGE).as_str(), "MELEE"));
    }

    #[test]
    fn a_throws_reach_is_its_flight_at_45_degrees() {
        let frag = WeaponDef { projectile_speed: inches(800.0), ..Default::default() };
        // v² / g with BO2's gravity (800 in/s²): 800 inches.
        assert!((throw_range(&frag) - inches(800.0)).abs() < 1e-3);
    }
}

#[cfg(test)]
mod target_finder_tests {
    /// The player's install: BO2's target finder is the `rangefinder` attachment, and its
    /// `attachmentType` is the kind the code tests (`TARGET_FINDER_KIND`, 0x13), the only one.
    #[test]
    #[ignore]
    fn the_installs_target_finder_is_kind_0x13() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR"));
        let (_, atts) = crate::paks::MpPool::load(&dir).unwrap().weapons().unwrap();
        let finders: Vec<&str> = atts.iter().filter(|a| a.kind == zhbo_core::attachments::TARGET_FINDER_KIND as u32).map(|a| a.name.as_str()).collect();
        assert!(!finders.is_empty() && finders.iter().all(|n| n.starts_with("rangefinder")), "{finders:?}");
    }

    /// The grenades carry BO2's per-surface bounce (`parallelBounce`/`perpendicularBounce`) and the
    /// knife its lunge (`meleeChargeRange`, the charge times).
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn grenades_bounce_and_the_knife_lunges_by_the_install() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let mp = crate::paks::MpPool::load(&dir).unwrap();
        let (bo2, _) = mp.weapons().unwrap();
        let def = |id: &str| crate::weapons::to_def(bo2.iter().find(|w| w.name == id).unwrap_or_else(|| panic!("{id}")), &std::collections::HashMap::new());
        let frag = def("frag_grenade_mp");
        println!("frag bounce {:?}", frag.bounce);
        assert!(!frag.bounce.is_empty() && frag.bounce.iter().all(|&(_, a, p)| (0.0..=1.5).contains(&a) && (0.0..=1.5).contains(&p)), "{:?}", frag.bounce);
        // The held knife lunges (116 in, charge time 1.16 s, delay 0.1 s); `knife_mp`, the melee
        // with a gun up, has no charge range.
        let held = def(zhbo_core::weapons::KNIFE_HELD);
        assert!(held.melee_charge_range > zhbo_core::weapons::MELEE_RANGE && held.melee_charge_time > 0.0, "{} {}", held.melee_charge_range, held.melee_charge_time);
        assert_eq!(def(zhbo_core::weapons::CLASS_MELEE).melee_charge_range, 0.0);
    }
}
