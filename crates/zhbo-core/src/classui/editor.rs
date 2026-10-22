//! BO2's allocation editor: the boxed grid (`customclass.lua`, `b-editor/*`) and its pickers
//! (the weapon carousel `cacweaponsp.lua`, `b-editor/016`; the content grids `cacattachmentsmenu.lua`,
//! `d-attach/002`, and `cacperksp.lua`, `e-perks/001`), drawn with BO2's materials and fonts.

use super::bo2::{self, argb, Frame, BLACK, BOII_ORANGE, OFF_WHITE};
use super::*;
use crate::lui::font::{Align, Family};
use crate::lui::Rect;

/// `CoD.CACClassLoadout.DefaultBackgroundColor` at `DefaultBackgroundAlpha`: every grid button's
/// gradient tint.
pub const BUTTON_TINT: (f32, f32, f32) = (0.0392, 0.0784, 0.0784);
pub const BUTTON_ALPHA: f32 = 0.4;

/// A grid button's background: `menu_mp_cac_grad_stretch` stretched over the box
/// (`growinggridbutton.lua`), tinted like BO2; the fallback is the same near-black plate.
fn button_bg(prims: &mut Vec<Prim>, f: &Frame, r: Rect) {
    let p = f.px(r);
    let fallback = vec![Prim::Rect { x: p.x, y: p.y, w: p.w, h: p.h, argb: argb(BUTTON_TINT, BUTTON_ALPHA + 0.1) }];
    bo2::image(prims, f, "menu_mp_cac_grad_stretch", r, argb(BUTTON_TINT, BUTTON_ALPHA + 0.35), fallback);
}

/// The selected box's frame: BO2's orange border plus, when it holds something, the circled `X`
/// (`b-editor/028`).
fn focus_frame(prims: &mut Vec<Prim>, f: &Frame, r: Rect) {
    bo2::border(prims, f, r, 2.0, argb(BOII_ORANGE, 1.0));
}

/// BO2's circled `X` remove affordance on the box's top-right corner (`b-editor/028`).
pub(super) fn remove_glyph(prims: &mut Vec<Prim>, f: &Frame, cell: &EditorCell) {
    let p = f.px(remove_rect(cell));
    let (cx, cy) = (p.x + p.w * 0.5, p.y + p.h * 0.5);
    let r = p.w * 0.5;
    let seg = 12;
    for i in 0..seg {
        let a0 = i as f32 / seg as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / seg as f32 * std::f32::consts::TAU;
        prims.push(Prim::Line { x0: cx + r * a0.cos(), y0: cy + r * a0.sin(), x1: cx + r * a1.cos(), y1: cy + r * a1.sin(), width: 1.5, argb: WHITE });
    }
    let k = r * 0.42;
    prims.push(Prim::Line { x0: cx - k, y0: cy - k, x1: cx + k, y1: cy + k, width: 1.8, argb: WHITE });
    prims.push(Prim::Line { x0: cx - k, y0: cy + k, x1: cx + k, y1: cy - k, width: 1.8, argb: WHITE });
}

/// A slot's name (`PRIMARY`, `PERK 1`, …) at a button's top-left, ExtraSmall at alpha 0.5.
fn slot_label(prims: &mut Vec<Prim>, f: &Frame, r: Rect, s: &str) {
    bo2::text(prims, f, Family::ExtraSmall, s, Rect { x: r.x + 3.0, y: r.y + 2.0, w: r.w, h: bo2::TEXT_EXTRA_SMALL }, Align::Left, argb(OFF_WHITE, 0.5));
}

/// The item's name under the slot label (`GrowingGridButton`'s subtitle: ExtraSmall, off-white).
fn slot_name(prims: &mut Vec<Prim>, f: &Frame, r: Rect, s: &str, line: f32) {
    bo2::text(prims, f, Family::ExtraSmall, s, Rect { x: r.x + 3.0, y: r.y + 2.0 + bo2::TEXT_EXTRA_SMALL * line, w: r.w, h: bo2::TEXT_EXTRA_SMALL }, Align::Left, argb(OFF_WHITE, 1.0));
}

