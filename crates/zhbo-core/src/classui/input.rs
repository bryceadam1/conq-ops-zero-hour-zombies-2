//! The shared input folds both hosts run: pointer and key edges, the pick screen's and the editor's frame.

use super::*;

/// The pick screen's keys 1-9,0, in list order: key 1 is the first row, key 0 the tenth. The keys
/// exist so a player can pick without the mouse; BO2 shows the same numerals on the rows.
pub(super) const PICK_KEYS: [Key; 10] = [
    Key::Key1, Key::Key2, Key::Key3, Key::Key4, Key::Key5, Key::Key6, Key::Key7, Key::Key8, Key::Key9, Key::Key0,
];

/// A cursor move smaller than this does not count as a hover (pixel jitter would fight the keys).
pub(super) const MOUSE_MOVED: f32 = 0.5;

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
/// lobby's list (the set's five slots, the footer actions) or the in-match pick-only list (the
/// slots, then the default classes). Pure
/// apart from reading the two registries (the list is rebuilt on demand, like the draw).
pub fn pick_frame(inp: &MenuPointer, prev: &MenuPrev, highlight: usize, manage: bool) -> PickFrame {
    let rows = pick_rows(manage);
    let mut out = PickFrame::default();
    if rows.is_empty() {
        return out;
    }
    let mut hl = highlight.min(rows.len() - 1);
    let layout = pick_layout(inp.screen_w, inp.screen_h, manage);
    let hover = pick_row_at(&layout, inp.mouse_x, inp.mouse_y, &rows);
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
            out.footer = pick_footer_at(&layout, inp.mouse_x, inp.mouse_y);
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
    let layout = editor_layout(inp.screen_w, inp.screen_h);
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
        if let Some((slot, index)) = editor_remove_at(&layout, &cells, editor, inp.mouse_x, inp.mouse_y).filter(|_| clicked) {
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
    let slot = editor_cell_at(&layout, &cells, inp.mouse_x, inp.mouse_y)
        .and_then(|i| cells.get(i))
        .and_then(|c| slots.iter().position(|s| *s == c.slot));
    let (start, _) = picker_window(editor.slot, editor.option);
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
pub(super) fn drawn_option_count(editor: &Editor, cats: &[Category]) -> usize {
    if cats.is_empty() {
        editor.options().len()
    } else {
        cats[category_of(cats, editor.option)].options.len()
    }
}

// ------------------------------------------------------------------------------------------------
// The shell's dialogs (BO2's `S`/`O` menus)
// ------------------------------------------------------------------------------------------------
