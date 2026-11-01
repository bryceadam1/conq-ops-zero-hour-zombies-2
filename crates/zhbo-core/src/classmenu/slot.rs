//! The editor's rows (slots) and the weapon categories the weapon slots list.

/// One editable slot of the class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// BO2's "Class Name" row at the top: Enter starts text entry on it.
    Name,
    Primary,
    PrimaryAtts,
    Secondary,
    SecondaryAtts,
    Lethal,
    Tactical,
    Perk1,
    Perk2,
    Perk3,
    /// The second row of a perk tier, shown only when the tier's Greed wildcard is on. BO2's create-
    /// a-class adds a duplicate "PERK n" row for the extra pick; the row is dead without the
    /// wildcard, so it is not in [`Slot::ALL`] and only the editor's dynamic list shows it.
    Perk1b,
    Perk2b,
    Perk3b,
    Wildcards,
    /// BO2's final row: confirming it saves the class and leaves the editor. It has no option list.
    Save,
}

impl Slot {
    /// Every slot, in BO2's create-a-class order. `Save` is the row below the elements, so it is
    /// last in the walk and the arrows wrap from it back to the name. The second perk rows
    /// ([`Slot::Perk1b`] and its siblings) are *not* here: they only exist while their Greed wildcard
    /// is on, so the editor's [`Editor::slots`] inserts them into a copy of this list.
    pub const ALL: [Slot; 12] = [
        Slot::Name,
        Slot::Primary,
        Slot::PrimaryAtts,
        Slot::Secondary,
        Slot::SecondaryAtts,
        Slot::Lethal,
        Slot::Tactical,
        Slot::Perk1,
        Slot::Perk2,
        Slot::Perk3,
        Slot::Wildcards,
        Slot::Save,
    ];

    /// BO2's header for the slot. The second perk rows read as their tier's header.
    pub fn label(self) -> &'static str {
        match self {
            Slot::Name => "CLASS NAME",
            Slot::Primary => "PRIMARY",
            Slot::PrimaryAtts => "ATTACHMENTS",
            Slot::Secondary => "SECONDARY",
            Slot::SecondaryAtts => "ATTACHMENTS",
            Slot::Lethal => "LETHAL",
            Slot::Tactical => "TACTICAL",
            Slot::Perk1 | Slot::Perk1b => "PERK 1",
            Slot::Perk2 | Slot::Perk2b => "PERK 2",
            Slot::Perk3 | Slot::Perk3b => "PERK 3",
            // BO2 calls the items `bonuscard_*` internally but the screen is `MENU_WILDCARDS_CAPS`
            // (`menus/cacbonuscards.lua`), so the picker titles itself WILDCARDS.
            Slot::Wildcards => "WILDCARDS",
            Slot::Save => "SAVE CLASS",
        }
    }

    /// The perk tier this slot picks, if it is a perk slot (either row of a tier).
    pub fn perk_tier(self) -> Option<u8> {
        match self {
            Slot::Perk1 | Slot::Perk1b => Some(1),
            Slot::Perk2 | Slot::Perk2b => Some(2),
            Slot::Perk3 | Slot::Perk3b => Some(3),
            _ => None,
        }
    }

    /// Whether this is the *second* row of a perk tier (the Greed one).
    pub fn is_extra_perk(self) -> bool {
        matches!(self, Slot::Perk1b | Slot::Perk2b | Slot::Perk3b)
    }

    /// How many cards a row holds when this slot's picker is one of BO2's content grids
    /// (`cacattachmentsmenu.lua:9`: `GridColumns` 6; `cacperksp.lua:7` and
    /// `cacbonuscards.lua:7`: 3). `None` for the weapon carousel (`cacweaponsp.lua`, walked by
    /// category, not by grid links) and the rows without an option list.
    pub fn option_columns(self) -> Option<usize> {
        match self {
            Slot::PrimaryAtts | Slot::SecondaryAtts => Some(6),
            Slot::Lethal
            | Slot::Tactical
            | Slot::Perk1
            | Slot::Perk2
            | Slot::Perk3
            | Slot::Perk1b
            | Slot::Perk2b
            | Slot::Perk3b
            | Slot::Wildcards => Some(3),
            Slot::Name | Slot::Primary | Slot::Secondary | Slot::Save => None,
        }
    }
}

/// The second perk row of a tier (the Greed one).
pub(super) fn extra_perk_slot(tier: u8) -> Slot {
    match tier {
        2 => Slot::Perk2b,
        3 => Slot::Perk3b,
        _ => Slot::Perk1b,
    }
}

/// The base (non-Greed) row of a perk tier.
pub(super) fn base_perk_slot(tier: u8) -> Slot {
    match tier {
        2 => Slot::Perk2,
        3 => Slot::Perk3,
        _ => Slot::Perk1,
    }
}
