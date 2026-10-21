//! The class screens' sound cues, alias for alias from the Lua's own call sites.
//!
//! BO2's menus do not have "a select sound": every widget and handler names the exact alias it
//! plays, and the same alias is reused across screens (`CoD.CACUtility`'s SFX table,
//! `caceditclass.lua`'s select cues, `cacchooseclass.lua`'s list and back-out, `bonuscardbutton.lua`'s
//! per-card equip). The names here are those aliases; the folds in `input.rs` decide which one a
//! frame calls for, and the hosts only play what they are handed, so no screen invents a cue.
//!
//! Every alias is converted from the player's own banks into an AudioEvent `ZHBO_UI_<alias>`
//! (`sounds::UiCue`); a `uin_*` alias that only exists in the single-player banks is left out and
//! the host logs it once, exactly as BO2's multiplayer menus are silent for it.

use crate::classmenu::Wildcard;
use crate::sounds::UiCue;

// ------------------------------------------------------------------------------------------------
// The choose-class screens
// ------------------------------------------------------------------------------------------------

/// A row of the lobby's class list gaining focus (`cacchooseclass.lua`: `classButtonList.gainFocusSFX
/// = "cac_main_nav"`).
pub const MAIN_NAV: &str = UiCue::CacMainNav.event();
/// Choosing a class to edit (`cacchooseclass.lua`'s `ChooseClass` handler: `Engine.PlaySound`).
pub const EDIT_LOADOUT: &str = UiCue::CacMainEditLoadout.event();
/// The choose-class screen backing out (`cacchooseclass.lua`: `setBackOutSFX("cac_main_exit_cac")`).
pub const EXIT_CAC: &str = UiCue::CacMainExitCac.event();
/// Entering create-a-class from the lobby (`gamelobby.lua`: `Engine.PlaySound("cac_enter_cac")`).
pub const ENTER_CAC: &str = UiCue::CacEnterCac.event();
/// The class-set pager (`cacchooseclass.lua`'s `Prev`/`NextClassSet`: `Engine.PlaySound`).
pub const SET_STEP: &str = UiCue::CacSafearea.event();
/// Opening a class-set/class dialog (`popup.lua`'s `OpenPopup`: `Engine.PlaySound`).
pub const DIALOG_OPEN: &str = UiCue::CacLoadoutEditSel.event();
/// The shell's mod-only SAVE row committing a class: BO2 saves on back-out with no cue of its
/// own, so the SAVE row plays a grid button's action (`luibutton.lua`'s `actionSFX`).
pub const SAVE: &str = UiCue::CacGridEquipItem.event();

/// The in-match class panel's entrance (`hud/class.lua` opens the class menu with
/// `Engine.PlaySound("uin_main_pause")`).
pub const IN_GAME_OPEN: &str = UiCue::UinMainPause.event();

// ------------------------------------------------------------------------------------------------
// The editor
// ------------------------------------------------------------------------------------------------

/// A loadout slot gaining focus (`CoD.CACUtility.GainFocusSFX`, set on every
/// `slotlistgridbutton.lua`'s slot list).
pub const LOADOUT_NAV: &str = UiCue::CacLoadoutNav.event();
/// The bonus-card strip gaining focus (`CoD.CACUtility.WildcardGainFocusSFX`).
pub const WILDCARD_NAV: &str = UiCue::CacLoadoutWildcardNav.event();
/// A content grid's focus (`luibutton.lua`/`listbox.lua`'s MP defaults).
pub const GRID_NAV: &str = UiCue::CacGridNav.event();
/// A content grid's action (`luibutton.lua`'s MP `actionSFX`; `caceditclass.lua` equips a grenade
/// with it).
pub const GRID_EQUIP: &str = UiCue::CacGridEquipItem.event();
/// The weapon carousel's sideways, up and down moves and its equip
/// (`cardcarousellist.lua` sets all four from `CoD.CACUtility`).
pub const SLIDE_LR: &str = UiCue::CacSlideNavLr.event();
pub const SLIDE_UP: &str = UiCue::CacSlideNavUp.event();
pub const SLIDE_DOWN: &str = UiCue::CacSlideNavDown.event();
pub const SLIDE_EQUIP: &str = UiCue::CacSlideEquipItem.event();
/// An attachment equipped (`attachmentgridbutton.lua`: `actionSFX = "cac_equipment_add"`).
pub const EQUIPMENT_ADD: &str = UiCue::CacEquipmentAdd.event();
/// An item cleared (`CoD.CACUtility.removeSFX`, the `R Remove` prompt's handler).
pub const ITEM_REMOVE: &str = UiCue::CacCmnItemRemove.event();
/// A menu or submenu backing out (`codmenu.lua`'s default MP `setBackOutSFX`; `caceditclass.lua`
/// sets the same on the editor).
pub const BACKOUT: &str = UiCue::CacCmnBackout.event();
/// A refused choice (`CoD.CACUtility.denySFX`: over budget, an attachment cap, a locked item).
pub const DENY: &str = UiCue::CacCmnDeny.event();
/// Opening the weapon picker (`caceditclass.lua`: `WeaponSelectSFX`).
pub const WEAPON_SELECT: &str = UiCue::CacLoadoutEditSel.event();
/// Opening any other submenu: attachments, perks, grenades, wildcards
/// (`caceditclass.lua`: `SubmenuSelectSFX`).
pub const SUBMENU_SELECT: &str = UiCue::CacLoadoutEditSubmenu.event();

