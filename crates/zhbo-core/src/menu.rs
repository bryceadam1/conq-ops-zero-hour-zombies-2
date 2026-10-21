//! The shell create-a-class host: BO2's lobby class menu, run with no match and no sim.
//!
//! The window is the engine's (a full-screen modal window from the lobby's Create a Class button;
//! host patch 0015 and `host/zhbo/ZhboMenu.cpp`); this module is the screen's state and behavior.
//! It runs the same `classui` screens the in-game `client` hosts — the pick list (presets, the
//! player's customs, CREATE A CLASS) and the allocation editor — through the shared folds
//! (`classui::pick_frame`/`editor_frame`) and draws through `classui::draw_screen`.
//!
//! The behavior is BO2's lobby create-a-class: Enter/click on a row opens the editor on that slot
//! (an existing custom, a preset as a template, or a fresh draft); the editor's Save writes back
//! into THAT slot of THAT class set, and the list's `S Class Set Options` / `O Class Options`
//! dialogs (`classui::Dialog`) rename/reset the set and rename/copy/reset/clear the class through
//! the `save_profile` one-shot. Left/Right (BO2's shoulders) walk the ten class sets, making the
//! shown one the active set a match will load (`cacchooseclass.lua`'s `SetActiveClassSet`), and
//! every edit lands in the set being browsed. There is no *pick* in the shell — no match is
//! running — and Escape/Back closes the screen (Escape backs out of a dialog or the editor first).
//! Everything is local UI state: no determinism or lockstep requirements (docs/CONTRACT.md's menu
//! ABI).

use crate::classmenu::{Draft, Editor, Outcome};
use crate::classui::cues;
use crate::classui::{self, Dialog, DialogItem, DialogOutcome, MenuPointer, MenuPrev, PickFooterAction, PickRow};
use crate::input::{Key, KeySet};

/// The menu frame's input (ABI: `ZhboMenuIn`). The bridge fills it from the shell window's draw
/// callback: keys from the platform poll, the pointer and buttons from the engine's cursor, the
/// wheel from the window's own GWM_WHEEL messages, the screen from the display.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MenuIn {
    /// Platform-neutral key-down set (see [`crate::input::KeySet`]).
    pub keys: KeySet,
    /// Absolute mouse cursor position (screen pixels).
    pub mouse_x: f32,
    pub mouse_y: f32,
    /// The mouse buttons' held state (see [`crate::input::mouse_buttons`]).
    pub mouse_buttons: u32,
    /// Wheel notches since last frame.
    pub wheel: f32,
    pub screen_w: f32,
    pub screen_h: f32,
}

/// The menu frame's output (ABI: `ZhboMenuOut`). The prims and the sound cues go through the
/// stored `ClientApi` callbacks inside the frame, so the only outputs are the close request and
/// the profile one-shot (mirroring `client::ClientOut`'s `save_profile`).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuOut {
    /// One-shot: the screen asked to close (Escape/Back on the pick list); the bridge pops the
    /// shell window.
    pub closed: i32,
    /// One-shot: write the profile file now. `1` = the editor saved a class (BO2's Save), `2` =
    /// the list deleted or copied a slot (see `classui::SAVE_PROFILE_*`).
    pub save_profile: i32,
    /// The profile file's full text, UTF-8 packed four bytes per word (see `profile::pack_text`).
    pub save_profile_payload: [u32; 1024],
    /// How many `save_profile_payload` words are valid.
    pub save_profile_payload_len: i32,
}

// Manual Default: `[u32; 1024]` has none (arrays above 32 don't), and the rest are plain.
impl Default for MenuOut {
    fn default() -> MenuOut {
        MenuOut { closed: 0, save_profile: 0, save_profile_payload: [0; 1024], save_profile_payload_len: 0 }
    }
}

/// Fill the profile-write one-shot with the profile's new text (mirror of the client's
/// `write_profile`, against the menu ABI's payload). The bridge writes the file only for this; a
/// file too large for the ABI payload is left unwritten rather than truncated.
fn write_profile(out: &mut MenuOut, reason: i32, text: &str) {
    let words = crate::profile::pack_text(text);
    if words.len() > out.save_profile_payload.len() {
        return;
    }
    let n = words.len();
    out.save_profile_payload[..n].copy_from_slice(&words);
    out.save_profile_payload_len = n as i32;
    out.save_profile = reason;
}

/// The shell screen's state. `Some` in the core while the window is up (`zhbo_menu_open` /
/// `zhbo_menu_close`); everything in it is client-local UI state.
pub struct MenuState {
    /// The open editor, or `None` while the pick list is up.
    editor: Option<Editor>,
    /// The class set the open editor belongs to, and the slot it was opened on (`None` for
    /// CREATE A CLASS: the class goes into the set's first free slot).
    editor_set: usize,
    editor_slot: Option<usize>,
    /// The open `S`/`O` dialog, its target class slot (the class dialog's), and the set it acts on.
    dialog: Option<Dialog>,
    dialog_slot: Option<usize>,
    dialog_set: usize,
    /// The pick list's highlighted row.
    highlight: usize,
    /// Seconds left of the deny rim (a refused choice), decayed per frame.
    deny: f32,
    /// The previous frame's keys/pointer, for edge and hover detection (like the client's own).
    prev: MenuPrev,
    /// BO2 UI cues queued by the frames (drained by the FFI layer into the host's sound callback).
    pub cues: Vec<&'static str>,
    /// The screen asked to close (Escape/Back on the pick list).
    closed: bool,
    /// The screen's UI clock in seconds (60 Hz steps, like the deny rim): LUI pulses and fades.
    clock: f32,
    /// The clock reading of the last class-set change, while `SetActiveClassSet`'s 250 ms
    /// slide/fade runs (`cacchooseclass.lua`); `None` once it has landed.
    set_change: Option<f32>,
}

