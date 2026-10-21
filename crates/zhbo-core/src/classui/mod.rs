//! BO2's create-a-class screens: the choose-class list, the allocation editor and the shell's
//! management dialogs.
//!
//! This module owns every pixel of the class screens: the layout, the Phase-1 converted art
//! (`hud::sprites`), the weapon/perk/attachment/wildcard icons, the segmented weapon stat bars, the
//! footer prompts and hit-testing. The state machines stay elsewhere — `classmenu` is the pure
//! editor. Two hosts run the screens: the in-game `client` (a match's pick) and the shell `menu` (the
//! lobby's create-a-class). Both drive them through the shared input folds below
//! ([`pick_frame`]/[`editor_frame`], so the navigation and hit-testing are written once) and both
//! draw them through [`draw_screen`]'s [`ScreenView`], so no host state leaks into the layout.
//! `hud` just delegates here, and the shell's set/class dialogs are drawn by [`draw_dialog`].
//!
//! The layout is BO2's (`cacchooseclass.lua`, `c-primary/*`, `b-editor/*`):
//!
//! * **Choose class.** The heading band and the `‹ CLASS SET n ›` pager with its ten-dot set marker
//!   over the list (`a-chooseclass/001`), the ten-point allocation bar and its `n/10` counter on the
//!   right (`b-editor/001`), the class rows down the left with BO2's hint under them, and the
//!   highlighted class's kit on the right in BO2's order (PRIMARY, SECONDARY, PERK 1-3, LETHAL,
//!   TACTICAL, BONUS CARDS).
//! * **Editor.** The class name as the heading, the allocation bar under it, and BO2's boxed grid:
//!   the primary and its attachment boxes, the secondary and its, the bonus-card strip down the left
//!   (`b-editor/001`); PERK 1-3, LETHAL and TACTICAL down the right (`b-editor/028`). The selected
//!   box is outlined in orange and carries BO2's circled `X`; `R` removes it.
//! * **Weapon picker.** BO2's `PRIMARY`/`SECONDARY` screen (`c-primary/001`, `s10-snipercat`): the
//!   category column in BO2's names, rotated so the active category is on top, the guns of that
//!   category as tiles, and the highlighted gun's four segmented bars in BO2's order (FIRE RATE,
//!   DAMAGE, RANGE, ACCURACY).
//!
//! The pick list is the ACTIVE class set's: the five defaults, then the set's five custom slots, with
//! the set a match loads into shown by the pager. The *shell* adds the final CREATE A CLASS row, the
//! set/class options (`S`/`O`, `menu::Dialog`) and walks the ten sets with Left/Right; in a match the
//! screen is pick-only (BO2 only lets you create classes in the lobby) and the pager is a read-only
//! display of the active set. Missing art falls back to drawn shapes (`Prim::Image`'s fallback), so
//! the screens work without the converted archive.

use crate::classes;
use crate::classmenu::{Draft, Editor, Slot, Wildcard};
use crate::hud::{self, sprites, Prim, DIM, GOLD, OUTLINE, PANEL, RED, WHITE};
use crate::input::Key;
use crate::input::KeySet;
use crate::profile;
use crate::weapons::{self, Class as WeaponClass, WeaponDef};


mod categories;
mod chrome;
mod dialog;
mod edit;
mod input;
mod layout;
mod screens;
#[cfg(test)]
mod tests;

pub use categories::*;
pub use chrome::*;
pub use dialog::*;
pub use edit::*;
pub use input::*;
pub use layout::*;
pub use screens::*;

/// How long the deny flash stays up after a refused choice (seconds).
pub const DENY_FLASH: f32 = 0.3;

/// A dark row behind the list's content.
pub(super) const ROW_FILL: u32 = 0x8C15_181C;
/// The highlighted row's fill, a touch brighter than BO2's gold edge.
pub(super) const ROW_ACTIVE: u32 = 0xD02A_2E33;
/// A row's separator / outline.
pub(super) const ROW_EDGE: u32 = 0x55C8_CCD0;
/// The pick list's right-hand details panel.
pub(super) const PANEL_FILL: u32 = 0x9010_1214;

/// BO2's selection orange: the create-a-class highlight and the spend bars
/// (`b-editor/001`, `c-primary/001`).
pub(super) const ORANGE: u32 = 0xFFFF_8A1E;
/// A boxed editor cell's fill and its idle edge (BO2's near-black tiles).
pub(super) const CELL_FILL: u32 = 0x8C14_161A;
pub(super) const CELL_EDGE: u32 = 0x70C8_CCD0;
/// The allocation bar's free segments and BO2's tile-strip fill.
pub(super) const BAR_FREE: u32 = 0x46FF_FFFF;
/// The dialog panel behind the shell's S/O menus (`a-chooseclass/021`).
pub(super) const DIALOG_FILL: u32 = 0xF01C_1E20;
pub(super) const DIALOG_EDGE: u32 = 0xB0D8_DCDE;
/// The footer prompts' key size (BO2's `ESC Back` line).
pub(super) const PROMPT_SIZE: f32 = 12.0;

