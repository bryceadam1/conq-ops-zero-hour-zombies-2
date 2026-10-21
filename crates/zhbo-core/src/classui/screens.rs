//! The screens' shared view: what a host shows this frame, and the dispatch to the pick list
//! (`pick.rs`) or the editor (`editor.rs`).

use super::*;

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
    /// The lobby's screen (class sets, set/class options). False in a match.
    pub manage: bool,
    /// The host's UI clock in seconds (the focused button's pulse, LUI transitions).
    pub now: f32,
    /// Seconds since the class set changed (`cacchooseclass.lua`'s `SetActiveClassSet`), while the
    /// list is sliding in and the preview fading in; `None` when the screen is settled.
    pub set_change: Option<f32>,
}

/// Draw the class screen for a host: the editor while one is open, else the pick list.
pub fn draw_screen(prims: &mut Vec<Prim>, w: f32, h: f32, view: &ScreenView) {
    match view.editor {
        Some(editor) => draw_editor(prims, w, h, editor, view.deny, view.now),
        None => draw_pick(prims, w, h, view),
    }
}
