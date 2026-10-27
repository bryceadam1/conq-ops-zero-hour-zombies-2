//! The editor's and the pickers' geometry, in LUI units, from the Lua (see `bo2.rs` for the frame).

use super::bo2::{self, Frame};
use super::*;
use crate::lui::{Place, Rect};

/// `CoD.CustomClass`'s grid (`patch_ui_mp/ui/t6/customclass.lua`'s `AddWidgets`): the element is
/// `CoD.Menu.Width` wide, centred, top 60, so element-relative and menu-relative units agree.
pub const ELEMENT_TOP: f32 = 60.0;
/// `ButtonHeight` (secondary, attachment rows, grenades, bonus cards).
pub const BUTTON_H: f32 = 71.0;
/// `PrimaryWeaponButtonHeight` (`ButtonHeight * 2 + MinorGridSpacing`).
pub const PRIMARY_H: f32 = 151.0;
/// `LeftSideButtonWidth` / `RightSideButtonWidth` (with `MajorGridSpacing` 9 between).
pub const LEFT_W: f32 = 500.0;
pub const RIGHT_X: f32 = 509.0;
pub const RIGHT_W: f32 = 355.0;
/// `MajorGridSpacing` / `MinorGridSpacing` (both 9).
pub const GRID_GAP: f32 = 9.0;

/// `CoD.SlotList`: a slot plate is `SlotHeight * 1.4` wide (`SlotWidth`), `Spacing` 15 apart; the
/// list is right-anchored inside the button's `InnerBorder` (5) and fills its height.
pub const SLOT_W: f32 = 64.0 * 1.4;
pub const SLOT_PITCH: f32 = SLOT_W + 15.0;
pub const INNER_BORDER: f32 = 5.0;

/// The picker grid's top. `CACAttachments.GridTop = 60` (`cacattachmentsmenu.lua:8`) is never read;
/// the menu's own layout would derive 83 from Big (48) + bar (10) + 25 (`:116-168`), but the
/// captures place the selected cell's top border at y≈305 on a 1890-tall screen — 80 LUI below the
/// menu top (`d-attach/002`, 3024×1890). The captures win; the 60 would sit inside the allocation
/// bar.
pub const GRID_TOP: f32 = 80.0;
/// `CoD.CACAttachments.GridItemSpacing` (and `CACPerksP`'s).
pub const GRID_ITEM_GAP: f32 = 8.0;
/// `CoD.CACAttachments.GridItemHeight`: `64 + ContentGridButton.TitleHeight + 10`
/// (`cacattachmentsmenu.lua:21-25`), where `TitleHeight` is `textSize.ExtraSmall` = 20
/// (`contentgridbutton.lua:5-10`, `codbase.lua:344-345`).
pub const ATTACH_GRID_H: f32 = 64.0 + bo2::TEXT_EXTRA_SMALL + 10.0;
/// The attachment picker's lower panel starts under three full grid rows and the hint line.
/// `cacattachmentsmenu.lua:238-247` computes it from the menu's own grid top (83): 83 + 3*94 +
/// 2*8 + 3 + 25 + 5 = 414; the captures put the Big title's box there (`d-attach/002`). It is not
/// tied to the captured [`GRID_TOP`] 80, which is 3 units higher.
pub const ATTACH_PANEL_TOP: f32 = 414.0;

/// The editor's geometry: the class name as the heading (Big), the allocation bar under it
/// (element top 3, 10 tall across the menu's width), then the left column (PRIMARY, ATTACHMENTS,
/// SECONDARY, ATTACHMENTS, WILDCARDS) and the right one (PERK 1-3, LETHAL, TACTICAL) of
/// `customclass.lua`, the mod's SAVE CLASS under them, the prompts at the menu's bottom.
#[derive(Clone, Copy, Debug)]
pub struct EditorLayout {
    pub frame: Frame,
    /// The class-name row (the screen's title), menu-relative (0, 0, 864, Big).
    pub title: Rect,
    /// The allocation bar (element top 3, 10 tall, the menu's width).
    pub bar: Rect,
    pub primary: Rect,
    pub primary_atts: Rect,
    pub secondary: Rect,
    pub secondary_atts: Rect,
    pub perks: [Rect; 3],
    pub lethal: Rect,
    pub tactical: Rect,
    /// The WILDCARDS label (ExtraSmall) and its three cards across the menu.
    pub wildcards_label: Rect,
    pub wildcards: [Rect; 3],
    /// The mod's own SAVE CLASS row (BO2 saves on back-out): below the grid, right column.
    pub save: Rect,
}