/// BO2's allocation editor: the class name as the heading with the allocation bar under it, the
/// boxed grid down the screen, and the prompts at the bottom. A weapon picker or an option grid
/// takes the screen over while one is open, as BO2's do.
pub fn draw_editor(prims: &mut Vec<Prim>, w: f32, h: f32, editor: &Editor, deny: f32, now: f32) {
    let l = editor_layout(w, h);
    let f = &l.frame;
    bo2::backdrop(prims, f, w, h);
    // A picker titles itself: BO2's weapon screen is `PRIMARY`/`SECONDARY` (`MPUI_<slot>_CAPS`),
    // its attachment screen `<WEAPON> ATTACHMENTS`; the grid is the class's name.
    let title = if editor.picking {
        picker_title(editor)
    } else {
        let name = editor_title(editor);
        if editor.naming { format!("{name}_") } else { name }
    };
    bo2::title(prims, f, &title);
    // The allocation readout, BO2's bar across the menu with the n/10 counter above its right end.
    let spent = editor.draft.spent();
    bo2::text(
        prims,
        f,
        Family::Condensed,
        &format!("{spent}/{}", crate::classmenu::PICK_10),
        Rect { x: l.bar.right() - 100.0, y: l.bar.y - bo2::TEXT_CONDENSED - 2.0, w: 100.0, h: bo2::TEXT_CONDENSED },
        Align::Right,
        argb(bo2::COD_ORANGE, 1.0),
    );
    preview::allocation_bar(prims, f, l.bar, spent, crate::classmenu::PICK_10);
    match editor.slot {
        Slot::Primary | Slot::Secondary if editor.picking => draw_weapon_picker(prims, &l, editor),
        _ if editor.picking => draw_option_grid(prims, &l, editor),
        _ => draw_editor_grid(prims, &l, editor, now),
    }
    // The prompt bars: `ESC Back` and `TAB Personalize Knife` on the left (`caceditclass.lua:46-63`
    // adds the knife prompt to the left bar; `b-editor/002` and `/028` show the pair), `R Remove`
    // on the right only while the highlighted slot holds something (`b-editor/002` vs `/028`).
    // The standalone pickers are separate menus and carry only `ESC Back` (`c-primary/001`,
    // `d-attach/002`); the mod's category pager and gun row are mouse/wheel targets, not prompts.
    let cells = editor_cells(&l, editor);
    let removable = cells.iter().any(|c| c.slot == editor.slot && cell_filled(editor, c));
    let mut left: Vec<(&str, &str)> = vec![("ESC", "Back")];
    let right: Vec<(&str, &str)> = if editor.naming {
        left = vec![("ENTER", "Done"), ("BKSP", "Delete")];
        Vec::new()
    } else if editor.picking {
        Vec::new()
    } else {
        left = vec![("ESC", "Back"), ("TAB", "Personalize Knife")];
        if removable {
            vec![("R", "Remove")]
        } else {
            Vec::new()
        }
    };
    let boxes = bo2::prompt_boxes(f, &left, &right);
    bo2::draw_prompts(prims, f, &boxes);
    deny_flash(prims, w, h, f.s(), deny);
}

/// The editor's title: the class's name, or BO2's name for the slot a fresh draft will land in
/// (`Custom n`, `a-chooseclass/002`).
fn editor_title(editor: &Editor) -> String {
    let name = editor.draft.name.trim();
    if !name.is_empty() {
        return name.to_string();
    }
    empty_slot_name(profile::get().customs().len().min(profile::MAX_CLASSES - 1))
}

/// A picker's title (`cacweaponsp.lua`: `MPUI_<slot>_CAPS`; `cacattachmentsmenu.lua`:
/// `<WEAPON> ATTACHMENTS`; `e-perks/001`'s `PERK 1`, `f-wildcards/001`'s `WILDCARDS`).
pub(super) fn picker_title(editor: &Editor) -> String {
    match editor.slot {
        Slot::Primary => "PRIMARY".to_string(),
        Slot::Secondary => "SECONDARY".to_string(),
        Slot::PrimaryAtts | Slot::SecondaryAtts => {
            let id = if editor.slot == Slot::PrimaryAtts { editor.draft.primary.as_str() } else { editor.draft.secondary.as_str() };
            format!("{} ATTACHMENTS", weapons::weapon(id).display_name.to_uppercase())
        }
        other => other.label().to_string(),
    }
}

