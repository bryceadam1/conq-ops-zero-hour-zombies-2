//! The editor's input fold: key and pointer frames move the highlight, open lists and save.

use super::*;

impl Editor {
    /// Fold one frame's key edges into the session. `valid` gates the save (the caller's own check
    /// of the budget, so the UI can also show it).
    pub fn step(&mut self, edges: Edges) -> Outcome {
        self.denied = false;
        self.personalize = None;
        if self.picking {
            return self.step_picking(edges);
        }
        // BO2's `TAB Personalize Knife` prompt opens the personalize screen without moving the
        // row highlight or touching the draft (`caceditclass.lua:52-63`).
        if edges.personalize {
            self.personalize = Some(PersonalizeKind::Knife);
            return Outcome::Continue;
        }
        // BO2's `AddCamoButton` on a weapon box that holds a gun (`caceditclass.lua:748`).
        if edges.camo {
            if let Some(kind) = self.camo_page() {
                self.personalize = Some(kind);
                return Outcome::Continue;
            }
        }
        // BO2's `AddReticleButton` on a slot holding a custom-reticle optic (`caceditclass.lua:825`).
        if edges.reticle {
            if let Some(kind) = self.reticle_page() {
                self.personalize = Some(kind);
                return Outcome::Continue;
            }
        }
        // A box is highlighted: the arrows follow BO2's grid links (`nav.rs`), Enter opens the list.
        for (pressed, dir) in [(edges.up, Dir::Up), (edges.down, Dir::Down), (edges.left, Dir::Left), (edges.right, Dir::Right)] {
            if pressed {
                self.navigate(dir);
            }
        }
        if edges.confirm {
            return self.confirm();
        }
        if edges.cancel {
            return self.back_out();
        }
        Outcome::Continue
    }

    /// Escape on the grid: BO2's back-out keeps the class (see [`Outcome::Save`]). Only a draft
    /// that breaks the budget cannot be kept; it closes unsaved with the deny cue rather than
    /// trapping the player in the editor.
    pub(super) fn back_out(&mut self) -> Outcome {
        if self.draft.valid() {
            Outcome::Save
        } else {
            self.denied = true;
            Outcome::Cancel
        }
    }

    /// Confirm the highlighted box, the shared body of Enter and a click on it: open its option list
    /// on the current choice.
    pub(super) fn confirm(&mut self) -> Outcome {
        self.picking = true;
        self.option = self.current_option_index();
        self.carousel.clear();
        Outcome::Continue
    }

    /// Fold one pointer frame (hover/click/wheel, already hit-tested by the view) into the session.
    /// Pure like [`Editor::step`]; a click on a row is Enter on it. A *hover* only moves the
    /// highlight on a real cursor move, so keyboard navigation is never overridden by the mouse
    /// resting somewhere; a click always acts on the row under the cursor.
    pub fn step_pointer(&mut self, pointer: PointerEvent) -> Outcome {
        self.denied = false;
        if self.picking {
            let n = self.options().len();
            // The wheel is LUI's Up/Down (t6mp `FUN_009376a0`): a grid row per notch.
            if pointer.wheel != 0 && n > 0 {
                self.step_rows(pointer.wheel, n);
            }
            if pointer.moved {
                if let Some(i) = pointer.option {
                    if i < n {
                        self.option = i;
                    }
                }
            }
            if pointer.click {
                if let Some(i) = pointer.option {
                    if i < n {
                        if self.apply(i) == ApplyResult::Denied {
                            self.denied = true;
                        }
                        self.picking = false;
                    }
                }
                // A click beside the open list does nothing: the picker owns the screen.
            }
            return Outcome::Continue;
        }
        // The wheel is LUI's Up/Down (t6mp `FUN_009376a0`): one grid link per notch.
        for _ in 0..pointer.wheel.unsigned_abs() {
            self.navigate(if pointer.wheel > 0 { Dir::Down } else { Dir::Up });
        }
        // Hover focuses the box (`luielement.lua:316-348`, running its `gain_focus` hook); a click
        // activates it (`luibutton.lua:106-128`).
        if pointer.moved || pointer.click {
            if let Some(f) = self.pointer_box(&pointer) {
                self.focus_on(f);
            }
        }
        if pointer.click && pointer.slot.is_some() {
            return self.confirm();
        }
        Outcome::Continue
    }

    /// The box a pointer frame hit: its row in [`Editor::slots`], and the card on the card row.
    pub(super) fn pointer_box(&self, pointer: &PointerEvent) -> Option<Focus> {
        let slot = self.slots().get(pointer.slot?).copied()?;
        Some(Focus { slot, card: if slot == Slot::Wildcards { pointer.card } else { 0 } })
    }