pub fn editor_layout(w: f32, h: f32) -> EditorLayout {
    let frame = Frame::new(w, h);
    let content = frame.in_menu(Place::new((false, false, -bo2::MENU_W / 2.0, bo2::MENU_W / 2.0), (true, true, ELEMENT_TOP, 0.0)));
    let at = |x: f32, y: f32, w: f32, h: f32| Rect { x: content.x + x, y: content.y + y, w, h };
    let row2 = PRIMARY_H + GRID_GAP; // primary -> attachments
    let row3 = row2 + BUTTON_H + GRID_GAP; // -> secondary / lethal
    let row4 = row3 + BUTTON_H + GRID_GAP; // -> attachments / tactical
    let wild_label_y = row4 + BUTTON_H + 5.0;
    let wild_y = wild_label_y + bo2::TEXT_EXTRA_SMALL + 2.0;
    let card_w = (bo2::MENU_W - 2.0 * GRID_GAP) / 3.0;
    EditorLayout {
        frame,
        title: frame.in_menu(Place::new((true, false, 0.0, bo2::MENU_W), (true, false, 0.0, bo2::TEXT_BIG))),
        bar: at(0.0, 3.0, bo2::MENU_W, 10.0),
        primary: at(0.0, 25.0, LEFT_W, PRIMARY_H),
        primary_atts: at(0.0, 25.0 + row2, LEFT_W, BUTTON_H),
        secondary: at(0.0, 25.0 + row3, LEFT_W, BUTTON_H),
        secondary_atts: at(0.0, 25.0 + row4, LEFT_W, BUTTON_H),
        perks: [25.0, 25.0 + BUTTON_H + GRID_GAP, 25.0 + 2.0 * (BUTTON_H + GRID_GAP)].map(|y| at(RIGHT_X, y, RIGHT_W, BUTTON_H)),
        lethal: at(RIGHT_X, 25.0 + row3, RIGHT_W, BUTTON_H),
        tactical: at(RIGHT_X, 25.0 + row4, RIGHT_W, BUTTON_H),
        wildcards_label: at(2.0, 25.0 + wild_label_y, bo2::MENU_W, bo2::TEXT_EXTRA_SMALL),
        wildcards: [0, 1, 2].map(|i| at(i as f32 * (card_w + GRID_GAP), 25.0 + wild_y, card_w, BUTTON_H)),
        save: at(RIGHT_X, 25.0 + wild_y + BUTTON_H + GRID_GAP, RIGHT_W, 36.0),
    }
}

/// One addressable box of the editor's screen: the slot it edits, which of the slot's repeated
/// boxes it is (attachment 2, bonus card 3, …), and where it sits (root units).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorCell {
    pub slot: Slot,
    pub index: usize,
    pub rect: Rect,
}

impl EditorCell {
    /// Whether a screen point is inside the box.
    pub fn contains(&self, f: &Frame, x: f32, y: f32) -> bool {
        bo2::hit(f, self.rect, x, y)
    }
}

/// The slot plates of a button: `n` plates right-anchored inside its `InnerBorder`, filled from
/// the left (`slotlistgridbutton.lua`, `slotlist.lua`). Plate `i`'s box (root units).
pub fn slot_plate(button: Rect, n: usize, i: usize) -> Rect {
    let width = n as f32 * SLOT_W + (n as f32 - 1.0) * (SLOT_PITCH - SLOT_W);
    let x = button.right() - INNER_BORDER - width + i as f32 * SLOT_PITCH;
    Rect { x, y: button.y + INNER_BORDER, w: SLOT_W, h: button.h - 2.0 * INNER_BORDER }
}