impl Default for MenuState {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuState {
    pub fn new() -> MenuState {
        // BO2's open sting plays once per open, like the in-game pick screen's entrance.
        MenuState {
            editor: None,
            editor_set: 0,
            editor_slot: None,
            dialog: None,
            dialog_slot: None,
            dialog_set: 0,
            highlight: 0,
            deny: 0.0,
            prev: MenuPrev::default(),
            cues: vec![cues::ENTER_CAC],
            closed: false,
            clock: 0.0,
            set_change: None,
        }
    }

    /// One frame of the screen: fold the input, then report the outputs. The prims and cues are
    /// rendered/drained by the FFI layer (`zhbo_menu_frame`).
    pub fn frame(&mut self, inp: &MenuIn) -> MenuOut {
        let mut out = MenuOut::default();
        let pointer = MenuPointer {
            keys: inp.keys,
            mouse_x: inp.mouse_x,
            mouse_y: inp.mouse_y,
            mouse_buttons: inp.mouse_buttons,
            wheel: inp.wheel,
            screen_w: inp.screen_w,
            screen_h: inp.screen_h,
        };
        // The menu ABI carries no dt (the shell window draws every rendered frame); the rim decays
        // a 60 Hz step per frame.
        self.deny = (self.deny - 1.0 / 60.0).max(0.0);
        self.clock += 1.0 / 60.0;
        // A set switch's slide/fade is over once its duration has run.
        if self.set_change.is_some_and(|start| self.clock - start >= classui::SET_SWITCH_MS / 1000.0) {
            self.set_change = None;
        }
        if self.dialog.is_some() {
            self.dialog_frame(&pointer, &mut out);
        } else if let Some(editor) = self.editor.take() {
            self.editor_frame(editor, &pointer, &mut out);
        } else {
            self.pick_frame(&pointer, &mut out);
        }
        self.prev.update(&pointer);
        out.closed = self.closed as i32;
        out
    }

    /// What the screen draws (the same `ScreenView` the in-game Client produces, plus the shell's
    /// editor and management affordances).
    pub fn view(&self) -> classui::ScreenView<'_> {
        let set_change = self.set_change.map(|start| (self.clock - start).max(0.0));
        classui::ScreenView { highlight: self.highlight, editor: self.editor.as_ref(), deny: self.deny, manage: true, now: self.clock, set_change }
    }

    /// The open `S`/`O` dialog, for the host to draw over the screen (`classui::draw_dialog`).
    pub fn dialog(&self) -> Option<&Dialog> {
        self.dialog.as_ref()
    }

    /// Play BO2's deny cue and light the screen's rim (an over-budget choice, a full profile).
    fn deny(&mut self) {
        self.cues.push(cues::DENY);
        self.deny = classui::DENY_FLASH;
    }

    /// Open the editor on `draft`, saving back into the set's `slot` (`None`: a new class).
    fn open_editor(&mut self, draft: Draft, slot: Option<usize>) {
        self.editor = Some(Editor::new(draft));
        self.editor_set = crate::profile::active_set();
        self.editor_slot = slot;
        // BO2's ChooseClass handler: the lobby plays cac_main_edit_loadout as the editor opens.
        self.cues.push(cues::EDIT_LOADOUT);
        // The opening key's edge must not reach the editor on the same frame.
        self.prev.buttons = 0;
    }

    /// Open BO2's `S`/`O` dialog (`None` slot: the class-set dialog).
    fn open_dialog(&mut self, dialog: Dialog, slot: Option<usize>) {
        self.dialog = Some(dialog);
        self.dialog_slot = slot;
        self.dialog_set = crate::profile::active_set();
        // popup.lua's OpenPopup: a dialog opens with the loadout-editor select cue.
        self.cues.push(cues::DIALOG_OPEN);
        self.prev.buttons = 0;
    }

    /// The `S`/`O` dialog's frame: Enter confirms an entry (or commits the typed name), Escape backs
    /// out to the list, and each entry's edit persists through the profile one-shot.
    fn dialog_frame(&mut self, pointer: &MenuPointer, out: &mut MenuOut) {
        let Some(mut dialog) = self.dialog.take() else { return };
        let was_naming = dialog.naming;
        let outcome = classui::dialog_step(&mut dialog, &self.prev, pointer);
        match outcome {
            DialogOutcome::Continue => {
                self.dialog = Some(dialog);
            }
            DialogOutcome::Back => {
                self.cues.push(cues::BACKOUT);
            }
            DialogOutcome::Confirm(item) => {
                if item == DialogItem::Rename && !was_naming {
                    // The entry opens the text field on the current name (BO2's rename popup).
                    let seed = self.dialog_seed();
                    dialog.begin_rename(&seed);
                    self.cues.push(cues::GRID_EQUIP);
                    self.dialog = Some(dialog);
                    return;
                }
                let name = dialog.buffer.trim().to_string();
                self.apply_dialog(item, &name, out);
            }
        }
    }

    /// The name a rename starts from: the set's, or the highlighted class's.
    fn dialog_seed(&self) -> String {
        match self.dialog.as_ref().map(|d| d.kind) {
            Some(classui::DialogKind::ClassOptions) => {
                let slot = self.dialog_slot.unwrap_or(0);
                crate::profile::get().customs_in(self.dialog_set).and_then(|c| c.get(slot)).map(|c| c.name.clone()).unwrap_or_default()
            }
            _ => crate::profile::get().set_name(self.dialog_set),
        }
    }

