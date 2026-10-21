//! BO2's choose-class screens, laid out from the Lua in LUI units.
//!
//! * The lobby's `CACChooseClass` (`patch_mp/ui_mp/t6/menus/cacchooseclass.lua`): the CHOOSE
//!   CLASS title, the class-set header (`‹ CLASS SET n ›` over the set pips, 218 x 35 at the
//!   title's bottom + 10), the five custom slots of the active set as `CoD9Button`s (30 tall,
//!   `ButtonList.ButtonSpacing` 2 apart, Condensed, `BOIIOrange` when focused), the hint under them,
//!   and the highlighted class's `ClassPreview` from `width / 3 - 50` to the menu's right edge.
//! * The match's `changeclass` (`hud/changeclass.lua`): the same title, a 250-wide list at the
//!   title's bottom holding the set's name, a half-button spacer, the five slots, another spacer
//!   and the default classes, and a `Menu.Width - ButtonList.DefaultWidth * 1.5` wide preview on the
//!   right.

use super::bo2::{self, argb, Frame, BOII_ORANGE, OFF_WHITE};
use super::preview;
use super::*;
use crate::lui::font::{Align, Family};
use crate::lui::{Place, Rect};

/// `CoD.CoD9Button.Height` and `CoD.ButtonList.ButtonSpacing`.
pub const ROW_H: f32 = 30.0;
pub const ROW_SPACING: f32 = 2.0;
/// `CoD.ButtonList.DefaultWidth`.
pub const LIST_DEFAULT_W: f32 = 350.0;
/// The class-set header's height (`cacchooseclass.lua`: `L6_2 = 35`).
pub const SET_HEADER_H: f32 = 35.0;
/// `changeclass.lua`'s list width.
pub const CHANGE_CLASS_LIST_W: f32 = 250.0;
/// BO2's class-set hint (`MENU_CLASS_SET_HINT`), under the lobby's class list.
pub const SET_HINT: &str = "You will load into the game with this class set.";

/// The pick screen's geometry, in root units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickLayout {
    pub frame: Frame,
    /// The lobby's screen (class sets, options); false for the match's change-class menu.
    pub manage: bool,
    /// The class-set header (lobby only).
    pub header: Option<Rect>,
    /// The button list: its left, top and width (it runs to the menu's bottom).
    pub list: Rect,
    /// The class preview.
    pub preview: Rect,
}

pub fn pick_layout(w: f32, h: f32, manage: bool) -> PickLayout {
    let frame = Frame::new(w, h);
    let title = bo2::TEXT_BIG;
    if manage {
        // `classPreviewTop = TitleHeight + 10`, `buttonListWidth = width / 3 - 50`; the header and
        // the list are 20 narrower, and the list starts under the header plus 10.
        let top = title + 10.0;
        let preview_left = MENU_W_THIRD - 50.0;
        let list_w = preview_left - 20.0;
        let header = frame.in_menu(Place::new((true, false, 0.0, list_w), (true, false, top, top + SET_HEADER_H)));
        let list = frame.in_menu(Place::new((true, false, 0.0, list_w), (true, true, top + SET_HEADER_H + 10.0, 0.0)));
        let preview = frame.in_menu(Place::new((true, true, preview_left, 0.0), (true, true, top, 0.0)));
        PickLayout { frame, manage, header: Some(header), list, preview }
    } else {
        let list = frame.in_menu(Place::new((true, false, 0.0, CHANGE_CLASS_LIST_W), (true, true, title, 0.0)));
        let preview_w = bo2::MENU_W - LIST_DEFAULT_W * 1.5;
        let preview = frame.in_menu(Place::new((false, true, -preview_w, 0.0), (true, true, title, 0.0)));
        PickLayout { frame, manage, header: None, list, preview }
    }
}

const MENU_W_THIRD: f32 = bo2::MENU_W / 3.0;

/// One line of the button list: a class button, or the match list's set name and spacers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ListItem {
    /// The match list's set name (Condensed text, not a button).
    SetName,
    /// `ButtonList:addSpacer(CoD9Button.Height / 2)`.
    Spacer,
    Row(usize),
}

