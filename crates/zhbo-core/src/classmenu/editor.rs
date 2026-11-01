//! The live editing session: its state, the rows it walks and the option lists it shows.

use super::*;

/// The live editing session. `None` when the editor is closed.
#[derive(Clone, Debug, PartialEq)]
pub struct Editor {
    /// The class being edited.
    pub draft: Draft,
    /// The highlighted slot.
    pub slot: Slot,
    /// The highlighted option within the slot's list.
    pub option: usize,
    /// Whether the slot's option list is open (picking) or the slot row is just highlighted.
    pub picking: bool,
    /// Whether the class-name row has text entry (BO2's name field): printable keys edit the name
    /// instead of moving the highlight, and Enter/Escape leave it.
    pub naming: bool,
    /// One-shot: the last confirm was refused (over budget, or an attachment at its cap). The
    /// caller plays BO2's deny cue and clears this; the state itself stays pure.
    pub denied: bool,
    /// One-shot: the player pressed Tab on the class row (`caceditclass.lua:52-63`'s
    /// `knifePersonalization` prompt) or `C`/`V` on the primary weapon/optic row. The caller opens
    /// the personalize screen and clears this; the pure editor only records the action.
    pub personalize: Option<PersonalizeKind>,
}

/// The editor's answer to one pointer frame, already hit-tested by the view (the editor knows
/// nothing about pixels). Hovering moves the highlight; a click confirms the hovered row, exactly
/// as Enter does on it; the wheel walks the list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PointerEvent {
    /// Hovered slot row (an index into [`Editor::slots`]).
    pub slot: Option<usize>,
    /// Hovered option row (an index into [`Editor::options`]) while a list is open.
    pub option: Option<usize>,
    /// The left button went down this frame.
    pub click: bool,
    /// Wheel notches this frame (positive scrolls down the list).
    pub wheel: i32,
    /// The cursor moved this frame. Only then does a hover move the highlight (a resting cursor
    /// must not fight the keyboard); a click always acts on the row under it.
    pub moved: bool,
}

impl PointerEvent {
    /// Whether this frame carries a pointer action at all (click or wheel).
    pub fn is_action(&self) -> bool {
        self.click || self.wheel != 0
    }
}

/// What [`Editor::apply`] did with an option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyResult {
    /// The draft changed.
    Changed,
    /// The option was already the draft's value (picking it again).
    NoChange,
    /// The choice would break the budget or a cap: the draft is untouched.
    Denied,
}

impl Editor {
    pub fn new(draft: Draft) -> Editor {
        Editor { draft, slot: Slot::Primary, option: 0, picking: false, naming: false, denied: false, personalize: None }
    }

    /// The rows to draw and walk, in order: [`Slot::ALL`] with a second perk row spliced in after a
    /// tier whose Greed wildcard is on (BO2's duplicate "PERK n" row).
    pub fn slots(&self) -> Vec<Slot> {
        let mut out: Vec<Slot> = Vec::with_capacity(Slot::ALL.len() + 3);
        for s in Slot::ALL {
            out.push(s);
            if let Some(tier) = s.perk_tier() {
                if !s.is_extra_perk() && self.draft.wildcards.contains(&greed_wildcard(tier)) {
                    out.push(extra_perk_slot(tier));
                }
            }
        }
        out
    }

    /// The highlighted slot's index in [`Editor::slots`], for the view's scrolling.
    pub fn slot_index(&self) -> usize {
        self.slots().iter().position(|s| *s == self.slot).unwrap_or(0)
    }

    /// The option list for the highlighted slot: display strings, in the order the arrows walk.
    /// The lists are rebuilt on demand so a weapon change re-derives the attachment list.
    pub fn options(&self) -> Vec<String> {
        match self.slot {
            Slot::Name => Vec::new(),
            Slot::Primary => weapon_options(&self.gun_list(true)),
            Slot::PrimaryAtts => attachment_options(&self.draft.primary),
            Slot::Secondary => weapon_options(&self.gun_list(false)),
            Slot::SecondaryAtts => attachment_options(&self.draft.secondary),
            Slot::Lethal => equipment_options(&self.offhand_list(true)),
            Slot::Tactical => equipment_options(&self.offhand_list(false)),
            Slot::Perk1 | Slot::Perk1b => perk_options(1),
            Slot::Perk2 | Slot::Perk2b => perk_options(2),
            Slot::Perk3 | Slot::Perk3b => perk_options(3),
            Slot::Wildcards => wildcard_list().iter().map(|w| w.name().to_string()).collect(),
            Slot::Save => Vec::new(),
        }
    }

