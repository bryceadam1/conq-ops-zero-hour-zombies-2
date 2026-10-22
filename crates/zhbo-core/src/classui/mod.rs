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
//! The layout is BO2's, in LUI units (`bo2.rs` owns the frame; `cacchooseclass.lua`,
//! `changeclass.lua`, `classpreview.lua`, `c-primary/*`, `b-editor/*` are the sources):
//!
//! * **Choose class.** The CHOOSE CLASS title, the `‹ CLASS SET n ›` header with its set pips
//!   (lobby only), and the button list down the left: the lobby (`cacchooseclass.lua`'s
//!   `UpdateButtonList`) lists the active set's five custom slots — filled ones by name, untouched
//!   ones as BO2's `Custom n` (`a-chooseclass/002`) — with BO2's set hint under them; a match
//!   (`changeclass.lua`) shows the set's name, then the same five slots, then the default classes
//!   in BO2's order (SMG, CQB, Assault, LMG, Sniper). The highlighted class's `ClassPreview` sits
//!   right (`preview.rs`): the `n/10` allocation counter and ten-bar strip, then PRIMARY, SECONDARY,
//!   PERK 1-3, LETHAL, TACTICAL and WILDCARDS. The focused row pulses `BOIIOrange`
//!   (`cod9button.lua`).
//! * **Editor.** The class name as the heading, the allocation bar under it, and BO2's boxed grid:
//!   the primary and its attachment boxes, the secondary and its, the bonus-card strip down the left
//!   (`b-editor/001`); PERK 1-3, LETHAL and TACTICAL down the right (`b-editor/028`). The selected
//!   box is outlined in orange and carries BO2's circled `X`; `R` removes it.
//! * **Weapon picker.** BO2's `PRIMARY`/`SECONDARY` screen (`c-primary/001`, `s10-snipercat`): the
//!   category column in BO2's names, rotated so the active category is on top, the guns of that
//!   category as tiles, and the highlighted gun's four segmented bars in BO2's order (FIRE RATE,
//!   DAMAGE, RANGE, ACCURACY).
//!
//! The *shell* walks the ten sets with Left/Right and carries the set/class options (`S`/`O`,
//! `menu::Dialog`); in a match the screen is pick-only (BO2 only creates classes in the lobby) and
//! an untouched slot is not pickable. Missing art falls back to drawn shapes (`Prim::Image`'s
//! fallback), so the screens work without the converted archive.

use crate::classes;
use crate::classmenu::{Draft, Editor, Slot, Wildcard};
use crate::hud::{self, sprites, Prim, RED, WHITE};
use crate::input::Key;
use crate::input::KeySet;
use crate::profile;
use crate::weapons::{self, Class as WeaponClass, WeaponDef};


pub mod bo2;
mod categories;
mod chrome;
pub mod cues;
mod dialog;
mod edit;
mod editor;
mod input;
mod layout;
mod pick;
mod preview;
mod screens;
#[cfg(test)]
mod tests;

pub use categories::*;
pub use chrome::*;
pub use dialog::*;
pub use edit::*;
pub use editor::*;
pub use input::*;
pub use layout::*;
pub use pick::*;
pub use screens::*;

/// How long the deny flash stays up after a refused choice (seconds).
pub const DENY_FLASH: f32 = 0.3;

/// The dialog panel behind the shell's S/O menus (`a-chooseclass/021` samples the popup interior
/// at ~36/255 over an ~10/255 dimmed backdrop).
pub(super) const DIALOG_FILL: u32 = 0xFF26_2626;
pub(super) const DIALOG_EDGE: u32 = 0xB0D8_DCDE;

/// One row of the pick screen's composed list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickRow {
    /// An index into the shipped registry (`classes::get`).
    Preset(usize),
    /// An index into the player's profile (`profile::classes`).
    Custom(usize),
    /// An untouched custom slot of the active set (`Custom n`, BO2 always shows all five): picking
    /// it in the lobby opens the editor on a fresh class for that slot.
    Empty(usize),
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
    /// The rows, in BO2's order: the active set's five custom slots (filled, then untouched), then
    /// in a match the default classes (`changeclass.lua`).
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
    let rows = pick_rows_for(&classes::get(), p.customs().len(), manage);
    let set = p.active_set();
    PickList { rows, set, sets: profile::MAX_SETS, set_name: p.set_name(set) }
}

/// The active set's rows alone (the list's shape without the pager data).
pub fn pick_rows(manage: bool) -> Vec<PickRow> {
    pick_list(manage).rows
}

/// The list for a caller that already snapshotted the two registries (the draw does, so both
/// halves of one frame describe the same lists even if a test thread saves concurrently).
///
/// The lobby's `CACChooseClass` lists the active set's five custom slots and nothing else
/// (`cacchooseclass.lua`'s `UpdateButtonList`). The match's `changeclass` lists the same five, then
/// the default classes in its order: SMG, CQB, Assault, LMG, Sniper (`AddDefaultClassButtons`,
/// `class_smg` 10 .. `class_sniper` 14); a preset outside those five keeps its registry order after
/// them.
pub fn pick_rows_for(presets: &[classes::ClassDef], customs: usize, manage: bool) -> Vec<PickRow> {
    let customs = customs.min(profile::MAX_CLASSES);
    let mut rows: Vec<PickRow> = (0..customs).map(PickRow::Custom).collect();
    rows.extend((customs..profile::MAX_CLASSES).map(PickRow::Empty));
    if !manage {
        const ORDER: [&str; 5] = ["smg", "cqb", "assault", "lmg", "sniper"];
        let mut order: Vec<usize> = (0..presets.len()).collect();
        order.sort_by_key(|&j| ORDER.iter().position(|o| presets[j].id.eq_ignore_ascii_case(o)).unwrap_or(ORDER.len()));
        rows.extend(order.into_iter().map(PickRow::Preset));
    }
    rows
}

/// The number key that activates pick row `i` (BO2's 1-9,0), or `None` past the tenth. The
/// autopilot derives its seeded custom's key from the live list through this.
pub fn pick_key(i: usize) -> Option<Key> {
    PICK_KEYS.get(i).copied()
}