/// The editor's boxed grid (`b-editor/001`, `/028`).
fn draw_editor_grid(prims: &mut Vec<Prim>, l: &EditorLayout, editor: &Editor, now: f32) {
    let _ = now;
    let f = &l.frame;
    let cells = editor_cells(l, editor);
    for cell in &cells {
        draw_cell(prims, l, editor, cell);
    }
    // The attachment rows' label; a weapon that offers none says so in its place (b-editor/002).
    for (button, id) in [(l.primary_atts, editor.draft.primary.as_str()), (l.secondary_atts, editor.draft.secondary.as_str())] {
        if offers_attachments(id) {
            slot_label(prims, f, button, "ATTACHMENTS");
        } else {
            bo2::text(prims, f, Family::ExtraSmall, "Attachments not available", Rect { x: button.x + 3.0, y: button.y + 2.0, w: button.w, h: bo2::TEXT_EXTRA_SMALL }, Align::Left, argb(OFF_WHITE, 0.5));
        }
    }
    // The WILDCARDS label over the strip (`MENU_WILDCARDS_CAPS`).
    slot_label(prims, f, l.wildcards_label, "WILDCARDS");
}

/// One boxed cell of the grid.
fn draw_cell(prims: &mut Vec<Prim>, l: &EditorLayout, editor: &Editor, cell: &EditorCell) {
    let f = &l.frame;
    let r = cell.rect;
    let on = cell.slot == editor.slot;
    let filled = cell_filled(editor, cell);
    match cell.slot {
        Slot::Name => {} // the title draws it
        Slot::Save => {
            button_bg(prims, f, r);
            if on {
                focus_frame(prims, f, r);
            }
            bo2::text(prims, f, Family::Condensed, "SAVE CLASS", r, Align::Center, argb(if on { BOII_ORANGE } else { OFF_WHITE }, 1.0));
        }
        Slot::Primary | Slot::Secondary => {
            button_bg(prims, f, r);
            slot_label(prims, f, r, if cell.slot == Slot::Primary { "PRIMARY" } else { "SECONDARY" });
            let id = if cell.slot == Slot::Primary { editor.draft.primary.as_str() } else { editor.draft.secondary.as_str() };
            if filled {
                // `CACWeaponButton`: the image `AspectRatio` 2, `ImageBorder` 10, 80 from the right.
                let def = weapons::weapon(id);
                let ih = r.h - 2.0 * 10.0;
                let iw = ih * 2.0;
                let pic = Rect { x: r.right() - 90.0 - iw, y: r.y + 10.0, w: iw, h: ih };
                bo2::sprite(prims, f, &crate::names::cac_weapon_image(base_id(&def.id)), pic, argb(OFF_WHITE, 1.0), Vec::new());
                bo2::text(prims, f, Family::Condensed, &def.display_name, Rect { x: r.x + 3.0, y: r.bottom() - bo2::TEXT_CONDENSED - 2.0, w: r.w, h: bo2::TEXT_CONDENSED }, Align::Left, argb(OFF_WHITE, 1.0));
            }
            if on {
                focus_frame(prims, f, r);
                if filled {
                    remove_glyph(prims, f, cell);
                }
            }
        }
        Slot::PrimaryAtts | Slot::SecondaryAtts => {
            button_bg(prims, f, r);
            let atts = if cell.slot == Slot::PrimaryAtts { &editor.draft.primary_atts } else { &editor.draft.secondary_atts };
            if let Some(short) = atts.get(cell.index) {
                let icon = r.h - 8.0;
                if let Some(name) = sprites::attachment_icon(short) {
                    bo2::sprite(prims, f, name, Rect { x: r.x + (r.w - icon) * 0.5, y: r.y + 4.0, w: icon, h: icon }, argb(OFF_WHITE, 1.0), Vec::new());
                }
            }
            if on {
                focus_frame(prims, f, r);
                if filled {
                    remove_glyph(prims, f, cell);
                }
            }
        }
        Slot::Lethal | Slot::Tactical | Slot::Perk1 | Slot::Perk2 | Slot::Perk3 | Slot::Perk1b | Slot::Perk2b | Slot::Perk3b => {
            // The slot's plate carries its icon; the button's label and name(s) draw once, left.
            button_bg(prims, f, r);
            if let Some(art) = cell_icon(editor, cell) {
                let icon = r.h;
                bo2::sprite(prims, f, &art, Rect { x: r.x + (r.w - icon) * 0.5, y: r.y, w: icon, h: icon }, argb(OFF_WHITE, 1.0), Vec::new());
            }
            if on {
                focus_frame(prims, f, r);
                if filled {
                    remove_glyph(prims, f, cell);
                }
            }
            if cell.index == 0 {
                let button = button_of(l, cell.slot);
                let (label, names) = button_text(editor, cell.slot);
                slot_label(prims, f, button, label);
                for (k, name) in names.iter().enumerate() {
                    slot_name(prims, f, button, name, k as f32 + 1.0);
                }
            }
        }
        Slot::Wildcards => {
            button_bg(prims, f, r);
            if let Some(w) = editor.draft.wildcards.get(cell.index) {
                let art = sprites::wildcard_icon(*w);
                let ah = r.h - bo2::TEXT_CONDENSED - 10.0;
                let aw = ah * 1.3;
                bo2::sprite(prims, f, art, Rect { x: r.x + (r.w - aw) * 0.5, y: r.y + 4.0, w: aw, h: ah }, argb(OFF_WHITE, 1.0), Vec::new());
                bo2::text(prims, f, Family::Condensed, w.name(), Rect { x: r.x + 3.0, y: r.bottom() - bo2::TEXT_CONDENSED - 2.0, w: r.w, h: bo2::TEXT_CONDENSED }, Align::Left, argb(OFF_WHITE, 1.0));
            }
            if on {
                focus_frame(prims, f, r);
                if filled {
                    remove_glyph(prims, f, cell);
                }
            }
        }
    }
}

