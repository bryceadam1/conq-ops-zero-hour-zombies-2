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

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use zhbo_core::attachments::{self, AttachmentDef};
use zhbo_core::units::inches;
use zhbo_core::weapons::{CamoOption, Class, Effect, FireMode, GunKick, Kick, Loadout, Place, Reticle, Specialties, Sway, Table, ViewPos, WeaponDef, ALT_CLIPS, MELEE_DAMAGE};
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

/// The replacement optics whose unique supplies a combo's 2D ADS overlay: ACOG (1), IR/dual-band
/// (14), IS (15), VZOOM (29). The base-parts cycle probed these kinds; the set belongs to the
/// converter, not the parser.
const REPLACEMENT_OPTIC_KINDS: [u32; 4] = [1, 14, 15, 29];

/// The overlay fields a combo's mounted replacement optic supplies: the base parsed weapon's
/// `attachment_uniques` for the combo's structural kinds, with the `combined`-mask preference
/// `viewmodels::combo` uses. `None` when the combo mounts no replacement optic (the base def's
/// fields stand); `Some` with empty strings/0 when it mounts one whose unique has no overlay (the
/// physical sight stays).
fn overlay_for(w: &Weapon, shorts: &[&str]) -> Option<(String, String, u32)> {
    let mut mask = 0u32;
    let mut optic: Option<u32> = None;
    for short in shorts {
        let Some((kind, _)) = w.attachment_variants.iter().find(|(_, id)| short_name(id) == *short) else { continue };
        mask |= 1 << kind;
        if REPLACEMENT_OPTIC_KINDS.contains(kind) && optic.is_none() {
            optic = Some(*kind);
        }
    }
    let optic = optic?;
    let exact = (shorts.len() > 1).then(|| w.attachment_uniques.iter().find(|u| u.combined == mask)).flatten();
    let unique = exact
        .filter(|u| REPLACEMENT_OPTIC_KINDS.contains(&u.kind))
        .or_else(|| w.attachment_uniques.iter().find(|u| u.kind == optic && u.combined == 0));
    Some(match unique {
        Some(u) => (u.overlay_material.clone(), u.overlay_material_low_res.clone(), u.overlay_reticle),
        None => (String::new(), String::new(), 0),
    })
}

/// The attachment's metadata from BO2's tables: the full name and description keys' localized text,
/// and its attribute-table deltas.
#[derive(Clone, Debug, Default)]
pub struct AttachmentMeta {
    pub full_name: String,
    pub description: String,
    pub attribute_deltas: [f32; 5],
}

/// BO2's attachment metadata from the player's `mp/attachmenttable.csv` (`patch_mp`) and
/// `mp/attributestable.csv` (`patch_ui_mp`). The attachment table gives each short name's display
/// and description keys; the attribute table gives its stat-bar deltas. Missing rows are omitted.
pub fn attachment_metadata(
    patch: &zhbo_formats::t6zone::Zone,
    patch_ui: &zhbo_formats::t6zone::Zone,
    names: &HashMap<String, String>,
) -> HashMap<String, AttachmentMeta> {
    let mut meta = HashMap::new();
    let Ok(Some(rows)) = zhbo_formats::stringtable::rows(patch, "mp/attachmenttable.csv") else { return meta };
    for row in rows {
        // Column 4 is the display name key, column 5 the short name, column 8 the description key.
        let (Some(short), Some(display_key), Some(desc_key)) = (row.get(4), row.get(3), row.get(7)) else { continue };
        let full_name = names.get(display_key).map(|s| s.trim().to_string()).unwrap_or_default();
        let description = names.get(desc_key).map(|s| s.trim().to_string()).unwrap_or_default();
        meta.insert(short.clone(), AttachmentMeta { full_name, description, attribute_deltas: [0.0; 5] });
    }
    // The attribute table's Lua constants are 0-based: damage = col 3, range = col 4,
    // accuracy = col 2, mobility = col 6, fire rate = col 5.
    let Ok(Some(rows)) = zhbo_formats::stringtable::rows(patch_ui, "mp/attributestable.csv") else { return meta };
    for row in rows {
        if row.first().map(String::as_str) != Some("weapon_attachment") { continue; }
        let Some(short) = row.get(1) else { continue };
        let num = |i: usize| row.get(i).and_then(|s| s.trim().parse::<f32>().ok()).unwrap_or(0.0);
        let deltas = [num(3), num(4), num(2), num(6), num(5)];
        meta.entry(short.clone()).or_default().attribute_deltas = deltas;
    }
    meta
}

