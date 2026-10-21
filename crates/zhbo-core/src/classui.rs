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

/// How long the deny flash stays up after a refused choice (seconds).
pub const DENY_FLASH: f32 = 0.3;

/// A dark row behind the list's content.
const ROW_FILL: u32 = 0x8C15_181C;
/// The highlighted row's fill, a touch brighter than BO2's gold edge.
const ROW_ACTIVE: u32 = 0xD02A_2E33;
/// A row's separator / outline.
const ROW_EDGE: u32 = 0x55C8_CCD0;
/// The pick list's right-hand details panel.
const PANEL_FILL: u32 = 0x9010_1214;

/// BO2's selection orange: the create-a-class highlight and the spend bars
/// (`b-editor/001`, `c-primary/001`).
const ORANGE: u32 = 0xFFFF_8A1E;
/// A boxed editor cell's fill and its idle edge (BO2's near-black tiles).
const CELL_FILL: u32 = 0x8C14_161A;
const CELL_EDGE: u32 = 0x70C8_CCD0;
/// The allocation bar's free segments and BO2's tile-strip fill.
const BAR_FREE: u32 = 0x46FF_FFFF;
/// The dialog panel behind the shell's S/O menus (`a-chooseclass/021`).
const DIALOG_FILL: u32 = 0xF01C_1E20;
const DIALOG_EDGE: u32 = 0xB0D8_DCDE;
/// The footer prompts' key size (BO2's `ESC Back` line).
const PROMPT_SIZE: f32 = 12.0;

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

/// The pick screen's geometry: the heading and the set pager across the top, the list down the left
/// with BO2's "You will load into the game with this class set." under it, the selected class's kit
/// on the right, the prompt strip at the bottom (`a-chooseclass/001`).
#[derive(Clone, Copy, Debug)]
pub struct PickLayout {
    pub title_y: f32,
    pub header_h: f32,
    pub content_x: f32,
    pub content_w: f32,
    /// The `‹ CLASS SET n ›` row and its ten-dot marker.
    pub pager_y: f32,
    pub pager_h: f32,
    pub list_x: f32,
    pub list_y: f32,
    pub list_w: f32,
    pub list_h: f32,
    pub row_h: f32,
    /// BO2's "You will load into the game with this class set." line under the list.
    pub hint_y: f32,
    pub panel_x: f32,
    pub panel_y: f32,
    pub panel_w: f32,
    pub panel_h: f32,
    pub footer_y: f32,
}

pub fn pick_layout(w: f32, h: f32, s: f32) -> PickLayout {
    let title_y = 12.0 * s;
    let header_h = 46.0 * s;
    let content_x = w * 0.05;
    let content_w = w * 0.90;
    let pager_y = title_y + header_h + 2.0 * s;
    let pager_h = 32.0 * s;
    let list_x = content_x;
    let list_y = pager_y + pager_h + 10.0 * s;
    let list_w = content_w * 0.46;
    let footer_y = h - 34.0 * s;
    let list_h = (footer_y - list_y - 56.0 * s).max(80.0 * s);
    let panel_x = content_x + content_w * 0.52;
    PickLayout {
        title_y,
        header_h,
        content_x,
        content_w,
        pager_y,
        pager_h,
        list_x,
        list_y,
        list_w,
        list_h,
        row_h: 34.0 * s,
        hint_y: list_y + list_h + 8.0 * s,
        panel_x,
        panel_y: list_y,
        panel_w: content_x + content_w - panel_x,
        panel_h: list_h,
        footer_y,
    }
}

/// The allocation bar and counter's place in the heading: BO2 puts the bar under the pager on the
/// right half of the choose-class screen and across the whole width in the editor.
fn allocation_bar_rect(l: &PickLayout, s: f32) -> (f32, f32, f32) {
    (l.panel_x, l.pager_y + 10.0 * s, l.panel_w)
}

/// How many pick rows fit in the list's panel.
pub fn pick_visible(l: &PickLayout) -> usize {
    ((l.list_h - 4.0) / l.row_h).max(1.0) as usize
}

/// The first row shown so the highlighted row is in the window (the list scrolls, like BO2's).
pub fn pick_first(highlighted: usize, visible: usize) -> usize {
    highlighted.saturating_sub(visible.saturating_sub(1))
}

/// The prompt width at `s` (the engine font's advance is close enough for the layout and the
/// prompt's clickable box).
fn prompt_width(p: &Prompt, s: f32) -> f32 {
    (p.key.chars().count() + p.label.chars().count() + 1) as f32 * PROMPT_SIZE * 6.0 / 7.0 * s
}

/// The shell's footer prompts, right-aligned in the prompt strip: BO2's `S Class Set Options`,
/// `O Class Options`, with `ESC Back` at the left (each is clickable). Empty for the in-match
/// pick-only footer (there is nothing to manage).
pub fn pick_footer_actions(l: &PickLayout, w: f32, s: f32, manage: bool) -> Vec<(PickFooterAction, f32, f32, f32, f32)> {
    if !manage {
        return Vec::new();
    }
    let y = l.footer_y + 3.0 * s;
    let fh = 22.0 * s;
    let gap = 24.0 * s;
    let right = (l.content_x + l.content_w - 10.0 * s).min(w * 0.95 - 10.0 * s);
    let mut out = Vec::with_capacity(3);
    let back = PickFooterAction::Back.prompt();
    out.push((PickFooterAction::Back, l.content_x + 10.0 * s, y, prompt_width(&back, s), fh));
    // The right group is laid out back to front from the strip's right edge.
    let set = PickFooterAction::SetOptions.prompt();
    let class = PickFooterAction::ClassOptions.prompt();
    let (set_w, class_w) = (prompt_width(&set, s), prompt_width(&class, s));
    let set_x = right - class_w - gap - set_w;
    out.push((PickFooterAction::SetOptions, set_x, y, set_w, fh));
    out.push((PickFooterAction::ClassOptions, set_x + set_w + gap, y, class_w, fh));
    out
}

/// The footer action under a screen point, if any (shell only).
pub fn pick_footer_at(l: &PickLayout, w: f32, s: f32, x: f32, y: f32, manage: bool) -> Option<PickFooterAction> {
    pick_footer_actions(l, w, s, manage)
        .into_iter()
        .find(|(_, bx, by, bw, bh)| x >= *bx && x <= bx + bw && y >= *by && y <= by + bh)
        .map(|(action, _, _, _, _)| action)
}

/// The row under a screen point, if any. `first` is [`pick_first`] for the current highlight.
pub fn pick_row_at(l: &PickLayout, x: f32, y: f32, rows: usize, first: usize) -> Option<usize> {
    if x < l.list_x || x > l.list_x + l.list_w || y < l.list_y || y > l.list_y + l.list_h {
        return None;
    }
    let j = ((y - l.list_y) / l.row_h) as usize;
    let within = y - l.list_y - j as f32 * l.row_h;
    let i = first + j;
    (j < pick_visible(l) && i < rows && within <= l.row_h - 6.0).then_some(i)
}

// ------------------------------------------------------------------------------------------------
// The editor's geometry: BO2's boxed grid
// ------------------------------------------------------------------------------------------------

/// The editor's geometry (`b-editor/001`): the heading (the class name) with the allocation bar
/// under it, the primary/attachments/secondary/attachments/bonus-card boxes down the left, PERK 1-3,
/// LETHAL, TACTICAL and SAVE down the right, and the prompt strip at the bottom.
#[derive(Clone, Copy, Debug)]
pub struct EditorLayout {
    pub title_y: f32,
    pub header_h: f32,
    pub content_x: f32,
    pub content_w: f32,
    pub bar_y: f32,
    pub bar_h: f32,
    /// The first box row's top.
    pub top: f32,
    pub gap: f32,
    pub left_x: f32,
    pub left_w: f32,
    pub right_x: f32,
    pub right_w: f32,
    pub primary_y: f32,
    pub primary_h: f32,
    pub atts_y: f32,
    pub atts_h: f32,
    pub secondary_y: f32,
    pub secondary_h: f32,
    pub atts2_y: f32,
    pub wild_y: f32,
    pub wild_h: f32,
    pub perk_h: f32,
    pub offhand_h: f32,
    pub save_h: f32,
    /// The right column's rows below the three perk rows (BO2's LETHAL, TACTICAL, then our Save).
    pub lethal_y: f32,
    pub tactical_y: f32,
    pub save_y: f32,
    pub footer_y: f32,
    /// The opening weapon/option list: rows this high, starting here.
    pub row_h: f32,
}

pub fn editor_layout(w: f32, h: f32, s: f32) -> EditorLayout {
    let title_y = 12.0 * s;
    let header_h = 46.0 * s;
    let content_x = w * 0.06;
    let content_w = w * 0.88;
    let bar_y = title_y + header_h + 2.0 * s;
    let bar_h = 7.0 * s;
    let top = bar_y + 22.0 * s;
    let gap = 8.0 * s;
    let left_x = content_x;
    let left_w = content_w * 0.575;
    let right_x = content_x + content_w * 0.595;
    let right_w = content_x + content_w - right_x;
    let primary_y = top;
    let primary_h = 170.0 * s;
    let atts_y = primary_y + primary_h + gap;
    let atts_h = 42.0 * s;
    let secondary_y = atts_y + atts_h + gap;
    let secondary_h = 96.0 * s;
    let atts2_y = secondary_y + secondary_h + gap;
    let wild_y = atts2_y + atts_h + gap;
    let wild_h = 120.0 * s;
    let perk_h = 88.0 * s;
    let offhand_h = 82.0 * s;
    let save_h = 36.0 * s;
    let lethal_y = top + 3.0 * (perk_h + gap);
    let tactical_y = lethal_y + offhand_h + gap;
    let save_y = tactical_y + offhand_h + gap;
    EditorLayout {
        title_y,
        header_h,
        content_x,
        content_w,
        bar_y,
        bar_h,
        top,
        gap,
        left_x,
        left_w,
        right_x,
        right_w,
        primary_y,
        primary_h,
        atts_y,
        atts_h,
        secondary_y,
        secondary_h,
        atts2_y,
        wild_y,
        wild_h,
        perk_h,
        offhand_h,
        save_h,
        lethal_y,
        tactical_y,
        save_y,
        footer_y: h - 34.0 * s,
        row_h: 34.0 * s,
    }
}

/// One addressable box of the editor's screen: the slot it edits, which of the slot's repeated
/// boxes it is (attachment 2, bonus card 3, …), and where it sits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorCell {
    pub slot: Slot,
    pub index: usize,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl EditorCell {
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    /// Whether a point is inside the box.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

/// The editor's boxes, in the walk order of [`Slot::ALL`] (the second perk rows beside their tier,
/// the attachment and bonus-card boxes in slot order).
pub fn editor_cells(l: &EditorLayout, editor: &Editor) -> Vec<EditorCell> {
    let d = &editor.draft;
    let mut out: Vec<EditorCell> = Vec::with_capacity(22);
    let mut cell = |slot, index, x, y, w, h| out.push(EditorCell { slot, index, x, y, w, h });
    // The heading is the class-name row (BO2's editor titles the screen with the class name).
    cell(Slot::Name, 0, l.content_x, l.title_y, l.content_w * 0.55, l.header_h);
    cell(Slot::Primary, 0, l.left_x, l.primary_y, l.left_w, l.primary_h);
    // The attachment boxes sit right of their label, BO2's row. A weapon that offers no
    // attachments has no boxes at all ("Attachments not available", b-editor/002).
    let box_w = l.left_w * 0.18;
    let box_gap = l.left_w * 0.035;
    let first_x = l.left_x + l.left_w * 0.36;
    if offers_attachments(&d.primary) {
        for i in 0..d.primary_att_max() {
            let x = first_x + i as f32 * (box_w + box_gap);
            cell(Slot::PrimaryAtts, i, x, l.atts_y, box_w.min(l.left_x + l.left_w - x), l.atts_h);
        }
    }
    cell(Slot::Secondary, 0, l.left_x, l.secondary_y, l.left_w, l.secondary_h);
    if offers_attachments(&d.secondary) {
        for i in 0..d.secondary_att_max() {
            let x = first_x + i as f32 * (box_w + box_gap);
            cell(Slot::SecondaryAtts, i, x, l.atts2_y, box_w.min(l.left_x + l.left_w - x), l.atts_h);
        }
    }
    cell(Slot::Lethal, 0, l.right_x + l.right_w * 0.46, l.lethal_y, l.right_w * 0.36, l.offhand_h);
    cell(Slot::Tactical, 0, l.right_x + l.right_w * 0.46, l.tactical_y, l.right_w * 0.36, l.offhand_h);
    for tier in 1..=3u8 {
        let y = l.top + (tier as f32 - 1.0) * (l.perk_h + l.gap);
        cell(perk_slot(tier), 0, l.right_x + l.right_w * 0.46, y, l.right_w * 0.36, l.perk_h);
        if d.wildcards.contains(&greed_wildcard(tier)) {
            cell(perk_extra_slot(tier), 1, l.right_x + l.right_w * 0.86, y + l.perk_h * 0.25, l.right_w * 0.12, l.perk_h * 0.5);
        }
    }
    let wild_w = (l.left_w - 2.0 * l.gap) / 3.0;
    for i in 0..3 {
        cell(Slot::Wildcards, i, l.left_x + i as f32 * (wild_w + l.gap), l.wild_y, wild_w, l.wild_h);
    }
    let save_y = l.save_y;
    cell(Slot::Save, 0, l.right_x + l.right_w * 0.46, save_y, l.right_w * 0.36, l.save_h);
    out
}

/// The box under a screen point.
pub fn editor_cell_at(cells: &[EditorCell], x: f32, y: f32) -> Option<usize> {
    cells.iter().position(|c| c.contains(x, y))
}

/// BO2's circled `X` on a filled, selected box: where it draws and what it covers.
pub fn remove_rect(cell: &EditorCell, s: f32) -> (f32, f32, f32, f32) {
    let d = 18.0 * s;
    (cell.x + cell.w - d - 4.0 * s, cell.y + 4.0 * s, d, d)
}

/// The `(slot, box)` a click clears at this point: only the selected slot's filled boxes carry the
/// `X` (BO2 shows it on the highlighted slot alone).
pub fn editor_remove_at(cells: &[EditorCell], editor: &Editor, s: f32, x: f32, y: f32) -> Option<(Slot, usize)> {
    cells.iter().find_map(|c| {
        if c.slot != editor.slot || !cell_filled(editor, c) {
            return None;
        }
        let (rx, ry, rw, rh) = remove_rect(c, s);
        (x >= rx && x <= rx + rw && y >= ry && y <= ry + rh).then_some((c.slot, c.index))
    })
}

/// Whether a box holds something (so it draws a value and can be removed).
pub fn cell_filled(editor: &Editor, cell: &EditorCell) -> bool {
    let d = &editor.draft;
    match cell.slot {
        Slot::Name => !d.name.trim().is_empty(),
        Slot::Primary => !d.primary.is_empty(),
        Slot::PrimaryAtts => d.primary_atts.get(cell.index).is_some(),
        Slot::Secondary => !d.secondary.is_empty(),
        Slot::SecondaryAtts => d.secondary_atts.get(cell.index).is_some(),
        Slot::Lethal => !d.lethal.is_empty(),
        Slot::Tactical => !d.tactical.is_empty(),
        Slot::Perk1 | Slot::Perk2 | Slot::Perk3 => {
            d.perks[(cell.slot.perk_tier().unwrap_or(1) as usize) - 1].is_some()
        }
        Slot::Perk1b | Slot::Perk2b | Slot::Perk3b => {
            d.extra_perks[(cell.slot.perk_tier().unwrap_or(1) as usize) - 1].is_some()
        }
        Slot::Wildcards => d.wildcards.get(cell.index).is_some(),
        Slot::Save => false,
    }
}

/// The perk slot of a tier's first row.
fn perk_slot(tier: u8) -> Slot {
    match tier {
        2 => Slot::Perk2,
        3 => Slot::Perk3,
        _ => Slot::Perk1,
    }
}

/// The perk slot of a tier's Greed row.
fn perk_extra_slot(tier: u8) -> Slot {
    match tier {
        2 => Slot::Perk2b,
        3 => Slot::Perk3b,
        _ => Slot::Perk1b,
    }
}

/// The Greed wildcard that opens a tier's second perk row.
fn greed_wildcard(tier: u8) -> Wildcard {
    match tier {
        2 => Wildcard::Perk2Greed,
        3 => Wildcard::Perk3Greed,
        _ => Wildcard::Perk1Greed,
    }
}

/// The opening option list's rect: the weapon picker's tiles fill the right column, every other
/// picker's rows fill the left one.
pub fn picker_rect(l: &EditorLayout, slot: Slot) -> (f32, f32, f32, f32) {
    match slot {
        Slot::Primary | Slot::Secondary => (l.right_x, l.top, l.right_w, l.footer_y - 20.0 - l.top),
        _ => (l.left_x, l.top, l.left_w, l.footer_y - 20.0 - l.top),
    }
}

/// How many option rows fit in the picker.
pub fn picker_rows(l: &EditorLayout, slot: Slot) -> usize {
    let (_, _, _, h) = picker_rect(l, slot);
    (h / l.row_h).max(1.0) as usize
}

/// The first shown option (scrolled so the highlighted one shows) and how many fit.
pub fn picker_window(l: &EditorLayout, slot: Slot, highlighted: usize) -> (usize, usize) {
    let shown = picker_rows(l, slot);
    (highlighted.saturating_sub(shown.saturating_sub(1)), shown)
}

/// The drawn row under a screen point, already translated to its index in the shown window.
pub fn picker_option_at(l: &EditorLayout, slot: Slot, x: f32, y: f32, first: usize, count: usize) -> Option<usize> {
    let (px, py, pw, _) = picker_rect(l, slot);
    if x < px || x > px + pw || y < py || y > l.footer_y {
        return None;
    }
    let j = ((y - py) / l.row_h) as usize;
    let within = y - py - j as f32 * l.row_h;
    let i = first + j;
    (j < picker_rows(l, slot) && i < count && within <= l.row_h - 6.0).then_some(i)
}

// ------------------------------------------------------------------------------------------------
// The weapon picker's category column
// ------------------------------------------------------------------------------------------------

/// One category of a weapon slot's list: BO2's display name and the option indices it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Category {
    pub class: WeaponClass,
    pub name: &'static str,
    /// The option indices (into `Editor::options`) of the guns in this category, in list order.
    pub options: Vec<usize>,
}