/// The button a plate-only cell belongs to (for its label), by slot.
fn button_of(l: &EditorLayout, slot: Slot) -> Rect {
    match slot {
        Slot::Lethal => l.lethal,
        Slot::Tactical => l.tactical,
        Slot::Perk1 => l.perks[0],
        Slot::Perk2 => l.perks[1],
        Slot::Perk3 => l.perks[2],
        Slot::Perk1b => l.perks[0],
        Slot::Perk2b => l.perks[1],
        Slot::Perk3b => l.perks[2],
        _ => l.primary,
    }
}

/// A perk/grenade button's label and its item names (the Greed row's name shares the button).
fn button_text(editor: &Editor, slot: Slot) -> (&'static str, Vec<String>) {
    let d = &editor.draft;
    match slot {
        Slot::Lethal => ("LETHAL", if d.lethal.is_empty() { Vec::new() } else { vec![weapons::weapon(&d.lethal).display_name.clone()] }),
        Slot::Tactical => ("TACTICAL", if d.tactical.is_empty() { Vec::new() } else { vec![weapons::weapon(&d.tactical).display_name.clone()] }),
        _ => {
            let tier = slot.perk_tier().unwrap_or(1) as usize - 1;
            let label = match tier {
                0 => "PERK 1",
                1 => "PERK 2",
                _ => "PERK 3",
            };
            let mut names: Vec<String> = Vec::new();
            if let Some(p) = d.perks[tier] {
                names.push(p.def().id.to_string());
            }
            if let Some(p) = d.extra_perks[tier] {
                names.push(p.def().id.to_string());
            }
            (label, names)
        }
    }
}

/// The icon of a box's current value.
pub(super) fn cell_icon(editor: &Editor, cell: &EditorCell) -> Option<String> {
    let d = &editor.draft;
    match cell.slot {
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
        _ => None,
    }
}

/// Whether a weapon offers any attachments at all (BO2's "Attachments not available", `b-editor/002`).
pub(super) fn offers_attachments(id: &str) -> bool {
    !id.is_empty() && !weapons::weapon(id).allowed.is_empty()
}