/// The core's definition of a BO2 attachment. `names` maps localize keys to text.
pub fn attachment_def(a: &bo2::Attachment, names: &HashMap<String, String>, meta: &HashMap<String, AttachmentMeta>) -> AttachmentDef {
    let short = short_name(&a.name);
    let m = meta.get(&short).cloned().unwrap_or_default();
    let fire = a.fire_type.map(|f| match f {
        bo2::fire::FULLAUTO => FireMode::Auto,
        bo2::fire::BURST2 | bo2::fire::BURST3 | bo2::fire::BURST4 | bo2::fire::BURST5 => FireMode::Burst((f - bo2::fire::BURST2 + 2) as u8),
        _ => FireMode::Semi,
    });
    AttachmentDef {
        id: a.name.clone(),
        short: short.clone(),
        display_name: names.get(&a.display_name).map(|s| s.trim().to_string()).unwrap_or_else(|| short.clone()),
        full_name: m.full_name,
        description: m.description,
        attribute_deltas: m.attribute_deltas,
        point: a.point as u8,
        silenced: a.silenced,
        laser: a.laser_sight,
        fire,
        clip: if fire.is_some() { a.clip_size.max(0) as u16 } else { 0 },
        damage_range_scale: a.damage_range_scale,
        ads_fov: if a.ads_zoom_fov > 1.0 { a.ads_zoom_fov } else { 0.0 },
        ads_in_scale: a.ads_in_scale,
        ads_out_scale: a.ads_out_scale,
        ads_move_scale: a.ads_move_scale,
        hip_spread_min_scale: a.hip_spread_min_scale,
        hip_spread_max_scale: a.hip_spread_max_scale,
        fire_time_scale: a.fire_time_scale,
        reload_scale: a.reload_scale,
        reload_empty_scale: a.reload_empty_scale,
        reload_add_scale: a.reload_add_scale,
        clip_scale: a.clip_size_scale,
        recoil_reduction: a.recoil_reduction,
        specialties: Specialties::from_words(a.perks.0, a.perks.1),
    }
}

/// BO2's perk rows from the player's `mp/statstable.csv` (`patch_mp`): each `specialty` row's
/// reference (column d, `PERKS_LIGHTWEIGHT` -> `LIGHTWEIGHT`) and the specialties it grants
/// (column e, `|`-separated). Rows with a specialty the core does not know are skipped (Ghidra
/// G1's index table names all 62).
pub fn perk_rows(patch: &zhbo_formats::t6zone::Zone) -> Vec<(String, Specialties)> {
    let Ok(Some(rows)) = zhbo_formats::stringtable::rows(patch, "mp/statstable.csv") else { return Vec::new() };
    rows.iter()
        .filter(|r| r.get(2).is_some_and(|c| c == "specialty"))
        .filter_map(|r| {
            let stats = r.get(3)?.strip_prefix("PERKS_")?.to_string();
            let set = Specialties::parse(r.get(4)?).ok()?;
            (!set.is_empty()).then_some((stats, set))
        })
        .collect()
}

/// BO2's base camo cells: `camo_none` plus 18 (`lua-dump/patch_mp/mp/attachmenttable.csv:32-50`).
/// The DLC/MTX rows (`:51-129`) are separate tabs the class screen does not have yet.
pub const BASE_CAMO_OPTIONS: usize = 19;

/// BO2's base reticle cells (`mp/attachmenttable.csv:57-67`: `reticle_0..10`); the DLC/MTX rows
/// (`:87-119`) are separate tabs.
pub const BASE_RETICLE_OPTIONS: usize = 11;

/// BO2's reticle colours (`mp/attachmenttable.csv:68-74`: `reticle_color_1..7`).
pub const BASE_RETICLE_COLORS: usize = 7;

/// The `weaponoption` rows of one group in `mp/attachmenttable.csv` (`patch_mp`), in table order.
fn weapon_options(patch: &zhbo_formats::t6zone::Zone, names: &HashMap<String, String>, group: &str, take: usize) -> Vec<CamoOption> {
    let Ok(Some(rows)) = zhbo_formats::stringtable::rows(patch, "mp/attachmenttable.csv") else { return Vec::new() };
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
            Some(CamoOption { id: id.to_string(), name, image, sort })
        })
        .collect()
}

/// BO2's camo options: every `camo` row (`mp/attachmenttable.csv:32-129`), `camo_none` first.
/// Column 4 is the id, 3 the `MPUI_*` name, 6 the `menu_camo_*_pattern` thumbnail and 5 the sort.
pub fn camo_options(patch: &zhbo_formats::t6zone::Zone, names: &HashMap<String, String>) -> Vec<CamoOption> {
    weapon_options(patch, names, "camo", usize::MAX)
}

/// BO2's base reticle options (`reticle_0..10`), in the table's order. The art is the optic's
/// `mtl_t6_attach_optic_{reflex,holo,acog}_reticle_XX` image (`cacreticles.lua`).
pub fn reticle_options(patch: &zhbo_formats::t6zone::Zone, names: &HashMap<String, String>) -> Vec<CamoOption> {
    weapon_options(patch, names, "reticle", BASE_RETICLE_OPTIONS)
}

/// BO2's reticle colours (`reticle_color_1..7`): the names are Red, Green, Blue, Purple, Teal,
/// Yellow, Orange (`MPUI_RETICLE_COLOR_*`).
pub fn reticle_color_options(patch: &zhbo_formats::t6zone::Zone, names: &HashMap<String, String>) -> Vec<CamoOption> {
    weapon_options(patch, names, "reticle_color", BASE_RETICLE_COLORS)
}

/// The `camo_<weapon>` asset a weapon's camo set uses. BO2's asset names shorten a few weapon
/// names (`saiga12` -> `camo_saiga`, `scar` -> `camo_scarh`, `qbb95` -> `camo_qbb`,
/// `evoskorpion` -> `camo_scorpionevo`, the held knife -> `camo_knife`); the rest are
/// `camo_<base>`. A weapon BO2 shipped no set for (some DLC guns) gets none.
pub fn camo_material_for(weapon: &str, camo_sets: &HashSet<String>) -> Option<String> {
    let base = weapon.strip_suffix("_mp").unwrap_or(weapon);
    let base = base.strip_prefix("sf_").or_else(|| base.strip_prefix("gl_")).unwrap_or(base);
    let base = base.strip_suffix("_dw").or_else(|| base.strip_suffix("_lh")).unwrap_or(base);
    let name = match base {
        "saiga12" => "camo_saiga",
        "scar" => "camo_scarh",
        "qbb95" => "camo_qbb",
        "evoskorpion" => "camo_scorpionevo",
        "knife_held" => "camo_knife",
        "riotshield" => "camo_riot_shield",
        _ => {
            let name = format!("camo_{base}");
            return camo_sets.contains(&name).then_some(name);
        }
    };
    camo_sets.contains(name).then(|| name.to_string())
}