    /// The offhand ids the lethal (`true`) or the tactical slot lists with the draft's cards: under
    /// Tactician the lethal slot lists the tactical items ([`items::listed_slot`]).
    pub(super) fn offhand_list(&self, lethal: bool) -> Vec<&'static str> {
        items::offhand_ids(lethal, &self.draft.wildcards)
    }

    /// The guns a weapon slot lists (`cacweaponsp.lua:616-720`, `AddWeaponGroupCarousel` at
    /// `:788-835`): its own statstable slot's; under Overkill both slots list the primary items
    /// only, less the gun the other slot holds.
    pub(super) fn gun_list(&self, primary: bool) -> Vec<&'static str> {
        let mut ids = gun_ids(&items::slots_of(primary, &self.draft.wildcards));
        if self.draft.wildcards.contains(&Wildcard::Overkill) {
            let other = if primary { &self.draft.secondary } else { &self.draft.primary };
            ids.retain(|id| id != other);
        }
        ids
    }

    /// Whether option `i` of the highlighted slot is already in the draft (so the list can mark it).
    /// A weapon/equipment/perk/wildcard option is "chosen" when it is the current value; an
    /// attachment option is chosen when the weapon carries its short.
    pub fn option_chosen(&self, i: usize) -> bool {
        match self.slot {
            Slot::Name => false,
            Slot::Primary => self.gun_list(true).get(i).is_some_and(|id| *id == self.draft.primary),
            Slot::Secondary => self.gun_list(false).get(i).is_some_and(|id| *id == self.draft.secondary),
            Slot::PrimaryAtts => {
                attachment_shorts(&self.draft.primary).get(i).is_some_and(|s| self.draft.primary_atts.iter().any(|a| a == s))
            }
            Slot::SecondaryAtts => {
                attachment_shorts(&self.draft.secondary).get(i).is_some_and(|s| self.draft.secondary_atts.iter().any(|a| a == s))
            }
            Slot::Lethal => self.offhand_list(true).get(i).is_some_and(|id| *id == self.draft.lethal),
            Slot::Tactical => self.offhand_list(false).get(i).is_some_and(|id| *id == self.draft.tactical),
            Slot::Perk1 | Slot::Perk2 | Slot::Perk3 | Slot::Perk1b | Slot::Perk2b | Slot::Perk3b => {
                // The tier's list marks a perk when *either* row of the tier holds it.
                let tier = self.slot.perk_tier().unwrap_or(1);
                let list = perk_list(tier);
                let idx = (tier as usize).clamp(1, 3) - 1;
                list.get(i).is_some_and(|p| self.draft.perks[idx] == Some(*p) || self.draft.extra_perks[idx] == Some(*p))
            }
            Slot::Wildcards => wildcard_list().get(i).is_some_and(|w| self.draft.wildcards.contains(w)),
            Slot::Save => false,
        }
    }

    /// The weapon option `i` of the highlighted slot selects, for the view's icon and stat bars.
    /// `None` when the slot does not list guns (perks, attachments, wildcards).
    pub fn option_weapon(&self, i: usize) -> Option<&'static crate::weapons::WeaponDef> {
        let id = match self.slot {
            Slot::Primary => self.gun_list(true).get(i).copied(),
            Slot::Secondary => self.gun_list(false).get(i).copied(),
            Slot::Lethal => self.offhand_list(true).get(i).copied(),
            Slot::Tactical => self.offhand_list(false).get(i).copied(),
            _ => None,
        }?;
        Some(weapons::weapon(id))
    }

    /// The attachment short option `i` of an attachment slot, for its icon.
    pub fn option_attachment(&self, i: usize) -> Option<&'static str> {
        match self.slot {
            Slot::PrimaryAtts => attachment_shorts(&self.draft.primary).get(i).copied(),
            Slot::SecondaryAtts => attachment_shorts(&self.draft.secondary).get(i).copied(),
            _ => None,
        }
    }

    /// Whether the highlighted primary attachment is one BO2 personalizes the reticle of.
    pub(super) fn option_is_optic(&self) -> bool {
        self.option_attachment(self.option).is_some_and(|s| crate::hud::OPTICS.contains(&s))
    }

    /// The perk option `i` of a perk row, for its icon.
    pub fn option_perk(&self, i: usize) -> Option<Perk> {
        self.slot.perk_tier().and_then(|tier| perk_list(tier).get(i).copied())
    }

    /// The wildcard option `i` of the wildcard slot, for its icon.
    pub fn option_wildcard(&self, i: usize) -> Option<Wildcard> {
        (self.slot == Slot::Wildcards).then(|| wildcard_list().get(i).copied()).flatten()
    }
}

/// The editor's outcome for one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The editor stays open; the caller keeps drawing it.
    Continue,
    /// The player confirmed a valid class: the caller saves it to the profile and closes.
    Save,
    /// The player cancelled: the caller closes without saving.
    Cancel,
}