// ------------------------------------------------------------------------------------------------
// The weapon picker (`cacweaponsp.lua`, `b-editor/016`)
// ------------------------------------------------------------------------------------------------

/// BO2's weapon screen: the slot's title, the allocation readout, the highlighted gun's card with
/// its segmented bars right of it, the category's guns as tiles flowing right, and the category
/// column at the bottom left with the active on top.
fn draw_weapon_picker(prims: &mut Vec<Prim>, l: &EditorLayout, editor: &Editor) {
    let f = &l.frame;
    let slot = editor.slot;
    let cats = weapon_categories(editor);
    let active = category_of(&cats, editor.option);

    // BO2's carousel: the highlighted gun's card grows (`HighlightedItemWidth`); its successors
    // flow right as plain tiles (`b-editor/016`). The card IS the highlighted option.
    let opts = cats.get(active).map(|c| c.options.clone()).unwrap_or_default();
    let highlighted = opts.iter().position(|i| *i == editor.option).unwrap_or(0);
    let (first, shown) = picker_window(slot, highlighted);
    if let Some(def) = editor.option_weapon(editor.option) {
        let card = carousel_card(l);
        button_bg(prims, f, card);
        focus_frame(prims, f, card);
        let iw = card.w - 60.0;
        let ih = iw / 2.0;
        let pic = Rect { x: card.x + (card.w - iw) * 0.5, y: card.y + 12.0, w: iw, h: ih };
        bo2::sprite(prims, f, &crate::names::cac_weapon_image(base_id(&def.id)), pic, argb(OFF_WHITE, 1.0), Vec::new());
        bo2::text(prims, f, Family::Condensed, &def.display_name, Rect { x: card.x + 3.0, y: card.bottom() - bo2::TEXT_CONDENSED - 2.0, w: card.w, h: bo2::TEXT_CONDENSED }, Align::Left, argb(BOII_ORANGE, 1.0));
        // The four segmented bars, right of the card and under the tiles (`cacweaponsp.lua`'s
        // WeaponAttributeList, `b-editor/016`).
        if def.class.is_gun() {
            let x = CAROUSEL_CARD_W + 80.0;
            let r = f.in_menu(crate::lui::Place::new((true, false, x, x + bo2::MENU_W / 2.0), (true, false, 330.0, 430.0)));
            stat_bars(prims, f, r, &weapon_stats(def));
        }
    }
    // The tiles clip at the menu's edge, as BO2's carousel's partial cards do (`b-editor/016`).
    let p = f.px(f.menu);
    prims.push(Prim::ClipBegin { x: p.x, y: 0.0, w: p.w, h: p.h });
    for (j, opt) in opts.iter().enumerate().skip(first + 1).take(shown) {
        let r = carousel_tile(l, j - first - 1);
        let Some(def) = editor.option_weapon(*opt) else { continue };
        button_bg(prims, f, r);
        let iw = r.w - 40.0;
        let ih = iw / 2.0;
        let pic = Rect { x: r.x + (r.w - iw) * 0.5, y: r.y + 12.0, w: iw, h: ih };
        bo2::sprite(prims, f, &crate::names::cac_weapon_image(base_id(&def.id)), pic, argb(OFF_WHITE, 1.0), Vec::new());
        bo2::text(prims, f, Family::Condensed, &def.display_name, Rect { x: r.x + 3.0, y: r.bottom() - bo2::TEXT_CONDENSED - 2.0, w: r.w, h: bo2::TEXT_CONDENSED }, Align::Left, argb(OFF_WHITE, 1.0));
        if editor.option_chosen(*opt) {
            bo2::text(prims, f, Family::Condensed, "*", Rect { x: r.right() - 20.0, y: r.y + 2.0, w: 20.0, h: bo2::TEXT_CONDENSED }, Align::Center, argb(bo2::YELLOW_GLOW, 1.0));
        }
    }
    prims.push(Prim::ClipEnd);

    // BO2's category column: the active category orange on top, the rest following it in order.
    let column = category_column(&cats, active);
    let mut y = f.menu.bottom() - bo2::PROMPT_H - 20.0 - column.len() as f32 * 26.0;
    for (i, ci) in column.iter().enumerate() {
        let on = *ci == active;
        let cat = &cats[*ci];
        let r = Rect { x: f.menu.x, y, w: CAROUSEL_CARD_W, h: if on { bo2::TEXT_CONDENSED } else { bo2::TEXT_EXTRA_SMALL } };
        bo2::text(prims, f, if on { Family::Condensed } else { Family::ExtraSmall }, cat.name, r, Align::Left, argb(if on { BOII_ORANGE } else { OFF_WHITE }, 1.0));
        if on {
            let p = f.px(Rect { x: f.menu.x, y: y + bo2::TEXT_CONDENSED + 4.0, w: CAROUSEL_CARD_W * 0.9, h: 1.0 });
            prims.push(Prim::Rect { x: p.x, y: p.y, w: p.w, h: p.h.max(1.0), argb: argb(OFF_WHITE, 0.4) });
        }
        y += 26.0 + if i == 0 { bo2::TEXT_CONDENSED - bo2::TEXT_EXTRA_SMALL } else { 0.0 };
    }
}

