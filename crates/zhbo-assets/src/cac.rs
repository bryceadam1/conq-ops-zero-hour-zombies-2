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
    Sprite { name, source: Source::Material(zone, std::borrow::Cow::Borrowed(material)), max }
}

/// The largest side of a wildcard card's art in the atlas.
const WILDCARD_MAX: u32 = 96;

/// The wildcard grid's art: each bonus card's image (`mp/statstable.csv` column 6,
/// `menu_mp_bonuscard_*`, a `common_mp` material — `bonuscardbutton.lua` draws
/// `GetItemImage`) under the core's sprite name for the card's rule
/// ([`zhbo_core::hud::sprites::wildcard_icon`]). A card the editor has no rule for has no sprite.
pub fn wildcard_sprites(cards: &[zhbo_core::weapons::BonusCard]) -> Vec<(String, Source, u32)> {
    cards
        .iter()
        .filter(|c| !c.image.is_empty())
        .filter_map(|c| {
            let w = zhbo_core::classmenu::Wildcard::by_reference(&c.reference)?;
            Some((names::wildcard_icon(w).to_string(), Source::Material("common_mp", c.image.clone().into()), WILDCARD_MAX))
        })
        .collect()
}


/// Every item's image material by its reference name (`mp7` -> `menu_mp_weapons_mp7`), from the
/// player's `mp/statstable.csv` in `patch_mp.ff` (column 4 the reference, 6 the image). Empty when
/// the zone or table can't be read: the screens then keep their drawn fallbacks.
pub fn item_images(bo2_dir: &std::path::Path) -> std::collections::HashMap<String, String> {
    item_rows(bo2_dir).into_iter().map(|(reference, (image, _))| (reference, image)).collect()
}

/// [`item_images`] with each item's loadout slot (column 13: `primarygrenade`, `specialgrenade`,
/// …), which decides the picture's size suffix (`_big` for guns, `_256` for offhands).
pub fn item_rows(bo2_dir: &std::path::Path) -> std::collections::HashMap<String, (String, String)> {
    let Ok(zone) = crate::generals::load_zone(bo2_dir, "patch_mp") else { return Default::default() };
    let Ok(Some(table)) = zhbo_formats::stringtable::find(&zone, "mp/statstable.csv") else { return Default::default() };
    table
        .rows
        .iter()
        .filter_map(|r| Some((r.get(4)?.clone(), (r.get(6)?.clone(), r.get(13).cloned().unwrap_or_default()))))
        .filter(|(reference, (image, _))| !reference.is_empty() && !image.is_empty())
        .collect()
}

/// Fixed CaC art: the perk icon set plus the menu chrome. The perks are in `weapons::PERKS` order
/// (the core names enforce it); the wildcards' art comes from their cards ([`wildcard_sprites`]).
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
    fn fixed_sprites_cover_the_core_name_list() {
        let mut spec: Vec<&str> = SPRITES.iter().map(|s| s.name).collect();
        // Attachment icons are made from the catalog and the wildcards from their cards at
        // conversion time, and the HUD art belongs to `zhbo_assets::hud::SPRITES`; only the CaC
        // fixed names live in this list.
        let mut fixed: Vec<&str> = names::ALL
            .iter()
            .copied()
            .filter(|n| !n.contains("_Attach_") && !n.contains("_Wildcard_") && !crate::hud::SPRITES.iter().any(|s| s.name == *n))
            .collect();
        spec.sort_unstable();
        fixed.sort_unstable();
        assert_eq!(spec, fixed, "every fixed perk/chrome name has a BO2 material");
    }

    /// The wildcard art is each card's own image (statstable column 6), one sprite per card under
    /// the core's name for its rule — every name the class screen draws. For BO2's eight cards
    /// (the built-in rows) these are the materials the fixed table used to name.
    #[test]
    fn every_wildcard_draws_its_cards_image() {
        let cards = zhbo_core::weapons::parse_full_table(zhbo_core::weapons::BUILTIN).unwrap().bonus_cards;
        let sprites = wildcard_sprites(&cards);
        let mut got: Vec<&str> = sprites.iter().map(|(n, _, _)| n.as_str()).collect();
        let mut want: Vec<&str> = names::WILDCARD_ICONS.to_vec();
        got.sort_unstable();
        want.sort_unstable();
        assert_eq!(got, want);
        let tactician = sprites.iter().find(|(n, _, _)| n == "ZHBO_BO2_Wildcard_Tactician").unwrap();
        assert!(matches!(&tactician.1, Source::Material("common_mp", m) if m == "menu_mp_bonuscard_two_tacticals"));
        assert!(sprites.iter().all(|(_, _, max)| *max == 96));
    }
}
