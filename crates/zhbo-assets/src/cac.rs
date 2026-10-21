//! The create-a-class art pack: perk, wildcard and attachment icons plus the menu chrome, converted
//! from BO2's own `code_post_gfx_mp`/`ui_mp`/`common_mp` materials.
//!
//! The sprite names are the ABI into `zhbo_core::hud::sprites`; the material names are BO2's own
//! (`cac_mods_red_dot`, `menu_mp_bonuscard_perk_1_greed`, ...). Everything here degrades to the
//! report's `missing` list: a name absent from this install never fails the conversion, and the
//! class screen keeps its drawn fallback.

use crate::hud::{Source, Sprite};
use zhbo_core::hud::sprites as names;

const fn mat(name: &'static str, zone: &'static str, material: &'static str, max: u32) -> Sprite {
    Sprite { name, source: Source::Material(zone, material), max }
}

/// BO2's attachment icon material for each catalog attachment short name.
///
/// The catalog's shorts come from the converted weapon table (`weapons::short_name`), so this table
/// is looked up per attachment at conversion time: a new BO2 attachment the mod has no icon for is
/// reported instead of silently dropped. BO2's own UI string table pairs each `MPUI_*` with these
/// materials: `swayreduc` is `cac_mods_bors`, `ir` is `cac_mods_dual_band`, `dualoptic` is
/// `cac_mods_combo_r_a`, and the launcher is the HUD's `hud_grenade_launcher`.
const ATTACHMENTS: &[(&str, &str, &str)] = &[
    ("reflex", "code_post_gfx_mp", "cac_mods_red_dot"),
    ("steadyaim", "code_post_gfx_mp", "cac_mods_laser"),
    ("silencer", "code_post_gfx_mp", "cac_mods_suppressor"),
    ("dualclip", "code_post_gfx_mp", "cac_mods_dual_clip"),
    ("holo", "code_post_gfx_mp", "cac_mods_holographic"),
    ("grip", "code_post_gfx_mp", "cac_mods_foregrip"),
    ("fastads", "code_post_gfx_mp", "cac_mods_pistol_grip"),
    ("fmj", "code_post_gfx_mp", "cac_mods_fmj"),
    ("extbarrel", "code_post_gfx_mp", "cac_mods_barrel_extend"),
    ("rangefinder", "code_post_gfx_mp", "cac_mods_rangefinder"),
    ("stalker", "code_post_gfx_mp", "cac_mods_extended_stock"),
    ("extclip", "code_post_gfx_mp", "cac_mods_extended_mag"),
    ("sf", "code_post_gfx_mp", "cac_mods_trigger_group"),
    ("rf", "code_post_gfx_mp", "cac_mods_rapid_fire"),
    ("mms", "code_post_gfx_mp", "cac_mods_mms"),
    ("acog", "code_post_gfx_mp", "cac_mods_acog"),
    ("dualoptic", "code_post_gfx_mp", "cac_mods_combo_r_a"),
    ("gl", "ui_mp", "hud_grenade_launcher"),
    ("vzoom", "code_post_gfx_mp", "cac_mods_var_zoom"),
    ("ir", "code_post_gfx_mp", "cac_mods_dual_band"),
    ("swayreduc", "code_post_gfx_mp", "cac_mods_bors"),
    ("is", "code_post_gfx_mp", "cac_mods_ironsight"),
    ("tacknife", "code_post_gfx_mp", "cac_mods_tact_knife"),
    ("dw", "code_post_gfx_mp", "cac_mods_dual_wield"),
    ("stackfire", "code_post_gfx_mp", "cac_mods_tribolt"),
];

/// The material (zone, name) for an attachment short, if this pack has its icon.
pub fn attachment_material(short: &str) -> Option<(&'static str, &'static str)> {
    ATTACHMENTS.iter().find(|(s, _, _)| *s == short).map(|(_, zone, material)| (*zone, *material))
}