    /// Apply a confirmed dialog entry. Every change is a profile edit, so it rides the same
    /// one-shot as the editor's save.
    fn apply_dialog(&mut self, item: DialogItem, name: &str, out: &mut MenuOut) {
        let set = self.dialog_set;
        let slot = self.dialog_slot;
        match item {
            DialogItem::Rename => {
                let text = match self.dialog.as_ref().map(|d| d.kind) {
                    Some(classui::DialogKind::ClassOptions) => match slot {
                        Some(slot) => {
                            let mut class = match crate::profile::get().customs_in(set).and_then(|c| c.get(slot)).cloned() {
                                Some(c) => c,
                                None => {
                                    self.deny();
                                    return;
                                }
                            };
                            class.name = name.to_string();
                            crate::profile::save_in_set(set, slot, class)
                        }
                        None => None,
                    },
                    _ => crate::profile::rename_set(set, name),
                };
                match text {
                    Some(text) => {
                        write_profile(out, classui::SAVE_PROFILE_EDIT, &text);
                        self.cues.push(cues::GRID_EQUIP);
                    }
                    None => self.deny(),
                }
            }
            DialogItem::Copy => match slot {
                Some(slot) => match crate::profile::copy_in_slot(slot) {
                    Some((_name, text)) => {
                        write_profile(out, classui::SAVE_PROFILE_EDIT, &text);
                        // The copy is appended to the set's filled slots: highlight it.
                        let row = PickRow::Custom(crate::profile::get().customs().len().saturating_sub(1));
                        if let Some(i) = classui::pick_rows(true).iter().position(|r| *r == row) {
                            self.highlight = i;
                        }
                        self.cues.push(cues::GRID_EQUIP);
                    }
                    None => self.deny(),
                },
                None => self.deny(),
            },
            DialogItem::ResetSet => match crate::profile::reset_set(set) {
                Some(text) => {
                    write_profile(out, classui::SAVE_PROFILE_EDIT, &text);
                    self.cues.push(cues::SAVE);
                }
                None => self.deny(),
            },
            DialogItem::ResetDefault | DialogItem::ClearLoadout => {
                let Some(slot) = slot else {
                    self.deny();
                    return;
                };
                let Some(existing) = crate::profile::get().customs_in(set).and_then(|c| c.get(slot)).cloned() else {
                    self.deny();
                    return;
                };
                // BO2's class dialog: reset puts the base kit back, clear empties every slot and
                // keeps the class where it is.
                let mut class = if item == DialogItem::ResetDefault {
                    Draft::default().to_custom()
                } else {
                    crate::classes::CustomClass::default()
                };
                class.name = existing.name.clone();
                match crate::profile::save_in_set(set, slot, class) {
                    Some(text) => {
                        write_profile(out, classui::SAVE_PROFILE_CLASS, &text);
                        self.cues.push(if item == DialogItem::ResetDefault { cues::SAVE } else { cues::ITEM_REMOVE });
                    }
                    None => self.deny(),
                }
            }
        }
    }