/// BO2's segmented stat readout: four rows of blocks, FIRE RATE first (`c-primary/001`).
fn stat_bars(prims: &mut Vec<Prim>, f: &Frame, r: Rect, stats: &WeaponStats) {
    stat_bars_delta(prims, f, r, stats, None, 100.0, r.h / 4.0);
}

/// The same bars with BO2's attachment-modified values: white up to the base value, green segments
/// where the highlighted attachment helps and red where it hurts (`attributebarlist.lua:16-30`).
fn stat_bars_delta(prims: &mut Vec<Prim>, f: &Frame, r: Rect, base: &WeaponStats, modified: Option<&WeaponStats>, label_w: f32, row_h: f32) {
    // BO2's `setupDashes` draws a row of small blocks; an unlit block is an outline, a lit one a
    // filled block (`attributebarlist.lua:266-273`; `d-attach/002`, `c-primary/001`). The capture
    // puts 18 dashes from x≈563 to ≈1280 on a 3024-wide screen, i.e. a 272-unit bar.
    let bar_w = (r.w - label_w).clamp(1.0, 272.0);
    let pitch = (bar_w / STAT_SEGMENTS as f32).max(1.0);
    let dash_w = 7.0;
    for (i, label) in STAT_LABELS.iter().enumerate() {
        let top = r.y + i as f32 * row_h;
        bo2::text(prims, f, Family::ExtraSmall, label, Rect { x: r.x, y: top, w: label_w, h: bo2::TEXT_EXTRA_SMALL }, Align::Left, argb(OFF_WHITE, 0.4));
        let base_lit = stat_lit(stat_value(base, i));
        let mod_lit = modified.map_or(base_lit, |m| stat_lit(stat_value(m, i)));
        for k in 0..STAT_SEGMENTS {
            let b = Rect { x: r.x + label_w + k as f32 * pitch, y: top + 5.0, w: dash_w, h: 5.0 };
            let colour = if k < base_lit.min(mod_lit) {
                argb(OFF_WHITE, 0.6)
            } else if mod_lit > base_lit && k < mod_lit {
                argb((0.25, 0.65, 0.25), 1.0)
            } else if mod_lit < base_lit && k < base_lit {
                argb((0.70, 0.25, 0.25), 1.0)
            } else {
                argb(BLACK, 0.35)
            };
            bo2::fill(prims, f, b, colour);
            let p = f.px(b);
            hud::outline_rect(prims, p.x, p.y, p.w, p.h, 1.0, argb(OFF_WHITE, if k < base_lit.max(mod_lit) { 0.15 } else { 0.22 }));
        }
    }
}

// ------------------------------------------------------------------------------------------------
// The option grids (`cacattachmentsmenu.lua`, `cacperksp.lua`)
// ------------------------------------------------------------------------------------------------