/// The primary slot's category order, BO2's (`c-primary/001`: SUBMACHINE GUNS, ASSAULT RIFLES,
/// SHOTGUNS, LIGHT MACHINE GUNS, SNIPER RIFLES, SPECIALS).
const PRIMARY_CATEGORY_ORDER: [WeaponClass; 7] = [
    WeaponClass::Smg,
    WeaponClass::Rifle,
    WeaponClass::Shotgun,
    WeaponClass::Lmg,
    WeaponClass::Sniper,
    WeaponClass::Special,
    WeaponClass::Launcher,
];

/// The secondary slot's category order, BO2's (`d-attach/037`: PISTOLS, LAUNCHERS, SPECIALS).
const SECONDARY_CATEGORY_ORDER: [WeaponClass; 3] = [WeaponClass::Pistol, WeaponClass::Launcher, WeaponClass::Special];

/// BO2's display name for a weapon category.
pub fn category_name(class: WeaponClass) -> &'static str {
    match class {
        WeaponClass::Smg => "SUBMACHINE GUNS",
        WeaponClass::Rifle => "ASSAULT RIFLES",
        WeaponClass::Shotgun => "SHOTGUNS",
        WeaponClass::Lmg => "LIGHT MACHINE GUNS",
        WeaponClass::Sniper => "SNIPER RIFLES",
        WeaponClass::Special => "SPECIALS",
        WeaponClass::Launcher => "LAUNCHERS",
        WeaponClass::Pistol => "PISTOLS",
        _ => "OTHER",
    }
}

/// The category order of a weapon slot: its own table first, then the other slot's (the secondary
/// gains the primary classes under Overkill).
fn category_order(slot: Slot) -> Vec<WeaponClass> {
    let mut out: Vec<WeaponClass> = match slot {
        Slot::Secondary => SECONDARY_CATEGORY_ORDER.to_vec(),
        _ => PRIMARY_CATEGORY_ORDER.to_vec(),
    };
    for c in PRIMARY_CATEGORY_ORDER.into_iter().chain(SECONDARY_CATEGORY_ORDER) {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// The weapon slot's categories, in BO2's column order. Empty for a slot that does not list guns.
pub fn weapon_categories(editor: &Editor) -> Vec<Category> {
    if !matches!(editor.slot, Slot::Primary | Slot::Secondary) {
        return Vec::new();
    }
    let order = category_order(editor.slot);
    let mut out: Vec<Category> = Vec::new();
    for i in 0..editor.options().len() {
        let Some(def) = editor.option_weapon(i) else { continue };
        match out.iter_mut().find(|c| c.class == def.class) {
            Some(c) => c.options.push(i),
            None => out.push(Category { class: def.class, name: category_name(def.class), options: vec![i] }),
        }
    }
    out.sort_by_key(|c| order.iter().position(|x| *x == c.class).unwrap_or(usize::MAX));
    out
}

/// Which category holds the highlighted option (0 when it holds none).
pub fn category_of(cats: &[Category], option: usize) -> usize {
    cats.iter().position(|c| c.options.contains(&option)).unwrap_or(0)
}

/// The category column as BO2 draws it (`s10-snipercat`): the active category on top, then the rest
/// in their own order, wrapping (`SNIPER RIFLES` first, `SPECIALS` … after it).
pub fn category_column(cats: &[Category], active: usize) -> Vec<usize> {
    if cats.is_empty() {
        return Vec::new();
    }
    let active = active.min(cats.len() - 1);
    (0..cats.len()).map(|i| (active + i) % cats.len()).collect()
}

/// Move the highlight to the neighbouring category (BO2's Left/Right), landing on its first gun.
pub fn step_category(editor: &mut Editor, cats: &[Category], step: i32) -> bool {
    if cats.is_empty() || step == 0 {
        return false;
    }
    let active = category_of(cats, editor.option);
    let next = (active as i32 + step).rem_euclid(cats.len() as i32) as usize;
    let target = cats[next].options[0];
    let moved = target != editor.option;
    editor.option = target;
    moved
}

/// Move the highlight within the active category (BO2's Up/Down), wrapping inside it.
pub fn step_within_category(editor: &mut Editor, cats: &[Category], step: i32) -> bool {
    if cats.is_empty() || step == 0 {
        return false;
    }
    let cat = &cats[category_of(cats, editor.option)];
    if cat.options.is_empty() {
        return false;
    }
    let pos = cat.options.iter().position(|i| *i == editor.option).unwrap_or(0);
    let next = (pos as i32 + step).rem_euclid(cat.options.len() as i32) as usize;
    let target = cat.options[next];
    let moved = target != editor.option;
    editor.option = target;
    moved
}

// ------------------------------------------------------------------------------------------------
// Removing a slot (BO2's `R Remove` and the circled X)
// ------------------------------------------------------------------------------------------------

/// Clear one box of a slot: the attachment in that box, the bonus card in that box, or the whole
/// single-value slot. False when the box was already empty.
pub fn clear_cell(d: &mut Draft, slot: Slot, index: usize) -> bool {
    match slot {
        Slot::Name => {
            if d.name.is_empty() {
                return false;
            }
            d.name.clear();
            true
        }
        Slot::Primary => {
            if d.primary.is_empty() {
                return false;
            }
            // BO2's `ClearWeapon` takes the attachments with the gun.
            d.primary.clear();
            d.primary_atts.clear();
            true
        }
        Slot::Secondary => {
            if d.secondary.is_empty() {
                return false;
            }
            d.secondary.clear();
            d.secondary_atts.clear();
            true
        }
        Slot::PrimaryAtts => {
            if index >= d.primary_atts.len() {
                return false;
            }
            d.primary_atts.remove(index);
            true
        }
        Slot::SecondaryAtts => {
            if index >= d.secondary_atts.len() {
                return false;
            }
            d.secondary_atts.remove(index);
            true
        }
        Slot::Lethal => {
            if d.lethal.is_empty() {
                return false;
            }
            d.lethal.clear();
            true
        }
        Slot::Tactical => {
            if d.tactical.is_empty() {
                return false;
            }
            d.tactical.clear();
            true
        }
        Slot::Perk1 | Slot::Perk2 | Slot::Perk3 => {
            let tier = (slot.perk_tier().unwrap_or(1) as usize) - 1;
            d.perks[tier].take().is_some()
        }
        Slot::Perk1b | Slot::Perk2b | Slot::Perk3b => {
            let tier = (slot.perk_tier().unwrap_or(1) as usize) - 1;
            d.extra_perks[tier].take().is_some()
        }
        Slot::Wildcards => {
            if index >= d.wildcards.len() {
                return false;
            }
            let w = d.wildcards.remove(index);
            // Turning the card off drops what it enabled, exactly like un-picking it in the editor.
            match w {
                Wildcard::PrimaryGunfighter => d.primary_atts.truncate(2),
                Wildcard::SecondaryGunfighter => d.secondary_atts.truncate(1),
                _ => {
                    if let Some(tier) = w.greed_tier() {
                        d.extra_perks[(tier as usize) - 1] = None;
                    }
                }
            }
            true
        }
        Slot::Save => false,
    }
}

/// Clear the whole slot: every attachment of the strip, every bonus card, or the single value.
pub fn clear_slot(d: &mut Draft, slot: Slot) -> bool {
    match slot {
        Slot::PrimaryAtts => !std::mem::take(&mut d.primary_atts).is_empty(),
        Slot::SecondaryAtts => !std::mem::take(&mut d.secondary_atts).is_empty(),
        Slot::Wildcards => {
            let had = !d.wildcards.is_empty();
            while !d.wildcards.is_empty() {
                clear_cell(d, Slot::Wildcards, 0);
            }
            had
        }
        other => clear_cell(d, other, 0),
    }
}

// ------------------------------------------------------------------------------------------------
// The hosts' shared input fold
// ------------------------------------------------------------------------------------------------

/// The pick screen's keys 1-9,0, in list order: key 1 is the first row, key 0 the tenth. The keys
/// exist so a player can pick without the mouse; BO2 shows the same numerals on the rows.
const PICK_KEYS: [Key; 10] = [
    Key::Key1, Key::Key2, Key::Key3, Key::Key4, Key::Key5, Key::Key6, Key::Key7, Key::Key8, Key::Key9, Key::Key0,
];

/// A cursor move smaller than this does not count as a hover (pixel jitter would fight the keys).
const MOUSE_MOVED: f32 = 0.5;

/// `save_profile`'s reasons on both hosts' one-shots (`client::ClientOut`, `menu::MenuOut`): the
/// editor's Save (the autotest's "saved to the profile" case) and the shell's set/class options.
/// Both carry the whole profile file's text (`profile::pack_text`).
pub const SAVE_PROFILE_CLASS: i32 = 1;
pub const SAVE_PROFILE_EDIT: i32 = 2;

/// A frame of menu input, projected from the host's own input struct (`client::ClientIn` in a
/// match, `menu::MenuIn` in the shell): the key set, the pointer and the screen it draws on.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MenuPointer {
    pub keys: KeySet,
    pub mouse_x: f32,
    pub mouse_y: f32,
    pub mouse_buttons: u32,
    pub wheel: f32,
    pub screen_w: f32,
    pub screen_h: f32,
}

impl MenuPointer {
    /// The BO2 scale factor the screens are laid out in (as `hud::draw`'s).
    pub fn scale(&self) -> f32 {
        (self.screen_h / 720.0).max(0.5)
    }
}

/// The edges a host remembers between frames: the previous frame's keys and pointer, so a fresh
/// press/click fires once. `Client` and the shell `MenuState` each keep their own.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MenuPrev {
    pub keys: KeySet,
    pub mouse_x: f32,
    pub mouse_y: f32,
    pub buttons: u32,
}

impl MenuPrev {
    /// `k` went down this frame (fresh press).
    pub fn edge(&self, now: &MenuPointer, k: Key) -> bool {
        now.keys.is_down(k) && !self.keys.is_down(k)
    }

    /// The left button went down this frame.
    pub fn clicked(&self, now: &MenuPointer) -> bool {
        now.mouse_buttons & crate::input::mouse_buttons::LEFT != 0 && self.buttons & crate::input::mouse_buttons::LEFT == 0
    }

    /// The cursor moved since the previous frame (beyond the jitter threshold).
    pub fn moved(&self, now: &MenuPointer) -> bool {
        (now.mouse_x - self.mouse_x).abs() > MOUSE_MOVED || (now.mouse_y - self.mouse_y).abs() > MOUSE_MOVED
    }

    /// Remember this frame for the next.
    pub fn update(&mut self, now: &MenuPointer) {
        self.keys = now.keys;
        self.mouse_x = now.mouse_x;
        self.mouse_y = now.mouse_y;
        self.buttons = now.mouse_buttons;
    }
}

/// One frame of the pick screen, folded: navigation (hover, arrows, the wheel) and the intents a
/// host turns into its own behavior — `activate` is a row the player confirmed (in-game it picks;
/// the shell opens the editor on it), `delete`/`copy` name a *profile* slot (shell only). Escape is
/// reported in `back` (both hosts close on it). The `S`/`O` dialog keys are read by the shell host
/// itself (`menu.rs`), which owns the dialogs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PickFrame {
    /// The highlighted row after this frame's navigation (clamped to the list).
    pub highlight: usize,
    /// The highlight moved this frame (hover, arrows or the wheel) without an activation.
    pub navigated: bool,
    /// A row was activated (a number key, Enter or a click).
    pub activate: Option<usize>,
    /// Backspace on a custom row: the profile slot to delete.
    pub delete: Option<usize>,
    /// C on a custom row: the profile slot to copy into the set's next free slot.
    pub copy: Option<usize>,
    /// A clickable footer action (the shell's set/class options and back affordances).
    pub footer: Option<PickFooterAction>,
    /// Left/Right on the shell's list: move the active class set by this many (−1/+1, wrapping).
    /// A match ignores it (the set is picked in the lobby) and shows the pager as a read-only
    /// display of the active set.
    pub set_step: i32,
    /// Escape went down this frame.
    pub back: bool,
}

/// Fold one frame of pick-screen input. `highlight` is the host's stored row; `manage` selects the
/// shell's list (with CREATE A CLASS and the footer actions) or the in-match pick-only list. Pure
/// apart from reading the two registries (the list is rebuilt on demand, like the draw).
pub fn pick_frame(inp: &MenuPointer, prev: &MenuPrev, highlight: usize, manage: bool) -> PickFrame {
    let rows = pick_rows(manage);
    let mut out = PickFrame::default();
    if rows.is_empty() {
        return out;
    }
    let mut hl = highlight.min(rows.len() - 1);
    let s = inp.scale();
    let layout = pick_layout(inp.screen_w, inp.screen_h, s);
    let first = pick_first(hl, pick_visible(&layout));
    let hover = pick_row_at(&layout, inp.mouse_x, inp.mouse_y, rows.len(), first);
    if prev.moved(inp) {
        if let Some(i) = hover {
            if i != hl {
                hl = i;
                out.navigated = true;
            }
        }
    }
    let mut step = 0i32;
    if prev.edge(inp, Key::Up) {
        step -= 1;
    }
    if prev.edge(inp, Key::Down) {
        step += 1;
    }
    if inp.wheel.abs() >= 1.0 {
        step += inp.wheel.signum() as i32;
    }
    if step != 0 {
        hl = (hl as i32 + step).rem_euclid(rows.len() as i32) as usize;
        out.navigated = true;
    }

    // A number key, Enter or a click activates the highlighted row.
    for (i, key) in PICK_KEYS.iter().enumerate() {
        if prev.edge(inp, *key) && i < rows.len() {
            hl = i;
            out.activate = Some(i);
        }
    }
    if prev.edge(inp, Key::Enter) {
        out.activate = Some(hl);
    }
    if prev.clicked(inp) {
        if let Some(i) = hover {
            hl = i;
            out.activate = Some(i);
        } else {
            // The shell's footer prompts (set/class options, back) are clickable.
            out.footer = pick_footer_at(&layout, inp.screen_w, s, inp.mouse_x, inp.mouse_y, manage);
        }
    }
    if out.activate.is_none() && out.footer.is_none() {
        if prev.edge(inp, Key::Backspace) {
            if let Some(PickRow::Custom(j)) = rows.get(hl).copied() {
                out.delete = Some(j);
            }
        } else if prev.edge(inp, Key::C) {
            if let Some(PickRow::Custom(j)) = rows.get(hl).copied() {
                out.copy = Some(j);
            }
        }
        // The shell's set pager (BO2's shoulders): Left/Right move the active class set. A match
        // is pick-only, so it never changes the set the fight loads into.
        if manage {
            if prev.edge(inp, Key::Left) {
                out.set_step -= 1;
            }
            if prev.edge(inp, Key::Right) {
                out.set_step += 1;
            }
        }
    }
    out.back = prev.edge(inp, Key::Escape);
    out.highlight = hl;
    out
}