/// Fixed CaC art: the perk and wildcard icon sets plus the menu chrome. The perks are in
/// `weapons::PERKS` order and the wildcards in `Wildcard::ALL` order (the core names enforce it).
pub const SPRITES: &[Sprite] = &[
    // Perk icons, one per table entry. BO2's internal asset names differ for three perks: Toughness
    // is `perk_warrior`, Extreme Conditioning `perk_marathon`, Dead Silence `perk_ninja`.
    mat(names::PERK_ICONS[0], "code_post_gfx_mp", "perk_lightweight", 64),
    mat(names::PERK_ICONS[1], "code_post_gfx_mp", "perk_hardline", 64),
    mat(names::PERK_ICONS[2], "code_post_gfx_mp", "perk_blind_eye", 64),
    mat(names::PERK_ICONS[3], "code_post_gfx_mp", "perk_flak_jacket", 64),
    mat(names::PERK_ICONS[4], "code_post_gfx_mp", "perk_ghost", 64),
    mat(names::PERK_ICONS[5], "code_post_gfx_mp", "perk_warrior", 64),
    mat(names::PERK_ICONS[6], "code_post_gfx_mp", "perk_cold_blooded", 64),
    mat(names::PERK_ICONS[7], "code_post_gfx_mp", "perk_fast_hands", 64),
    mat(names::PERK_ICONS[8], "code_post_gfx_mp", "perk_hardwired", 64),
    mat(names::PERK_ICONS[9], "code_post_gfx_mp", "perk_scavenger", 64),
    mat(names::PERK_ICONS[10], "code_post_gfx_mp", "perk_dexterity", 64),
    mat(names::PERK_ICONS[11], "code_post_gfx_mp", "perk_marathon", 64),
    mat(names::PERK_ICONS[12], "code_post_gfx_mp", "perk_hacker", 64),
    mat(names::PERK_ICONS[13], "code_post_gfx_mp", "perk_tactical_mask", 64),
    mat(names::PERK_ICONS[14], "code_post_gfx_mp", "perk_ninja", 64),
    mat(names::PERK_ICONS[15], "code_post_gfx_mp", "perk_awareness", 64),
    // The eight wildcards: BO2's bonus-card art. `two_tacticals` is its internal name for Tactician.
    mat(names::WILDCARD_ICONS[0], "common_mp", "menu_mp_bonuscard_perk_1_greed", 96),
    mat(names::WILDCARD_ICONS[1], "common_mp", "menu_mp_bonuscard_perk_2_greed", 96),
    mat(names::WILDCARD_ICONS[2], "common_mp", "menu_mp_bonuscard_perk_3_greed", 96),
    mat(names::WILDCARD_ICONS[3], "common_mp", "menu_mp_bonuscard_overkill", 96),
    mat(names::WILDCARD_ICONS[4], "common_mp", "menu_mp_bonuscard_primary_gunfighter", 96),
    mat(names::WILDCARD_ICONS[5], "common_mp", "menu_mp_bonuscard_secondary_gunfighter", 96),
    mat(names::WILDCARD_ICONS[6], "common_mp", "menu_mp_bonuscard_two_tacticals", 96),
    mat(names::WILDCARD_ICONS[7], "common_mp", "menu_mp_bonuscard_danger_close", 96),
    // Menu chrome: BO2's create-a-class backing strip (a vertical gradient stretched across the
    // screen), the lobby backdrop, its grid/glow, and the row/panel/tile frames the screen draws.
    mat(names::MENU_BACKGROUND, "code_post_gfx_mp", "menu_mp_cac_backing", 128),
    mat(names::MENU_BACKDROP, "ui_mp", "lui_arraybkg", 512),
    mat(names::MENU_GRID, "ui_mp", "menu_mp_cac_grid_pattern", 8),
    mat(names::MENU_GLOW, "ui_mp", "lui_overlay_glow", 128),
    mat(names::ROW_FILL, "ui_mp", "menu_mp_cac_prim_fill", 256),
    mat(names::ROW_HIGHLIGHT, "ui_mp", "menu_mp_cac_prim_hl", 256),
    mat(names::ROW_LINE, "ui_mp", "menu_mp_cac_prim_line", 256),
    mat(names::PANEL_FILL, "ui_mp", "menu_mp_cac_second_fill", 256),
    mat(names::PANEL_HIGHLIGHT, "ui_mp", "menu_mp_cac_second_hl", 256),
    mat(names::PANEL_LINE, "ui_mp", "menu_mp_cac_second_line", 256),
    mat(names::PANEL_EDGE, "ui_mp", "menu_mp_cac_second_wc", 64),
    mat(names::TILE_FILL, "ui_mp", "menu_mp_cac_attach_fill", 96),
    mat(names::TILE_HIGHLIGHT, "ui_mp", "menu_mp_cac_attach_hl", 96),
    mat(names::WILDCARD_FILL, "ui_mp", "menu_mp_cac_wcard_fill", 96),
    mat(names::WILDCARD_HIGHLIGHT, "ui_mp", "menu_mp_cac_wcard_hl", 96),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_table_short_has_one_icon_material() {
        let names: Vec<&str> = names::ATTACHMENT_ICONS.iter().map(|(short, _)| *short).collect();
        let materials: Vec<&str> = ATTACHMENTS.iter().map(|(short, _, _)| *short).collect();
        assert_eq!(names, materials, "the converter table covers exactly the core icon shorts");
        for &(short, zone, material) in ATTACHMENTS {
            assert!(names::attachment_icon(short).is_some(), "{short}");
            assert!(!zone.is_empty() && !material.is_empty(), "{short}");
        }
    }

    #[test]
    fn fixed_sprites_cover_the_core_name_list() {
        let mut spec: Vec<&str> = SPRITES.iter().map(|s| s.name).collect();
        // Attachment icons are made from the catalog at conversion time, and the HUD art belongs to
        // `zhbo_assets::hud::SPRITES`; only the CaC fixed names live in this list.
        let mut fixed: Vec<&str> = names::ALL
            .iter()
            .copied()
            .filter(|n| !n.contains("_Attach_") && !crate::hud::SPRITES.iter().any(|s| s.name == *n))
            .collect();
        spec.sort_unstable();
        fixed.sort_unstable();
        assert_eq!(spec, fixed, "every fixed perk/wildcard/chrome name has a BO2 material");
    }
}
