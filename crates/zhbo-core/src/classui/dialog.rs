//! The shell's set and class option dialogs: their entries, input fold and drawing.

use super::*;

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
pub(super) fn dialog_box(w: f32, h: f32, s: f32, items: usize) -> (f32, f32, f32, f32) {
    let bw = 460.0 * s;
    let bh = (120.0 + items as f32 * 30.0) * s;
    ((w - bw) * 0.5, (h - bh) * 0.5, bw, bh)
}

/// The entry row under a screen point, if any.
pub(super) fn dialog_item_at(d: &Dialog, w: f32, h: f32, s: f32, x: f32, y: f32) -> Option<usize> {
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