/// The editor's input for one frame, folded: key edges and the pointer, hit-tested against the
/// editor's own layout; the pure `classmenu` state machine does the walking. The result carries
/// everything a host needs for its side effects: the outcome (the host owns saving/cancelling),
/// the deny flag (the host plays the cue and lights the rim) and the navigation/selection cue the
/// frame calls for, by the same rules the in-game screen uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorFrame {
    pub outcome: crate::classmenu::Outcome,
    /// The last choice was refused (over budget, an attachment cap).
    pub denied: bool,
    /// `R` (or the box's `X`) cleared the highlighted slot this frame.
    pub removed: bool,
    /// The frame's UI cue, if any.
    pub cue: Option<&'static str>,
}

/// Fold one frame of editor input. `editor` is the live session; it is always returned in place
/// (the hosts drop it on Save/Cancel).
pub fn editor_frame(editor: &mut Editor, inp: &MenuPointer, prev: &MenuPrev) -> EditorFrame {
    use crate::classmenu::Edges;
    let s = inp.scale();
    let layout = editor_layout(inp.screen_w, inp.screen_h, s);
    let mut edges = Edges::between(&prev.keys, &inp.keys);
    let clicked = prev.clicked(inp);
    let moved = prev.moved(inp);
    // A weapon picker is two-dimensional, as BO2's is: Left/Right walk the category column, Up/Down
    // the guns inside the active category. The fold consumes those edges here, before the pure
    // editor sees them, so the highlighted option always stays inside the drawn category.
    let cats = if editor.picking && !editor.naming { weapon_categories(editor) } else { Vec::new() };
    if !cats.is_empty() {
        if edges.left {
            step_category(editor, &cats, -1);
        }
        if edges.right {
            step_category(editor, &cats, 1);
        }
        if edges.up {
            step_within_category(editor, &cats, -1);
        }
        if edges.down {
            step_within_category(editor, &cats, 1);
        }
        edges.up = false;
        edges.down = false;
        edges.left = false;
        edges.right = false;
    }
    let cells = editor_cells(&layout, editor);
    // The box's circled X (mouse) and `R` (keyboard) clear the highlighted slot, BO2's `R Remove`.
    let mut removed = false;
    if !editor.picking && !editor.naming {
        if let Some((slot, index)) = editor_remove_at(&cells, editor, s, inp.mouse_x, inp.mouse_y).filter(|_| clicked) {
            removed = clear_cell(&mut editor.draft, slot, index);
        } else if prev.edge(inp, Key::R) {
            removed = clear_slot(&mut editor.draft, editor.slot);
        }
    }
    // Hit-test every frame, not only when the cursor moved: a click with the cursor at rest is
    // still a click on the row under it. `moved` gates hover-only highlight changes (see
    // `classmenu::PointerEvent`), so a resting cursor cannot fight the keyboard.
    // The cells repeat a slot (three bonus-card boxes, two attachment boxes), so a hit box maps to
    // the walk's slot index, not to its own.
    let slots = editor.slots();
    let slot = editor_cell_at(&cells, inp.mouse_x, inp.mouse_y)
        .and_then(|i| cells.get(i))
        .and_then(|c| slots.iter().position(|s| *s == c.slot));
    let options = if editor.picking { editor.options() } else { Vec::new() };
    let (start, _) = picker_window(&layout, editor.slot, editor.option);
    let drawn = if editor.picking {
        picker_option_at(&layout, editor.slot, inp.mouse_x, inp.mouse_y, start, drawn_option_count(editor, &cats))
    } else {
        None
    };
    // A weapon picker's drawn rows are the active category's guns; every other picker's are the
    // option list itself.
    let option = drawn.map(|row| {
        if cats.is_empty() {
            start + row
        } else {
            let cat = &cats[category_of(&cats, editor.option)];
            cat.options.get(start + row).copied().unwrap_or(editor.option)
        }
    });
    let _ = options;
    let pointer = crate::classmenu::PointerEvent { slot, option, click: clicked && !removed, wheel: inp.wheel.trunc() as i32, moved };
    let before_slot = editor.slot;
    let before_option = editor.option;
    let before_picking = editor.picking;
    let before_naming = editor.naming;
    let before_draft = editor.draft.clone();
    // Hover moves the highlight only on a real move; a click/wheel is always an action. While the
    // name field has text entry, keys win unless the frame also clicked (to leave the field).
    let hover = moved && !editor.naming && (pointer.slot.is_some() || pointer.option.is_some());
    let has_pointer = pointer.is_action() || hover || (clicked && !removed);
    let outcome = if has_pointer { editor.step_pointer(pointer) } else { editor.step(edges) };
    let denied = editor.denied;
    editor.denied = false;
    // Opening a list or the name field is a select; closing one after choosing is a select, while
    // backing out of it is BO2's back cue. A slot/option move is the navigate cue, and removing a
    // slot is BO2's remove (the back cue).
    let cue = if removed {
        Some(crate::sounds::UI_BACK)
    } else {
        match outcome {
            crate::classmenu::Outcome::Continue => {
                if editor.picking != before_picking || editor.naming != before_naming {
                    let opened = editor.picking || editor.naming;
                    let chose = editor.draft != before_draft;
                    Some(if opened || chose { crate::sounds::UI_SELECT } else { crate::sounds::UI_BACK })
                } else if editor.slot != before_slot || editor.option != before_option {
                    Some(crate::sounds::UI_NAVIGATE)
                } else if editor.draft != before_draft {
                    Some(crate::sounds::UI_SELECT)
                } else {
                    None
                }
            }
            _ => None,
        }
    };
    EditorFrame { outcome, denied, removed, cue }
}

/// How many rows the open picker draws: the active category's guns, or the option list itself.
fn drawn_option_count(editor: &Editor, cats: &[Category]) -> usize {
    if cats.is_empty() {
        editor.options().len()
    } else {
        cats[category_of(cats, editor.option)].options.len()
    }
}

// ------------------------------------------------------------------------------------------------
// The shell's dialogs (BO2's `S`/`O` menus)
// ------------------------------------------------------------------------------------------------

/// Which of the shell's two dialogs is up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogKind {
    /// BO2's `S Class Set Options` (`a-chooseclass/021`).
    SetOptions,
    /// BO2's `O Class Options` (`a-chooseclass/023`).
    ClassOptions,
}

impl DialogKind {
    /// The dialog's title, BO2's heading.
    pub fn title(self) -> &'static str {
        match self {
            DialogKind::SetOptions => "CLASS SET OPTIONS",
            DialogKind::ClassOptions => "CLASS OPTIONS",
        }
    }
}

/// One entry of a shell dialog. BO2's set dialog offers RENAME and COPY; its class dialog RENAME,
/// COPY, RESET TO DEFAULT and CLEAR LOADOUT (`a-chooseclass/021`, `/023`). Our profile has no
/// set-copy operation, so the set dialog offers RENAME and RESET instead of COPY.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogItem {
    /// Rename the set (or the highlighted class): opens the dialog's text field.
    Rename,
    /// Copy the highlighted class into the set's next free slot.
    Copy,
    /// Empty the set's custom slots (BO2's set dialog has no reset; this is ours).
    ResetSet,
    /// Put BO2's default kit back into the highlighted class.
    ResetDefault,
    /// Empty the highlighted class's slots, keeping its name and slot.
    ClearLoadout,
}

impl DialogItem {
    /// The entry's label, as the dialog draws it.
    pub fn label(self) -> &'static str {
        match self {
            DialogItem::Rename => "RENAME",
            DialogItem::Copy => "COPY",
            DialogItem::ResetSet => "RESET SET",
            DialogItem::ResetDefault => "RESET TO DEFAULT",
            DialogItem::ClearLoadout => "CLEAR LOADOUT",
        }
    }
}

/// What one frame of dialog input asks the host to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogOutcome {
    /// Nothing (a hover, a move).
    Continue,
    /// The player confirmed an entry (`Dialog::buffer` carries the typed name on a rename).
    Confirm(DialogItem),
    /// Escape: close the dialog.
    Back,
}

/// The shell's open dialog: its kind, the name it acts on, the entry list and the text field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dialog {
    pub kind: DialogKind,
    /// The line under the title: the set's name, or the highlighted class's name (BO2's subtitle).
    pub subtitle: String,
    pub items: Vec<DialogItem>,
    pub highlight: usize,
    /// The text field is open (a rename is being typed).
    pub naming: bool,
    /// The text field's contents.
    pub buffer: String,
}

impl Dialog {
    /// The class-set dialog for `set_name`.
    pub fn set_options(set_name: &str) -> Dialog {
        Dialog {
            kind: DialogKind::SetOptions,
            subtitle: set_name.to_string(),
            items: vec![DialogItem::Rename, DialogItem::ResetSet],
            highlight: 0,
            naming: false,
            buffer: String::new(),
        }
    }

    /// The class dialog for the class called `class_name`.
    pub fn class_options(class_name: &str) -> Dialog {
        Dialog {
            kind: DialogKind::ClassOptions,
            subtitle: class_name.to_string(),
            items: vec![DialogItem::Rename, DialogItem::Copy, DialogItem::ResetDefault, DialogItem::ClearLoadout],
            highlight: 0,
            naming: false,
            buffer: String::new(),
        }
    }

    /// The highlighted entry.
    pub fn item(&self) -> Option<DialogItem> {
        self.items.get(self.highlight).copied()
    }

    /// Start the text field on the highlighted rename, seeded with `text`.
    pub fn begin_rename(&mut self, text: &str) {
        self.naming = true;
        self.buffer = text.chars().take(crate::classmenu::NAME_MAX).collect();
    }
}

/// The typeable character of a key (`A`-`Z`, the digits and Space), for the dialog's text field.
fn typed_char(k: Key) -> Option<char> {
    match k as u8 {
        0..=25 => Some((b'A' + k as u8) as char),
        26..=31 => Some((b'0' + (k as u8 - 26 + 1)) as char),
        45..=48 => Some(['7', '8', '9', '0'][(k as u8 - 45) as usize]),
        _ if k == Key::Space => Some(' '),
        _ => None,
    }
}

/// Fold one frame of dialog input: Up/Down walk the entries, Enter confirms (or commits the typed
/// name), Escape backs out, a click confirms the entry under the cursor.
pub fn dialog_step(d: &mut Dialog, prev: &MenuPrev, now: &MenuPointer) -> DialogOutcome {
    if d.naming {
        for k in Key::ALL.iter().copied() {
            if prev.edge(now, k) {
                if let Some(c) = typed_char(k) {
                    if d.buffer.chars().count() < crate::classmenu::NAME_MAX {
                        d.buffer.push(c);
                    }
                }
            }
        }
        if prev.edge(now, Key::Backspace) {
            d.buffer.pop();
        }
        if prev.edge(now, Key::Enter) {
            d.naming = false;
            return DialogOutcome::Confirm(DialogItem::Rename);
        }
        if prev.edge(now, Key::Escape) || prev.clicked(now) {
            d.naming = false;
        }
        return DialogOutcome::Continue;
    }
    let n = d.items.len();
    if n > 0 {
        if prev.edge(now, Key::Up) {
            d.highlight = (d.highlight + n - 1) % n;
        }
        if prev.edge(now, Key::Down) {
            d.highlight = (d.highlight + 1) % n;
        }
    }
    if prev.edge(now, Key::Enter) {
        if let Some(item) = d.item() {
            return DialogOutcome::Confirm(item);
        }
    }
    if prev.clicked(now) {
        let s = now.scale();
        if let Some(i) = dialog_item_at(d, now.screen_w, now.screen_h, s, now.mouse_x, now.mouse_y) {
            d.highlight = i;
            if let Some(item) = d.item() {
                return DialogOutcome::Confirm(item);
            }
        }
    }
    if prev.edge(now, Key::Escape) {
        return DialogOutcome::Back;
    }
    DialogOutcome::Continue
}

/// The dialog's box on screen (BO2's centred panel).
fn dialog_box(w: f32, h: f32, s: f32, items: usize) -> (f32, f32, f32, f32) {
    let bw = 460.0 * s;
    let bh = (120.0 + items as f32 * 30.0) * s;
    ((w - bw) * 0.5, (h - bh) * 0.5, bw, bh)
}

/// The entry row under a screen point, if any.
fn dialog_item_at(d: &Dialog, w: f32, h: f32, s: f32, x: f32, y: f32) -> Option<usize> {
    let (bx, by, bw, _) = dialog_box(w, h, s, d.items.len());
    if x < bx || x > bx + bw {
        return None;
    }
    let top = by + (76.0 + if d.naming { 26.0 } else { 0.0 }) * s;
    let row = 30.0 * s;
    if y < top {
        return None;
    }
    let i = ((y - top) / row) as usize;
    (i < d.items.len()).then_some(i)
}

// ------------------------------------------------------------------------------------------------
// Shared chrome
// ------------------------------------------------------------------------------------------------

/// The full-screen BO2 menu: background art, backdrop, grid and glow, with the dark fallback.
///
/// BO2's create-a-class screens sit on a near-black panel (`b-editor/001`): the art shows through a
/// dark scrim, so the boxes, rows and text read the same at any map brightness.
fn background(prims: &mut Vec<Prim>, w: f32, h: f32) {
    prims.push(Prim::Image {
        name: sprites::MENU_BACKGROUND.to_string(),
        x: 0.0,
        y: 0.0,
        w,
        h,
        argb: 0xFFFF_FFFF,
        fallback: vec![Prim::Rect { x: 0.0, y: 0.0, w, h, argb: 0xF010_1214 }],
    });
    prims.push(Prim::Image {
        name: sprites::MENU_BACKDROP.to_string(),
        x: 0.0,
        y: 0.0,
        w,
        h,
        argb: 0xE0FF_FFFF,
        fallback: Vec::new(),
    });
    prims.push(Prim::Image {
        name: sprites::MENU_GRID.to_string(),
        x: 0.0,
        y: 0.0,
        w,
        h,
        argb: 0x30FF_FFFF,
        fallback: Vec::new(),
    });
    prims.push(Prim::Rect { x: 0.0, y: 0.0, w, h, argb: 0xD814_1618 });
}

/// BO2's heading band: the three-piece GunTitle art across the content's width (BO2 puts the heading
/// at the top left, above a rule — `a-chooseclass/001`, `b-editor/001`).
fn banner(prims: &mut Vec<Prim>, x: f32, w: f32, s: f32, y: f32, header_h: f32) -> f32 {
    let cap = header_h * 0.5;
    let mid_w = (w - 2.0 * cap).max(40.0 * s);
    let mut fallback = hud::slant_box(x + cap, y, mid_w, header_h, PANEL, Some(OUTLINE));
    fallback.push(Prim::Rect { x: x + cap + 4.0 * s, y: y + 6.0 * s, w: 3.0 * s, h: header_h - 12.0 * s, argb: GOLD });
    prims.push(Prim::Image { name: sprites::GUN_TITLE[0].to_string(), x, y, w: cap, h: header_h, argb: 0xFFFF_FFFF, fallback: Vec::new() });
    prims.push(Prim::Image { name: sprites::GUN_TITLE[1].to_string(), x: x + cap, y, w: mid_w, h: header_h, argb: 0xFFFF_FFFF, fallback });
    prims.push(Prim::Image { name: sprites::GUN_TITLE[2].to_string(), x: x + cap + mid_w, y, w: cap, h: header_h, argb: 0xFFFF_FFFF, fallback: Vec::new() });
    x + cap + 16.0 * s
}