/// The option tables the converter feeds the weapon table: BO2's camo rows, each weapon's
/// `camo_<weapon>` set (`common_patch_mp`), and the base reticle/colour rows.
pub struct OptionTables<'a> {
    pub camo: &'a [CamoOption],
    pub camo_sets: &'a HashSet<String>,
    pub reticles: &'a [CamoOption],
    pub reticle_colors: &'a [CamoOption],
}

/// The whole table: every player weapon, the alt modes their attachments add, and every
/// attachment.
pub fn table(bo2: &[Weapon], atts: &[bo2::Attachment], names: &HashMap<String, String>) -> Table {
    let sets = HashSet::new();
    let opts = OptionTables { camo: &[], camo_sets: &sets, reticles: &[], reticle_colors: &[] };
    table_with_metadata(bo2, atts, names, &HashMap::new(), &opts)
}

/// The whole table with attachment metadata (full name, description and attribute-table deltas)
/// and the camo/reticle/colour option tables.
pub fn table_with_metadata(
    bo2: &[Weapon],
    atts: &[bo2::Attachment],
    names: &HashMap<String, String>,
    meta: &HashMap<String, AttachmentMeta>,
    opts: &OptionTables,
) -> Table {
    let mut weapons: Vec<WeaponDef> = bo2.iter().filter(|w| is_player_weapon(w)).map(|w| to_def(w, names)).collect();
    let mut extra: Vec<String> = Vec::new();
    for d in &weapons {
        extra.extend(d.alt_modes.iter().map(|(_, alt)| alt.clone()));
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
                // BO2 counts the alt launcher's grenades as rounds, not magazines.
                if d.alt_own_ammo {
                    d.start_reserve = (w.start_ammo - w.clip_size).max(0) as u16;
                    d.max_reserve = ((w.max_ammo - w.clip_size).max(0) as u16).max(d.start_reserve);
                }
            }
            weapons.push(d);
        }
    }
    let attachments = atts.iter().filter(|a| a.kind != 0 && !a.name.starts_with(',')).map(|a| attachment_def(a, names, meta)).collect();
    for d in &mut weapons {
        d.camo = opts.camo.iter().take(BASE_CAMO_OPTIONS).map(|c| c.id.clone()).collect();
        d.camo_material = camo_material_for(&d.id, opts.camo_sets).unwrap_or_default();
    }
    Table {
        weapons,
        attachments,
        camo: opts.camo.to_vec(),
        reticles: opts.reticles.to_vec(),
        reticle_colors: opts.reticle_colors.to_vec(),
        tablet_meshes: 0,
        perks: Vec::new(),
    }
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
/// base defs by the core's attachment rule, with the overlay of the mounted optic.
fn build_variant(table: &Table, id: &str, bo2: &[Weapon]) -> Option<(WeaponDef, Option<WeaponDef>)> {
    let (base_id, shorts) = attachments::split_id(id);
    let base = table.weapons.iter().find(|d| d.id == base_id)?;
    let atts = attachments::resolve(base, &shorts, |a| table.attachments.iter().find(|x| x.id == a));
    let mut d = attachments::apply(base, &atts);
    let combo_shorts: Vec<&str> = d.attachments.iter().map(String::as_str).collect();
    if let Some((overlay_material, overlay_material_low_res, overlay_reticle)) =
        bo2.iter().find(|w| w.name == base_id).and_then(|w| overlay_for(w, &combo_shorts))
    {
        d.overlay_material = overlay_material;
        d.overlay_material_low_res = overlay_material_low_res;
        d.overlay_reticle = overlay_reticle;
    }
    let mut alt_out = None;
    if let Some((short, alt_id)) = base.alt_modes.iter().find(|(short, _)| d.attachments.contains(short)) {
        if let Some(alt_base) = table.weapons.iter().find(|w| &w.id == alt_id) {
            // Select fire and the second optic keep the weapon's other attachments.
            let others: Vec<&AttachmentDef> = atts.iter().copied().filter(|a| &a.short != short).collect();
            let mut alt = if alt_base.alt_own_ammo { alt_base.clone() } else { attachments::apply(alt_base, &others) };
            let alt_shorts: Vec<&str> = alt.attachments.iter().map(String::as_str).collect();
            if let Some((overlay_material, overlay_material_low_res, overlay_reticle)) =
                bo2.iter().find(|w| &w.name == alt_id).and_then(|w| overlay_for(w, &alt_shorts))
            {
                alt.overlay_material = overlay_material;
                alt.overlay_material_low_res = overlay_material_low_res;
                alt.overlay_reticle = overlay_reticle;
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

/// The alt mode entry of a converted weapon, if it has one.
pub fn alt_of<'a>(table: &'a Table, id: &str) -> Option<&'a WeaponDef> {
    let d = table.weapons.iter().find(|d| d.id == id)?;
    table.weapons.iter().find(|a| !d.alt.is_empty() && a.id == d.alt)
}

/// Mark alt-mode clips on a viewmodel: an alt mode's clips sit after its weapon's.
pub fn alt_clip(slot: usize) -> usize {
    slot + ALT_CLIPS as usize
}

/// BO2's base aim-down-sights walking speed, as a share of the normal walk: one definition in the
/// core, so the converter's base build and the core's attachment replacement walk agree.
pub(crate) use zhbo_core::weapons::ADS_MOVE_BASE;

/// Where the weapon table goes in the archive (the bridge hands it to the core).
pub use zhbo_core::names::WEAPON_TABLE_PATH as TABLE_PATH;

/// BO2's localized strings for the display names: `zone/<language>/<lang>_code_post_gfx_mp.ff`
/// with `<lang>_patch_mp.ff` over it. Empty if the language zones can't be read.
pub fn names(bo2_dir: &std::path::Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for (dir, prefix) in [("english", "en")] {
        for zone in ["code_post_gfx_mp", "patch_mp"] {
            let path = bo2_dir.join("zone").join(dir).join(format!("{prefix}_{zone}.ff"));
            let Ok(file) = std::fs::read(&path) else { continue };
            let Ok(ff) = zhbo_formats::fastfile::read(&file) else { continue };
            let Ok(z) = zhbo_formats::t6zone::load(&ff) else { continue };
            out.extend(zhbo_formats::localize::entries(&z).unwrap_or_default());
        }
    }
    out
}

/// Sound events and Zero Hour FX weapons for the converted weapons. Each gun gets its own player
/// gunshot (BO2's `_plr` alias with its tail and low end) and third-person gunshot, played by its
/// own FX weapon (`ZHBO_FX_<id>`); the loadout's offhands and knife supply the throw, pin, blast and
/// swing sounds. Sets each definition's `fire_sound` and `fx_weapon`; returns the events and the
/// FX weapon INI.
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
            let npc_event = format!("ZHBO_BO2_FireNpc_{key}");
            if let Some(npc) = &c.fire_sound {
                events.push(DynSfx { event: npc_event.clone(), layers: layers(npc), kind: Kind::Others, limit: 3 });
            }
            let fx = format!("ZHBO_FX_{key}");
            let launcher = d.class == Class::Launcher;
            ini.push_str(&format!(
                "Weapon {fx}\n  PrimaryDamage           = 0.0\n  PrimaryDamageRadius     = 0.0\n  AttackRange             = 1000.0\n  \
                 DamageType              = SMALL_ARMS\n  DeathType               = NORMAL\n  WeaponSpeed             = 999999.0\n  \
                 ProjectileObject        = NONE\n  FireFX                  = {}\n  FireSound               = {npc_event}\n  \
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
                let event = format!("ZHBO_BO2_FireNpc_{key_sil}");
                events.push(DynSfx { event: event.clone(), layers: layers(&npc), kind: Kind::Others, limit: 3 });
                d.silenced_fire_sound_npc = event;
            }
            let fx_sil = format!("ZHBO_FX_{key_sil}");
            ini.push_str(&format!(
                "Weapon {fx_sil}\n  PrimaryDamage           = 0.0\n  PrimaryDamageRadius     = 0.0\n  AttackRange             = 1000.0\n  \
                 DamageType              = SMALL_ARMS\n  DeathType               = NORMAL\n  WeaponSpeed             = 999999.0\n  \
                 ProjectileObject        = NONE\n  FireFX                  = NONE\n  FireSound               = {}\n  \
                 RadiusDamageAffects     = ENEMIES\n  DelayBetweenShots       = 0\n  ClipSize                = 0\nEnd\n\n",
                d.silenced_fire_sound_npc
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
            events.extend(general(zhbo_core::sounds::PULL_PIN, &w.pullback_sound_player, Kind::Player));
            events.extend(general(zhbo_core::sounds::EXPLOSION, &w.projectile_explosion_sound, Kind::World));
        }
        if *id == l.melee {
            events.extend(general(zhbo_core::sounds::MELEE_SWING, &w.melee_swipe_sound_player, Kind::Player));
        }
    }
    // The def-level sounds (raise, putaway, ADS zoom, melee, fire start/last, pull-back, blast):
    // one event per BO2 alias any def names, shared by every def that names it.
    let mut def_events = BTreeSet::new();
    for d in defs.iter() {
        for (event, world) in d.def_sound_events() {
            if let Some(alias) = zhbo_core::names::def_sound_alias(event) {
                if def_events.insert(event.to_string()) {
                    let kind = if world { Kind::World } else { Kind::Player };
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

/// `ids` and the alt modes they reach: each one's current alt (`d.alt`) and every alt weapon its
/// attachments add (`alt_modes`), each once, in first-seen order.
pub fn with_alt_modes(defs: &[WeaponDef], ids: &[String]) -> Vec<String> {
    let mut out: Vec<String> = ids.to_vec();
    for id in ids {
        let Some(d) = defs.iter().find(|d| &d.id == id) else { continue };
        let alts = std::iter::once(&d.alt).chain(d.alt_modes.iter().map(|(_, alt)| alt));
        for alt in alts.filter(|a| !a.is_empty()) {
            if !out.contains(alt) {
                out.push(alt.clone());
            }
        }
    }
    out
}

/// A weapon a player can carry in BO2 multiplayer (not killstreak, turret or alt-mode entries).
pub fn is_player_weapon(w: &Weapon) -> bool {
    use manifest::{EXCLUDED_WEAPONS, LETHALS, TACTICALS};
    if w.name == "knife_held_mp" {
        return true;
    }
    if w.name.starts_with("remote_") || w.name.contains("killstreak") || w.name == "knife_mp" {
        return false;
    }
    if LETHALS.contains(&w.name.as_str()) || TACTICALS.contains(&w.name.as_str()) {
        return true;
    }
    w.is_gun() && !w.name.contains("wager") && !w.name.starts_with("inventory_") && w.view_model.is_some() && !EXCLUDED_WEAPONS.contains(&w.name.as_str())
}

fn class_of(w: &Weapon) -> Class {
    use manifest::{LETHALS, TACTICALS};
    let name = w.name.as_str();
    if LETHALS.contains(&name) {
        return if matches!(name, "bouncingbetty_mp" | "claymore_mp" | "satchel_charge_mp") { Class::Equipment } else { Class::Lethal };
    }
    if TACTICALS.contains(&name) {
        return if matches!(name, "trophy_system_mp" | "tactical_insertion_mp" | "pda_hack_mp") { Class::Equipment } else { Class::Tactical };
    }
    if w.weap_type == bo2::kind::MELEE {
        return Class::Melee;
    }
    if w.view_model.as_deref().is_some_and(|m| m.contains("_sniper_")) {
        return Class::Sniper;
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
        bo2::fire::FULLAUTO | bo2::fire::STACKED => FireMode::Auto,
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

pub fn to_def(w: &Weapon, names: &HashMap<String, String>) -> WeaponDef {
    let class = class_of(w);
    let clip = w.clip_size.max(0) as u16;
    let (start_reserve, max_reserve) = if class.is_offhand() {
        // Offhands: how many you spawn with, and carry at most.
        (w.start_ammo.max(1) as u16, w.max_ammo.max(w.start_ammo).max(1) as u16)
    } else if w.ammo_clip_relative || (w.start_ammo > 0 && w.start_ammo < w.clip_size) {
        // BO2 multiplayer counts gun ammo in magazines (the loaded one included): the AN-94's 4/8
        // is 30/90 at spawn and 210 spare at most.
        (((w.start_ammo - 1).max(0) * w.clip_size) as u16, ((w.max_ammo - 1).max(0) * w.clip_size) as u16)
    } else {
        ((w.start_ammo - w.clip_size).max(0) as u16, (w.max_ammo - w.clip_size).max(0) as u16)
    };
    let range = |r: f32| if r >= 999_999.0 { inches(100_000.0) } else { inches(r) };
    let key = manifest::display_name_key(w.display_name.trim_start_matches("WEAPON_"));
    let display_name = names
        .get(key)
        .or_else(|| names.get(&format!("WEAPON_{key}")))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| w.name.trim_end_matches("_mp").to_uppercase());
    let effect = effect_of(w);
    let sticky = manifest::STICKY.contains(&w.name.as_str());
    let mut damage = [0.0; 6];
    let mut damage_range = [0.0; 6];
    for i in 0..6 {
        damage[i] = w.damage[i] as f32;
        damage_range[i] = if w.damage_range[i] > 0.0 { range(w.damage_range[i]) } else { 0.0 };
    }
    let mult = |i: usize| w.location_multipliers.get(i).copied().filter(|m| *m > 0.0).unwrap_or(1.0);
    let shells = w.reload_start_time > 0.0 && w.reload_ammo_add > 0;
    let d = WeaponDef {
        id: w.name.clone(),
        equipment_kind: manifest::equipment_kind_of(&w.name),
        display_name,
        class,
        fire: fire_mode(w),
        fire_time: w.fire_time.max(0.01),
        burst_delay: w.burst_delay,
        spin_up_time: w.spin_up_time,
        pellets: w.shot_count.max(1) as u16,
        clip,
        start_reserve,
        max_reserve: max_reserve.max(start_reserve),
        reload_time: w.reload_time,
        reload_empty_time: if w.reload_empty_time > 0.0 { w.reload_empty_time } else { w.reload_time },
        reload_add_time: w.reload_add_time,
        reload_empty_add_time: if w.reload_empty_add_time > 0.0 { w.reload_empty_add_time } else { w.reload_add_time },
        reload_start_time: if shells { w.reload_start_time } else { 0.0 },
        reload_end_time: if shells { w.reload_end_time } else { 0.0 },
        reload_ammo_add: if shells { w.reload_ammo_add.max(1) as u16 } else { 0 },
        raise_time: w.raise_time,
        first_raise_time: if w.first_raise_time > 0.0 { w.first_raise_time } else { w.raise_time },
        drop_time: w.drop_time,
        quick_raise_time: if w.quick_raise_time > 0.0 { w.quick_raise_time } else { w.raise_time * 0.6 },
        quick_drop_time: if w.quick_drop_time > 0.0 { w.quick_drop_time } else { w.drop_time * 0.6 },
        sprint_out_time: w.sprint_out_time,
        ads_in_time: w.ads_in_time.max(0.05),
        ads_out_time: w.ads_out_time.max(0.05),
        ads_fov: w.ads_zoom_fov,
        damage,
        damage_range,
        head_multiplier: mult(bo2::HIT_LOCATION_HEAD),
        max_range: if w.max_range > 0.0 { inches(w.max_range) } else { inches(8000.0) },
        hip_spread: [w.hip_spread_stand, w.hip_spread_ducked, w.hip_spread_prone],
        spread_decay: w.hip_spread_decay,
        spread_fire_add: w.hip_spread_fire_add,
        spread_move_add: w.hip_spread_move_add,
        ads_spread: w.ads_spread,
        ads_kick: kick(w.ads_view_kick),
        hip_kick: kick(w.hip_view_kick),
        move_scale: if w.move_speed_scale > 0.0 { w.move_speed_scale } else { 1.0 },
        // BO2's adsMoveSpeedScale multiplies the base ADS walk speed (rifles 1, LMGs 0.8, SMGs,
        // pistols and shotguns 2); the base itself is engine tuning, about a third of a walk.
        ads_move_scale: (ADS_MOVE_BASE * if w.ads_move_speed_scale > 0.0 { w.ads_move_speed_scale } else { 1.0 }).min(1.0),
        sprint_duration_scale: if w.sprint_duration_scale > 0.0 { w.sprint_duration_scale } else { 1.0 },
        melee_damage: if w.melee_damage > 0 { w.melee_damage as f32 } else { MELEE_DAMAGE },
        melee_time: if w.melee_time > 0.0 { w.melee_time } else { 0.6 },
        effect,
        fuse_time: w.fuse_time,
        cook: w.cook_off_hold,
        explosion_radius: inches(w.explosion_radius as f32),
        explosion_radius_min: inches(w.explosion_radius_min as f32),
        explosion_inner_damage: w.explosion_inner_damage as f32,
        explosion_outer_damage: w.explosion_outer_damage as f32,
        projectile_speed: inches(w.projectile_speed as f32),
        projectile_speed_up: inches(w.projectile_speed_up as f32),
        projectile_gravity: w.class != bo2::class::ROCKETLAUNCHER,
        impact_explode: w.projectile_impact_explode,
        // BO2's impact bucket: which `FxImpactTable` entry this weapon's hits use (R8d).
        impact_type: w.impact_type as i32,
        sticky,
        fx_weapon: String::new(),
        view_flash_fx: String::new(),
        view_shell_fx: String::new(),
        view_last_shot_shell_fx: String::new(),
        explosion_fx: match effect {
            Effect::Explode => "FX_BoobyTrapExplosion",
            Effect::Stun => "WeaponFX_RangerFlashBangGrenadeDetonation",
            Effect::Emp => "WeaponFX_EMPPulseImpact",
            Effect::Smoke => "FX_OverlordSmoke",
            Effect::Impact | Effect::None => "",
        }
        .to_string(),
        projectile_object: String::new(),
        // A gun's shot is its own `ZHBO_BO2_Fire_<id>` mix (`sounds_and_fx`); an offhand's throw is
        // its def-level alias.
        fire_sound: if class.is_offhand() { def_sound(&w.fire_sound_player) } else { String::new() },
        empty_fire_sound: String::new(),
        silenced_fire_sound: String::new(),
        silenced_fire_sound_npc: String::new(),
        silenced_fx_weapon: String::new(),
        silenced_view_flash_fx: String::new(),
        explode_sound: def_sound(&w.projectile_explosion_sound),
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
        silenced_fire_start_sound: String::new(),
        attachment_fire_sounds: Vec::new(),
        attachment_fire_start_sounds: Vec::new(),
        viewmodel: -1,
        anims: w.anims.iter().enumerate().take(128).filter(|(_, a)| a.is_some()).fold(0u128, |m, (i, _)| m | (1u128 << i)),
        clip_base: 0,
        world_meshes: 0,
        attachments: Vec::new(),
        allowed: w.attachment_variants.iter().map(|(_, id)| (short_name(id), id.clone())).collect(),
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
        silenced: w.silenced,
        laser: false,
        fmj: false,
        target_finder: false,
        recoil_reduction: (0.0, 0.0),
        place: manifest::place_of(&w.name),
        specialties: Specialties::from_words(w.perks.0, w.perks.1),
        view_kick_min: w.view_kick_min,
        view_kick_center: w.view_kick_center,
        ads_gun_kick: gun_kick(w.ads_gun_kick),
        hip_gun_kick: gun_kick(w.hip_gun_kick),
        gun_max: w.gun_max,
        recoil_return_rate: w.recoil.2,
        hip_sway: sway(w.hip_sway),
        // BO2's aimed sway has no horizontal/vertical scales of its own (G4): the hip ones apply.
        ads_sway: Sway { horiz_scale: w.hip_sway.horiz_scale, vert_scale: w.hip_sway.vert_scale, ..sway(w.ads_sway) },
        view_pos: view_pos(w.view_pos),
        reticle: reticle(&w.reticle),
        camo: Vec::new(),
        camo_material: String::new(),
        overlay_material: w.overlay_material.clone(),
        overlay_material_low_res: w.overlay_material_low_res.clone(),
        overlay_reticle: w.overlay_reticle,
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
    d.damage[0] * RTS_DAMAGE * d.pellets.max(1) as f32
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
        set(weapon, "FireSound", format!("ZHBO_BO2_FireNpc_{key}"));
    }
    // The third slot only stands in for the throw (zhbo-core throws the BO2 grenade).
    set("ZHBO_GeneralGrenade", "FireSound", zhbo_core::sounds::THROW.to_string());
    out
}

/// How far a thrown offhand flies at the speed zhbo-core throws it with (`Sim::rts_update`): the
/// farthest throw, at 45 degrees.
pub fn throw_range(d: &WeaponDef) -> f32 {
    let speed = if d.projectile_speed > 0.0 { d.projectile_speed } else { inches(800.0) };
    speed * speed / zhbo_core::projectiles::GRAVITY
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
            let npc = format!("ZHBO_BO2_FireNpc_{}", d.id.replace(['+', '~'], "_"));
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
            let sound = if has_event(zhbo_core::sounds::MELEE_SWING) { zhbo_core::sounds::MELEE_SWING } else { "NONE" };
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
pub fn converted_base(all: &[Weapon]) -> Vec<String> {
    let l = Loadout::default();
    let mut ids = vec![l.primary, l.secondary, l.lethal, l.tactical, l.melee];
    // Every weapon's art, so any of them can be picked up, bought or swapped to; `=0` converts only
    // the loadout's (quicker while developing).
    if !std::env::var("ZHBO_CONVERT_ALL_WEAPONS").is_ok_and(|v| v == "0") {
        for w in all.iter().filter(|w| is_player_weapon(w)) {
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
/// `the_enumerator_matches_the_conformance_expected_rows` pins the two together.
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
            let key = combo.key();
            let id = if key.is_empty() { w.name.clone() } else { format!("{}+{}", w.name, key) };
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
    let mut ids = converted_base(all);
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

    #[test]
    fn maps_bo2_types_to_classes() {
        let rifle = Weapon { name: "an94_mp".into(), class: bo2::class::RIFLE, view_model: Some("t6_wpn_ar_an94_view".into()), ..Default::default() };
        assert_eq!(class_of(&rifle), Class::Rifle);
        let sniper = Weapon { name: "dsr50_mp".into(), class: bo2::class::RIFLE, view_model: Some("t6_wpn_sniper_dsr50_view".into()), ..Default::default() };
        assert_eq!(class_of(&sniper), Class::Sniper);
        let frag = Weapon { name: "frag_grenade_mp".into(), weap_type: bo2::kind::GRENADE, ..Default::default() };
        assert_eq!((class_of(&frag), effect_of(&frag)), (Class::Lethal, Effect::Explode));
        let conc = Weapon { name: "concussion_grenade_mp".into(), weap_type: bo2::kind::GRENADE, ..Default::default() };
        assert_eq!((class_of(&conc), effect_of(&conc)), (Class::Tactical, Effect::Stun));
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

    #[test]
    fn clip_relative_ammo_counts_magazines() {
        let w = Weapon { name: "an94_mp".into(), clip_size: 30, start_ammo: 4, max_ammo: 8, ammo_clip_relative: false, fire_time: 0.096, ..Default::default() };
        let d = to_def(&w, &HashMap::new());
        assert_eq!((d.clip, d.start_reserve, d.max_reserve), (30, 90, 210));
        assert_eq!(d.display_name, "AN94");
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
        let fmj = bo2::AttachmentUnique { name: "au_as50_fmj".into(), kind: 10, ..Default::default() };
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
                AttachmentDef { id: "silencer".into(), short: "silencer".into(), point: 2, silenced: true, ..Default::default() },
                AttachmentDef { id: "grip".into(), short: "grip".into(), point: 3, ..Default::default() },
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
        let att = |short: &str, silenced| zhbo_core::attachments::AttachmentDef { short: short.into(), silenced, point: 0, ..Default::default() };
        let (sf, sil) = (att("sf", false), att("silencer", true));
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

    #[test]
    fn a_combined_unique_wins_the_overlay() {
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
                AttachmentDef { id: "ir".into(), short: "ir".into(), point: 1, ..Default::default() },
                AttachmentDef { id: "swayreduc".into(), short: "swayreduc".into(), ..Default::default() },
            ],
            ..Default::default()
        };
        add_variants(&mut table, &["as50_mp+swayreduc+ir".into()], std::slice::from_ref(&w));
        let d = table.weapons.iter().find(|d| d.id == "as50_mp+ir+swayreduc").expect("the combined combo");
        assert_eq!(d.overlay_material, "scope_overlay_ir_combined", "the exact combined unique wins");
        assert_eq!(d.overlay_reticle, 3);
    }

    /// The converter's legal-combo set must equal the conformance expected set: the mechanical
    /// agreement between `all_legal_combos` and BO2's own CaC rows. The variant's `_mp` suffix is
    /// stripped so `an94_mp+reflex` compares to the conformance `item` `an94+reflex`.
    /// This is a parity-gap detector, not a unit test: it lives in the conformance family's
    /// red-by-design set (the blocking suite runs `--skip conformance`). Its two known gaps are
    /// tracked cycles: the alt-weapon combo bases (`dualoptic_*`/`sf_*` families, 2,379 rows —
    /// BO2 offers the alt weapons as their own attachment bases; the mod's enumeration covers
    /// only base weapons) and the dual-wield representation (`beretta93r_dw_mp`-style separate
    /// weapons vs the mod's `+dw` attachment, 5 rows — the N3 item).
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn conformance_the_enumerator_matches_the_expected_rows() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let common = crate::generals::load_zone(&dir, "common_mp").unwrap();
        let patch = crate::generals::load_zone(&dir, crate::fx::ZONE).unwrap();
        let all = zhbo_formats::weapon::all(&common).unwrap();
        let ours: std::collections::BTreeSet<String> = all_legal_combos(&patch, &all)
            .into_iter()
            .map(|id| {
                let (base, rest) = id.split_once('+').map(|(b, r)| (b, Some(r))).unwrap_or((&id, None));
                let base = base.strip_suffix("_mp").unwrap_or(base);
                match rest {
                    Some(r) => format!("{base}+{r}"),
                    None => base.to_string(),
                }
            })
            .collect();
        let expected = zhbo_conformance::bo2::expected(&dir).unwrap().0;
        let theirs: std::collections::BTreeSet<String> = expected
            .iter()
            .map(|r| {
                let base = r.key.0.strip_suffix("_mp").unwrap_or(r.key.0.as_str());
                if r.key.1.is_empty() { base.to_string() } else { format!("{base}+{}", r.key.1) }
            })
            .collect();
        assert_eq!(ours, theirs, "the converter's legal-combo set must equal the conformance expected set");
    }

    /// The full set includes every statstable-backed base id and at least one combination per base
    /// weapon that has attachments. A base id the Create-a-Class enumerator does not offer (the
    /// loadout's melee `knife_held_mp`, a table row rather than a CaC item) is excluded by the same
    /// `items` filter the enumerator uses. The plan's §7.2 direction: every legal combo is in the
    /// full `converted()` set.
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_legal_combo_set_covers_every_base_weapon() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let common = crate::generals::load_zone(&dir, "common_mp").unwrap();
        let patch = crate::generals::load_zone(&dir, crate::fx::ZONE).unwrap();
        let all = zhbo_formats::weapon::all(&common).unwrap();
        let base = converted_base(&all);
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
                AttachmentDef { id: "acog".into(), short: "acog".into(), point: 1, ..Default::default() },
                AttachmentDef { id: "fmj".into(), short: "fmj".into(), ..Default::default() },
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
    fn camo_materials_follow_bo2s_asset_names() {
        let sets: HashSet<String> = ["camo_an94", "camo_saiga", "camo_scarh", "camo_qbb", "camo_scorpionevo", "camo_knife", "camo_riot_shield"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        for (weapon, want) in [
            ("an94_mp", "camo_an94"),
            ("sf_an94_mp", "camo_an94"),
            ("gl_an94_mp", "camo_an94"),
            ("an94_dw_mp", "camo_an94"),
            ("saiga12_mp", "camo_saiga"),
            ("scar_mp", "camo_scarh"),
            ("qbb95_mp", "camo_qbb"),
            ("evoskorpion_mp", "camo_scorpionevo"),
            ("knife_held_mp", "camo_knife"),
            ("riotshield_mp", "camo_riot_shield"),
        ] {
            assert_eq!(camo_material_for(weapon, &sets).as_deref(), Some(want), "{weapon}");
        }
        assert_eq!(camo_material_for("mtar_mp", &sets), None);
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
        assert_eq!(camo_material_for("an94_mp", &sets).as_deref(), Some("camo_an94"));
        assert_eq!(camo_material_for("saiga12_mp", &sets).as_deref(), Some("camo_saiga"));
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_installs_reticle_options_are_bo2s_base_tabs() {
        let bo2_dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let patch = crate::generals::load_zone(&bo2_dir, crate::fx::ZONE).unwrap();
        let names = names(&bo2_dir);
        let reticles = reticle_options(&patch, &names);
        assert_eq!(reticles.len(), BASE_RETICLE_OPTIONS);
        assert_eq!(reticles.first().map(|r| r.id.as_str()), Some("reticle_0"));
        assert!(reticles.iter().all(|r| r.name == "Dot"), "{reticles:?}");
        let colors = reticle_color_options(&patch, &names);
        assert_eq!(
            colors.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["Red", "Green", "Blue", "Purple", "Teal", "Yellow", "Orange"]
        );
    }

    #[test]
    fn the_weapon_camo_list_is_bo2s_base_camo_tab() {
        let camo: Vec<CamoOption> = (0..25)
            .map(|i| CamoOption { id: format!("camo_{i}"), name: String::new(), image: String::new(), sort: 0 })
            .collect();
        let w = Weapon {
            name: "an94_mp".into(),
            class: bo2::class::RIFLE,
            view_model: Some("t6_wpn_ar_an94_view".into()),
            ..Default::default()
        };
        let sets = HashSet::new();
        let opts = OptionTables { camo: &camo, camo_sets: &sets, reticles: &[], reticle_colors: &[] };
        let table = table_with_metadata(&[w], &[], &HashMap::new(), &HashMap::new(), &opts);
        assert_eq!(table.weapons.len(), 1);
        assert_eq!(table.weapons[0].camo.len(), BASE_CAMO_OPTIONS, "the base tab is camo_none + 18");
        assert_eq!(table.camo.len(), 25, "the table keeps every row for the DLC tabs");
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_installs_perk_rows_are_the_built_in_sets_and_weapons_carry_their_bits() {
        use zhbo_core::weapons::{Perk, Specialty};
        let bo2 = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let patch = crate::generals::load_zone(&bo2, crate::fx::ZONE).unwrap();
        let rows = perk_rows(&patch);
        for perk in Perk::ALL {
            let d = perk.def();
            let (_, set) = rows.iter().find(|(k, _)| k == d.stats).unwrap_or_else(|| panic!("statstable has {}", d.stats));
            assert_eq!(*set, Specialties::of(d.specialties), "{}: the built-in set is the install's row", d.id);
        }
        // In this install only FMJ grants specialties through the data (`armorpiercing`,
        // `bulletpenetration`); fast mag and quickdraw work through their reload/ADS time scales.
        let common = crate::generals::load_zone(&bo2, "common_mp").unwrap();
        let atts = bo2::attachments(&common).unwrap();
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
