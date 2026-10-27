//! The shell's set and class option dialogs: their entries, input fold and drawing.

use super::bo2::{self, argb, BOII_ORANGE, OFF_WHITE};
use super::*;
use crate::lui::font::{Align, Family};

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
pub(super) fn typed_char(k: Key) -> Option<char> {
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
    // The entries are a `CoD.ButtonList` (`cacclasssetoptions.lua:121`, `cacclassoptions.lua:157`):
    // a wrapping vertical list that Up/Down and the wheel walk, and whose buttons take focus under
    // a moving cursor (`luielement.lua:316-348`).
    let n = d.items.len();
    if n > 0 {
        let step = LuiNav::read(prev, now).vertical();
        d.highlight = (d.highlight as i32 + step).rem_euclid(n as i32) as usize;
        if prev.moved(now) {
            let s = now.scale();
            if let Some(i) = dialog_item_at(d, now.screen_w, now.screen_h, s, now.mouse_x, now.mouse_y) {
                d.highlight = i;
            }
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

/// BO2's popup body: `CoD.Menu.SmallPopupWidth` x `SmallPopupHeight` (448 x 256,
/// `codmenu.lua:35-36`), centred; the class dialog asks for 15 more (`cacclassoptions.lua:15-18`).
pub(super) const DIALOG_W: f32 = 448.0;
pub(super) const DIALOG_H: f32 = 256.0;
pub(super) const DIALOG_CLASS_H: f32 = DIALOG_H + 15.0;
/// `CoD.textSize.Big` / `Default` / `Condensed` and `CoD9Button.Height` (`codbase.lua:344-348`,
/// `cod9button.lua:6`).
const DIALOG_TITLE_H: f32 = 48.0;
const DIALOG_SUBTITLE_H: f32 = 25.0;
const DIALOG_ITEM_H: f32 = 30.0;
/// The button list starts under the header and subtitle plus 20 (`cacclasssetoptions.lua:120-134`,
/// `cacclassoptions.lua:120-134`).
const DIALOG_LIST_TOP: f32 = DIALOG_TITLE_H + DIALOG_SUBTITLE_H + 20.0;

/// The dialog's box on screen (BO2's centred panel).
pub(super) fn dialog_box(w: f32, h: f32, s: f32, kind: DialogKind) -> (f32, f32, f32, f32) {
    let bw = DIALOG_W * s;
    let bh = match kind {
        DialogKind::SetOptions => DIALOG_H,
        DialogKind::ClassOptions => DIALOG_CLASS_H,
    } * s;
    ((w - bw) * 0.5, (h - bh) * 0.5, bw, bh)
}

/// The entry row under a screen point, if any.
pub(super) fn dialog_item_at(d: &Dialog, w: f32, h: f32, s: f32, x: f32, y: f32) -> Option<usize> {
    let (bx, by, bw, _) = dialog_box(w, h, s, d.kind);
    if x < bx || x > bx + bw {
        return None;
    }
    let top = by + (DIALOG_LIST_TOP + if d.naming { DIALOG_SUBTITLE_H } else { 0.0 }) * s;
    let row = DIALOG_ITEM_H * s;
    if y < top {
        return None;
    }
    let i = ((y - top) / row) as usize;
    (i < d.items.len()).then_some(i)
}

// ------------------------------------------------------------------------------------------------
// Shared chrome
// ------------------------------------------------------------------------------------------------


/// Draw one of the shell's dialogs over the screen BO2 way (`a-chooseclass/021`): the dimmed screen,
/// the centred panel with its title, subtitle and entries, and the `ESC Back` prompt. The `S`/`O`
/// dialogs are popups (`cacchooseclass.lua`'s `OpenClassOptions`/`OpenClassSetOptions` call
/// `openPopup`), so `open` runs `CoD.Menu.animateIn`'s 250 ms ease-out alpha fade
/// (`codmenu.lua:1985-1990`, `:2300-2326`); `None` draws the settled popup.
pub fn draw_dialog(prims: &mut Vec<Prim>, w: f32, h: f32, d: &Dialog, open: Option<f32>) {
    if settle(open) >= 1.0 {
        draw_dialog_body(prims, w, h, d);
    } else {
        let mut group = Vec::new();
        draw_dialog_body(&mut group, w, h, d);
        bo2::fade(&mut group, crate::lui::anim::ease(settle(open), false, true));
        prims.extend(group);
    }
}

/// The settled dialog prims (no open fade).
fn draw_dialog_body(prims: &mut Vec<Prim>, w: f32, h: f32, d: &Dialog) {
    let s = (h / 720.0).max(0.5);
    let f = bo2::Frame::new(w, h);
    prims.push(Prim::Rect { x: 0.0, y: 0.0, w, h, argb: 0xB400_0000 });
    let (bx, by, bw, bh) = dialog_box(w, h, s, d.kind);
    let body = crate::lui::Rect { x: bx / s, y: by / s, w: bw / s, h: bh / s };
    // `addSmallPopupBackground_Multiplayer` draws a 498-wide image for the 448-wide menu
    // (`codmenu.lua:1400-1417`): 25 units of texture bleed each side. The captured popup shows
    // ~480 visible units, so the frame sits 16 units outside the content box (`a-chooseclass/021`).
    let chrome = crate::lui::Rect { x: body.x - 16.0, y: body.y, w: body.w + 32.0, h: body.h };
    let p = f.px(chrome);
    prims.push(Prim::Rect { x: p.x, y: p.y, w: p.w, h: p.h, argb: DIALOG_FILL });
    // The popup's own frame slices (`codmenu.lua:1400-1493`): `menu_mp_popup_top` and
    // `menu_mp_popup_bottom` carry the bevelled corners and the bottom pointer, and
    // `menu_mp_popup_stretch` fills the body. The slices replace the manual outline.
    let top_h = 32.0;
    let bottom_h = 32.0;
    bo2::image(prims, &f, "menu_mp_popup_top", crate::lui::Rect { x: chrome.x, y: chrome.y, w: chrome.w, h: top_h }, argb(OFF_WHITE, 1.0), Vec::new());
    bo2::image(
        prims,
        &f,
        "menu_mp_popup_stretch",
        crate::lui::Rect { x: chrome.x, y: chrome.y + top_h, w: chrome.w, h: (chrome.h - top_h - bottom_h).max(0.0) },
        argb(OFF_WHITE, 1.0),
        Vec::new(),
    );
    bo2::image(
        prims,
        &f,
        "menu_mp_popup_bottom",
        crate::lui::Rect { x: chrome.x, y: chrome.bottom() - bottom_h, w: chrome.w, h: bottom_h + 10.0 * s },
        argb(OFF_WHITE, 1.0),
        Vec::new(),
    );

    // Header `Big` at 0,0, the set/class name in `Default` under it at alpha 0.5, then the
    // `CoD9Button` list at 93 (`cacclasssetoptions.lua:48-137`, `cacclassoptions.lua:79-137`).
    let title = crate::lui::Rect { x: body.x, y: body.y, w: body.w, h: DIALOG_TITLE_H };
    bo2::text(prims, &f, Family::Big, d.kind.title(), title, Align::Left, argb(OFF_WHITE, 1.0));
    let sub = crate::lui::Rect { x: body.x, y: body.y + DIALOG_TITLE_H, w: body.w, h: DIALOG_SUBTITLE_H };
    bo2::text(prims, &f, Family::Default, &d.subtitle, sub, Align::Left, argb(OFF_WHITE, 0.5));
    let mut top = body.y + DIALOG_LIST_TOP;
    if d.naming {
        let field = crate::lui::Rect { x: body.x, y: top, w: body.w, h: DIALOG_SUBTITLE_H };
        bo2::text(prims, &f, Family::Default, &format!("{}_", d.buffer), field, Align::Left, argb(OFF_WHITE, 1.0));
        top += DIALOG_SUBTITLE_H;
    }
    for (i, item) in d.items.iter().enumerate() {
        let row = crate::lui::Rect { x: body.x, y: top + i as f32 * DIALOG_ITEM_H, w: body.w, h: DIALOG_ITEM_H };
        let on = i == d.highlight && !d.naming;
        let colour = if on { BOII_ORANGE } else { OFF_WHITE };
        bo2::text(prims, &f, Family::Condensed, item.label(), row, Align::Left, argb(colour, 1.0));
    }
    // The popup is its own menu, so its one prompt is the normal footer (`codmenu.lua:185-226`).
    let boxes = bo2::prompt_boxes(&f, &[("ESC", "Back")], &[]);
    bo2::draw_prompts(prims, &f, &boxes);
}