/// A screen's heading in the band.
fn heading(prims: &mut Vec<Prim>, x: f32, y: f32, s: f32, text: &str, argb: u32) {
    prims.push(Prim::Text { x, y: y + 10.0 * s, text: text.to_string(), argb, size: (24.0 * s) as i32, align: 0 });
}

/// A slanted list row in BO2's slate: a ROW_FILL/ROW_HIGHLIGHT bar with a ROW_LINE under it.
///
/// The converted row art is a whole framed tile, so stretching it to a list row's width smears its
/// border across the row (the drawn bar reads as BO2's plain row instead); the thin ROW_LINE art is
/// a line and stretches correctly.
fn row(prims: &mut Vec<Prim>, x: f32, y: f32, w: f32, h: f32, s: f32, active: bool) {
    let fill = if active { ROW_ACTIVE } else { ROW_FILL };
    let edge = if active { Some(GOLD) } else { Some(ROW_EDGE) };
    prims.extend(hud::slant_box(x, y, w, h, fill, edge));
    if active {
        prims.push(Prim::Rect { x: x + 4.0 * s, y: y + 4.0 * s, w: 3.0 * s, h: h - 8.0 * s, argb: GOLD });
    }
    prims.push(Prim::Image {
        name: sprites::ROW_LINE.to_string(),
        x,
        y: y + h - 2.0 * s,
        w,
        h: 1.5 * s,
        argb: 0x80FF_FFFF,
        fallback: vec![Prim::Rect { x, y: y + h - 2.0 * s, w, h: 1.5 * s, argb: ROW_EDGE }],
    });
}

/// A panel: BO2's dark plate. The converted `PANEL_FILL`/`PANEL_LINE` art are framed tiles (an
/// orange border and a zigzag rule) that smear when stretched to a panel, so the plate draws its own
/// fill and outline, exactly like the missing-art fallbacks did.
fn panel(prims: &mut Vec<Prim>, x: f32, y: f32, w: f32, h: f32, s: f32) {
    prims.push(Prim::Rect { x, y, w, h, argb: PANEL_FILL });
    hud::outline_rect(prims, x, y, w, h, 1.5 * s, OUTLINE);
}

/// The footer strip: a slanted gold-edged command line, BO2's prompt strip.
fn footer_bar(prims: &mut Vec<Prim>, w: f32, s: f32, y: f32) {
    let x = w * 0.05;
    prims.extend(hud::slant_box(x, y, w - 2.0 * x, 26.0 * s, 0x8010_1214, Some(GOLD)));
}

/// One footer prompt: BO2's gold key then the white label. Returns the prompt's width.
fn draw_prompt(prims: &mut Vec<Prim>, x: f32, y: f32, s: f32, p: &Prompt) -> f32 {
    let size = (PROMPT_SIZE * s) as i32;
    let adv = PROMPT_SIZE * 6.0 / 7.0 * s;
    prims.push(Prim::Text { x, y, text: p.key.to_string(), argb: GOLD, size, align: 0 });
    let tx = x + (p.key.chars().count() as f32 + 1.0) * adv;
    prims.push(Prim::Text { x: tx, y, text: p.label.to_string(), argb: WHITE, size, align: 0 });
    prompt_width(p, s)
}

/// The prompt strip: `left` prompts from the left margin, `right` prompts ending at the right one.
fn footer_prompts(prims: &mut Vec<Prim>, w: f32, s: f32, y: f32, left: &[Prompt], right: &[Prompt]) {
    footer_bar(prims, w, s, y);
    let ty = y + 7.0 * s;
    let gap = 24.0 * s;
    let mut x = w * 0.05 + 12.0 * s;
    for p in left {
        x += draw_prompt(prims, x, ty, s, p) + gap;
    }
    let right_edge = w * 0.95 - 12.0 * s;
    let mut x = right_edge - right.iter().map(|p| prompt_width(p, s) + gap).sum::<f32>() + gap;
    for p in right {
        x += draw_prompt(prims, x, ty, s, p) + gap;
        let _ = x;
    }
}

/// A red rim while a denied choice's flash is up.
fn deny_flash(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, flash: f32) {
    if flash <= 0.0 {
        return;
    }
    let a = ((flash / DENY_FLASH).min(1.0) * 150.0) as u32;
    let col = (a << 24) | (RED & 0x00FF_FFFF);
    let t = 5.0 * s;
    prims.push(Prim::Rect { x: 0.0, y: 0.0, w, h: t, argb: col });
    prims.push(Prim::Rect { x: 0.0, y: h - t, w, h: t, argb: col });
    prims.push(Prim::Rect { x: 0.0, y: 0.0, w: t, h, argb: col });
    prims.push(Prim::Rect { x: w - t, y: 0.0, w: t, h, argb: col });
}

// ------------------------------------------------------------------------------------------------
// The allocation readout
// ------------------------------------------------------------------------------------------------

/// The allocation bar's lit segments: BO2 lights one per point spent (`allocationbar.lua`'s
/// `AddBars`, clamped to `Engine.GetMaxAllocation`).
pub fn allocation_lit(spent: i32, total: i32) -> usize {
    spent.clamp(0, total.max(0)) as usize
}

/// BO2's segmented allocation bar: ten blocks, one per point, `spent` of them orange.
pub fn draw_allocation(prims: &mut Vec<Prim>, x: f32, y: f32, w: f32, s: f32, spent: i32, total: i32) {
    let n = total.max(1) as usize;
    let gap = 3.0 * s;
    let seg_w = ((w - gap * (n as f32 - 1.0)) / n as f32).max(2.0);
    let lit = allocation_lit(spent, total);
    for i in 0..n {
        prims.push(Prim::Rect {
            x: x + i as f32 * (seg_w + gap),
            y,
            w: seg_w,
            h: 6.0 * s,
            argb: if i < lit { ORANGE } else { BAR_FREE },
        });
    }
}

/// BO2's `n/10` counter, right-aligned at `right` (BO2's numerals are its digit art; a plain text
/// fallback keeps it legible when the art is small or missing).
pub fn draw_allocation_counter(prims: &mut Vec<Prim>, right: f32, y: f32, s: f32, spent: i32, total: i32) {
    let col = if spent > total { RED } else { ORANGE };
    prims.push(Prim::Text {
        x: right,
        y,
        text: format!("{}/{}", spent.max(0), total.max(0)),
        argb: col,
        size: (20.0 * s) as i32,
        align: 2,
    });
}

// ------------------------------------------------------------------------------------------------
// Element icons
// ------------------------------------------------------------------------------------------------

/// A weapon/perk/attachment/wildcard icon with a text fallback (the first letters of its name).
/// `size` is already scaled; the fallback text scales with it.
fn icon(prims: &mut Vec<Prim>, name: Option<&str>, x: f32, y: f32, size: f32, argb: u32, fallback: &str) {
    let short: String = fallback.chars().filter(|c| !c.is_whitespace()).take(3).collect();
    let fallback = vec![Prim::Text { x: x + size * 0.5, y: y + size * 0.28, text: short, argb, size: (size * 0.38) as i32, align: 1 }];
    match name {
        Some(n) => prims.push(Prim::Image { name: n.to_string(), x, y, w: size, h: size, argb, fallback }),
        None => prims.extend(fallback),
    }
}

/// The per-weapon HUD icon the converter emits (`ZHBO_BO2_Icon_<id>`); an attached variant uses its
/// base gun's icon (`an94_mp+grip` -> `ZHBO_BO2_Icon_an94_mp`).
fn weapon_icon(prims: &mut Vec<Prim>, id: &str, x: f32, y: f32, size: f32, argb: u32, label: &str) {
    let base = id.split('+').next().unwrap_or(id);
    let name = format!("ZHBO_BO2_Icon_{base}");
    icon(prims, Some(&name), x, y, size, argb, label);
}

/// A weapon id's base gun (the icon and catalog entry without its attachments).
fn base_id(id: &str) -> &str {
    id.split('+').next().unwrap_or(id)
}

// ------------------------------------------------------------------------------------------------
// Weapon stat bars
// ------------------------------------------------------------------------------------------------

/// BO2's four menu bars, each 0..1. They are normalised against the gun catalog's own extremes, so
/// no number is hand-copied here.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponStats {
    pub damage: f32,
    pub range: f32,
    pub fire_rate: f32,
    pub accuracy: f32,
}

/// BO2's bar labels, in the order the weapon screens list them (`c-primary/001`: FIRE RATE, DAMAGE,
/// RANGE, ACCURACY).
pub const STAT_LABELS: [&str; 4] = ["FIRE RATE", "DAMAGE", "RANGE", "ACCURACY"];

/// The bars' segments per row, as BO2's are (`c-primary/001`).
pub const STAT_SEGMENTS: usize = 18;

/// One bar's value by row index, in [`STAT_LABELS`] order.
pub fn stat_value(stats: &WeaponStats, row: usize) -> f32 {
    match row {
        0 => stats.fire_rate,
        1 => stats.damage,
        2 => stats.range,
        _ => stats.accuracy,
    }
}

/// How many of a bar's segments are lit.
pub fn stat_lit(value: f32) -> usize {
    (value.clamp(0.0, 1.0) * STAT_SEGMENTS as f32).round() as usize
}

/// The catalog's gun extremes, in one pass (no allocation).
fn gun_bounds() -> (f32, f32, f32, f32, f32) {
    let guns = || weapons::catalog().all().iter().copied().filter(|d| d.class.is_gun());
    let max_damage = guns().map(|d| d.damage[0]).fold(0.0, f32::max);
    let max_range = guns().map(|d| d.max_range).fold(0.0, f32::max);
    let max_rpm = guns().map(|d| d.rounds_per_minute()).fold(0.0, f32::max);
    let min_spread = guns().map(|d| d.hip_spread[0].0).fold(f32::MAX, f32::min);
    let max_spread = guns().map(|d| d.hip_spread[0].0).fold(0.0, f32::max);
    (max_damage, max_range, max_rpm, min_spread, max_spread)
}

/// One gun's bars, derived only from its table fields: first damage point, max range, rounds per
/// minute, and the standing hip spread (tighter is more accurate).
pub fn weapon_stats(def: &WeaponDef) -> WeaponStats {
    let (max_damage, max_range, max_rpm, min_spread, max_spread) = gun_bounds();
    let ratio = |v: f32, max: f32| if max > 0.0 { (v / max).clamp(0.0, 1.0) } else { 0.0 };
    let accuracy = if max_spread > min_spread {
        1.0 - ((def.hip_spread[0].0 - min_spread) / (max_spread - min_spread)).clamp(0.0, 1.0)
    } else {
        1.0
    };
    WeaponStats {
        damage: ratio(def.damage[0], max_damage),
        range: ratio(def.max_range, max_range),
        fire_rate: ratio(def.rounds_per_minute(), max_rpm),
        accuracy,
    }
}

/// BO2's segmented stat readout: four rows of [`STAT_SEGMENTS`] blocks, FIRE RATE first. Spent
/// blocks are white, free ones BO2's dark slots (`c-primary/001`).
fn stat_bars(prims: &mut Vec<Prim>, x: f32, y: f32, w: f32, s: f32, stats: &WeaponStats) {
    let row_h = 19.0 * s;
    let label_w = 78.0 * s;
    let bar_x = x + label_w;
    let bar_w = (w - label_w).max(40.0 * s);
    let gap = 3.0 * s;
    let seg_w = ((bar_w - gap * (STAT_SEGMENTS as f32 - 1.0)) / STAT_SEGMENTS as f32).max(1.0);
    for (i, label) in STAT_LABELS.iter().enumerate() {
        let top = y + i as f32 * row_h;
        prims.push(Prim::Text { x, y: top + 2.0 * s, text: (*label).to_string(), argb: DIM, size: (10.0 * s) as i32, align: 0 });
        let lit = stat_lit(stat_value(stats, i));
        for k in 0..STAT_SEGMENTS {
            prims.push(Prim::Rect {
                x: bar_x + k as f32 * (seg_w + gap),
                y: top + 3.0 * s,
                w: seg_w,
                h: 8.0 * s,
                argb: if k < lit { 0xD8FF_FFFF } else { 0x4810_1214 },
            });
        }
    }
}

// ------------------------------------------------------------------------------------------------
// The hosts' shared view
// ------------------------------------------------------------------------------------------------

/// What a host shows on the class screen this frame: the pick list (and its highlighted row) or
/// the open editor, plus the shared deny-rim counter. The in-game `Client` and the shell
/// `MenuState` both produce one; the drawing below never sees the hosts themselves.
pub struct ScreenView<'a> {
    /// The pick list's highlighted row (unused while the editor is up).
    pub highlight: usize,
    /// The open allocation editor, if any (shell only; a match is pick-only).
    pub editor: Option<&'a Editor>,
    /// Seconds left of the deny rim (a refused choice).
    pub deny: f32,
    /// The shell screen: CREATE A CLASS row, set/class options. False in a match.
    pub manage: bool,
}

/// Draw the class screen for a host: the editor while one is open, else the pick list.
pub fn draw_screen(prims: &mut Vec<Prim>, w: f32, h: f32, view: &ScreenView) {
    let s = (h / 720.0).max(0.5);
    match view.editor {
        Some(editor) => draw_editor(prims, w, h, s, editor, view.deny),
        None => draw_pick(prims, w, h, s, view.highlight, view.deny, view.manage),
    }
}

// ------------------------------------------------------------------------------------------------
// The pick screen
// ------------------------------------------------------------------------------------------------