/// BO2's content-grid picker: the slot's title, the allocation readout, the options as cards
/// (six columns of attachments, three of anything else), and the highlighted entry's name under
/// the grid.
fn draw_option_grid(prims: &mut Vec<Prim>, l: &EditorLayout, editor: &Editor) {
    let f = &l.frame;
    let opts = editor.options();
    let (first, shown) = picker_window(editor.slot, editor.option);
    for (i, name) in opts.iter().enumerate().skip(first).take(shown) {
        // `grid_cell` places the drawn order; the window's first option is cell 0.
        let r = grid_cell(l, editor.slot, i - first);
        let on = i == editor.option;
        let chosen = editor.option_chosen(i);
        button_bg(prims, f, r);
        if on {
            focus_frame(prims, f, r);
        }
        if let Some(art) = option_icon(editor, i) {
            let icon = r.h - bo2::TEXT_EXTRA_SMALL - 12.0;
            bo2::sprite(prims, f, &art, Rect { x: r.x + (r.w - icon) * 0.5, y: r.y + 6.0, w: icon, h: icon }, argb(OFF_WHITE, 1.0), Vec::new());
        }
        bo2::text(prims, f, Family::Condensed, name, Rect { x: r.x + 3.0, y: r.bottom() - bo2::TEXT_CONDENSED - 2.0, w: r.w, h: bo2::TEXT_CONDENSED }, Align::Left, argb(if on { BOII_ORANGE } else { OFF_WHITE }, 1.0));
        if chosen {
            bo2::text(prims, f, Family::Condensed, "*", Rect { x: r.right() - 20.0, y: r.y + 2.0, w: 20.0, h: bo2::TEXT_CONDENSED }, Align::Center, argb(bo2::YELLOW_GLOW, 1.0));
        }
    }
    if opts.is_empty() {
        bo2::text(prims, f, Family::Condensed, "NOTHING TO CHOOSE", Rect { x: f.menu.x, y: GRID_TOP, w: 400.0, h: bo2::TEXT_CONDENSED }, Align::Left, argb(OFF_WHITE, 0.5));
    }
    if matches!(editor.slot, Slot::PrimaryAtts | Slot::SecondaryAtts) {
        // BO2's attachment menu carries the highlighted entry's whole panel under the grid
        // (`cacattachmentsmenu.lua:238-536`, `d-attach/002`).
        draw_attachment_panel(prims, l, editor);
    } else if let Some(name) = opts.get(editor.option) {
        // The other content pickers carry only the highlighted entry's name under the grid
        // (`e-perks/001`, `f-wildcards/001`).
        let cols = grid_columns(editor.slot);
        let h = if cols == 6 { ATTACH_GRID_H } else { 96.0 + bo2::TEXT_EXTRA_SMALL + 10.0 };
        let rows = opts.len().div_ceil(cols);
        let y = GRID_TOP + rows as f32 * (h + GRID_ITEM_GAP) + 10.0;
        let r = f.in_menu(crate::lui::Place::new((true, false, 0.0, bo2::MENU_W * 0.5), (true, false, y, y + bo2::TEXT_CONDENSED)));
        bo2::text(prims, f, Family::Condensed, name, r, Align::Left, argb(OFF_WHITE, 1.0));
    }
}

/// The highlighted attachment's definition, from the current weapon's allowed list.
fn highlighted_attachment(editor: &Editor) -> Option<&'static crate::attachments::AttachmentDef> {
    let (base_id, _) = match editor.slot {
        Slot::PrimaryAtts => (editor.draft.primary.as_str(), &editor.draft.primary_atts),
        _ => (editor.draft.secondary.as_str(), &editor.draft.secondary_atts),
    };
    let short = editor.option_attachment(editor.option)?;
    let base = weapons::weapon(base_id);
    let asset_id = base.allowed.iter().find(|(s, _)| s == short).map(|(_, a)| a.as_str())?;
    weapons::catalog().attachment(asset_id)
}