/// One row of the pick screen's composed list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickRow {
    /// An index into the shipped registry (`classes::get`).
    Preset(usize),
    /// An index into the player's profile (`profile::classes`).
    Custom(usize),
    /// The final CREATE A CLASS row: opens the editor on a fresh draft.
    Create,
}

/// One of the shell pick screen's footer prompts. In a match the footer is prompts only (the screen
/// is pick-only and Escape closes it, so there is nothing to manage).
///
/// BO2's choose-class footer (`a-chooseclass/001`) reads `ESC Back`, `S Class Set Options`,
/// `O Class Options`; the keys are the same here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickFooterAction {
    /// Close the screen (Escape).
    Back,
    /// Open the class-set options dialog (rename/reset the set) — BO2's `S`.
    SetOptions,
    /// Open the highlighted class's options dialog (rename/copy/reset/clear) — BO2's `O`.
    ClassOptions,
}

impl PickFooterAction {
    /// The key that triggers the action, as the footer shows it.
    pub fn key(self) -> &'static str {
        match self {
            PickFooterAction::Back => "ESC",
            PickFooterAction::SetOptions => "S",
            PickFooterAction::ClassOptions => "O",
        }
    }

    /// The action's label, as the footer shows it.
    pub fn label(self) -> &'static str {
        match self {
            PickFooterAction::Back => "Back",
            PickFooterAction::SetOptions => "Class Set Options",
            PickFooterAction::ClassOptions => "Class Options",
        }
    }

    /// The footer prompt for the action.
    pub fn prompt(self) -> Prompt {
        Prompt { key: self.key(), label: self.label() }
    }
}

/// One key hint in a screen's footer: BO2's gold key then the label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prompt {
    pub key: &'static str,
    pub label: &'static str,
}

/// One class set's pick list: the rows plus the set they describe, for the screen's pager.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickList {
    /// The rows, in BO2's order: the five default classes, the active set's custom slots, then (in
    /// the shell only) CREATE A CLASS.
    pub rows: Vec<PickRow>,
    /// The active class set, 0-based.
    pub set: usize,
    /// How many class sets the profile has (BO2's ten).
    pub sets: usize,
    /// The active set's display name (`CLASS SET 1`, or the player's own).
    pub set_name: String,
}

impl PickList {
    /// The active set as the pager shows it: 1-based, like the row numerals.
    pub fn set_number(&self) -> usize {
        self.set + 1
    }

    /// Which of the pager's ten dots is the filled marker (clamped into range, so a hand-edited
    /// profile naming set 99 still draws inside the row).
    pub fn marker(&self) -> usize {
        self.set.min(self.sets.saturating_sub(1))
    }
}

/// The pick screen's list for the ACTIVE class set (`cacchooseclass.lua`: the set is what a match
/// loads into). Rebuilt on demand (the profile is the single source), like the draw. `manage` is
/// true for the shell host (`menu.rs`), false for the in-match pick.
pub fn pick_list(manage: bool) -> PickList {
    // One snapshot of the profile, so the rows and the pager describe the same set even if a
    // test thread saves meanwhile.
    let p = profile::get();
    let rows = pick_rows_for(classes::get().len(), p.customs().len(), manage);
    let set = p.active_set();
    PickList { rows, set, sets: profile::MAX_SETS, set_name: p.set_name(set) }
}

/// The active set's rows alone (the list's shape without the pager data).
pub fn pick_rows(manage: bool) -> Vec<PickRow> {
    pick_list(manage).rows
}

/// The list for a caller that already snapshotted the two registries (the draw does, so both
/// halves of one frame describe the same lists even if a test thread saves concurrently).
pub fn pick_rows_for(presets: usize, customs: usize, manage: bool) -> Vec<PickRow> {
    let extra = usize::from(manage);
    let mut rows = Vec::with_capacity(presets + customs + extra);
    rows.extend((0..presets).map(PickRow::Preset));
    rows.extend((0..customs).map(PickRow::Custom));
    if manage {
        rows.push(PickRow::Create);
    }
    rows
}

/// The number key that activates pick row `i` (BO2's 1-9,0), or `None` past the tenth. The
/// autopilot derives its seeded custom's key from the live list through this.
pub fn pick_key(i: usize) -> Option<Key> {
    PICK_KEYS.get(i).copied()
}