/// BO2's choose-class screen: the heading and the class-set pager across the top, the list of
/// presets, the player's customs and (in the shell) CREATE A CLASS down the left with BO2's hint
/// under it, the highlighted class's kit on the right, the prompts at the bottom.
/// `highlighted` is the host's local row (the in-game client's or the shell menu's); the keys and
/// mouse drive it through [`pick_frame`]. `manage` selects the shell's list and footer actions.
pub fn draw_pick(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, highlighted: usize, deny: f32, manage: bool) {
    background(prims, w, h);
    let l = pick_layout(w, h, s);

    // Snapshot both registries once: the rows, the pager and the details panel must describe the
    // same lists.
    let presets = classes::get();
    let prof = profile::get();
    let customs: &[crate::classes::CustomClass] = prof.customs();
    let list = PickList {
        rows: pick_rows_for(presets.len(), customs.len(), manage),
        set: prof.active_set(),
        sets: profile::MAX_SETS,
        set_name: prof.set_name(prof.active_set()),
    };
    let rows = &list.rows;
    let title_x = banner(prims, l.content_x, l.content_w, s, l.title_y, l.header_h);
    heading(prims, title_x, l.title_y, s, "CHOOSE CLASS", WHITE);
    if rows.is_empty() {
        // A match with no registry and no customs: draw the chrome, never index an empty list.
        footer_prompts(prims, w, s, l.footer_y, &[Prompt { key: "ESC", label: "Close" }], &[]);
        deny_flash(prims, w, h, s, deny);
        return;
    }
    let active = highlighted.min(rows.len() - 1);

    // The class-set pager over the list (`classsetpager` in BO2): `‹ CLASS SET n ›` with the ten-dot
    // marker under it. The shell steps it with Left/Right; a match shows the active set read-only.
    set_pager(prims, s, l.list_x + l.list_w * 0.5, l.pager_y, &list, manage);
    // The highlighted class's allocation, top right in the heading, with BO2's segmented bar under
    // it (the same readout the editor carries).
    let spent = class_allocation(&rows[active], &presets, customs);
    let (bx, by, bw) = allocation_bar_rect(&l, s);
    draw_allocation_counter(prims, bx + bw, l.title_y + 12.0 * s, s, spent, crate::classmenu::PICK_10);
    draw_allocation(prims, bx, by, bw, s, spent, crate::classmenu::PICK_10);

    panel(prims, l.list_x - 8.0 * s, l.list_y - 8.0 * s, l.list_w + 16.0 * s, l.list_h + 16.0 * s, s);
    let visible = pick_visible(&l);
    let first = pick_first(active, visible);
    for (i, r) in rows.iter().enumerate().skip(first).take(visible) {
        let y = l.list_y + (i - first) as f32 * l.row_h;
        let on = i == active;
        row(prims, l.list_x, y, l.list_w, l.row_h - 6.0 * s, s, on);
        // BO2's slot numeral (1-10 by key), or a plus for the new-class row.
        let (name, primary, primary_id) = match *r {
            PickRow::Preset(j) => {
                let c = &presets[j];
                let kit = classes::loadout_of(c);
                (c.name.clone(), weapons::weapon(&kit.primary).display_name.clone(), kit.primary.clone())
            }
            PickRow::Custom(j) => {
                let c = &customs[j];
                (c.name.clone(), weapons::weapon(&c.primary).display_name.clone(), c.primary.clone())
            }
            PickRow::Create => ("CREATE A CLASS".to_string(), "Ten points of allocation".to_string(), String::new()),
        };
        if matches!(r, PickRow::Create) {
            prims.push(Prim::Text { x: l.list_x + 22.0 * s, y: y + 4.0 * s, text: "+".to_string(), argb: GOLD, size: (22.0 * s) as i32, align: 0 });
        } else {
            hud::draw_number(prims, i as u32 + 1, l.list_x + 44.0 * s, y + 2.0 * s, 24.0 * s, if on { GOLD } else { WHITE });
        }
        let tx = l.list_x + 62.0 * s;
        prims.push(Prim::Text { x: tx, y: y + 3.0 * s, text: name, argb: if on { ORANGE } else { WHITE }, size: (14.0 * s) as i32, align: 0 });
        let icon_x = l.list_x + l.list_w - 48.0 * s;
        let icon_y = y + 3.0 * s;
        if primary_id.is_empty() {
            prims.push(Prim::Text { x: icon_x - 4.0 * s, y: y + 17.0 * s, text: primary, argb: DIM, size: (10.0 * s) as i32, align: 2 });
        } else {
            weapon_icon(prims, &primary_id, icon_x, icon_y, 24.0 * s, if on { WHITE } else { DIM }, &primary);
            prims.push(Prim::Text { x: icon_x - 8.0 * s, y: y + 17.0 * s, text: primary, argb: DIM, size: (10.0 * s) as i32, align: 2 });
        }
    }

    if rows.len() > visible {
        prims.push(Prim::Text {
            x: l.list_x + l.list_w - 6.0 * s,
            y: l.list_y + l.list_h + 2.0 * s,
            text: format!("{} / {}", active + 1, rows.len()),
            argb: DIM,
            size: (10.0 * s) as i32,
            align: 2,
        });
    }
    if manage {
        // BO2's hint under the class list (a-chooseclass/001). It is the shell's alone: a match is
        // already running, so the set is not what the fight loads into.
        chevron(prims, l.list_x + 6.0 * s, l.hint_y + 3.0 * s, 6.0 * s, ORANGE, 1);
        prims.push(Prim::Text {
            x: l.list_x + 20.0 * s,
            y: l.hint_y,
            text: "You will load into the game with this class set.".to_string(),
            argb: DIM,
            size: (12.0 * s) as i32,
            align: 0,
        });
    }
    draw_pick_details(prims, &l, s, &rows[active], &presets, customs);

    if manage {
        // BO2's choose-class prompt strip: `ESC Back` left, `S Class Set Options` and
        // `O Class Options` right (each clickable).
        let actions = [
            PickFooterAction::Back.prompt(),
            PickFooterAction::SetOptions.prompt(),
            PickFooterAction::ClassOptions.prompt(),
        ];
        footer_prompts(prims, w, s, l.footer_y, &actions[..1], &actions[1..]);
    } else {
        // In a match the screen is pick-only: Escape closes it and there is no management.
        footer_prompts(
            prims,
            w,
            s,
            l.footer_y,
            &[Prompt { key: "1-9", label: "Choose" }, Prompt { key: "ENTER", label: "Select" }],
            &[Prompt { key: "ESC", label: "Close" }],
        );
    }
    deny_flash(prims, w, h, s, deny);
}

/// How many of the ten points the highlighted row's class spends (BO2's counter beside the pager).
fn class_allocation(row: &PickRow, presets: &[classes::ClassDef], customs: &[crate::classes::CustomClass]) -> i32 {
    match *row {
        PickRow::Preset(j) => {
            let stored = crate::classes::CustomClass::from_spec(&presets[j].spec);
            Draft::from_stored(&stored).spent()
        }
        PickRow::Custom(j) => Draft::from_stored(&customs[j]).spent(),
        PickRow::Create => 0,
    }
}

/// BO2's class-set pager: `‹ CLASS SET n ›` with a ten-dot bar under it marking the position within
/// the ten sets (`a-chooseclass/001`..`/010`). In the shell the chevrons are lit (Left/Right step
/// the sets); in a match they are dim, because the set is picked in the lobby.
fn set_pager(prims: &mut Vec<Prim>, s: f32, cx: f32, y: f32, list: &PickList, manage: bool) {
    let col = if manage { ORANGE } else { DIM };
    let chev = 11.0 * s;
    chevron(prims, cx - 74.0 * s, y + 6.0 * s, chev, col, -1);
    chevron(prims, cx + 74.0 * s - chev * 0.6, y + 6.0 * s, chev, col, 1);
    prims.push(Prim::Text {
        x: cx,
        y: y + 2.0 * s,
        text: list.set_name.clone(),
        argb: WHITE,
        size: (13.0 * s) as i32,
        align: 1,
    });
    // The ten-dot set marker, BO2's diamond on the active set.
    let n = list.sets.max(1);
    let step = 15.0 * s;
    let left = cx - step * (n as f32 - 1.0) * 0.5;
    let my = y + 23.0 * s;
    for i in 0..n {
        let mx = left + i as f32 * step;
        if i == list.marker() {
            diamond(prims, mx, my, 5.0 * s, ORANGE);
        } else {
            prims.push(Prim::Rect { x: mx - 1.5 * s, y: my - 1.5 * s, w: 3.0 * s, h: 3.0 * s, argb: DIM });
        }
    }
}

/// A chevron (`‹`/`›`) drawn as two strokes: `dir` −1 points left, +1 right. `x, y` is its top-left.
fn chevron(prims: &mut Vec<Prim>, x: f32, y: f32, size: f32, argb: u32, dir: i32) {
    let mid = y + size * 0.5;
    let (x0, x1) = if dir < 0 { (x + size * 0.5, x) } else { (x, x + size * 0.5) };
    prims.push(Prim::Line { x0, y0: y, x1, y1: mid, width: 2.0, argb });
    prims.push(Prim::Line { x0: x1, y0: mid, x1: x0, y1: y + size, width: 2.0, argb });
}

/// A filled diamond (the pager's active-set marker, BO2's little orange lozenge).
fn diamond(prims: &mut Vec<Prim>, cx: f32, cy: f32, r: f32, argb: u32) {
    for i in 0..(r * 2.0).max(1.0) as i32 {
        let f = i as f32 / (r * 2.0).max(1.0);
        let half = r * (1.0 - (f * 2.0 - 1.0).abs());
        prims.push(Prim::Rect { x: cx - half, y: cy - r + i as f32, w: half * 2.0, h: 1.0, argb });
    }
}

/// BO2's circled `X` remove affordance, on the selected box's top-right corner (`b-editor/028`).
fn remove_glyph(prims: &mut Vec<Prim>, cell: &EditorCell, s: f32) {
    let (x, y, d, _) = remove_rect(cell, s);
    let (cx, cy) = (x + d * 0.5, y + d * 0.5);
    let r = d * 0.5;
    let seg = 12;
    for i in 0..seg {
        let a0 = i as f32 / seg as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / seg as f32 * std::f32::consts::TAU;
        prims.push(Prim::Line {
            x0: cx + r * a0.cos(),
            y0: cy + r * a0.sin(),
            x1: cx + r * a1.cos(),
            y1: cy + r * a1.sin(),
            width: 1.5,
            argb: WHITE,
        });
    }
    let k = r * 0.42;
    prims.push(Prim::Line { x0: cx - k, y0: cy - k, x1: cx + k, y1: cy + k, width: 1.8, argb: WHITE });
    prims.push(Prim::Line { x0: cx - k, y0: cy + k, x1: cx + k, y1: cy - k, width: 1.8, argb: WHITE });
}

/// The highlighted class's kit on the right, in BO2's order (`a-chooseclass/001`): PRIMARY,
/// SECONDARY, PERK 1, PERK 2, PERK 3, LETHAL, TACTICAL, BONUS CARDS.
fn draw_pick_details(prims: &mut Vec<Prim>, l: &PickLayout, s: f32, row: &PickRow, presets: &[classes::ClassDef], customs: &[crate::classes::CustomClass]) {
    let x = l.panel_x + 14.0 * s;
    let w = l.panel_w - 28.0 * s;
    panel(prims, l.panel_x, l.panel_y, l.panel_w, l.panel_h, s);
    let mut y = l.panel_y + 12.0 * s;
    if matches!(*row, PickRow::Create) {
        prims.push(Prim::Text { x, y, text: "CREATE A CLASS".to_string(), argb: ORANGE, size: (18.0 * s) as i32, align: 0 });
        y += 30.0 * s;
        for line in ["Name it, pick ten points of kit,", "then save it to your profile.", "", "It joins this list and can be", "picked again in any match."] {
            prims.push(Prim::Text { x, y, text: (*line).to_string(), argb: DIM, size: (12.0 * s) as i32, align: 0 });
            y += 18.0 * s;
        }
        return;
    }
    let (primary, secondary, lethal, tactical, perks, wildcards) = match *row {
        PickRow::Preset(j) => {
            let kit = classes::loadout_of(&presets[j]);
            (kit.primary.clone(), kit.secondary.clone(), kit.lethal.clone(), kit.tactical.clone(), kit.perks.clone(), Vec::new())
        }
        PickRow::Custom(j) => {
            let c = &customs[j];
            // The repaired draft: the details panel shows the class the game would actually apply.
            let draft = Draft::from_stored(c);
            (draft.primary_id(), draft.secondary_id(), draft.lethal.clone(), draft.tactical.clone(), draft.perks_in_order(), draft.wildcards.clone())
        }
        PickRow::Create => unreachable!("handled above"),
    };
    // The rows' spacing follows the panel's height, so the kit spreads down it as BO2's does.
    let unit = l.panel_h / 15.0;
    // The two weapons as BO2's icon-and-name blocks.
    for (label, id) in [("PRIMARY", primary.as_str()), ("SECONDARY", secondary.as_str())] {
        let def = weapons::weapon(id);
        let size = 64.0 * s;
        weapon_icon(prims, &def.id, x, y, size, WHITE, &def.display_name);
        prims.push(Prim::Text { x: x + size + 12.0 * s, y: y + 2.0 * s, text: label.to_string(), argb: DIM, size: (10.0 * s) as i32, align: 0 });
        prims.push(Prim::Text { x: x + size + 12.0 * s, y: y + 15.0 * s, text: def.display_name.clone(), argb: WHITE, size: (15.0 * s) as i32, align: 0 });
        // The weapon's attachments as their own little icons on the right.
        let mut ax = x + w - 22.0 * s;
        for short in def.attachments.iter().rev() {
            icon(prims, sprites::attachment_icon(short), ax, y + 8.0 * s, 20.0 * s, DIM, short);
            ax -= 24.0 * s;
        }
        y += (size + unit).max(unit * 2.0);
    }
    // PERK 1-3: each tier's pick, with the Greed rows' second pick beside it.
    for tier in 1..=3u8 {
        let picks: Vec<&crate::weapons::Perk> = perks.iter().filter(|p| p.def().tier == tier).collect();
        let size = 28.0 * s;
        let mut px = x;
        for perk in &picks {
            icon(prims, Some(sprites::perk_icon(**perk)), px, y, size, WHITE, perk.def().id);
            px += size + 4.0 * s;
        }
        prims.push(Prim::Text { x: px + 6.0 * s, y: y + 2.0 * s, text: format!("PERK {tier}"), argb: DIM, size: (10.0 * s) as i32, align: 0 });
        let names: Vec<&str> = picks.iter().map(|p| p.def().id).collect();
        prims.push(Prim::Text { x: px + 6.0 * s, y: y + 15.0 * s, text: if names.is_empty() { "—".into() } else { names.join(", ") }, argb: WHITE, size: (12.0 * s) as i32, align: 0 });
        y += size + unit * 0.5;
    }
    for (label, id) in [("LETHAL", lethal.as_str()), ("TACTICAL", tactical.as_str())] {
        let def = weapons::weapon(id);
        let size = 28.0 * s;
        weapon_icon(prims, &def.id, x, y, size, WHITE, &def.display_name);
        prims.push(Prim::Text { x: x + size + 8.0 * s, y: y + 2.0 * s, text: label.to_string(), argb: DIM, size: (10.0 * s) as i32, align: 0 });
        prims.push(Prim::Text { x: x + size + 8.0 * s, y: y + 15.0 * s, text: def.display_name.clone(), argb: WHITE, size: (12.0 * s) as i32, align: 0 });
        y += size + unit * 0.5;
    }
    if !wildcards.is_empty() {
        prims.push(Prim::Text { x, y, text: "BONUS CARDS".to_string(), argb: DIM, size: (10.0 * s) as i32, align: 0 });
        y += 15.0 * s;
        for wildcard in &wildcards {
            icon(prims, Some(sprites::wildcard_icon(*wildcard)), x, y, 18.0 * s, WHITE, wildcard.name());
            prims.push(Prim::Text { x: x + 24.0 * s, y: y + 2.0 * s, text: wildcard.name().to_string(), argb: DIM, size: (11.0 * s) as i32, align: 0 });
            y += 21.0 * s;
        }
    }
}

// ------------------------------------------------------------------------------------------------
// The editor
// ------------------------------------------------------------------------------------------------

/// The highlighted weapon whose bars the editor shows: the option being hovered while a list is
/// open, else the slot's own gun.
fn editor_stats_weapon(editor: &Editor) -> Option<&'static WeaponDef> {
    let def = if editor.picking {
        editor.option_weapon(editor.option)?
    } else {
        match editor.slot {
            Slot::Primary => weapons::weapon(&editor.draft.primary),
            Slot::Secondary => weapons::weapon(&editor.draft.secondary),
            _ => return None,
        }
    };
    def.class.is_gun().then_some(def)
}

/// The icon of a box's current value.
fn cell_icon(editor: &Editor, cell: &EditorCell) -> Option<String> {
    let d = &editor.draft;
    match cell.slot {
        Slot::Primary => (!d.primary.is_empty()).then(|| format!("ZHBO_BO2_Icon_{}", base_id(&d.primary))),
        Slot::Secondary => (!d.secondary.is_empty()).then(|| format!("ZHBO_BO2_Icon_{}", base_id(&d.secondary))),
        Slot::PrimaryAtts => d.primary_atts.get(cell.index).and_then(|a| sprites::attachment_icon(a)).map(str::to_string),
        Slot::SecondaryAtts => d.secondary_atts.get(cell.index).and_then(|a| sprites::attachment_icon(a)).map(str::to_string),
        Slot::Lethal => (!d.lethal.is_empty()).then(|| format!("ZHBO_BO2_Icon_{}", base_id(&d.lethal))),
        Slot::Tactical => (!d.tactical.is_empty()).then(|| format!("ZHBO_BO2_Icon_{}", base_id(&d.tactical))),
        Slot::Perk1 | Slot::Perk2 | Slot::Perk3 => {
            let tier = (cell.slot.perk_tier().unwrap_or(1) as usize) - 1;
            d.perks[tier].map(|p| sprites::perk_icon(p).to_string())
        }
        Slot::Perk1b | Slot::Perk2b | Slot::Perk3b => {
            let tier = (cell.slot.perk_tier().unwrap_or(1) as usize) - 1;
            d.extra_perks[tier].map(|p| sprites::perk_icon(p).to_string())
        }
        Slot::Wildcards => d.wildcards.get(cell.index).map(|w| sprites::wildcard_icon(*w).to_string()),
        Slot::Name | Slot::Save => None,
    }
}

/// What a box shows as its value.
fn cell_value(editor: &Editor, cell: &EditorCell) -> String {
    let d = &editor.draft;
    match cell.slot {
        Slot::PrimaryAtts => d.primary_atts.get(cell.index).map(|s| att_display(s)).unwrap_or_default(),
        Slot::SecondaryAtts => d.secondary_atts.get(cell.index).map(|s| att_display(s)).unwrap_or_default(),
        Slot::Wildcards => d.wildcards.get(cell.index).map(|w| w.name().to_string()).unwrap_or_default(),
        other => crate::classmenu::slot_value(d, other),
    }
}

