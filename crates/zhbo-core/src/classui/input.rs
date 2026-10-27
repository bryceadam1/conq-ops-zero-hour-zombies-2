//! The shared input folds both hosts run: pointer and key edges, the pick screen's and the editor's frame.

use super::*;
use crate::classui::cues;

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

/// BO2's PC keys for LUI's shoulder buttons: t6mp's `gamepad_button` dispatcher
/// (`FUN_009376a0`) sends `A`/`a` as `shoulderl` and `D`/`d` as `shoulderr` (the pad's LB/RB). The
/// choose-class screen pages its class sets on them (`cacchooseclass.lua:299-318`), the
/// personalize screens their tabs (`tabmanagerlite.lua:535-555`).
pub const SHOULDER_L: Key = Key::A;
pub const SHOULDER_R: Key = Key::D;

/// One frame of LUI's navigation buttons, translated from the PC input exactly as t6mp's
/// `gamepad_button` dispatcher (`FUN_009376a0`) does: the arrow keys are `up`/`down`/`left`/
/// `right`, every mouse-wheel notch is `up`/`down` (`MWHEELUP`/`MWHEELDOWN` with the `mwheel`
/// qualifier, which no create-a-class screen tells apart from the arrows), and `A`/`D` are the
/// shoulders. Every class screen reads its moves through this, so the wheel moves exactly what Up/
/// Down move on that screen: a list row, a grid row, an editor link, a carousel category.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LuiNav {
    /// `up` presses this frame (the Up arrow plus wheel-up notches).
    pub up: u32,
    /// `down` presses this frame (the Down arrow plus wheel-down notches).
    pub down: u32,
    pub left: bool,
    pub right: bool,
    pub shoulder_l: bool,
    pub shoulder_r: bool,
}

impl LuiNav {
    pub fn read(prev: &MenuPrev, now: &MenuPointer) -> LuiNav {
        let notches = wheel_notches(now);
        LuiNav {
            up: prev.edge(now, Key::Up) as u32 + (-notches).max(0) as u32,
            down: prev.edge(now, Key::Down) as u32 + notches.max(0) as u32,
            left: prev.edge(now, Key::Left),
            right: prev.edge(now, Key::Right),
            shoulder_l: prev.edge(now, SHOULDER_L),
            shoulder_r: prev.edge(now, SHOULDER_R),
        }
    }

    /// The net vertical move, positive down.
    pub fn vertical(&self) -> i32 {
        self.down as i32 - self.up as i32
    }

    /// The net horizontal move, positive right.
    pub fn horizontal(&self) -> i32 {
        self.right as i32 - self.left as i32
    }

    /// The net shoulder move, positive for `shoulderr`.
    pub fn shoulder(&self) -> i32 {
        self.shoulder_r as i32 - self.shoulder_l as i32
    }
}