/// The editor's boxes, in the walk order of [`Slot::ALL`] (the second perk rows beside their tier,
/// the attachment and bonus-card boxes in slot order).
pub fn editor_cells(l: &EditorLayout, editor: &Editor) -> Vec<EditorCell> {
    let d = &editor.draft;
    let mut out: Vec<EditorCell> = Vec::with_capacity(22);
    let mut cell = |slot, index, rect| out.push(EditorCell { slot, index, rect });
    // The heading is the class-name row (BO2's editor titles the screen with the class name).
    cell(Slot::Name, 0, l.title);
    cell(Slot::Primary, 0, l.primary);
    // The attachment plates sit right of their label in the row's slot list, BO2's grid. A weapon
    // that offers no attachments has no plates at all ("Attachments not available", b-editor/002).
    if offers_attachments(&d.primary) {
        for i in 0..d.primary_att_max() {
            cell(Slot::PrimaryAtts, i, slot_plate(l.primary_atts, 3, i));
        }
    }
    cell(Slot::Secondary, 0, l.secondary);
    if offers_attachments(&d.secondary) {
        for i in 0..d.secondary_att_max() {
            cell(Slot::SecondaryAtts, i, slot_plate(l.secondary_atts, 3, i));
        }
    }
    cell(Slot::Lethal, 0, slot_plate(l.lethal, 2, 0));
    cell(Slot::Tactical, 0, slot_plate(l.tactical, 2, 0));
    for tier in 1..=3u8 {
        let button = l.perks[(tier - 1) as usize];
        cell(perk_slot(tier), 0, slot_plate(button, 2, 0));
        if d.wildcards.contains(&greed_wildcard(tier)) {
            cell(perk_extra_slot(tier), 1, slot_plate(button, 2, 1));
        }
    }
    for (i, r) in l.wildcards.iter().enumerate() {
        cell(Slot::Wildcards, i, *r);
    }
    cell(Slot::Save, 0, l.save);
    out
}

/// The box under a screen point.
pub fn editor_cell_at(l: &EditorLayout, cells: &[EditorCell], x: f32, y: f32) -> Option<usize> {
    cells.iter().position(|c| c.contains(&l.frame, x, y))
}

/// BO2's circled `X` on a filled, selected box: where it draws and what it covers (root units;
/// centred on the box's top-right corner, `b-editor/028`).
pub fn remove_rect(cell: &EditorCell) -> Rect {
    let d = 20.0;
    Rect { x: cell.rect.right() - d * 0.5, y: cell.rect.y - d * 0.5, w: d, h: d }
}

/// The `(slot, box)` a click clears at this point: only the highlighted box's filled plates carry
/// the `X` (BO2 shows it on the highlighted button alone).
pub fn editor_remove_at(l: &EditorLayout, cells: &[EditorCell], editor: &Editor, x: f32, y: f32) -> Option<(Slot, usize)> {
    cells.iter().find_map(|c| {
        if !cell_focused(editor, c) || !cell_filled(editor, c) {
            return None;
        }
        bo2::hit(&l.frame, remove_rect(c), x, y).then_some((c.slot, c.index))
    })
}