/// An attachment short as the catalog names it.
fn att_display(short: &str) -> String {
    let d = weapons::catalog();
    d.all()
        .iter()
        .find_map(|w| w.allowed.iter().find(|(s, _)| s == short).map(|(_, asset)| asset.clone()))
        .and_then(|asset| d.attachment(&asset).map(|a| a.display_name.clone()))
        .unwrap_or_else(|| short.to_string())
}

/// Whether a weapon offers any attachments at all (BO2's "Attachments not available", `b-editor/002`).
fn offers_attachments(id: &str) -> bool {
    !id.is_empty() && !weapons::weapon(id).allowed.is_empty()
}

/// BO2's allocation editor: the class name as the heading with the allocation bar under it, the
/// boxed grid down the screen, and the prompts at the bottom. A weapon picker or an option list
/// takes the screen over while one is open, as BO2's do.
pub fn draw_editor(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, editor: &Editor, deny: f32) {
    background(prims, w, h);
    let l = editor_layout(w, h, s);
    match editor.slot {
        Slot::Primary | Slot::Secondary if editor.picking => draw_weapon_picker(prims, w, h, s, &l, editor),
        _ if editor.picking => draw_option_picker(prims, w, h, s, &l, editor),
        _ => draw_editor_grid(prims, w, h, s, &l, editor),
    }
    deny_flash(prims, w, h, s, deny);
}

/// The editor's boxed grid (`b-editor/001`, `/028`).
fn draw_editor_grid(prims: &mut Vec<Prim>, w: f32, _h: f32, s: f32, l: &EditorLayout, editor: &Editor) {
    let d = &editor.draft;
    let title_x = banner(prims, l.content_x, l.content_w, s, l.title_y, l.header_h);
    let name = if d.name.trim().is_empty() { "CREATE A CLASS".to_string() } else { d.name.clone() };
    let name_on = editor.slot == Slot::Name;
    prims.push(Prim::Text {
        x: title_x,
        y: l.title_y + 10.0 * s,
        text: if editor.naming { format!("{name}_") } else { name },
        argb: if name_on { ORANGE } else { WHITE },
        size: (24.0 * s) as i32,
        align: 0,
    });
    // The allocation readout, BO2's bar across the content's width with the n/10 counter above its
    // right end.
    draw_allocation_counter(prims, l.content_x + l.content_w, l.title_y + 12.0 * s, s, d.spent(), crate::classmenu::PICK_10);
    draw_allocation(prims, l.content_x, l.bar_y, l.content_w, s, d.spent(), crate::classmenu::PICK_10);

    // The group labels BO2 sets beside the box rows. A weapon that offers no attachments says so in
    // place of the label and boxes (b-editor/002).
    if offers_attachments(&d.primary) {
        group_label(prims, l.left_x, l.atts_y + 12.0 * s, s, "ATTACHMENTS");
    } else {
        prims.push(Prim::Text { x: l.left_x + 12.0 * s, y: l.atts_y + 12.0 * s, text: "Attachments not available".to_string(), argb: DIM, size: (12.0 * s) as i32, align: 0 });
    }
    if offers_attachments(&d.secondary) {
        group_label(prims, l.left_x, l.atts2_y + 12.0 * s, s, "ATTACHMENTS");
    } else {
        prims.push(Prim::Text { x: l.left_x + 12.0 * s, y: l.atts2_y + 12.0 * s, text: "Attachments not available".to_string(), argb: DIM, size: (12.0 * s) as i32, align: 0 });
    }
    group_label(prims, l.left_x, l.wild_y - 18.0 * s, s, "BONUS CARDS");
    for tier in 1..=3u8 {
        let y = l.top + (tier as f32 - 1.0) * (l.perk_h + l.gap);
        group_label(prims, l.right_x, y + 14.0 * s, s, if tier == 1 { "PERK 1" } else if tier == 2 { "PERK 2" } else { "PERK 3" });
        let pick = d.perks[(tier as usize) - 1].map(|p| p.def().id.to_string());
        prims.push(Prim::Text {
            x: l.right_x,
            y: y + 30.0 * s,
            text: pick.unwrap_or_else(|| "—".into()),
            argb: if d.perks[(tier as usize) - 1].is_some() { WHITE } else { DIM },
            size: (13.0 * s) as i32,
            align: 0,
        });
    }
    group_label(prims, l.right_x, l.lethal_y + 14.0 * s, s, "LETHAL");
    prims.push(Prim::Text {
        x: l.right_x,
        y: l.lethal_y + 30.0 * s,
        text: if d.lethal.is_empty() { "—".to_string() } else { weapons::weapon(&d.lethal).display_name.clone() },
        argb: if d.lethal.is_empty() { DIM } else { WHITE },
        size: (13.0 * s) as i32,
        align: 0,
    });
    let tac_y = l.tactical_y + 14.0 * s;
    group_label(prims, l.right_x, tac_y, s, "TACTICAL");
    prims.push(Prim::Text {
        x: l.right_x,
        y: tac_y + 16.0 * s,
        text: if d.tactical.is_empty() { "—".to_string() } else { weapons::weapon(&d.tactical).display_name.clone() },
        argb: if d.tactical.is_empty() { DIM } else { WHITE },
        size: (13.0 * s) as i32,
        align: 0,
    });

    let cells = editor_cells(l, editor);
    for cell in &cells {
        draw_cell(prims, s, editor, cell);
    }

    let mut left = vec![Prompt { key: "ESC", label: "Back" }, Prompt { key: "ENTER", label: "Open" }];
    if editor.naming {
        left = vec![Prompt { key: "TYPE", label: "Name" }, Prompt { key: "BKSP", label: "Delete" }, Prompt { key: "ENTER", label: "Done" }];
    }
    // BO2 shows `R Remove` only while the highlighted slot holds something (`b-editor/002` has
    // none, `/028` has it).
    let removable = cells.iter().any(|c| c.slot == editor.slot && cell_filled(editor, c));
    let right: &[Prompt] = if removable && !editor.naming { &[Prompt { key: "R", label: "Remove" }] } else { &[] };
    footer_prompts(prims, w, s, l.footer_y, &left, right);
}

/// A small group label beside the box rows (BO2's grey caps).
fn group_label(prims: &mut Vec<Prim>, x: f32, y: f32, s: f32, text: &str) {
    prims.push(Prim::Text { x, y, text: text.to_string(), argb: DIM, size: (11.0 * s) as i32, align: 0 });
}

/// Whether a box itself draws its value. BO2 writes the weapons' and bonus cards' names inside
/// their boxes (`MP7` in the primary box, the card names in the strip), while the perk and offhand
/// rows read beside their icon (`b-editor/028`) and the heading already is the class name.
fn cell_draws_value(slot: Slot) -> bool {
    matches!(slot, Slot::Primary | Slot::Secondary | Slot::PrimaryAtts | Slot::SecondaryAtts | Slot::Wildcards)
}

/// One boxed cell: BO2's tile with the value and the icon, an orange outline and the circled X when
/// it is the highlighted slot.
fn draw_cell(prims: &mut Vec<Prim>, s: f32, editor: &Editor, cell: &EditorCell) {
    let on = cell.slot == editor.slot;
    let filled = cell_filled(editor, cell);
    // BO2's boxed tiles: a dark plate with a thin border, orange when the box is selected. The
    // converted TILE art is a small framed tile that smears when stretched to a big box.
    prims.push(Prim::Rect { x: cell.x, y: cell.y, w: cell.w, h: cell.h, argb: CELL_FILL });
    hud::outline_rect(prims, cell.x, cell.y, cell.w, cell.h, if on { 2.0 } else { 1.0 } * s, if on { ORANGE } else { CELL_EDGE });
    if cell.slot == Slot::Save {
        prims.push(Prim::Text {
            x: cell.x + cell.w * 0.5,
            y: cell.y + cell.h * 0.28,
            text: "SAVE CLASS".to_string(),
            argb: if on { ORANGE } else { WHITE },
            size: (13.0 * s) as i32,
            align: 1,
        });
        return;
    }
    let icon_size = (cell.h * 0.62).min(cell.w * 0.55);
    if let Some(art) = cell_icon(editor, cell) {
        let value = cell_value(editor, cell);
        icon(prims, Some(&art), cell.x + cell.w * 0.5 - icon_size * 0.5 - cell.w * 0.12, cell.y + (cell.h - icon_size) * 0.4, icon_size, if on { WHITE } else { DIM }, &value);
    }
    if !filled {
        return;
    }
    if cell_draws_value(cell.slot) {
        let value = cell_value(editor, cell);
        let size = if cell.h > 60.0 * s { 15.0 * s } else { 11.0 * s };
        prims.push(Prim::Text {
            x: cell.x + 8.0 * s,
            y: cell.y + cell.h - size - 6.0 * s,
            text: value,
            argb: if on { ORANGE } else { WHITE },
            size: size as i32,
            align: 0,
        });
    }
    if on {
        remove_glyph(prims, cell, s);
    }
}

/// BO2's weapon screen (`c-primary/001`, `d-attach/037`): the slot's title, the allocation readout,
/// the highlighted gun's segmented bars, the category column at the bottom left with the active
/// category on top, and the category's guns as tiles on the right.
fn draw_weapon_picker(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, l: &EditorLayout, editor: &Editor) {
    let d = &editor.draft;
    let slot = editor.slot;
    let title = if slot == Slot::Secondary { "SECONDARY" } else { "PRIMARY" };
    let title_x = banner(prims, l.content_x, l.content_w, s, l.title_y, l.header_h);
    heading(prims, title_x, l.title_y, s, title, WHITE);
    draw_allocation_counter(prims, l.content_x + l.content_w, l.title_y + 12.0 * s, s, d.spent(), crate::classmenu::PICK_10);
    draw_allocation(prims, l.content_x, l.bar_y, l.content_w, s, d.spent(), crate::classmenu::PICK_10);

    let cats = weapon_categories(editor);
    let active = category_of(&cats, editor.option);
    // The highlighted gun's name and segmented bars, BO2's left column.
    if let Some(def) = editor.option_weapon(editor.option) {
        prims.push(Prim::Text { x: l.left_x, y: l.top + 6.0 * s, text: def.display_name.clone(), argb: if def.class.is_gun() { WHITE } else { ORANGE }, size: (26.0 * s) as i32, align: 0 });
        weapon_icon(prims, &def.id, l.left_x, l.top + 44.0 * s, 96.0 * s, WHITE, &def.display_name);
        if def.class.is_gun() {
            stat_bars(prims, l.left_x, l.top + 152.0 * s, l.left_w, s, &weapon_stats(def));
        }
    }
    // BO2's category column: the active category orange on top, the rest following it, with its rule
    // under the active entry.
    let cat_y = l.footer_y - 40.0 * s - cats.len() as f32 * 26.0 * s;
    let column = category_column(&cats, active);
    for (i, ci) in column.iter().enumerate() {
        let cat = &cats[*ci];
        let y = cat_y + i as f32 * 26.0 * s;
        let on = *ci == active;
        prims.push(Prim::Text {
            x: l.left_x,
            y,
            text: cat.name.to_string(),
            argb: if on { ORANGE } else { WHITE },
            size: if on { (17.0 * s) as i32 } else { (13.0 * s) as i32 },
            align: 0,
        });
        if on {
            prims.push(Prim::Rect { x: l.left_x, y: y + 22.0 * s, w: l.left_w * 0.9, h: 1.0 * s, argb: 0x60FF_FFFF });
        }
    }
    // The category's guns as tiles, the highlighted one outlined in orange.
    let (px, py, pw, _) = picker_rect(l, slot);
    let opts = cats.get(active).map(|c| c.options.clone()).unwrap_or_default();
    let highlighted = opts.iter().position(|i| *i == editor.option).unwrap_or(0);
    let (first, shown) = picker_window(l, slot, highlighted);
    for (row_i, opt) in opts.iter().enumerate().skip(first).take(shown) {
        let y = py + (row_i - first) as f32 * l.row_h;
        let on = *opt == editor.option;
        let Some(def) = editor.option_weapon(*opt) else { continue };
        tile(prims, s, px, y, pw, l.row_h - 6.0 * s, on, def.id.as_str(), &def.display_name, editor.option_chosen(*opt));
    }
    if opts.len() > shown {
        prims.push(Prim::Text {
            x: px + pw - 6.0 * s,
            y: py + shown as f32 * l.row_h + 2.0 * s,
            text: format!("{} / {}", highlighted + 1, opts.len()),
            argb: DIM,
            size: (10.0 * s) as i32,
            align: 2,
        });
    }
    let _ = (w, h);
    footer_prompts(
        prims,
        w,
        s,
        l.footer_y,
        &[Prompt { key: "ESC", label: "Back" }],
        &[
            Prompt { key: "LEFT/RIGHT", label: "Category" },
            Prompt { key: "UP/DOWN", label: "Gun" },
            Prompt { key: "ENTER", label: "Choose" },
        ],
    );
}

/// One weapon tile of the picker (`c-primary/001`'s strip, drawn as a row in our column).
#[allow(clippy::too_many_arguments)] // a tile's rect, its state and its label parts
fn tile(prims: &mut Vec<Prim>, s: f32, x: f32, y: f32, w: f32, h: f32, on: bool, id: &str, name: &str, chosen: bool) {
    prims.push(Prim::Rect { x, y, w, h, argb: if on { ROW_ACTIVE } else { CELL_FILL } });
    hud::outline_rect(prims, x, y, w, h, if on { 2.0 } else { 1.0 } * s, if on { ORANGE } else { CELL_EDGE });
    let icon_size = (h - 8.0 * s).min(w * 0.45);
    weapon_icon(prims, id, x + 6.0 * s, y + 4.0 * s, icon_size, WHITE, name);
    prims.push(Prim::Text {
        x: x + icon_size + 16.0 * s,
        y: y + h * 0.5 - 8.0 * s,
        text: name.to_string(),
        argb: if on { ORANGE } else { WHITE },
        size: (14.0 * s) as i32,
        align: 0,
    });
    if chosen {
        prims.push(Prim::Text { x: x + w - 14.0 * s, y: y + h * 0.5 - 8.0 * s, text: "*".to_string(), argb: GOLD, size: (16.0 * s) as i32, align: 2 });
    }
}

/// The attachments/perks/bonus-card/offhand picker: BO2's list screen, the entry rows over the
/// title of the slot they fill.
fn draw_option_picker(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, l: &EditorLayout, editor: &Editor) {
    let d = &editor.draft;
    let title_x = banner(prims, l.content_x, l.content_w, s, l.title_y, l.header_h);
    heading(prims, title_x, l.title_y, s, editor.slot.label(), WHITE);
    draw_allocation_counter(prims, l.content_x + l.content_w, l.title_y + 12.0 * s, s, d.spent(), crate::classmenu::PICK_10);
    draw_allocation(prims, l.content_x, l.bar_y, l.content_w, s, d.spent(), crate::classmenu::PICK_10);

    let opts = editor.options();
    let (px, py, pw, ph) = picker_rect(l, editor.slot);
    panel(prims, px - 8.0 * s, py - 8.0 * s, pw + 16.0 * s, ph + 16.0 * s, s);
    let (start, shown) = picker_window(l, editor.slot, editor.option);
    for (j, name) in opts.iter().enumerate().skip(start).take(shown) {
        let y = py + (j - start) as f32 * l.row_h;
        let on = j == editor.option;
        let chosen = editor.option_chosen(j);
        row(prims, px, y, pw, l.row_h - 5.0 * s, s, on);
        if let Some(art) = option_icon(editor, j) {
            icon(prims, Some(&art), px + 8.0 * s, y + 3.0 * s, 24.0 * s, if on || chosen { WHITE } else { DIM }, name);
        }
        if chosen {
            prims.push(Prim::Text { x: px + pw - 14.0 * s, y: y + 6.0 * s, text: "*".to_string(), argb: GOLD, size: (14.0 * s) as i32, align: 2 });
        }
        prims.push(Prim::Text {
            x: px + 40.0 * s,
            y: y + 4.0 * s,
            text: name.clone(),
            argb: if on { ORANGE } else if chosen { WHITE } else { DIM },
            size: (12.0 * s) as i32,
            align: 0,
        });
    }
    if opts.is_empty() {
        prims.push(Prim::Text { x: px + 10.0 * s, y: py + 8.0 * s, text: "NOTHING TO CHOOSE".to_string(), argb: DIM, size: (12.0 * s) as i32, align: 0 });
    }
    if opts.len() > shown {
        prims.push(Prim::Text {
            x: px + pw - 10.0 * s,
            y: py + shown as f32 * l.row_h + 4.0 * s,
            text: format!("{} / {}", editor.option + 1, opts.len()),
            argb: DIM,
            size: (11.0 * s) as i32,
            align: 2,
        });
    }
    // The hovered gun's bars, for the weapon-derived lists (lethal/tactical are not guns, so this
    // is only the secondary under Overkill's primary categories).
    if let Some(def) = editor_stats_weapon(editor) {
        let by = py + ph - 96.0 * s;
        if by > py {
            prims.push(Prim::Text { x: px, y: by, text: def.display_name.clone(), argb: WHITE, size: (12.0 * s) as i32, align: 0 });
            stat_bars(prims, px, by + 16.0 * s, pw, s, &weapon_stats(def));
        }
    }
    let _ = (w, h);
    footer_prompts(
        prims,
        w,
        s,
        l.footer_y,
        &[Prompt { key: "ESC", label: "Back" }],
        &[Prompt { key: "UP/DOWN", label: "Select" }, Prompt { key: "ENTER", label: "Choose" }],
    );
}