/// The list's lines in order: the lobby's are its rows; the match's put the set's name and a
/// spacer before the custom slots and a spacer between them and the default classes.
pub fn list_items(rows: &[PickRow], manage: bool) -> Vec<ListItem> {
    if manage {
        return (0..rows.len()).map(ListItem::Row).collect();
    }
    let mut out = vec![ListItem::SetName, ListItem::Spacer];
    let mut customs_done = false;
    for (i, r) in rows.iter().enumerate() {
        if matches!(r, PickRow::Preset(_)) && !customs_done {
            out.push(ListItem::Spacer);
            customs_done = true;
        }
        out.push(ListItem::Row(i));
    }
    out
}

fn item_h(item: ListItem) -> f32 {
    match item {
        ListItem::SetName => bo2::TEXT_CONDENSED,
        ListItem::Spacer => ROW_H / 2.0,
        ListItem::Row(_) => ROW_H,
    }
}

/// Each row's button box (root units), indexed like `rows`.
pub fn row_rects(l: &PickLayout, rows: &[PickRow]) -> Vec<Rect> {
    let mut out = vec![Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }; rows.len()];
    let mut y = l.list.y;
    for item in list_items(rows, l.manage) {
        if let ListItem::Row(i) = item {
            out[i] = Rect { x: l.list.x, y, w: l.list.w, h: ROW_H };
        }
        y += item_h(item) + ROW_SPACING;
    }
    out
}

/// The list's bottom (root units): where the hint goes.
fn list_bottom(l: &PickLayout, rows: &[PickRow]) -> f32 {
    l.list.y + list_items(rows, l.manage).into_iter().map(|i| item_h(i) + ROW_SPACING).sum::<f32>()
}

/// The row under a screen point, if any.
pub fn pick_row_at(l: &PickLayout, x: f32, y: f32, rows: &[PickRow]) -> Option<usize> {
    row_rects(l, rows).iter().position(|r| bo2::hit(&l.frame, *r, x, y))
}

/// The footer: `ESC Back` at the left; in the lobby `O Class Options` rightmost with
/// `S Class Set Options` beside it (`cacchooseclass.lua` adds them in that order).
pub fn pick_footer_actions(l: &PickLayout) -> Vec<(PickFooterAction, Rect)> {
    let left = [PickFooterAction::Back];
    let right: &[PickFooterAction] = if l.manage { &[PickFooterAction::ClassOptions, PickFooterAction::SetOptions] } else { &[] };
    let as_pairs = |a: &[PickFooterAction]| a.iter().map(|x| (x.key(), x.label())).collect::<Vec<_>>();
    let boxes = bo2::prompt_boxes(&l.frame, &as_pairs(&left), &as_pairs(right));
    left.iter().chain(right.iter()).copied().zip(boxes.into_iter().map(|b| b.rect)).collect()
}

/// The footer action under a screen point, if any.
pub fn pick_footer_at(l: &PickLayout, x: f32, y: f32) -> Option<PickFooterAction> {
    pick_footer_actions(l).into_iter().find(|(_, r)| bo2::hit(&l.frame, *r, x, y)).map(|(a, _)| a)
}

/// What a row shows: its name and the kit the preview draws (`None` for an empty slot).
pub(super) fn row_class(row: &PickRow, presets: &[classes::ClassDef], customs: &[crate::classes::CustomClass]) -> (String, Option<Draft>) {
    match *row {
        PickRow::Preset(j) => {
            let stored = crate::classes::CustomClass::from_spec(&presets[j].spec);
            (presets[j].name.clone(), Some(Draft::from_stored(&stored)))
        }
        PickRow::Custom(j) => (customs[j].name.clone(), Some(Draft::from_stored(&customs[j]))),
        PickRow::Empty(slot) => (empty_slot_name(slot), None),
    }
}

/// BO2's name for an untouched custom slot (`Custom 1`..`Custom 5`, `a-chooseclass/002`).
pub fn empty_slot_name(slot: usize) -> String {
    format!("Custom {}", slot + 1)
}

/// `SetActiveClassSet`'s slide/fade (`cacchooseclass.lua`: `beginAnimation("onscreen", 250, …)` on
/// the class list, `fade_in` 250 on the preview).
pub const SET_SWITCH_MS: f32 = 250.0;
/// How far left of its resting place the list starts: `-320 - buttonListWidth - 20` in the Lua,
/// which is the list's own width plus 20 (`setLeftRight` aims the right edge at `-320`).
const SET_SLIDE_INSET: f32 = 20.0;