/// A bonus card's equip sound (`bonuscardbutton.lua`'s `EquipSFX`, keyed by the stat names). BO2's
/// table has seven entries and no alias for `bonuscard_two_tacticals` (our Tactician), whose card
/// falls back to the grid button's own action sound, exactly as the Lua's lookup does.
pub fn wildcard_equip(w: Wildcard) -> &'static str {
    match w {
        Wildcard::PrimaryGunfighter => UiCue::CacWildcardEquip1.event(),
        Wildcard::SecondaryGunfighter => UiCue::CacWildcardEquip2.event(),
        Wildcard::Overkill => UiCue::CacWildcardEquip3.event(),
        Wildcard::DangerClose => UiCue::CacWildcardEquip4.event(),
        Wildcard::Perk1Greed => UiCue::CacWildcardEquip5.event(),
        Wildcard::Perk2Greed => UiCue::CacWildcardEquip6.event(),
        Wildcard::Perk3Greed => UiCue::CacWildcardEquip7.event(),
        Wildcard::Tactician => GRID_EQUIP,
    }
}

// ------------------------------------------------------------------------------------------------
// The hosts' shared decisions
// ------------------------------------------------------------------------------------------------

/// A pick row gaining focus: the lobby's list has its own sound, a match's borrows the grid's.
pub fn row_nav(manage: bool) -> &'static str {
    if manage {
        MAIN_NAV
    } else {
        GRID_NAV
    }
}

/// A pick row being activated: the lobby opens the editor on it, a match equips the class.
pub fn row_activate(manage: bool) -> &'static str {
    if manage {
        EDIT_LOADOUT
    } else {
        GRID_EQUIP
    }
}

/// Leaving the pick screen: the lobby's choose-class screen has its own back-out, the in-match
/// pick is a plain menu backing out.
pub fn screen_back(manage: bool) -> &'static str {
    if manage {
        EXIT_CAC
    } else {
        BACKOUT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cue_names_a_converted_alias() {
        // The events the screens play must be in the converter's alias table, or the host would log
        // a missing event for a cue the game is supposed to have.
        for cue in [
            MAIN_NAV,
            EDIT_LOADOUT,
            EXIT_CAC,
            ENTER_CAC,
            SET_STEP,
            DIALOG_OPEN,
            SAVE,
            IN_GAME_OPEN,
            LOADOUT_NAV,
            WILDCARD_NAV,
            GRID_NAV,
            GRID_EQUIP,
            SLIDE_LR,
            SLIDE_UP,
            SLIDE_DOWN,
            SLIDE_EQUIP,
            EQUIPMENT_ADD,
            ITEM_REMOVE,
            BACKOUT,
            DENY,
            WEAPON_SELECT,
            SUBMENU_SELECT,
        ] {
            assert!(UiCue::ALL.iter().any(|c| c.event() == cue), "{cue}");
        }
        for w in Wildcard::ALL {
            let cue = wildcard_equip(w);
            assert!(UiCue::ALL.iter().any(|c| c.event() == cue), "{w:?}: {cue}");
        }
    }

    #[test]
    fn the_carousel_and_grid_cues_are_distinct_aliases() {
        // A slip here would make the weapon picker sound like a perk list.
        let all = [LOADOUT_NAV, WILDCARD_NAV, GRID_NAV, GRID_EQUIP, SLIDE_LR, SLIDE_UP, SLIDE_DOWN, SLIDE_EQUIP, EQUIPMENT_ADD, ITEM_REMOVE, BACKOUT, DENY];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a, b, "two actions share a cue");
            }
        }
        // `wildcard_equip` maps BO2's seven aliased cards to seven distinct aliases; the eighth
        // (Tactician, `bonuscard_two_tacticals`) falls back to the grid's action.
        let mut cards: Vec<&str> = Wildcard::ALL.into_iter().map(wildcard_equip).collect();
        cards.sort_unstable();
        cards.dedup();
        assert_eq!(cards.len(), Wildcard::ALL.len(), "no two cards share an alias");
        assert_eq!(cards.iter().filter(|c| **c != GRID_EQUIP).count(), 7, "seven card aliases plus the fallback");
    }
}