/// The attachment picker's lower half: the highlighted attachment's Big title, its description
/// line, the four stat bars with the attachment applied, the weapon image, and the `Equipped`
/// slot-list box (`cacattachmentsmenu.lua:268-536`; `d-attach/002`). The title and description
/// come from the attachment table's localized keys (`mp/attachmenttable.csv`), and the bars use
/// BO2's attribute-table deltas (`mp/attributestable.csv`).
fn draw_attachment_panel(prims: &mut Vec<Prim>, l: &EditorLayout, editor: &Editor) {
    let f = &l.frame;
    let opts = editor.options();
    let Some(name) = opts.get(editor.option) else { return };
    let att = highlighted_attachment(editor);
    let title = att.map(|a| a.full_name.as_str()).unwrap_or(name);
    let title_rect = attachment_title(l);
    bo2::text(prims, f, Family::Big, title, title_rect, Align::Left, argb(OFF_WHITE, 1.0));

    if let Some(a) = att {
        if !a.description.is_empty() {
            let desc = attachment_description(l);
            bo2::text(prims, f, Family::Default, &a.description, desc, Align::Left, argb(OFF_WHITE, 0.4));
        }
    }

    let (base, _modified) = attachment_weapon(editor);
    let stats = weapon_stats(base);
    let modified_stats = att.map(|a| {
        let mut m = stats;
        m.damage = (m.damage + a.attribute_deltas[0] / 100.0).clamp(0.0, 1.0);
        m.range = (m.range + a.attribute_deltas[1] / 100.0).clamp(0.0, 1.0);
        m.accuracy = (m.accuracy + a.attribute_deltas[2] / 100.0).clamp(0.0, 1.0);
        m.fire_rate = (m.fire_rate + a.attribute_deltas[4] / 100.0).clamp(0.0, 1.0);
        m
    });
    stat_bars_delta(prims, f, attachment_stats(l), &stats, modified_stats.as_ref(), 70.0, bo2::TEXT_EXTRA_SMALL);

    let pic = attachment_weapon_image(l);
    bo2::sprite(prims, f, &crate::names::cac_weapon_image(base_id(&base.id)), pic, argb(OFF_WHITE, 1.0), Vec::new());

    // The `Equipped` slot list: the weapon's attachment dips, right of the stats
    // (`cacattachmentsmenu.lua:452-536`).
    let dips = match editor.slot {
        Slot::PrimaryAtts => editor.draft.primary_att_max(),
        Slot::SecondaryAtts => editor.draft.secondary_att_max(),
        _ => 0,
    };
    let box_ = attachment_slot_box(l, dips);
    let bp = f.px(box_);
    hud::outline_rect(prims, bp.x, bp.y, bp.w, bp.h, 1.0, argb(OFF_WHITE, 0.20));
    bo2::text(prims, f, Family::Default, "Equipped", attachment_slot_title(l, dips), Align::Left, argb(OFF_WHITE, 0.5));
    // The slot list's dips are notches along the box's bottom edge, not full cells
    // (`slotlist.lua`'s `BorderDip`; `d-attach/002`).
    for i in 0..dips {
        let notch = Rect { x: box_.x + 10.0 + i as f32 * 84.0, y: box_.bottom() - 10.0, w: 64.0, h: 10.0 };
        let p = f.px(notch);
        prims.push(Prim::Rect { x: p.x, y: p.y, w: p.w, h: p.h, argb: argb(bo2::BLACK, 0.25) });
        hud::outline_rect(prims, p.x, p.y, p.w, p.h, 1.0, argb(OFF_WHITE, 0.18));
    }
}

/// The weapon as the attachment panel shows it: the draft's gun without and with the highlighted
/// attachment applied (`ContentButtonHighlighted` calls `updateWeaponAttributes(weapon, content)`).
fn attachment_weapon(editor: &Editor) -> (&'static crate::weapons::WeaponDef, Option<&'static crate::weapons::WeaponDef>) {
    let (base_id, current) = match editor.slot {
        Slot::PrimaryAtts => (editor.draft.primary.as_str(), &editor.draft.primary_atts),
        _ => (editor.draft.secondary.as_str(), &editor.draft.secondary_atts),
    };
    let base = weapons::weapon(base_id);
    let Some(short) = editor.option_attachment(editor.option) else { return (base, None) };
    let mut shorts: Vec<&str> = current.iter().map(String::as_str).collect();
    if !shorts.contains(&short) {
        shorts.push(short);
    }
    let modified = weapons::weapon(&crate::attachments::variant_id(base_id, &shorts));
    (base, Some(modified))
}

/// The icon of an option row in the editor's list.
pub(super) fn option_icon(editor: &Editor, i: usize) -> Option<String> {
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