    /// The pick list's frame: browse the active set's slots; Left/Right walk the ten sets (making
    /// the shown one the active set, BO2's "you will load into the game with this class set"); an
    /// activated row opens the editor on it (an existing custom, a preset as a template, or a fresh
    /// draft); `S` opens the class-set options, `O` the highlighted class's; Escape closes the
    /// screen.
    fn pick_frame(&mut self, pointer: &MenuPointer, out: &mut MenuOut) {
        let rows = classui::pick_rows(true);
        // BO2's choose-class keys (`a-chooseclass/001`): S = Class Set Options, O = Class Options.
        // `O` needs a custom class to act on (a preset is read-only, CREATE has nothing yet).
        if self.prev.edge(pointer, Key::S) {
            let name = crate::profile::get().set_name(crate::profile::active_set());
            self.open_dialog(Dialog::set_options(&name), None);
            return;
        }
        if self.prev.edge(pointer, Key::O) {
            match rows.get(self.highlight).copied() {
                Some(PickRow::Custom(j)) => {
                    let name = crate::profile::get().customs_in(crate::profile::active_set()).and_then(|c| c.get(j)).map(|c| c.name.clone()).unwrap_or_default();
                    self.open_dialog(Dialog::class_options(&name), Some(j));
                }
                _ => self.deny(),
            }
            return;
        }
        let frame = classui::pick_frame(pointer, &self.prev, self.highlight, true);
        self.highlight = frame.highlight;
        // The set pager: switching sets is a profile change (the active set persists), so it rides
        // the same one-shot as the edits. The switch owns the frame: the stale row indices the
        // fold computed against the old set are not acted on.
        let switched = frame.set_step != 0 && match crate::profile::cycle_active(frame.set_step) {
            Some(text) => {
                write_profile(out, classui::SAVE_PROFILE_EDIT, &text);
                self.highlight = 0;
                // `SetActiveClassSet` slides the new set's list in and fades its preview in.
                self.set_change = Some(self.clock);
                true
            }
            None => false,
        };
        if !switched && frame.navigated && frame.activate.is_none() {
            self.cues.push(cues::row_nav(true));
        }
        if switched {
            // Prev/NextClassSet plays cac_safearea (`cacchooseclass.lua`).
            self.cues.push(cues::SET_STEP);
            if frame.back {
                self.cues.push(cues::screen_back(true));
                self.closed = true;
            }
            return;
        }
        // The footer's clickable prompts are the same operations as their keys.
        let footer_custom = match rows.get(self.highlight).copied() {
            Some(PickRow::Custom(j)) => Some(j),
            _ => None,
        };
        let delete = frame.delete;
        let copy = frame.copy;
        if let Some(i) = frame.activate {
            match rows.get(i).copied() {
                // A preset opens as a template: its kit in a fresh draft, named by the profile on
                // save (the preset itself is never written).
                Some(PickRow::Preset(j)) => {
                    if let Some(c) = crate::classes::get().get(j) {
                        let stored = crate::classes::CustomClass::from_spec(&c.spec);
                        self.open_editor(Draft::from_stored(&stored), None);
                    }
                }
                Some(PickRow::Custom(j)) => {
                    if let Some(c) = crate::profile::classes().get(j) {
                        self.open_editor(Draft::from_stored(c), Some(j));
                    }
                }
                // An untouched slot: a fresh class for the set's first free slot (BO2 keeps a set's
                // filled slots first, so that is the slot picked unless the player skips ahead).
                Some(PickRow::Empty(_)) => self.open_editor(Draft::default(), None),
                None => {}
            }
        } else if let Some(j) = delete {
            match crate::profile::remove_in_slot(j) {
                Some(text) => {
                    write_profile(out, classui::SAVE_PROFILE_EDIT, &text);
                    self.highlight = self.highlight.min(classui::pick_rows(true).len().saturating_sub(1));
                    self.cues.push(cues::ITEM_REMOVE);
                }
                None => self.deny(),
            }
        } else if let Some(j) = copy {
            match crate::profile::copy_in_slot(j) {
                Some((_new_name, text)) => {
                    write_profile(out, classui::SAVE_PROFILE_EDIT, &text);
                    // The copy is appended to the set's filled slots: highlight it.
                    let row = PickRow::Custom(crate::profile::get().customs().len().saturating_sub(1));
                    if let Some(i) = classui::pick_rows(true).iter().position(|r| *r == row) {
                        self.highlight = i;
                    }
                    self.cues.push(cues::GRID_EQUIP);
                }
                None => self.deny(),
            }
        }
        match frame.footer {
            Some(PickFooterAction::SetOptions) => {
                let name = crate::profile::get().set_name(crate::profile::active_set());
                self.open_dialog(Dialog::set_options(&name), None);
            }
            Some(PickFooterAction::ClassOptions) => match footer_custom {
                Some(j) => {
                    let name = crate::profile::get().customs_in(crate::profile::active_set()).and_then(|c| c.get(j)).map(|c| c.name.clone()).unwrap_or_default();
                    self.open_dialog(Dialog::class_options(&name), Some(j));
                }
                None => self.deny(),
            },
            Some(PickFooterAction::Back) => {
                self.cues.push(cues::screen_back(true));
                self.closed = true;
            }
            None => {}
        }
        if frame.back {
            self.cues.push(cues::screen_back(true));
            self.closed = true;
        }
    }

