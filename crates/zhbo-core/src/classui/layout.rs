//! The screens' geometry: the pick and editor layouts, the editor's boxes and the pickers' windows, and their hit-testing.

use super::*;

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
pub(super) fn allocation_bar_rect(l: &PickLayout, s: f32) -> (f32, f32, f32) {
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
pub(super) fn prompt_width(p: &Prompt, s: f32) -> f32 {
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
pub(super) fn perk_slot(tier: u8) -> Slot {
    match tier {
        2 => Slot::Perk2,
        3 => Slot::Perk3,
        _ => Slot::Perk1,
    }
}

/// The perk slot of a tier's Greed row.
pub(super) fn perk_extra_slot(tier: u8) -> Slot {
    match tier {
        2 => Slot::Perk2b,
        3 => Slot::Perk3b,
        _ => Slot::Perk1b,
    }
}

/// The Greed wildcard that opens a tier's second perk row.
pub(super) fn greed_wildcard(tier: u8) -> Wildcard {
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