/// BO2's choose-class screen for a host (see the module doc): the host's highlighted row, its UI
/// clock (the focused button's pulse) and, while `SetActiveClassSet` runs, the switch's age.
pub fn draw_pick(prims: &mut Vec<Prim>, w: f32, h: f32, view: &ScreenView<'_>) {
    let ScreenView { highlight: highlighted, deny, manage, now, .. } = *view;
    let settle = view.set_change.map_or(1.0, |since| (since * 1000.0 / SET_SWITCH_MS).clamp(0.0, 1.0));
    let l = pick_layout(w, h, manage);
    let f = &l.frame;
    bo2::backdrop(prims, f, w, h);
    bo2::title(prims, f, "CHOOSE CLASS");

    let presets = classes::get();
    let prof = profile::get();
    let customs: &[crate::classes::CustomClass] = prof.customs();
    let rows = pick_rows_for(&presets, customs.len(), manage);
    let set = prof.active_set();
    let set_name = prof.set_name(set);
    if let Some(header) = l.header {
        set_header(prims, f, header, &set_name, set, profile::MAX_SETS);
    }
    let active = highlighted.min(rows.len().saturating_sub(1));
    let rects = row_rects(&l, &rows);
    // The set switch: the list slides in from the left with an ease-out (`onscreen`) and the
    // preview fades in with an ease-in (`fade_in`), both over 250 ms. Outside a switch this is the
    // static layout exactly.
    let settle = settle.clamp(0.0, 1.0);
    let switching = settle < 1.0;
    let slide = if switching { (l.list.w + SET_SLIDE_INSET) * (1.0 - crate::lui::anim::ease(settle, false, true)) } else { 0.0 };
    if switching {
        // The list is only inside the menu while it slides: clip it so it cannot paint over the
        // backing frame's edge (the menu is the element that animates in BO2).
        let m = f.px(f.menu);
        prims.push(Prim::ClipBegin { x: m.x, y: m.y, w: m.w, h: m.h });
    }
    let mut y = l.list.y;
    for item in list_items(&rows, manage) {
        match item {
            ListItem::SetName => {
                let r = Rect { x: l.list.x - slide, y, w: l.list.w, h: bo2::TEXT_CONDENSED };
                bo2::text(prims, f, Family::Condensed, &set_name, r, Align::Left, argb(OFF_WHITE, 1.0));
            }
            ListItem::Spacer => {}
            ListItem::Row(i) => {
                let (name, _) = row_class(&rows[i], &presets, customs);
                let cell = rects[i];
                class_button(prims, f, Rect { x: cell.x - slide, ..cell }, &name, i == active, now);
            }
        }
        y += item_h(item) + ROW_SPACING;
    }
    if manage {
        let top = list_bottom(&l, &rows);
        bo2::hint(prims, f, Rect { x: l.list.x - slide, y: top, w: l.list.w, h: bo2::TEXT_DEFAULT }, SET_HINT);
    }
    if switching {
        prims.push(Prim::ClipEnd);
    }
    if let Some(row) = rows.get(active) {
        let (_, draft) = row_class(row, &presets, customs);
        if switching {
            // The preview draws into its own list so the fade is one alpha multiplier over the
            // whole block, as LUI's `alphaMultiplier` on the element is.
            let mut group = Vec::new();
            preview::draw(&mut group, f, l.preview, draft.as_ref());
            bo2::fade(&mut group, crate::lui::anim::ease(settle, true, false));
            prims.extend(group);
        } else {
            preview::draw(prims, f, l.preview, draft.as_ref());
        }
    }
    let footer: Vec<_> = pick_footer_actions(&l)
        .into_iter()
        .map(|(a, rect)| bo2::PromptBox { key: a.key().to_string(), label: a.label().to_string(), rect })
        .collect();
    bo2::draw_prompts(prims, f, &footer);
    deny_flash(prims, w, h, f.s(), deny);
}

