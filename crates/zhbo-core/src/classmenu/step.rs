//! The editor's input fold: key and pointer frames move the highlight, open lists and save.

use super::*;

impl Editor {
    /// Fold one frame's key edges into the session. `valid` gates the save (the caller's own check
    /// of the budget, so the UI can also show it).
    pub fn step(&mut self, edges: Edges) -> Outcome {
        self.denied = false;
        self.personalize = None;
        if self.naming {
            return self.step_naming(edges);
        }
        if self.picking {
            return self.step_picking(edges);
        }
        // BO2's `TAB Personalize Knife` prompt opens the personalize screen without moving the
        // row highlight or touching the draft (`caceditclass.lua:52-63`).
        if edges.personalize {
            self.personalize = Some(PersonalizeKind::Knife);
            return Outcome::Continue;
        }
        // BO2's `AddCamoButton` on the primary weapon row (`caceditclass.lua:748`).
        if edges.camo && self.slot == Slot::Primary {
            self.personalize = Some(PersonalizeKind::Camo);
            return Outcome::Continue;
        }
        // BO2's `AddReticleButton` on the primary optic row (`caceditclass.lua:825`).
        if edges.reticle && self.slot == Slot::PrimaryAtts && self.option_is_optic() {
            self.personalize = Some(PersonalizeKind::Reticle);
            return Outcome::Continue;
        }
        // The slot row is highlighted: up/down move the highlight, Enter opens the list.
        if edges.up {
            self.slot = self.prev_slot(self.slot);
            self.option = 0;
        }
        if edges.down {
            self.slot = self.next_slot(self.slot);
            self.option = 0;
        }
        if edges.confirm {
            return self.confirm();
        }
        if edges.cancel {
            return Outcome::Cancel;
        }
        Outcome::Continue
    }

    /// Confirm the highlighted row, the shared body of Enter and a click on a row: Save leaves the
    /// editor (only when the draft fits the budget), the name row starts text entry, everything else
    /// opens its option list on the current choice.
    pub(super) fn confirm(&mut self) -> Outcome {
        if self.slot == Slot::Save {
            // BO2 refuses an over-budget save; surface it rather than closing silently.
            if self.draft.valid() {
                return Outcome::Save;
            }
            self.denied = true;
            return Outcome::Continue;
        }
        if self.slot == Slot::Name {
            self.naming = true;
            return Outcome::Continue;
        }
        self.picking = true;
        self.option = self.current_option_index();
        Outcome::Continue
    }

    /// Fold one pointer frame (hover/click/wheel, already hit-tested by the view) into the session.
    /// Pure like [`Editor::step`]; a click on a row is Enter on it. A *hover* only moves the
    /// highlight on a real cursor move, so keyboard navigation is never overridden by the mouse
    /// resting somewhere; a click always acts on the row under the cursor.
    pub fn step_pointer(&mut self, pointer: PointerEvent) -> Outcome {
        self.denied = false;
        if self.naming {
            // Clicking away leaves the name field; it never cancels the editor.
            if pointer.click {
                self.naming = false;
            }
            return Outcome::Continue;
        }
        if self.picking {
            let n = self.options().len();
            if pointer.wheel != 0 && n > 0 {
                self.option = (self.option as i32 + pointer.wheel).rem_euclid(n as i32) as usize;
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
                } else if let Some(i) = pointer.slot {
                    // A click outside the open list returns to the row walk and confirms that row.
                    if let Some(slot) = self.slots().get(i).copied() {
                        self.slot = slot;
                        self.option = 0;
                        self.picking = false;
                        return self.confirm();
                    }
                }
            }
            return Outcome::Continue;
        }
        if pointer.wheel != 0 {
            let list = self.slots();
            let i = list.iter().position(|s| *s == self.slot).unwrap_or(0) as i32;
            self.slot = list[(i + pointer.wheel).rem_euclid(list.len() as i32) as usize];
            self.option = 0;
        }
        if pointer.moved || pointer.click {
            if let Some(i) = pointer.slot {
                if let Some(slot) = self.slots().get(i).copied() {
                    self.slot = slot;
                    self.option = 0;
                }
            }
        }
        if pointer.click && pointer.slot.is_some() {
            return self.confirm();
        }
        Outcome::Continue
    }

    /// The name row has text entry: printable keys append, Backspace deletes, Enter/Escape leave
    /// the field (the text stays either way; Escape does not cancel the whole editor while naming).
    pub(super) fn step_naming(&mut self, edges: Edges) -> Outcome {
        for key in &edges.typed {
            if let Some(c) = char_of(*key) {
                if self.draft.name.chars().count() < NAME_MAX {
                    self.draft.name.push(c);
                }
            }
        }
        if edges.backspace {
            self.draft.name.pop();
        }
        if edges.confirm || edges.cancel {
            self.naming = false;
        }
        Outcome::Continue
    }

    /// The rows to walk, in order: [`Editor::slots`].
    pub(super) fn next_slot(&self, s: Slot) -> Slot {
        let list = self.slots();
        let i = list.iter().position(|x| *x == s).unwrap_or(0);
        list[(i + 1) % list.len()]
    }

    pub(super) fn prev_slot(&self, s: Slot) -> Slot {
        let list = self.slots();
        let i = list.iter().position(|x| *x == s).unwrap_or(0);
        list[(i + list.len() - 1) % list.len()]
    }

    /// The option list is open: the arrows move, Enter picks, Escape closes the list (one step
    /// back). A content grid is two-dimensional (`Slot::option_columns`, `contentgrid.lua`): Left/
    /// Right walk the row and wrap at its ends, Up/Down walk the column and wrap at the first/last
    /// row. The weapon carousel keeps the plain list walk here; its four arrows are consumed by the
    /// shared fold (`classui::editor_frame`) because they steer its categories.
    pub(super) fn step_picking(&mut self, edges: Edges) -> Outcome {
        // BO2's `AddReticleButton` on the primary optic row (`caceditclass.lua:825`).
        if edges.reticle && self.slot == Slot::PrimaryAtts && self.option_is_optic() {
            self.personalize = Some(PersonalizeKind::Reticle);
            return Outcome::Continue;
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
            Slot::Name => 0,
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
            Slot::Save => 0,
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