/// The icon of an option row in the editor's list.
fn option_icon(editor: &Editor, i: usize) -> Option<String> {
    if let Some(def) = editor.option_weapon(i) {
        return Some(format!("ZHBO_BO2_Icon_{}", base_id(&def.id)));
    }
    if let Some(short) = editor.option_attachment(i) {
        return sprites::attachment_icon(short).map(str::to_string);
    }
    if let Some(perk) = editor.option_perk(i) {
        return Some(sprites::perk_icon(perk).to_string());
    }
    editor.option_wildcard(i).map(|w| sprites::wildcard_icon(w).to_string())
}

// ------------------------------------------------------------------------------------------------
// The shell's dialogs
// ------------------------------------------------------------------------------------------------

/// Draw one of the shell's dialogs over the screen BO2 way (`a-chooseclass/021`): the dimmed screen,
/// the centred panel with its title, subtitle and entries, and the `ESC Back` prompt.
pub fn draw_dialog(prims: &mut Vec<Prim>, w: f32, h: f32, d: &Dialog) {
    let s = (h / 720.0).max(0.5);
    prims.push(Prim::Rect { x: 0.0, y: 0.0, w, h, argb: 0xB400_0000 });
    let (bx, by, bw, bh) = dialog_box(w, h, s, d.items.len());
    prims.push(Prim::Rect { x: bx, y: by, w: bw, h: bh, argb: DIALOG_FILL });
    hud::outline_rect(prims, bx, by, bw, bh, 1.5 * s, DIALOG_EDGE);
    prims.push(Prim::Text { x: bx + 22.0 * s, y: by + 16.0 * s, text: d.kind.title().to_string(), argb: WHITE, size: (24.0 * s) as i32, align: 0 });
    prims.push(Prim::Text { x: bx + 22.0 * s, y: by + 50.0 * s, text: d.subtitle.clone(), argb: DIM, size: (13.0 * s) as i32, align: 0 });
    let mut top = by + 76.0 * s;
    if d.naming {
        prims.push(Prim::Text { x: bx + 22.0 * s, y: top, text: format!("{}_", d.buffer), argb: ORANGE, size: (16.0 * s) as i32, align: 0 });
        top += 26.0 * s;
    }
    for (i, item) in d.items.iter().enumerate() {
        let y = top + i as f32 * 30.0 * s;
        let on = i == d.highlight && !d.naming;
        prims.push(Prim::Text { x: bx + 22.0 * s, y, text: item.label().to_string(), argb: if on { ORANGE } else { WHITE }, size: (16.0 * s) as i32, align: 0 });
    }
    // BO2's prompt strip below the panel, over the dimmed screen.
    prims.push(Prim::Rect { x: 0.0, y: h - 40.0 * s, w, h: 40.0 * s, argb: 0xFF10_1214 });
    footer_prompts(prims, w, s, h - 34.0 * s, &[Prompt { key: "ESC", label: "Back" }], &[]);
    let _ = bh;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classes::CustomClass;

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        crate::profile::lock()
    }

    fn custom(name: &str, primary: &str) -> CustomClass {
        CustomClass { name: name.into(), primary: primary.into(), ..CustomClass::default() }
    }

    fn texts(prims: &[Prim]) -> Vec<String> {
        prims
            .iter()
            .filter_map(|p| match p {
                Prim::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    /// One frame with `keys` freshly pressed (released the frame after).
    fn press(editor: &mut Editor, prev: &mut MenuPrev, keys: &[Key]) -> EditorFrame {
        let mut pointer = MenuPointer { screen_w: 1280.0, screen_h: 720.0, ..Default::default() };
        for k in keys {
            pointer.keys.set_down(*k, true);
        }
        let frame = editor_frame(editor, &pointer, prev);
        prev.update(&pointer);
        pointer.keys.clear();
        editor_frame(editor, &pointer, prev);
        prev.update(&pointer);
        frame
    }

    #[test]
    fn the_shell_pick_list_is_presets_then_customs_then_create_and_the_match_list_has_no_create() {
        let _g = lock();
        crate::profile::reset();
        let presets = classes::get().len();
        let rows = pick_rows(true);
        assert_eq!(rows.len(), presets + 1, "an empty profile: presets + CREATE");
        assert_eq!(rows[presets], PickRow::Create);
        // In a match there is no CREATE A CLASS row (picks only).
        let rows = pick_rows(false);
        assert_eq!(rows.len(), presets, "the in-match list is the presets alone");
        assert!(!rows.contains(&PickRow::Create));
        crate::profile::save(custom("AB", "an94_mp"));
        crate::profile::save(custom("Sniper", "dsr50_mp"));
        let rows = pick_rows(true);
        assert_eq!(rows.len(), presets + 3);
        assert_eq!(rows[presets], PickRow::Custom(0));
        assert_eq!(rows[presets + 1], PickRow::Custom(1));
        assert_eq!(rows[presets + 2], PickRow::Create);
        // The in-match list keeps the customs but drops CREATE.
        let rows = pick_rows(false);
        assert_eq!(rows.len(), presets + 2);
        assert_eq!(rows[presets], PickRow::Custom(0));
        assert!(!rows.contains(&PickRow::Create));
        crate::profile::reset();
    }

    #[test]
    fn the_pick_list_describes_the_active_class_set() {
        let _g = lock();
        crate::profile::reset();
        // Set 1 of ten, BO2's default name, the five defaults and (in the shell) CREATE.
        let presets = classes::get().len();
        let list = pick_list(true);
        assert_eq!((list.set, list.sets), (0, 10));
        assert_eq!(list.set_number(), 1);
        assert_eq!(list.marker(), 0, "the first dot is the marker on set 1");
        assert_eq!(list.set_name, "CLASS SET 1");
        assert_eq!(list.rows.iter().filter(|r| matches!(r, PickRow::Preset(_))).count(), presets);
        assert!(list.rows.iter().all(|r| !matches!(r, PickRow::Custom(_))), "no customs yet");
        assert_eq!(list.rows.last(), Some(&PickRow::Create));
        // A class saved in set 2 shows only there, and the pager's data follows the set.
        crate::profile::save(custom("Set One", "an94_mp"));
        crate::profile::rename_set(1, "SNIPERS");
        crate::profile::set_active(1);
        crate::profile::save(custom("Set Two", "dsr50_mp"));
        let list = pick_list(true);
        assert_eq!((list.set, list.set_number()), (1, 2));
        assert_eq!(list.set_name, "SNIPERS");
        assert_eq!(list.marker(), 1, "the second dot marks set 2");
        assert_eq!(list.rows[presets], PickRow::Custom(0));
        assert_eq!(list.rows[presets + 1], PickRow::Create, "one custom in this set");
        // Left/Right step the set only in the shell; a match's fold ignores them.
        let mut inp = MenuPointer { screen_w: 1280.0, screen_h: 720.0, ..Default::default() };
        let prev = MenuPrev::default();
        inp.keys.set_down(Key::Right, true);
        assert_eq!(pick_frame(&inp, &prev, 0, true).set_step, 1);
        assert_eq!(pick_frame(&inp, &prev, 0, false).set_step, 0);
        inp.keys.set_down(Key::Right, false);
        inp.keys.set_down(Key::Left, true);
        assert_eq!(pick_frame(&inp, &prev, 0, true).set_step, -1);
        crate::profile::reset();
    }

    /// The pager the shell's Left/Right drives: ten steps of +1 walk all ten sets and come back to
    /// the first, and the marker follows (BO2's pager wraps, `a-chooseclass/011`).
    #[test]
    fn the_set_pager_steps_through_all_ten_sets_and_wraps() {
        let _g = lock();
        crate::profile::reset();
        let seen: Vec<usize> = (0..profile::MAX_SETS)
            .map(|i| {
                if i > 0 {
                    assert!(crate::profile::cycle_active(1).is_some());
                }
                let list = pick_list(true);
                assert_eq!(list.marker(), i.min(profile::MAX_SETS - 1), "the marker follows the active set");
                list.set_number()
            })
            .collect();
        assert_eq!(seen, (1..=profile::MAX_SETS).collect::<Vec<_>>());
        // One more step wraps back to the first set, and a step back wraps to the last.
        crate::profile::cycle_active(1);
        assert_eq!(pick_list(true).set_number(), 1, "the pager wraps forward");
        crate::profile::cycle_active(-1);
        assert_eq!(pick_list(true).set_number(), profile::MAX_SETS, "and backward");
        crate::profile::reset();
    }

    #[test]
    fn pick_hit_testing_finds_every_row_in_the_window() {
        let l = pick_layout(1280.0, 720.0, 1.0);
        let visible = pick_visible(&l);
        for i in 0..visible.min(7) {
            let y = l.list_y + i as f32 * l.row_h + 4.0;
            assert_eq!(pick_row_at(&l, l.list_x + 10.0, y, 7, 0), Some(i), "row {i}");
        }
        // The scroll window: the first shown row is `first`, so the first slot is that row.
        let first = 2;
        assert_eq!(pick_row_at(&l, l.list_x + 10.0, l.list_y + 4.0, 7, first), Some(first));
        assert_eq!(pick_row_at(&l, l.list_x - 5.0, l.list_y + 5.0, 7, 0), None, "left of the list");
        assert_eq!(pick_row_at(&l, l.list_x + 10.0, l.list_y + 7.0 * l.row_h, 7, 0), None, "past the last row");
    }

    #[test]
    fn the_shell_footer_prompts_are_hit_tested_and_absent_in_a_match() {
        let l = pick_layout(1280.0, 720.0, 1.0);
        let actions = pick_footer_actions(&l, 1280.0, 1.0, true);
        assert_eq!(
            actions.iter().map(|(a, ..)| *a).collect::<Vec<_>>(),
            vec![PickFooterAction::Back, PickFooterAction::SetOptions, PickFooterAction::ClassOptions],
            "BO2's choose-class footer: ESC Back, S Class Set Options, O Class Options"
        );
        for (action, x, y, w, h) in &actions {
            assert_eq!(pick_footer_at(&l, 1280.0, 1.0, x + w * 0.5, y + h * 0.5, true), Some(*action));
        }
        assert_eq!(pick_footer_at(&l, 1280.0, 1.0, l.list_x, l.list_y, true), None, "not a row point");
        assert!(pick_footer_actions(&l, 1280.0, 1.0, false).is_empty(), "no management in a match");
        assert_eq!(pick_footer_at(&l, 1280.0, 1.0, 1200.0, l.footer_y + 5.0, false), None);
        // The prompts carry BO2's keys.
        assert_eq!((PickFooterAction::SetOptions.key(), PickFooterAction::SetOptions.label()), ("S", "Class Set Options"));
        assert_eq!((PickFooterAction::ClassOptions.key(), PickFooterAction::ClassOptions.label()), ("O", "Class Options"));
    }

    #[test]
    fn the_choose_class_footer_carries_bo2s_hint_in_the_shell_only() {
        let _g = lock();
        crate::profile::reset();
        let hint = "You will load into the game with this class set.";
        let mut shell = Vec::new();
        draw_pick(&mut shell, 1280.0, 720.0, 1.0, 0, 0.0, true);
        let shell_text = texts(&shell);
        assert!(shell_text.iter().any(|t| t == hint), "the shell shows BO2's hint: {shell_text:?}");
        assert!(shell_text.iter().any(|t| t == "CLASS SET 1"), "the pager names the set");
        assert!(shell_text.iter().any(|t| t == "Class Set Options"));
        assert!(shell_text.iter().any(|t| t == "Class Options"));
        // A match is already running: no hint, and the pager still shows the active set (read-only).
        let mut game = Vec::new();
        draw_pick(&mut game, 1280.0, 720.0, 1.0, 0, 0.0, false);
        let game_text = texts(&game);
        assert!(!game_text.iter().any(|t| t == hint), "no hint in a match: {game_text:?}");
        assert!(game_text.iter().any(|t| t == "CLASS SET 1"), "the read-only pager draws");
        assert!(!game_text.iter().any(|t| t == "Class Set Options"), "nothing to manage in a match");
        crate::profile::reset();
    }

    #[test]
    fn the_allocation_bar_lights_one_segment_per_point_and_the_counter_says_n_of_ten() {
        for spent in 0..=10 {
            assert_eq!(allocation_lit(spent, 10), spent as usize);
        }
        assert_eq!(allocation_lit(-2, 10), 0, "never negative");
        assert_eq!(allocation_lit(14, 10), 10, "clamped to the budget");
        let mut prims = Vec::new();
        draw_allocation(&mut prims, 0.0, 0.0, 200.0, 1.0, 7, 10);
        let lit = prims
            .iter()
            .filter(|p| matches!(p, Prim::Rect { argb, .. } if *argb == ORANGE))
            .count();
        assert_eq!(lit, 7, "seven of the ten blocks are spent");
        assert_eq!(prims.iter().filter(|p| matches!(p, Prim::Rect { .. })).count(), 10, "ten blocks");
    }

    #[test]
    fn pick_hit_testing_still_maps_the_set_pager_row() {
        // The pager sits above the list, so a click on it is not a row and the list starts below it.
        let l = pick_layout(1280.0, 720.0, 1.0);
        assert!(l.list_y > l.pager_y + l.pager_h, "the list starts under the pager");
        assert_eq!(pick_row_at(&l, l.list_x + 10.0, l.pager_y + 4.0, 7, 0), None, "the pager is not a row");
    }

    #[test]
    fn the_editor_grid_hit_tests_every_box() {
        let e = Editor::new(Draft::default());
        let l = editor_layout(1280.0, 720.0, 1.0);
        let cells = editor_cells(&l, &e);
        // Every box of the walk is on screen, and its middle maps back to it. A weapon that offers
        // no attachments has no attachment boxes (BO2's "Attachments not available").
        for slot in e.slots() {
            let offered = match slot {
                Slot::PrimaryAtts => offers_attachments(&e.draft.primary),
                Slot::SecondaryAtts => offers_attachments(&e.draft.secondary),
                _ => true,
            };
            assert_eq!(cells.iter().any(|c| c.slot == slot), offered, "{slot:?} box presence");
        }
        for (i, c) in cells.iter().enumerate() {
            let (cx, cy) = c.center();
            assert_eq!(editor_cell_at(&cells, cx, cy), Some(i), "{:?} box {i}", c.slot);
        }
        // The attachment boxes are one per attachment the gun may carry.
        let atts = cells.iter().filter(|c| c.slot == Slot::PrimaryAtts).count();
        assert_eq!(atts, e.draft.primary_att_max());
        // A point off the grid hits nothing.
        assert_eq!(editor_cell_at(&cells, 5.0, 5.0), None);
    }

    #[test]
    fn the_editor_screens_draw_their_boxes_and_prompts() {
        let mut e = Editor::new(Draft::default());
        e.slot = Slot::Perk2;
        let mut prims = Vec::new();
        draw_screen(&mut prims, 1280.0, 720.0, &ScreenView { highlight: 0, editor: Some(&e), deny: 0.0, manage: true });
        let t = texts(&prims);
        for label in ["PERK 1", "PERK 2", "PERK 3", "LETHAL", "TACTICAL", "ATTACHMENTS", "BONUS CARDS"] {
            assert!(t.iter().any(|x| x == label), "editor label {label}: {t:?}");
        }
        assert!(t.iter().any(|x| x == "Remove"), "the R Remove prompt: {t:?}");
        assert!(t.iter().any(|x| x.contains("AN-94")), "the held primary in its box");
        // The allocation readout replaced the old POINTS LEFT counter.
        assert!(!t.iter().any(|x| x == "POINTS LEFT"), "no Pick-10 wording: {t:?}");
        // A perk list shows the tier's grid.
        e.picking = true;
        let mut prims = Vec::new();
        draw_screen(&mut prims, 1280.0, 720.0, &ScreenView { highlight: 0, editor: Some(&e), deny: 0.0, manage: true });
        let t = texts(&prims);
        assert!(t.iter().any(|x| x == "Scavenger"), "tier 2 perks are listed: {t:?}");
        assert!(t.iter().any(|x| x == "FastHands"), "tier 2 perks are listed");
    }

    #[test]
    fn a_launcher_shows_attachments_not_available() {
        let mut d = Draft::default();
        d.secondary = "smaw_mp".into();
        let e = Editor::new(d);
        let mut prims = Vec::new();
        draw_screen(&mut prims, 1280.0, 720.0, &ScreenView { highlight: 0, editor: Some(&e), deny: 0.0, manage: true });
        let t = texts(&prims);
        assert!(t.iter().any(|x| x == "Attachments not available"), "BO2's launcher line: {t:?}");
    }

    #[test]
    fn the_weapon_list_opens_on_the_held_guns_category() {
        // The default draft's primary (the AN-94) is a rifle, so the list opens on that category
        // and the highlight is one of its guns.
        let mut e = Editor::new(Draft::default());
        let mut prev = MenuPrev::default();
        press(&mut e, &mut prev, &[Key::Enter]);
        assert!(e.picking, "Enter opens the primary list");
        let cats = weapon_categories(&e);
        assert!(!cats.is_empty());
        let active = category_of(&cats, e.option);
        assert_eq!(cats[active].name, "ASSAULT RIFLES", "the list opens on the held gun's category");
        assert_eq!(cats[active].options[0], e.option, "and on the held gun itself");
    }

    /// A two-category list, as BO2's primary list is: the helpers walk it without the catalog.
    fn two_categories() -> Vec<Category> {
        vec![
            Category { class: WeaponClass::Smg, name: "SUBMACHINE GUNS", options: vec![0, 1, 2] },
            Category { class: WeaponClass::Rifle, name: "ASSAULT RIFLES", options: vec![3, 4] },
        ]
    }

    #[test]
    fn left_right_walk_the_categories_and_up_down_the_guns_inside_one() {
        let cats = two_categories();
        let mut e = Editor::new(Draft::default());
        e.slot = Slot::Primary;
        e.picking = true;
        e.option = 1;
        // Down/Up stay inside the SMG category and wrap inside it.
        assert!(step_within_category(&mut e, &cats, 1));
        assert_eq!(e.option, 2);
        step_within_category(&mut e, &cats, 1);
        assert_eq!(e.option, 0, "Down at the end of a category wraps to its first gun");
        step_within_category(&mut e, &cats, -1);
        assert_eq!(e.option, 2, "Up at the start wraps to its last");
        // Right/Left move to the neighbouring category's first gun and wrap across the column.
        step_category(&mut e, &cats, 1);
        assert_eq!(e.option, 3, "Right lands on the next category's first gun");
        step_category(&mut e, &cats, 1);
        assert_eq!(e.option, 0, "the last category wraps back to the first");
        step_category(&mut e, &cats, -1);
        assert_eq!(e.option, 3, "and back to the last");
        // Through the fold: a right press moves the highlight to the next category.
        let mut e = Editor::new(Draft::default());
        let mut prev = MenuPrev::default();
        press(&mut e, &mut prev, &[Key::Enter]);
        let cats = weapon_categories(&e);
        let before = category_of(&cats, e.option);
        press(&mut e, &mut prev, &[Key::Right]);
        assert_eq!(category_of(&cats, e.option), (before + 1) % cats.len(), "the fold walks the categories");
        press(&mut e, &mut prev, &[Key::Left]);
        assert_eq!(category_of(&cats, e.option), before, "and back");
        // Down never leaves the category, however long the list is.
        press(&mut e, &mut prev, &[Key::Down]);
        assert_eq!(category_of(&cats, e.option), before, "Up/Down stay inside the active category");
    }

    #[test]
    fn enter_picks_the_highlighted_gun_of_the_active_category() {
        let mut e = Editor::new(Draft::default());
        let mut prev = MenuPrev::default();
        press(&mut e, &mut prev, &[Key::Enter]);
        let cats = weapon_categories(&e);
        let active = category_of(&cats, e.option);
        // Walk within the category, then choose.
        press(&mut e, &mut prev, &[Key::Down]);
        assert_eq!(category_of(&cats, e.option), active);
        let target = e.option_weapon(e.option).expect("a gun under the highlight").id.clone();
        press(&mut e, &mut prev, &[Key::Enter]);
        assert!(!e.picking);
        assert_eq!(e.draft.primary, target, "the highlighted gun is the draft's primary");
    }

    #[test]
    fn the_category_column_rotates_so_the_active_one_is_on_top() {
        let d = Draft::default();
        let mut e = Editor::new(d);
        e.slot = Slot::Primary;
        e.picking = true;
        let cats = weapon_categories(&e);
        let column = category_column(&cats, 2);
        assert_eq!(column[0], 2, "the active category is on top");
        assert_eq!(column.len(), cats.len(), "every category is listed once");
        let mut sorted = column.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..cats.len()).collect::<Vec<_>>(), "no duplicates");
    }

    #[test]
    fn the_secondary_slot_lists_pistols_first() {
        let d = Draft { secondary: "fnp45_mp".into(), ..Draft::default() };
        let mut e = Editor::new(d);
        e.slot = Slot::Secondary;
        e.picking = true;
        let cats = weapon_categories(&e);
        assert_eq!(cats[0].name, "PISTOLS", "BO2's secondary order starts with pistols");
        assert!(cats.iter().any(|c| c.name == "LAUNCHERS"), "and lists launchers");
    }

    #[test]
    fn r_clears_the_highlighted_slot_and_an_empty_one_is_a_no_op() {
        let mut d = Draft::default();
        d.primary_atts = vec!["gl".into(), "reflex".into()];
        let mut e = Editor::new(d);
        let mut prev = MenuPrev::default();
        // R on the primary clears the gun and its attachments (BO2's ClearWeapon).
        assert_eq!(e.slot, Slot::Primary);
        let frame = press(&mut e, &mut prev, &[Key::R]);
        assert!(frame.removed, "the remove fired");
        assert!(e.draft.primary.is_empty(), "the gun went");
        assert!(e.draft.primary_atts.is_empty(), "and its attachments");
        // A second R on the now-empty slot does nothing (no deny, no remove).
        let frame = press(&mut e, &mut prev, &[Key::R]);
        assert!(!frame.removed, "nothing left to remove");
        assert!(!frame.denied);
        // R on a perk tier clears that tier only.
        let mut e = Editor::new(Draft::default());
        let mut prev = MenuPrev::default();
        press(&mut e, &mut prev, &[Key::Down]); // Primary -> PrimaryAtts
        press(&mut e, &mut prev, &[Key::Down]); // -> Secondary
        press(&mut e, &mut prev, &[Key::Down]); // -> SecondaryAtts
        press(&mut e, &mut prev, &[Key::Down]); // -> Lethal
        press(&mut e, &mut prev, &[Key::Down]); // -> Tactical
        press(&mut e, &mut prev, &[Key::Down]); // -> Perk 1
        assert_eq!(e.slot, Slot::Perk1);
        let frame = press(&mut e, &mut prev, &[Key::R]);
        assert!(frame.removed);
        assert!(e.draft.perks[0].is_none(), "tier 1 is empty");
        assert!(e.draft.perks[1].is_some(), "tier 2 is untouched");
    }

    #[test]
    fn the_circled_x_clears_only_the_selected_filled_box() {
        let mut d = Draft::default();
        d.primary_atts = vec!["gl".into(), "reflex".into()];
        let mut e = Editor::new(d);
        let l = editor_layout(1280.0, 720.0, 1.0);
        // Select the attachment strip, then click the second attachment's X.
        e.slot = Slot::PrimaryAtts;
        let cells = editor_cells(&l, &e);
        let second = cells.iter().find(|c| c.slot == Slot::PrimaryAtts && c.index == 1).expect("the second box");
        let (rx, ry, rw, rh) = remove_rect(second, 1.0);
        assert_eq!(editor_remove_at(&cells, &e, 1.0, rx + rw * 0.5, ry + rh * 0.5), Some((Slot::PrimaryAtts, 1)));
        // The X is not live on an unselected slot (BO2 draws it on the highlighted one).
        e.slot = Slot::Primary;
        assert_eq!(editor_remove_at(&cells, &e, 1.0, rx + rw * 0.5, ry + rh * 0.5), None);
        // Clicking it removes exactly that attachment.
        let mut e = Editor::new(Draft { primary: "an94_mp".into(), primary_atts: vec!["gl".into(), "reflex".into()], ..Draft::default() });
        e.slot = Slot::PrimaryAtts;
        let cells = editor_cells(&l, &e);
        let second = cells.iter().find(|c| c.slot == Slot::PrimaryAtts && c.index == 1).cloned().expect("the second box");
        let (rx, ry, rw, rh) = remove_rect(&second, 1.0);
        let mut prev = MenuPrev::default();
        let mut pointer = MenuPointer { screen_w: 1280.0, screen_h: 720.0, ..Default::default() };
        pointer.mouse_x = rx + rw * 0.5;
        pointer.mouse_y = ry + rh * 0.5;
        let frame = editor_frame(&mut e, &pointer, &prev);
        prev.update(&pointer);
        pointer.mouse_buttons = crate::input::mouse_buttons::LEFT;
        let frame = editor_frame(&mut e, &pointer, &prev);
        assert!(frame.removed, "the click cleared the box");
        assert_eq!(e.draft.primary_atts, vec!["gl".to_string()], "the first attachment stays");
        assert!(!e.picking, "clearing does not open the list");
    }

    #[test]
    fn the_perk_lists_follow_bo2s_grid_order() {
        // e-perks/001, /006, /011: the grids read left to right, top to bottom.
        let expected = [
            vec!["Lightweight", "Hardline", "BlindEye", "FlakJacket", "Ghost"],
            vec!["Toughness", "ColdBlooded", "FastHands", "HardWired", "Scavenger"],
            vec!["Dexterity", "ExtremeConditioning", "Engineer", "TacticalMask", "DeadSilence", "Awareness"],
        ];
        let mut e = Editor::new(Draft::default());
        for (tier, want) in expected.iter().enumerate() {
            e.slot = perk_slot(tier as u8 + 1);
            assert_eq!(e.options(), *want, "perk tier {}", tier + 1);
        }
    }

    #[test]
    fn the_bonus_card_list_follows_bo2s_grid_order() {
        // f-wildcards/001: Perk 1 Greed, Perk 2 Greed, Perk 3 Greed / Overkill, Secondary Gunfighter,
        // Primary Gunfighter / Tactician, Danger Close.
        let mut e = Editor::new(Draft::default());
        e.slot = Slot::Wildcards;
        assert_eq!(
            e.options(),
            vec![
                "Perk 1 Greed",
                "Perk 2 Greed",
                "Perk 3 Greed",
                "Overkill",
                "Secondary Gunfighter",
                "Primary Gunfighter",
                "Tactician",
                "Danger Close"
            ]
        );
    }

    #[test]
    fn the_stat_bars_are_in_bo2s_order_and_segmented() {
        assert_eq!(STAT_LABELS, ["FIRE RATE", "DAMAGE", "RANGE", "ACCURACY"]);
        let an94 = weapons::weapon("an94_mp");
        let stats = weapon_stats(an94);
        assert_eq!(stat_value(&stats, 0), stats.fire_rate);
        assert_eq!(stat_value(&stats, 1), stats.damage);
        assert_eq!(stat_value(&stats, 2), stats.range);
        assert_eq!(stat_value(&stats, 3), stats.accuracy);
        assert_eq!(stat_lit(0.0), 0);
        assert_eq!(stat_lit(1.0), STAT_SEGMENTS);
        let (max_damage, _, _, _, _) = gun_bounds();
        let strongest = weapons::catalog().all().iter().copied().find(|d| d.class.is_gun() && (d.damage[0] - max_damage).abs() < 1e-4).expect("a max-damage gun");
        assert_eq!(weapon_stats(strongest).damage, 1.0);
        assert_eq!(weapon_stats(&WeaponDef { damage: [max_damage, 1.0, 0.0, 0.0, 0.0, 0.0], ..Default::default() }).damage, 1.0);
    }

    #[test]
    fn the_dialogs_follow_bo2s_entries_and_escape_backs_out() {
        let set = Dialog::set_options("CLASS SET 1");
        assert_eq!(set.kind.title(), "CLASS SET OPTIONS");
        assert_eq!(set.items.iter().map(|i| i.label()).collect::<Vec<_>>(), vec!["RENAME", "RESET SET"]);
        let class = Dialog::class_options("Custom 5");
        assert_eq!(class.kind.title(), "CLASS OPTIONS");
        assert_eq!(
            class.items.iter().map(|i| i.label()).collect::<Vec<_>>(),
            vec!["RENAME", "COPY", "RESET TO DEFAULT", "CLEAR LOADOUT"],
            "BO2's class dialog: a-chooseclass/023"
        );
        let mut d = class.clone();
        let mut prev = MenuPrev::default();
        let mut pointer = MenuPointer { screen_w: 1280.0, screen_h: 720.0, ..Default::default() };
        pointer.keys.set_down(Key::Down, true);
        assert_eq!(dialog_step(&mut d, &prev, &pointer), DialogOutcome::Continue);
        assert_eq!(d.highlight, 1, "Down walks the entries");
        prev.update(&pointer);
        pointer.keys.set_down(Key::Enter, true);
        assert_eq!(dialog_step(&mut d, &prev, &pointer), DialogOutcome::Confirm(DialogItem::Copy));
        prev.update(&pointer);
        pointer.keys.clear();
        pointer.keys.set_down(Key::Escape, true);
        assert_eq!(dialog_step(&mut d, &prev, &pointer), DialogOutcome::Back);
    }

    #[test]
    fn a_rename_types_into_the_dialog_field_and_commits_on_enter() {
        let mut d = Dialog::set_options("CLASS SET 1");
        d.begin_rename("CLASS SET 1");
        let mut prev = MenuPrev::default();
        let mut pointer = MenuPointer { screen_w: 1280.0, screen_h: 720.0, ..Default::default() };
        for k in [Key::Backspace, Key::Backspace, Key::Backspace, Key::Backspace, Key::Backspace, Key::Backspace, Key::Backspace, Key::Backspace, Key::Backspace, Key::Backspace, Key::Backspace] {
            pointer.keys.set_down(k, true);
            dialog_step(&mut d, &prev, &pointer);
            prev.update(&pointer);
            pointer.keys.set_down(k, false);
            dialog_step(&mut d, &prev, &pointer);
            prev.update(&pointer);
        }
        assert!(d.naming);
        for k in [Key::S, Key::N, Key::Key1] {
            pointer.keys.set_down(k, true);
            dialog_step(&mut d, &prev, &pointer);
            prev.update(&pointer);
            pointer.keys.set_down(k, false);
            dialog_step(&mut d, &prev, &pointer);
            prev.update(&pointer);
        }
        assert_eq!(d.buffer, "SN1");
        pointer.keys.set_down(Key::Enter, true);
        assert_eq!(dialog_step(&mut d, &prev, &pointer), DialogOutcome::Confirm(DialogItem::Rename));
        assert!(!d.naming, "Enter closes the field");
    }
}