    /// The editor's frame: Save persists into the slot (and set) the editor was opened on and
    /// returns to the list (no pick — no match is running); a full set refuses the save and keeps
    /// the editor open; Cancel returns without saving.
    fn editor_frame(&mut self, mut editor: Editor, pointer: &MenuPointer, out: &mut MenuOut) {
        let frame = classui::editor_frame(&mut editor, pointer, &self.prev);
        if frame.denied {
            self.deny();
        }
        if let Some(cue) = frame.cue {
            self.cues.push(cue);
        }
        match frame.outcome {
            Outcome::Save => {
                let class = editor.draft.to_custom();
                let set = self.editor_set;
                let slot = self.editor_slot.unwrap_or_else(|| {
                    crate::profile::get().customs_in(set).map(|c| c.len()).unwrap_or(0)
                });
                match crate::profile::save_in_set(set, slot, class) {
                    Some(text) => {
                        write_profile(out, classui::SAVE_PROFILE_CLASS, &text);
                        self.cues.push(cues::SAVE);
                        // Back to the list with the saved slot highlighted.
                        let row = PickRow::Custom(slot);
                        if let Some(i) = classui::pick_rows(true).iter().position(|r| *r == row) {
                            self.highlight = i;
                        }
                    }
                    None => {
                        // No free slot: BO2 cannot create a sixth class in a set. Surface it and
                        // keep the editor up rather than losing the draft.
                        self.deny();
                        self.editor = Some(editor);
                    }
                }
            }
            Outcome::Cancel => {
                self.cues.push(cues::BACKOUT);
            }
            Outcome::Continue => {
                self.editor = Some(editor);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classes::CustomClass;
    use crate::input::Key;

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        crate::profile::lock()
    }

    fn custom(name: &str, primary: &str) -> CustomClass {
        CustomClass { name: name.into(), primary: primary.into(), ..CustomClass::default() }
    }

    /// One menu frame with the given keys held (released again the frame after).
    fn press(menu: &mut MenuState, inp: &mut MenuIn, k: Key) -> MenuOut {
        inp.keys.set_down(k, true);
        let out = menu.frame(inp);
        inp.keys.set_down(k, false);
        menu.frame(inp);
        out
    }

    /// Walk to the Save row and confirm it (the editor must be open).
    fn save_open_editor(menu: &mut MenuState, inp: &mut MenuIn) -> MenuOut {
        while menu.editor.as_ref().expect("editor open").slot != crate::classmenu::Slot::Save {
            press(menu, inp, Key::Down);
        }
        press(menu, inp, Key::Enter)
    }

    #[test]
    fn a_set_switch_starts_the_luas_250ms_slide_and_a_row_move_does_not() {
        let _g = lock();
        crate::profile::reset();
        let mut menu = MenuState::new();
        let mut inp = MenuIn::default();
        menu.frame(&inp);
        assert!(menu.view().set_change.is_none(), "the screen opens settled");
        // A row move is not a set switch.
        press(&mut menu, &mut inp, Key::Down);
        assert!(menu.view().set_change.is_none(), "a row move has no slide");
        // Right/Left walk the sets: `SetActiveClassSet` slides the new list in over 250 ms.
        press(&mut menu, &mut inp, Key::Right);
        let since = menu.view().set_change.expect("the switch started the transition");
        assert!((0.0..classui::SET_SWITCH_MS / 1000.0).contains(&since), "just started: {since}");
        // The hosts draw it in the transition until the duration elapses.
        for _ in 0..16 {
            menu.frame(&inp);
        }
        assert!(menu.view().set_change.is_none(), "the 250 ms have run: the screen is settled again");
        crate::profile::reset();
    }

    #[test]
    fn enter_on_a_row_opens_the_editor_on_that_slot() {
        let _g = lock();
        crate::profile::reset();
        crate::profile::save(custom("AB", "tar21_mp"));
        let mut menu = MenuState::new();
        let mut inp = MenuIn::default();
        menu.frame(&inp); // settle the open frame

        // The lobby's list is the active set's five custom slots (`cacchooseclass.lua`,
        // `a-chooseclass/001`): a saved custom opens the editor on the class, name and all
        // (re-saving replaces it).
        assert_eq!(classui::pick_rows(true)[menu.highlight], PickRow::Custom(0));
        press(&mut menu, &mut inp, Key::Enter);
        let editor = menu.editor.as_ref().expect("a custom row opens the editor");
        assert_eq!(editor.draft.name, "AB");
        assert_eq!(editor.draft.primary, "tar21_mp");
        press(&mut menu, &mut inp, Key::Escape); // back to the list
        assert!(menu.editor.is_none(), "Escape backs out of the editor");

        // An untouched slot (BO2's `Custom n`) opens a fresh draft.
        press(&mut menu, &mut inp, Key::Down);
        assert_eq!(classui::pick_rows(true)[menu.highlight], PickRow::Empty(1));
        press(&mut menu, &mut inp, Key::Enter);
        let editor = menu.editor.as_ref().expect("an untouched slot opens a fresh draft");
        assert_eq!(editor.draft, Draft::default());
        crate::profile::reset();
    }

    #[test]
    fn browse_edit_save_writes_the_profile_text_out() {
        let _g = lock();
        crate::profile::reset();
        let mut menu = MenuState::new();
        let mut inp = MenuIn::default();
        menu.frame(&inp);

        // Open the editor on the last row (the set's last untouched slot), change the lethal, save.
        let rows = classui::pick_rows(true).len();
        press(&mut menu, &mut inp, Key::Up); // wraps to the last row
        assert_eq!(menu.highlight, rows - 1, "wrapped to the last row");
        assert_eq!(classui::pick_rows(true)[menu.highlight], PickRow::Empty(crate::profile::MAX_CLASSES - 1));
        press(&mut menu, &mut inp, Key::Enter);
        assert!(menu.editor.is_some());
        // The editor opens on Primary; walk down to the Lethal row (Primary -> PrimaryAtts ->
        // Secondary -> SecondaryAtts -> Lethal).
        for _ in 0..4 {
            press(&mut menu, &mut inp, Key::Down);
        }
        assert_eq!(menu.editor.as_ref().expect("editor open").slot, crate::classmenu::Slot::Lethal);
        press(&mut menu, &mut inp, Key::Enter); // open the Lethal slot's list
        // The list is a content grid (`contentgrid.lua`): its items sit in a row, so Right moves
        // along it (Down only leaves a row that has another one below).
        press(&mut menu, &mut inp, Key::Right);
        press(&mut menu, &mut inp, Key::Enter); // choose it
        let out = save_open_editor(&mut menu, &mut inp);
        assert!(menu.editor.is_none(), "saving closes the editor");

        // The one-shot carries the whole profile file's text; the profile kept the class.
        assert_eq!(out.save_profile, classui::SAVE_PROFILE_CLASS);
        assert!(out.save_profile_payload_len > 0);
        let words = &out.save_profile_payload[..out.save_profile_payload_len as usize];
        let text = crate::profile::unpack_text(words).expect("the payload decodes");
        assert_eq!(text, crate::profile::get().to_text(), "the one-shot is the profile file's text");
        assert_eq!(crate::profile::classes().len(), 1);
        let saved = &crate::profile::classes()[0];
        assert_ne!(saved.lethal, Draft::default().lethal, "the edit landed: {}", saved.lethal);
        assert_eq!(saved.name, "Custom 1", "the profile named the blank class (BO2's slot name)");
        // The list now shows the saved slot, highlighted.
        assert_eq!(classui::pick_rows(true)[menu.highlight], PickRow::Custom(0));
        crate::profile::reset();
    }

    #[test]
    fn delete_and_copy_persist_through_the_one_shot() {
        let _g = lock();
        crate::profile::reset();
        crate::profile::save(custom("AB", "an94_mp"));
        let mut menu = MenuState::new();
        let mut inp = MenuIn::default();
        menu.frame(&inp);

        // Copy the AB row (the list's first row): the profile grows and the copy is highlighted.
        assert_eq!(classui::pick_rows(true)[menu.highlight], PickRow::Custom(0));
        let out = press(&mut menu, &mut inp, Key::C);
        assert_eq!(out.save_profile, classui::SAVE_PROFILE_EDIT);
        assert_eq!(crate::profile::classes().len(), 2);
        assert_eq!(crate::profile::classes()[1].name, "AB COPY");
        let rows = classui::pick_rows(true);
        assert_eq!(rows[menu.highlight], PickRow::Custom(1), "the copy is highlighted");

        // Delete the copy: the profile shrinks back; the highlight stays on the row, now the set's
        // first untouched slot.
        let out = press(&mut menu, &mut inp, Key::Backspace);
        assert_eq!(out.save_profile, classui::SAVE_PROFILE_EDIT);
        assert_eq!(crate::profile::classes().len(), 1);
        assert_eq!(crate::profile::classes()[0].name, "AB");
        assert_eq!(classui::pick_rows(true)[menu.highlight], PickRow::Empty(1));

        // Backspace on an untouched slot does nothing (no profile write, no deny).
        let out = press(&mut menu, &mut inp, Key::Backspace);
        assert_eq!(out.save_profile, 0);
        assert_eq!(crate::profile::classes().len(), 1);
        crate::profile::reset();
    }

    #[test]
    fn escape_on_the_list_closes_the_screen() {
        let _g = lock();
        crate::profile::reset();
        let mut menu = MenuState::new();
        let mut inp = MenuIn::default();
        let out = press(&mut menu, &mut inp, Key::Escape);
        assert_eq!(out.closed, 1, "Escape on the pick list closes");
        assert_eq!(out.save_profile, 0);

        // While the editor is up, Escape backs out to the list instead of closing.
        let mut menu = MenuState::new();
        press(&mut menu, &mut inp, Key::Enter);
        assert!(menu.editor.is_some());
        let out = press(&mut menu, &mut inp, Key::Escape);
        assert_eq!(out.closed, 0, "the editor's cancel is not a close");
        assert!(menu.editor.is_none());
        let out = press(&mut menu, &mut inp, Key::Escape);
        assert_eq!(out.closed, 1);
    }

    #[test]
    fn the_open_cue_plays_once_and_the_view_draws() {
        let _g = lock();
        crate::profile::reset();
        let mut menu = MenuState::new();
        assert_eq!(menu.cues, vec![cues::ENTER_CAC], "the open sting queues on open");
        menu.cues.clear();
        let inp = MenuIn::default();
        menu.frame(&inp);
        assert!(menu.cues.is_empty(), "once per open");
        // The view draws the pick screen (background art, the banner, the rows).
        let mut prims = Vec::new();
        classui::draw_screen(&mut prims, 1280.0, 720.0, &menu.view());
        let texts: Vec<&str> = prims
            .iter()
            .filter_map(|p| match p {
                crate::hud::Prim::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.contains(&"CHOOSE CLASS"), "the banner draws: {texts:?}");
        assert!(texts.contains(&"Custom 1"), "the set's untouched slots draw: {texts:?}");
        crate::profile::reset();
    }

    #[test]
    fn a_click_activates_the_row_under_the_cursor() {
        let _g = lock();
        crate::profile::reset();
        crate::profile::save(custom("AB", "an94_mp"));
        crate::profile::save(custom("SMG", "mp7_mp"));
        let mut menu = MenuState::new();
        let mut inp = MenuIn { screen_w: 1280.0, screen_h: 720.0, ..Default::default() };
        menu.frame(&inp);
        // Aim at the second row (the SMG custom) and click.
        let l = classui::pick_layout(1280.0, 720.0, true);
        let rect = classui::row_rects(&l, &classui::pick_rows(true))[1];
        inp.mouse_x = rect.x + 10.0;
        inp.mouse_y = rect.y + 5.0;
        menu.frame(&inp); // the hover lands
        inp.mouse_buttons = crate::input::mouse_buttons::LEFT;
        menu.frame(&inp); // the click edge
        inp.mouse_buttons = 0;
        menu.frame(&inp);
        let editor = menu.editor.as_ref().expect("the click opened the editor");
        assert_eq!(editor.draft.primary, "mp7_mp", "the SMG custom's gun");
        crate::profile::reset();
    }

    #[test]
    fn left_and_right_walk_the_class_sets_and_persist_the_active_one() {
        let _g = lock();
        crate::profile::reset();
        crate::profile::save(custom("Set One", "an94_mp"));
        let mut menu = MenuState::new();
        let mut inp = MenuIn::default();
        menu.frame(&inp);

        // Right: the next set becomes the active one (BO2: the set a match loads into) and the
        // profile write one-shot carries it.
        let out = press(&mut menu, &mut inp, Key::Right);
        assert_eq!(out.save_profile, classui::SAVE_PROFILE_EDIT);
        assert_eq!(crate::profile::active_set(), 1);
        let text = crate::profile::unpack_text(&out.save_profile_payload[..out.save_profile_payload_len as usize]).expect("the payload");
        assert!(text.contains("active 2"), "{text}");
        assert_eq!(menu.highlight, 0, "the highlight restarts on the new set's list");
        // The new set is empty: the list is the five defaults and CREATE.
        assert_eq!(classui::pick_rows(true).iter().filter(|r| matches!(r, PickRow::Custom(_))).count(), 0);
        // Left twice wraps through the first set to the last.
        press(&mut menu, &mut inp, Key::Left);
        assert_eq!(crate::profile::active_set(), 0);
        press(&mut menu, &mut inp, Key::Left);
        assert_eq!(crate::profile::active_set(), 9, "BO2's shoulders wrap");
        crate::profile::reset();
    }

    #[test]
    fn a_saved_edit_lands_in_the_slot_it_was_opened_on() {
        let _g = lock();
        crate::profile::reset();
        crate::profile::save(custom("A", "an94_mp"));
        crate::profile::save(custom("B", "tar21_mp"));
        let mut menu = MenuState::new();
        let mut inp = MenuIn::default();
        menu.frame(&inp);
        // Walk to the second custom (the presets then A then B) and open it.
        let target = classui::pick_rows(true).iter().position(|r| *r == PickRow::Custom(1)).expect("the B row");
        for _ in 0..target {
            press(&mut menu, &mut inp, Key::Down);
        }
        press(&mut menu, &mut inp, Key::Enter);
        let editor = menu.editor.as_ref().expect("the B row opens the editor");
        assert_eq!(editor.draft.name, "B");
        assert_eq!(editor.draft.primary, "tar21_mp");
        // Edit the lethal and save: the class stays in slot 2, A untouched in slot 1.
        for _ in 0..4 {
            press(&mut menu, &mut inp, Key::Down);
        }
        press(&mut menu, &mut inp, Key::Enter);
        // The lethal list is a content grid: Right walks its row (Down leaves a row only when
        // another is below).
        press(&mut menu, &mut inp, Key::Right);
        press(&mut menu, &mut inp, Key::Enter);
        let out = save_open_editor(&mut menu, &mut inp);
        assert_eq!(out.save_profile, classui::SAVE_PROFILE_CLASS);
        let classes = crate::profile::classes();
        assert_eq!(classes.len(), 2, "the edit replaced B, it did not append");
        assert_eq!(classes[0].name, "A");
        assert_eq!(classes[1].name, "B");
        assert_ne!(classes[1].lethal, Draft::default().lethal, "the edit landed");
        assert_eq!(menu.highlight, classui::pick_rows(true).iter().position(|r| *r == PickRow::Custom(1)).unwrap());
        crate::profile::reset();
    }

    #[test]
    fn a_sixth_class_is_refused_and_the_editor_stays_open() {
        let _g = lock();
        crate::profile::reset();
        for i in 0..crate::profile::MAX_CLASSES - 1 {
            crate::profile::save(custom(&format!("Filler {i}"), "an94_mp"));
        }
        let mut menu = MenuState::new();
        let mut inp = MenuIn::default();
        menu.frame(&inp);
        // The set's one untouched slot is the last row; open a fresh draft on it.
        press(&mut menu, &mut inp, Key::Up);
        assert_eq!(classui::pick_rows(true)[menu.highlight], PickRow::Empty(crate::profile::MAX_CLASSES - 1));
        press(&mut menu, &mut inp, Key::Enter);
        assert!(menu.editor.is_some());
        // The set fills while the draft is open (the bridge writes the profile from elsewhere).
        crate::profile::save(custom("Filler last", "mp7_mp"));
        let out = save_open_editor(&mut menu, &mut inp);
        assert_eq!(out.save_profile, 0, "nothing was written");
        assert!(menu.editor.is_some(), "the draft survives: the editor stays up");
        assert_eq!(crate::profile::classes().len(), crate::profile::MAX_CLASSES);
        assert!(menu.cues.contains(&cues::DENY), "the denied save is surfaced");
        crate::profile::reset();
    }

    /// Type `text` into the open dialog's field (one frame per character).
    fn type_text(menu: &mut MenuState, inp: &mut MenuIn, text: &str) {
        for c in text.chars() {
            let key = match c {
                ' ' => Key::Space,
                'A'..='Z' => Key::ALL
                    .iter()
                    .copied()
                    .find(|k| *k as u8 == (c as u8 - b'A'))
                    .expect("a letter key"),
                _ => panic!("type_text handles letters and spaces"),
            };
            press(menu, inp, key);
        }
    }

    /// Walk to the highlighted custom row (the presets come first).
    fn to_custom(menu: &mut MenuState, inp: &mut MenuIn, slot: usize) {
        let target = classui::pick_rows(true).iter().position(|r| *r == PickRow::Custom(slot)).expect("the custom row");
        while menu.highlight != target {
            press(menu, inp, Key::Down);
        }
    }

    #[test]
    fn s_opens_the_class_set_dialog_and_rename_persists() {
        let _g = lock();
        crate::profile::reset();
        let mut menu = MenuState::new();
        let mut inp = MenuIn::default();
        menu.frame(&inp);
        // `S` (BO2's Class Set Options key) opens the dialog on the active set.
        press(&mut menu, &mut inp, Key::S);
        let dialog = menu.dialog().expect("S opens the class-set dialog");
        assert_eq!(dialog.kind, classui::DialogKind::SetOptions);
        assert_eq!(dialog.subtitle, "CLASS SET 1");
        assert_eq!(dialog.items, vec![DialogItem::Rename, DialogItem::ResetSet]);
        // Enter on RENAME opens the field seeded with the current name; Enter again commits it.
        press(&mut menu, &mut inp, Key::Enter);
        assert!(menu.dialog().expect("still open").naming, "the rename field is up");
        for _ in 0..12 {
            press(&mut menu, &mut inp, Key::Backspace);
        }
        type_text(&mut menu, &mut inp, "SNIPER");
        let out = press(&mut menu, &mut inp, Key::Enter);
        assert_eq!(out.save_profile, classui::SAVE_PROFILE_EDIT, "the rename persists through the one-shot");
        assert!(menu.dialog().is_none(), "committing closes the dialog");
        assert_eq!(crate::profile::set_name(0), "SNIPER");
        // The pick screen's pager now names the set.
        let mut prims = Vec::new();
        classui::draw_screen(&mut prims, 1280.0, 720.0, &menu.view());
        assert!(prims.iter().any(|p| matches!(p, crate::hud::Prim::Text { text, .. } if text == "SNIPER")));
        // Escape on the list still closes the screen (the dialog is gone).
        let out = press(&mut menu, &mut inp, Key::Escape);
        assert_eq!(out.closed, 1);
        crate::profile::reset();
    }

    #[test]
    fn the_class_set_dialog_resets_the_set() {
        let _g = lock();
        crate::profile::reset();
        crate::profile::save(custom("AB", "an94_mp"));
        crate::profile::save(custom("CD", "an94_mp"));
        let mut menu = MenuState::new();
        let mut inp = MenuIn::default();
        menu.frame(&inp);
        press(&mut menu, &mut inp, Key::S);
        press(&mut menu, &mut inp, Key::Down); // RESET SET
        let out = press(&mut menu, &mut inp, Key::Enter);
        assert_eq!(out.save_profile, classui::SAVE_PROFILE_EDIT);
        assert!(crate::profile::classes().is_empty(), "the set's slots are empty");
        assert_eq!(crate::profile::set_name(0), "CLASS SET 1", "the set's name stays");
        crate::profile::reset();
    }

    #[test]
    fn o_opens_the_class_dialog_and_its_entries_land() {
        let _g = lock();
        crate::profile::reset();
        crate::profile::save(custom("AB", "an94_mp"));
        let mut menu = MenuState::new();
        let mut inp = MenuIn::default();
        menu.frame(&inp);
        to_custom(&mut menu, &mut inp, 0);
        // `O` (BO2's Class Options key) opens the dialog on the highlighted class.
        press(&mut menu, &mut inp, Key::O);
        let dialog = menu.dialog().expect("O opens the class dialog");
        assert_eq!(dialog.kind, classui::DialogKind::ClassOptions);
        assert_eq!(dialog.subtitle, "AB");
        assert_eq!(dialog.items.len(), 4, "RENAME, COPY, RESET TO DEFAULT, CLEAR LOADOUT");
        // COPY appends the copy and highlights it.
        press(&mut menu, &mut inp, Key::Down);
        let out = press(&mut menu, &mut inp, Key::Enter);
        assert_eq!(out.save_profile, classui::SAVE_PROFILE_EDIT);
        assert_eq!(crate::profile::classes().len(), 2);
        assert_eq!(crate::profile::classes()[1].name, "AB COPY");
        assert!(menu.dialog().is_none());
        // CLEAR LOADOUT on the first class empties its slots and keeps its slot and name.
        to_custom(&mut menu, &mut inp, 0);
        press(&mut menu, &mut inp, Key::O);
        for _ in 0..3 {
            press(&mut menu, &mut inp, Key::Down);
        }
        let out = press(&mut menu, &mut inp, Key::Enter);
        assert_eq!(out.save_profile, classui::SAVE_PROFILE_CLASS);
        let cleared = &crate::profile::classes()[0];
        assert!(cleared.primary.is_empty() && cleared.lethal.is_empty(), "{}", cleared.primary);
        assert_eq!(cleared.name, "AB", "the class keeps its name and slot");
        // RESET TO DEFAULT puts the base kit back.
        to_custom(&mut menu, &mut inp, 0);
        press(&mut menu, &mut inp, Key::O);
        press(&mut menu, &mut inp, Key::Down);
        press(&mut menu, &mut inp, Key::Down);
        let out = press(&mut menu, &mut inp, Key::Enter);
        assert_eq!(out.save_profile, classui::SAVE_PROFILE_CLASS);
        assert_eq!(crate::profile::classes()[0].primary, Draft::default().primary_id(), "the base kit is back");
        crate::profile::reset();
    }

    #[test]
    fn o_on_an_empty_slot_is_refused_and_escape_backs_out_of_a_dialog_first() {
        let _g = lock();
        crate::profile::reset();
        let mut menu = MenuState::new();
        let mut inp = MenuIn::default();
        menu.frame(&inp);
        // An untouched slot has nothing to rename or copy: O is refused.
        assert!(matches!(classui::pick_rows(true)[menu.highlight], PickRow::Empty(_)));
        menu.cues.clear();
        press(&mut menu, &mut inp, Key::O);
        assert!(menu.dialog().is_none());
        assert!(menu.cues.contains(&cues::DENY), "the refused O is surfaced");
        // Escape in a dialog backs out to the list; the screen stays open.
        press(&mut menu, &mut inp, Key::S);
        assert!(menu.dialog().is_some());
        let out = press(&mut menu, &mut inp, Key::Escape);
        assert!(menu.dialog().is_none(), "Escape closes the dialog");
        assert_eq!(out.closed, 0, "and not the screen");
        let out = press(&mut menu, &mut inp, Key::Escape);
        assert_eq!(out.closed, 1, "the second Escape closes the screen");
        crate::profile::reset();
    }

    #[test]
    fn the_footer_prompts_open_the_dialogs_with_the_mouse() {
        let _g = lock();
        crate::profile::reset();
        crate::profile::save(custom("AB", "an94_mp"));
        let mut menu = MenuState::new();
        let mut inp = MenuIn { screen_w: 1280.0, screen_h: 720.0, ..Default::default() };
        menu.frame(&inp);
        to_custom(&mut menu, &mut inp, 0);
        let l = classui::pick_layout(1280.0, 720.0, true);
        let (_, r) = classui::pick_footer_actions(&l)
            .into_iter()
            .find(|(a, _)| *a == PickFooterAction::ClassOptions)
            .expect("the O prompt");
        let r = l.frame.px(r);
        inp.mouse_x = r.x + r.w * 0.5;
        inp.mouse_y = r.y + r.h * 0.5;
        menu.frame(&inp); // the hover lands
        inp.mouse_buttons = crate::input::mouse_buttons::LEFT;
        menu.frame(&inp); // the click edge
        inp.mouse_buttons = 0;
        menu.frame(&inp);
        assert_eq!(menu.dialog().map(|d| d.kind), Some(classui::DialogKind::ClassOptions), "the O prompt clicked");
        crate::profile::reset();
    }
}