/// Whether a box is the highlighted one: its slot is, and on the card row it is the highlighted
/// card (each bonus card is its own button, `customclass.lua:772-802`).
pub fn cell_focused(editor: &Editor, cell: &EditorCell) -> bool {
    cell.slot == editor.slot && (cell.slot != Slot::Wildcards || cell.index == editor.card)
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

// ------------------------------------------------------------------------------------------------
// The pickers' geometry (BO2's carousel and grids)
// ------------------------------------------------------------------------------------------------

/// `CoD.CACPrimaryWeapons.ItemWidth`/`ItemHeight` (the gun tiles) and its highlighted card
/// (`HighlightedItemWidth`/`HighlightedItemHeight`).
pub const CAROUSEL_ITEM_W: f32 = 192.0;
pub const CAROUSEL_ITEM_H: f32 = 146.0;
pub const CAROUSEL_CARD_W: f32 = 345.6;
pub const CAROUSEL_CARD_H: f32 = 217.8;
/// The carousel's top (under the title and allocation bar, `cacweaponsp.lua`) and the tiles' inset.
pub const CAROUSEL_TOP: f32 = 83.0;
const CAROUSEL_TILE_TOP: f32 = 97.0;
const CAROUSEL_TILE_PITCH: f32 = CAROUSEL_ITEM_W + 4.0;

/// The weapon picker's highlighted card (left, menu-relative).
pub fn carousel_card(l: &EditorLayout) -> Rect {
    l.frame.in_menu(Place::new((true, false, 0.0, CAROUSEL_CARD_W), (true, false, CAROUSEL_TOP, CAROUSEL_TOP + CAROUSEL_CARD_H)))
}

/// Tile `i` of the weapon picker's carousel (menu-relative), flowing right of the card.
pub fn carousel_tile(l: &EditorLayout, i: usize) -> Rect {
    let x = CAROUSEL_CARD_W + 12.0 + i as f32 * CAROUSEL_TILE_PITCH;
    l.frame.in_menu(Place::new((true, false, x, x + CAROUSEL_ITEM_W), (true, false, CAROUSEL_TILE_TOP, CAROUSEL_TILE_TOP + CAROUSEL_ITEM_H)))
}

/// How many tiles the carousel shows (the last may clip at the menu's edge, as BO2's does).
pub fn carousel_shown() -> usize {
    ((bo2::MENU_W - (CAROUSEL_CARD_W + 12.0)) / CAROUSEL_TILE_PITCH).ceil() as usize
}

/// A grid picker's columns for a slot: `CoD.CACAttachments.GridColumns` is 6,
/// `CoD.CACPerksP.GridColumns` 3 (the perks, wildcards and grenade pickers share it,
/// `e-perks/001`, `f-wildcards/001`). The count lives on the pure slot model
/// (`Slot::option_columns`, which the editor's own walk reads), so the picture and the arrows can
/// never disagree; the carousel has no grid and falls back to the common three.
pub fn grid_columns(slot: Slot) -> usize {
    slot.option_columns().unwrap_or(3)
}

/// One grid picker's cell box (menu-relative): `GridItemWidth` = (864 − spacing × (cols−1)) / cols,
/// the height `64 + TitleHeight + 10` for attachments (94) and `96 + TitleHeight + 10` for the rest
/// (126), rows `GridTop` down, `GridItemSpacing` apart.
pub fn grid_cell(l: &EditorLayout, slot: Slot, i: usize) -> Rect {
    let cols = grid_columns(slot);
    let w = (bo2::MENU_W - GRID_ITEM_GAP * (cols as f32 - 1.0)) / cols as f32;
    let h = if cols == 6 { ATTACH_GRID_H } else { 96.0 + bo2::TEXT_EXTRA_SMALL + 10.0 };
    let x = (i % cols) as f32 * (w + GRID_ITEM_GAP);
    let y = GRID_TOP + (i / cols) as f32 * (h + GRID_ITEM_GAP);
    l.frame.in_menu(Place::new((true, false, x, x + w), (true, false, y, y + h)))
}

/// The attachment picker's lower half (`cacattachmentsmenu.lua:247-267`): full menu width from
/// under the third grid row to the menu's bottom.
pub fn attachment_panel(l: &EditorLayout) -> Rect {
    l.frame.in_menu(Place::new((true, true, 0.0, 0.0), (true, true, ATTACH_PANEL_TOP, 0.0)))
}

/// The highlighted attachment's title (`cacattachmentsmenu.lua:268-305`: Big at panel top).
pub fn attachment_title(l: &EditorLayout) -> Rect {
    let p = attachment_panel(l);
    Rect { x: p.x, y: p.y, w: bo2::MENU_W * 0.5, h: bo2::TEXT_BIG }
}

/// The description line (`cacattachmentsmenu.lua:307-353`: Default, half the menu wide, alpha 0.4).
pub fn attachment_description(l: &EditorLayout) -> Rect {
    let p = attachment_panel(l);
    Rect { x: p.x, y: p.y + bo2::TEXT_BIG, w: bo2::MENU_W * 0.5, h: bo2::TEXT_DEFAULT }
}

/// The four stat bars (`cacattachmentsmenu.lua:354-372`: half the menu wide, from `Big + Default*2 +
/// 10` to 80 above the panel's bottom; their labels are the `maxTextWidth` 60 left of the bars).
pub fn attachment_stats(l: &EditorLayout) -> Rect {
    let p = attachment_panel(l);
    Rect {
        x: p.x,
        y: p.y + bo2::TEXT_BIG + 2.0 * bo2::TEXT_DEFAULT + 10.0,
        w: bo2::MENU_W * 0.5,
        h: p.h - (bo2::TEXT_BIG + 2.0 * bo2::TEXT_DEFAULT + 10.0) - 80.0,
    }
}

/// The large weapon image on the panel's right (`cacattachmentsmenu.lua:422-450`: 300 x 150,
/// right-anchored, lifted 40 above the panel top).
pub fn attachment_weapon_image(l: &EditorLayout) -> Rect {
    let p = attachment_panel(l);
    Rect { x: l.frame.menu.right() - 290.0, y: p.y - 40.0, w: 300.0, h: 150.0 }
}

/// The `Equipped` slot-list box (`cacattachmentsmenu.lua:452-501`: `SlotListSize + 10` tall, sitting
/// `Prompt.Height + 30` above the menu bottom; width from [`attachment_slot_box_w`]).
pub fn attachment_slot_box(l: &EditorLayout, dips: usize) -> Rect {
    let p = attachment_panel(l);
    let w = attachment_slot_box_w(dips);
    let h = 64.0 + 10.0;
    Rect { x: p.x + bo2::MENU_W * 0.5 + 10.0 + 5.0, y: p.bottom() - bo2::PROMPT_H - 30.0 - h, w, h }
}

/// `GetSlotlistBackgroundWidth`: half a spacing at each end, the dips at `SlotListSize` (64) with
/// `SlotListSpacing` (20) between (`cacattachmentsmenu.lua:707-724`).
pub fn attachment_slot_box_w(dips: usize) -> f32 {
    let n = dips.max(1) as f32;
    10.0 + 64.0 * n + 20.0 * (n - 1.0) + 10.0
}

/// The `Equipped` label above the slot list (`cacattachmentsmenu.lua:502-536`: Default, alpha 0.5,
/// two units above the box).
pub fn attachment_slot_title(l: &EditorLayout, dips: usize) -> Rect {
    let b = attachment_slot_box(l, dips);
    Rect { x: b.x, y: b.y - 2.0 - bo2::TEXT_DEFAULT, w: b.w, h: bo2::TEXT_DEFAULT }
}

/// The first option the picker shows (scrolled so the highlighted one is in view) and how many
/// cards/tiles fit. BO2's carousel IS its highlighted card: the window starts at it
/// (`b-editor/016`).
pub fn picker_window(slot: Slot, highlighted: usize) -> (usize, usize) {
    match slot {
        Slot::Primary | Slot::Secondary => (highlighted, carousel_shown()),
        _ => {
            let cols = grid_columns(slot);
            let rows = grid_rows_visible(slot);
            let row = highlighted / cols;
            let first_row = row.saturating_sub(rows.saturating_sub(1));
            (first_row * cols, rows * cols)
        }
    }
}

/// How many grid rows show at once. The attachment picker reserves the lower half for the
/// highlighted entry (`cacattachmentsmenu.lua:238-247`), so exactly three of its 94-unit rows fit
/// above `ATTACH_PANEL_TOP` (`d-attach/002`); the other pickers have no lower panel.
fn grid_rows_visible(slot: Slot) -> usize {
    if slot.option_columns() == Some(6) {
        return 3;
    }
    let h = 96.0 + bo2::TEXT_EXTRA_SMALL + 10.0;
    let avail = bo2::MENU_H - GRID_TOP - bo2::PROMPT_H - 130.0;
    ((avail + GRID_ITEM_GAP) / (h + GRID_ITEM_GAP)).max(1.0).floor() as usize
}

/// The drawn option under a screen point, already translated to its index in the shown window.
pub fn picker_option_at(l: &EditorLayout, slot: Slot, x: f32, y: f32, first: usize, count: usize) -> Option<usize> {
    match slot {
        Slot::Primary | Slot::Secondary => {
            if first < count && bo2::hit(&l.frame, carousel_card(l), x, y) {
                return Some(first);
            }
            for j in 0..carousel_shown() {
                let i = first + 1 + j;
                if i >= count {
                    break;
                }
                if bo2::hit(&l.frame, carousel_tile(l, j), x, y) {
                    return Some(i);
                }
            }
            None
        }
        _ => {
            let shown = grid_columns(slot) * grid_rows_visible(slot);
            for j in 0..shown {
                let i = first + j;
                if i >= count {
                    break;
                }
                if bo2::hit(&l.frame, grid_cell(l, slot, j), x, y) {
                    return Some(i);
                }
            }
            None
        }
    }
}