/// A `CoD9Button` label: Condensed, left-aligned; off-white, or `BOIIOrange` pulsing between alpha
/// 1 and 0.5 every `PulseDuration` (650 ms, eased out going up and in going down) while focused
/// (`cod9button.lua`'s `button_over`, `pulse_high`/`pulse_low`).
pub(super) fn class_button(prims: &mut Vec<Prim>, f: &Frame, r: Rect, label: &str, focused: bool, now: f32) {
    let colour = if focused { argb(BOII_ORANGE, pulse_alpha(now)) } else { argb(OFF_WHITE, 1.0) };
    bo2::text(prims, f, Family::Condensed, label, r, Align::Left, colour);
}

/// `CoD9Button.PulseDuration`.
pub const PULSE_MS: f32 = 650.0;

/// The focused button's alpha at `now`: down to 0.5 with `easeIn`, back to 1 with `easeOut`.
pub fn pulse_alpha(now: f32) -> f32 {
    let period = 2.0 * PULSE_MS / 1000.0;
    let t = (now.max(0.0) % period) / (PULSE_MS / 1000.0);
    if t < 1.0 {
        1.0 - 0.5 * crate::lui::anim::ease(t, true, false)
    } else {
        0.5 + 0.5 * crate::lui::anim::ease(t - 1.0, false, true)
    }
}

/// The class-set header (`cacchooseclass.lua`): the set's name centred in ExtraSmall at its top,
/// the shoulder prompts' arrows at either end (`ButtonPrompt.Height` square, centred on the text
/// line), and one 10-unit pip per set along its bottom, centred: the active set's
/// `menu_vis_carousel_pip_fill`, the others `menu_vis_carousel_pip` (`UpdateClassSetHeader`).
fn set_header(prims: &mut Vec<Prim>, f: &Frame, r: Rect, name: &str, set: usize, sets: usize) {
    let line = bo2::TEXT_EXTRA_SMALL;
    bo2::text(prims, f, Family::ExtraSmall, name, Rect { x: r.x, y: r.y, w: r.w, h: line }, Align::Center, argb(OFF_WHITE, 1.0));
    let a = bo2::PROMPT_H;
    let arrow_y = r.y + line / 2.0 - a / 2.0;
    arrow(prims, f, Rect { x: r.x, y: arrow_y, w: a, h: a }, -1);
    arrow(prims, f, Rect { x: r.right() - a, y: arrow_y, w: a, h: a }, 1);
    let pip = 10.0;
    let left = r.x + r.w / 2.0 - pip * sets as f32 / 2.0;
    for i in 0..sets {
        let rect = Rect { x: left + pip * i as f32, y: r.bottom() - pip, w: pip, h: pip };
        let material = if i == set { "menu_vis_carousel_pip_fill" } else { "menu_vis_carousel_pip" };
        let p = f.px(rect);
        let fallback = if i == set {
            vec![Prim::Rect { x: p.x + p.w * 0.25, y: p.y + p.h * 0.25, w: p.w * 0.5, h: p.h * 0.5, argb: argb(BOII_ORANGE, 1.0) }]
        } else {
            vec![Prim::Rect { x: p.x + p.w * 0.4, y: p.y + p.h * 0.4, w: p.w * 0.2, h: p.h * 0.2, argb: argb(OFF_WHITE, 0.6) }]
        };
        bo2::image(prims, f, material, rect, argb(OFF_WHITE, 1.0), fallback);
    }
}

/// The shoulder prompts' PC glyph (`^BBUTTON_CYCLE_LEFT^`/`RIGHT^`): a solid white triangle.
fn arrow(prims: &mut Vec<Prim>, f: &Frame, r: Rect, dir: i32) {
    let p = f.px(r);
    let (cy, half) = (p.y + p.h * 0.5, p.h * 0.32);
    let w = p.w * 0.45;
    let x0 = p.x + (p.w - w) * 0.5;
    let rows = (half * 2.0).max(1.0) as i32;
    for i in 0..rows {
        let t = i as f32 / rows as f32;
        let span = w * (1.0 - (t * 2.0 - 1.0).abs());
        let x = if dir < 0 { x0 + w - span } else { x0 };
        prims.push(Prim::Rect { x, y: cy - half + i as f32, w: span, h: 1.0, argb: argb(OFF_WHITE, 1.0) });
    }
}

const _: () = assert!(profile::MAX_CLASSES == 5, "BO2's sets hold five custom slots");