    /// Walk the open list `rows` rows (positive down) by the grid's column links, or by one
    /// option per step in a list without a grid.
    pub(super) fn step_rows(&mut self, rows: i32, n: usize) {
        for _ in 0..rows.unsigned_abs() {
            self.option = match self.slot.option_columns() {
                Some(cols) if rows > 0 => grid_down(self.option, n, cols),
                Some(cols) => grid_up(self.option, n, cols),
                None => (self.option as i32 + rows.signum()).rem_euclid(n as i32) as usize,
            };
        }
    }

    /// The option list is open: the arrows move, Enter picks, Escape closes the list (one step
    /// back). A content grid is two-dimensional (`Slot::option_columns`, `contentgrid.lua`): Left/
    /// Right walk the row and wrap at its ends, Up/Down walk the column and wrap at the first/last
    /// row. The weapon carousel keeps the plain list walk here; its four arrows are consumed by the
    /// shared fold (`classui::editor_frame`) because they steer its categories.
    pub(super) fn step_picking(&mut self, edges: Edges) -> Outcome {
        // BO2's `AddReticleButton` on a slot holding a custom-reticle optic (`caceditclass.lua:825`).
        if edges.reticle {
            if let Some(kind) = self.reticle_page() {
                self.personalize = Some(kind);
                return Outcome::Continue;
            }
        }
        let n = self.options().len();
        if n == 0 {
            if edges.cancel || edges.confirm {
                self.picking = false;
            }
            return Outcome::Continue;
        }
        match self.slot.option_columns() {
            Some(cols) => {
                if edges.left {
                    self.option = grid_left(self.option, n, cols);
                }
                if edges.right {
                    self.option = grid_right(self.option, n, cols);
                }
                if edges.up {
                    self.option = grid_up(self.option, n, cols);
                }
                if edges.down {
                    self.option = grid_down(self.option, n, cols);
                }
            }
            None => {
                if edges.up {
                    self.option = (self.option + n - 1) % n;
                }
                if edges.down {
                    self.option = (self.option + 1) % n;
                }
            }
        }
        if edges.confirm {
            if self.apply(self.option) == ApplyResult::Denied {
                self.denied = true;
            }
            self.picking = false;
        }
        if edges.cancel {
            self.picking = false;
        }
        Outcome::Continue
    }

    /// Where a weapon list opens: on the held gun's card (`cacweaponsp.lua:1020-1029` records it as
    /// `initialCarousel`/`initialCard`); when the list does not offer it (a pistol secondary under
    /// Overkill) no initial card is set and the carousel list keeps its default, the first
    /// carousel's first card (`cardcarousellist.lua:893`) — the first gun of the first category.
    fn weapon_open_index(&self) -> usize {
        let primary = self.slot == Slot::Primary;
        let ids = self.gun_list(primary);
        let held = if primary { &self.draft.primary } else { &self.draft.secondary };
        if let Some(i) = ids.iter().position(|id| id == held) {
            return i;
        }
        let order = crate::classui::category_order(self.slot);
        let rank = |id: &str| order.iter().position(|c| *c == weapons::weapon(id).cac_class()).unwrap_or(usize::MAX);
        (0..ids.len()).min_by_key(|&i| (rank(ids[i]), i)).unwrap_or(0)
    }

    /// The index in the option list of the slot's current value (so the list opens on it).
    pub(super) fn current_option_index(&self) -> usize {
        match self.slot {
            Slot::Primary | Slot::Secondary => self.weapon_open_index(),
            // BO2 focuses the button of the attachment the chosen slot holds, else the first
            // (`cacattachmentsmenu.lua:855-879`, `attachmentIndexToReplace`): the row's first.
            Slot::PrimaryAtts => attachment_index(&self.draft.primary, &self.draft.primary_atts),
            Slot::SecondaryAtts => attachment_index(&self.draft.secondary, &self.draft.secondary_atts),
            Slot::Lethal => equipment_index(&self.offhand_list(true), &self.draft.lethal),
            Slot::Tactical => equipment_index(&self.offhand_list(false), &self.draft.tactical),
            Slot::Perk1 | Slot::Perk1b => perk_index(1, self.perk_held(1)),
            Slot::Perk2 | Slot::Perk2b => perk_index(2, self.perk_held(2)),
            Slot::Perk3 | Slot::Perk3b => perk_index(3, self.perk_held(3)),
            Slot::Wildcards => 0,
        }
    }

    /// The perk the highlighted row holds: the tier's first pick on the base row, its Greed pick on
    /// the second row.
    pub(super) fn perk_held(&self, tier: u8) -> Option<Perk> {
        let idx = (tier as usize).clamp(1, 3) - 1;
        if self.slot.is_extra_perk() {
            self.draft.extra_perks[idx]
        } else {
            self.draft.perks[idx]
        }
    }
}