/// The wheel's whole notches this frame, positive down (the hosts' `wheel` convention).
pub fn wheel_notches(now: &MenuPointer) -> i32 {
    now.wheel.trunc() as i32
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
    /// The shell's set pager: move the active class set by this many (−1/+1, wrapping). BO2 pages
    /// on the shoulder buttons (`A`/`D` on PC) and on clicks on the `‹`/`›` prompts; the arrow keys
    /// never page (the class list is a vertical list with no left/right links). A match ignores it
    /// (the set is picked in the lobby) and shows the pager as a read-only display.
    pub set_step: i32,
    /// A click on one of the shell's set pips: make that set active (`ClassSetPip_LeftMouseUp`,
    /// `cacchooseclass.lua:1418-1432`).
    pub set_select: Option<usize>,
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
    // A `CoD.ButtonList` is a wrapping vertical list (`luiverticallist.lua:178-292`): Up/Down and
    // the wheel walk it, Left/Right have no link.
    let nav = LuiNav::read(prev, inp);
    let step = nav.vertical();
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
        } else if let Some(hit) = manage.then(|| pager_at(&layout, inp.mouse_x, inp.mouse_y)).flatten() {
            // The class-set header's `‹`/`›` prompts and its pips are clickable.
            match hit {
                PagerHit::Step(step) => out.set_step += step,
                PagerHit::Pip(set) => out.set_select = Some(set),
            }
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
        // The shell's set pager: BO2's shoulder prompts (`A`/`D`). A match is pick-only, so it never
        // changes the set the fight loads into.
        if manage {
            out.set_step += nav.shoulder();
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
    /// The personalize screen the editor asked for this frame (`TAB` knife, `C` camo, `V` reticle).
    pub personalize: Option<crate::classmenu::PersonalizeKind>,
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
    let mut wheel = wheel_notches(inp);
    // The weapon picker is BO2's card carousel list (`cardcarousellist.lua`): Left/Right walk the
    // guns of the active category and stop at its ends (`cardcarousel.lua:254-381`, the edge
    // bounce), Up/Down — and so the wheel, LUI's Up/Down — step the category and wrap
    // (`ScrollList`, `:723-772`). The fold consumes those edges here, before the pure editor sees
    // them, so the highlighted option always stays inside the drawn category.
    let cats = if editor.picking && !editor.naming { weapon_categories(editor) } else { Vec::new() };
    // The carousel's moves carry its own cue (`cardcarousellist.lua`): sideways is the LR slide,
    // up and down their own (`cac_slide_nav_*`), while the grids all share `cac_grid_nav`.
    let mut slide: Option<&'static str> = None;
    if !cats.is_empty() {
        if edges.left && step_within_category(editor, &cats, -1) {
            slide = Some(cues::SLIDE_LR);
        }
        if edges.right && step_within_category(editor, &cats, 1) {
            slide = Some(cues::SLIDE_LR);
        }
        let rows = edges.down as i32 - edges.up as i32 + wheel;
        for _ in 0..rows.unsigned_abs() {
            let step = rows.signum();
            if step_category(editor, &cats, step) {
                slide = Some(if step > 0 { cues::SLIDE_DOWN } else { cues::SLIDE_UP });
            }
        }
        edges.up = false;
        edges.down = false;
        edges.left = false;
        edges.right = false;
        wheel = 0;
    }
    // The categories just moved: re-derive them for the hit-test below.
    let cats = if cats.is_empty() { cats } else { weapon_categories(editor) };
    let cells = editor_cells(&layout, editor);
    // The box's circled X (mouse) and `R` (keyboard) clear the highlighted box, BO2's `R Remove`:
    // on the card row that is the highlighted card alone (each card is its own button).
    let mut removed = false;
    if !editor.picking && !editor.naming {
        if let Some((slot, index)) = editor_remove_at(&layout, &cells, editor, inp.mouse_x, inp.mouse_y).filter(|_| clicked) {
            removed = clear_cell(&mut editor.draft, slot, index);
        } else if prev.edge(inp, Key::R) {
            removed = if editor.slot == Slot::Wildcards {
                clear_cell(&mut editor.draft, Slot::Wildcards, editor.card)
            } else {
                clear_slot(&mut editor.draft, editor.slot)
            };
        }
    }
    // Hit-test every frame, not only when the cursor moved: a click with the cursor at rest is
    // still a click on the box under it. `moved` gates hover-only highlight changes (see
    // `classmenu::PointerEvent`), so a resting cursor cannot fight the keyboard.
    // The cells repeat a slot (three bonus-card boxes, two attachment boxes), so a hit box maps to
    // the walk's slot index plus, on the card row, which card. While a picker is open the grid is
    // not on screen, so it is not hit-tested at all.
    let slots = editor.slots();
    let hit = if editor.picking { None } else { editor_cell_at(&layout, &cells, inp.mouse_x, inp.mouse_y).and_then(|i| cells.get(i)) };
    let slot = hit.and_then(|c| slots.iter().position(|s| *s == c.slot));
    let card = hit.filter(|c| c.slot == Slot::Wildcards).map_or(0, |c| c.index);
    let start = if cats.is_empty() { picker_window(editor.slot, editor.option).0 } else { picker_window(editor.slot, position_in_category(&cats, editor.option)).0 };
    let drawn = if editor.picking {
        picker_option_at(&layout, editor.slot, inp.mouse_x, inp.mouse_y, start, drawn_option_count(editor, &cats))
    } else {
        None
    };
    // A weapon picker's drawn cards are the active category's guns (from its highlighted card);
    // every other picker's cells are the option list itself.
    let mut option = drawn.map(|i| {
        if cats.is_empty() {
            i
        } else {
            let cat = &cats[category_of(&cats, editor.option)];
            cat.options.get(i).copied().unwrap_or(editor.option)
        }
    });
    let mut click = clicked && !removed;
    if !cats.is_empty() {
        // The carousel's cards do not take focus on hover (`setHandleMouseMove(false)`,
        // `cardcarousel.lua:853-857`). A click on another card focuses it; a click on the focused
        // card is its action, the equip (`CardMouseClick`, `:1576-1643`).
        match option.filter(|_| click) {
            Some(o) if o != editor.option => {
                editor.option = o;
                slide = Some(cues::SLIDE_LR);
                option = None;
                click = false;
            }
            Some(_) => {}
            None => option = None,
        }
    }
    let pointer = crate::classmenu::PointerEvent { slot, card, option, click, wheel, moved: moved && cats.is_empty() };
    let before_slot = editor.slot;
    let before_card = editor.card;
    let before_option = editor.option;
    let before_picking = editor.picking;
    let before_naming = editor.naming;
    let before_draft = editor.draft.clone();
    // Hover moves the highlight only on a real move; a click/wheel is always an action. While the
    // name field has text entry, keys win unless the frame also clicked (to leave the field).
    let hover = moved && !editor.naming && (pointer.slot.is_some() || pointer.option.is_some());
    let has_pointer = pointer.is_action() || hover;
    let outcome = if has_pointer { editor.step_pointer(pointer) } else { editor.step(edges) };
    let denied = editor.denied;
    editor.denied = false;
    // Each event plays the alias the Lua plays at the same point (see `cues.rs`): opening the
    // weapon picker is `cac_loadout_edit_sel` and any other submenu `cac_loadout_edit_submenu`;
    // equipping names the item's own alias; backing out is the menu's `cac_cmn_backout`; a slot
    // move is the slot list's `cac_loadout_nav` (the bonus-card strip's own for its row) and an
    // option move the grid's or carousel's; `R`/the `X` is `cac_cmn_item_remove`.
    // A refused choice plays BO2's deny alone: the hosts surface `denied` (the cue and the rim), and
    // the Lua's handlers pick `denySFX` *instead of* the button's action sound (`cacperksp.lua`).
    let cue = if denied {
        None
    } else if removed {
        Some(cues::ITEM_REMOVE)
    } else {
        match outcome {
            crate::classmenu::Outcome::Continue => {
                let chose = editor.draft != before_draft;
                if editor.picking != before_picking {
                    if editor.picking {
                        // A weapon picker is CACEditClass's WeaponChosen; everything else is one of
                        // its submenus (`AttachmentSlotChosen`, `PerkSlotChosen`, …).
                        Some(if is_carousel(editor.slot) { cues::WEAPON_SELECT } else { cues::SUBMENU_SELECT })
                    } else if chose {
                        Some(equip_cue(editor))
                    } else {
                        Some(cues::BACKOUT)
                    }
                } else if editor.naming != before_naming {
                    // BO2's text field is a grid button too: opening or committing it is its action,
                    // backing out of it the menu's back-out.
                    Some(if editor.naming || chose { cues::GRID_EQUIP } else { cues::BACKOUT })
                } else if slide.is_some() || editor.option != before_option {
                    // The carousel's moves are folded before the snapshot, so `slide` (not the option
                    // delta) is what tells this frame apart.
                    Some(slide.unwrap_or(cues::GRID_NAV))
                } else if editor.slot != before_slot || (editor.slot == Slot::Wildcards && editor.card != before_card) {
                    Some(if editor.slot == Slot::Wildcards { cues::WILDCARD_NAV } else { cues::LOADOUT_NAV })
                } else if chose {
                    Some(equip_cue(editor))
                } else {
                    None
                }
            }
            _ => None,
        }
    };
    EditorFrame { outcome, denied, removed, personalize: editor.personalize, cue }
}

/// Whether a slot's picker is BO2's weapon carousel (the two gun slots), as opposed to a content
/// grid (`cacweaponsp.lua` versus `cacattachmentsmenu.lua`/`cacperksp.lua`).
fn is_carousel(slot: Slot) -> bool {
    matches!(slot, Slot::Primary | Slot::Secondary)
}

/// The alias an equip plays in the current slot: the carousel's slide for a gun, the equipment add
/// for an attachment (`attachmentgridbutton.lua`), the card's own alias for a wildcard
/// (`bonuscardbutton.lua`), and the grid's action for everything else (`luibutton.lua`).
fn equip_cue(editor: &Editor) -> &'static str {
    match editor.slot {
        Slot::Primary | Slot::Secondary => cues::SLIDE_EQUIP,
        Slot::PrimaryAtts | Slot::SecondaryAtts => cues::EQUIPMENT_ADD,
        Slot::Wildcards => editor.option_wildcard(editor.option).map_or(cues::GRID_EQUIP, cues::wildcard_equip),
        _ => cues::GRID_EQUIP,
    }
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
