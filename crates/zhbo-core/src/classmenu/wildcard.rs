//! BO2's bonus cards (wildcards): the rules each card unlocks and the grid's order.

use super::*;

/// BO2's bonus cards, as the editor's rules know them: each variant is the behaviour a card's
/// reference (`bonuscard_overkill`, the statstable's column 4) unlocks. What the grid shows and
/// charges — the name, description, slot, cost and the list order — is the card's row in the
/// weapon table ([`crate::weapons::BonusCard`], converted from the player's `mp/statstable.csv`),
/// reached through [`Wildcard::card`]; the grid lists [`wildcard_list`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Wildcard {
    /// A second Tier 1 (Perk 1) perk.
    Perk1Greed,
    /// A second Tier 2 (Perk 2) perk.
    Perk2Greed,
    /// A second Tier 3 (Perk 3) perk.
    Perk3Greed,
    /// A second Primary Weapon in the Secondary slot.
    Overkill,
    /// A third attachment on the Primary Weapon.
    PrimaryGunfighter,
    /// A second attachment on the Secondary Weapon.
    SecondaryGunfighter,
    /// A Tactical in the Lethal slot.
    Tactician,
    /// A second Lethal.
    DangerClose,
}

impl Wildcard {
    /// Every card the editor has a rule for (no particular order: the grid's order is the
    /// table's, [`wildcard_list`]).
    pub const ALL: [Wildcard; 8] = [
        Wildcard::Perk1Greed,
        Wildcard::Perk2Greed,
        Wildcard::Perk3Greed,
        Wildcard::Overkill,
        Wildcard::SecondaryGunfighter,
        Wildcard::PrimaryGunfighter,
        Wildcard::Tactician,
        Wildcard::DangerClose,
    ];

    /// The card's statstable reference (column 4): its stable identity.
    pub fn reference(self) -> &'static str {
        match self {
            Wildcard::Perk1Greed => "bonuscard_perk_1_greed",
            Wildcard::Perk2Greed => "bonuscard_perk_2_greed",
            Wildcard::Perk3Greed => "bonuscard_perk_3_greed",
            Wildcard::Overkill => "bonuscard_overkill",
            Wildcard::PrimaryGunfighter => "bonuscard_primary_gunfighter",
            Wildcard::SecondaryGunfighter => "bonuscard_secondary_gunfighter",
            Wildcard::Tactician => "bonuscard_two_tacticals",
            Wildcard::DangerClose => "bonuscard_danger_close",
        }
    }

    /// The card with this statstable reference.
    pub fn by_reference(reference: &str) -> Option<Wildcard> {
        Wildcard::ALL.into_iter().find(|w| w.reference() == reference)
    }

    /// The card's row in the installed weapon table.
    pub fn card(self) -> Option<&'static weapons::BonusCard> {
        weapons::catalog().bonus_card(self.reference())
    }

    /// BO2's localized name (the row's column 3); the reference when the table lacks the row.
    pub fn name(self) -> &'static str {
        self.card().map_or(self.reference(), |c| c.name.as_str())
    }

    /// BO2's localized description (column 7), empty when the table lacks the row.
    pub fn description(self) -> &'static str {
        self.card().map_or("", |c| c.description.as_str())
    }

    /// What the card costs of the ten (column 12). A card the table lacks is charged one point,
    /// the unit every Pick-10 element costs.
    pub fn cost(self) -> i32 {
        self.card().map_or(1, |c| c.cost)
    }

    /// The card this one cannot share a class with: BO2's wildcard grid marks Tactician and Danger
    /// Close mutually exclusive by name (`bonuscardgridbutton.lua:239-275`, the
    /// `MENU_BONUSCARD_TWO_TACTICALS_NA` / `_DANGER_CLOSE_NA` hints).
    pub fn excludes(self) -> Option<Wildcard> {
        match self {
            Wildcard::Tactician => Some(Wildcard::DangerClose),
            Wildcard::DangerClose => Some(Wildcard::Tactician),
            _ => None,
        }
    }

    /// The perk tier this greed wildcard opens a second slot for, if it is one.
    pub fn greed_tier(self) -> Option<u8> {
        match self {
            Wildcard::Perk1Greed => Some(1),
            Wildcard::Perk2Greed => Some(2),
            Wildcard::Perk3Greed => Some(3),
            _ => None,
        }
    }
}

/// The wildcard grid's cards, in BO2's order: the weapon table's bonus cards (the grid lists the
/// `bonuscard` group by sort key, then index — `Catalog::bonus_cards`), each one the editor has a
/// rule for.
pub fn wildcard_list() -> Vec<Wildcard> {
    weapons::catalog().bonus_cards().iter().filter_map(|c| Wildcard::by_reference(&c.reference)).collect()
}

/// The Greed wildcard that opens a tier's second perk row.
pub(super) fn greed_wildcard(tier: u8) -> Wildcard {
    match tier {
        2 => Wildcard::Perk2Greed,
        3 => Wildcard::Perk3Greed,
        _ => Wildcard::Perk1Greed,
    }
}
