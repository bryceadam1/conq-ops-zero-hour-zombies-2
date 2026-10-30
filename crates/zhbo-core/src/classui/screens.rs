//! The screens' shared view: what a host shows this frame, and the dispatch to the pick list
//! (`pick.rs`) or the editor (`editor.rs`).

use super::*;
use crate::classmenu::Personalize;

/// What a host shows on the class screen this frame: the pick list (and its highlighted row), the
/// open editor, or one of BO2's personalize screens, plus the shared deny-rim counter. The in-game
/// `Client` and the shell `MenuState` both produce one; the drawing below never sees the hosts
/// themselves.
pub struct ScreenView<'a> {
    /// The pick list's highlighted row (unused while the editor is up).
    pub highlight: usize,
    /// The open allocation editor, if any (shell only; a match is pick-only).
    pub editor: Option<&'a Editor>,
    /// The open personalize screen, if any. It is a submenu of the editor, so it draws in the
    /// editor's place while it is up.
    pub personalize: Option<&'a Personalize>,
    /// Seconds left of the deny rim (a refused choice).
    pub deny: f32,
    /// The lobby's screen (class sets, set/class options). False in a match.
    pub manage: bool,
    /// The host's UI clock in seconds (the focused button's pulse, LUI transitions).
    pub now: f32,
    /// Seconds since the class set changed (`cacchooseclass.lua`'s `SetActiveClassSet`), while the
    /// list is sliding in and the preview fading in; `None` when the screen is settled.
    pub set_change: Option<f32>,
    /// Seconds since the shown menu (or submenu) opened, while `CoD.Menu.animateIn`'s fade runs;
    /// `None` once it has landed (a settled screen draws byte-identically to the old static one).
    pub open: Option<f32>,
}

/// `CoD.Menu.animateIn`'s duration (`codmenu.lua:2300-2326`): `beginAnimation("animate_popup_in",
/// 250, false, true)` — 250 ms, no ease-in, an ease-out, alpha 0 → 1. `openPopup` runs it for
/// every popup it creates (`codmenu.lua:1985-1990`; the lobby opens the choose-class screen through
/// it, `partylobby.lua:429`), and the port's class screen and its in-place submenus use the same
/// menu-open animation.
pub const MENU_OPEN_MS: f32 = 250.0;

/// Draw the class screen for a host: the editor while one is open, else the pick list. A menu that
/// has just opened is one alpha group faded by `animateIn`'s curve, exactly as LUI fades the whole
/// menu element (`bo2::fade` is the `alphaMultiplier`).
pub fn draw_screen(prims: &mut Vec<Prim>, w: f32, h: f32, view: &ScreenView) {
    if settle(view.open) >= 1.0 {
        draw_body(prims, w, h, view);
    } else {
        let mut group = Vec::new();
        draw_body(&mut group, w, h, view);
        bo2::fade(&mut group, crate::lui::anim::ease(settle(view.open), false, true));
        prims.extend(group);
    }
}

/// The static screen, with no menu-open fade around it.
fn draw_body(prims: &mut Vec<Prim>, w: f32, h: f32, view: &ScreenView) {
    // A personalize screen is a submenu of the editor: BO2 opens it from the `TAB` prompt and it
    // takes the screen over until it closes (`caccamomenu.lua`, `cacreticles.lua`).
    if let Some(personalize) = view.personalize {
        draw_personalize(prims, w, h, personalize, view.editor.and_then(|e| personalize_weapon(personalize.kind, &e.draft)), view.deny, view.now);
        return;
    }
    match view.editor {
        Some(editor) => draw_editor(prims, w, h, editor, view.deny, view.now),
        None => draw_pick(prims, w, h, view),
    }
}

/// `open`'s progress through `MENU_OPEN_MS`, 1 when there is no open transition.
pub fn settle(open: Option<f32>) -> f32 {
    open.map_or(1.0, |since| (since * 1000.0 / MENU_OPEN_MS).clamp(0.0, 1.0))
}
