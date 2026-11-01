/*
** Conquer Ops - Zero Hour gameplay core: the perk definitions.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/


// ------------------------------------------------------------------------------------------------
// Loadout and perks
// ------------------------------------------------------------------------------------------------

use super::specialty::Specialties;

/// One BO2 perk's identity: spec id, tier, localize key and statstable reference.
///
/// What the perk *is* in BO2 — its localized name and description and the specialties it grants
/// (Ghidra G1) — is its `mp/statstable.csv` row, which the weapon table carries as a `Perk` block
/// ([`super::def::PerkRow`], converted from the player's own statstable; the built-in table carries
/// the same sixteen rows so tests and the no-install fallback behave identically). This table only
/// ties the mod's perk ids and tiers to those rows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerkDef {
    /// The name `ZHBO_LOADOUT` specs use (`perks=Lightweight,...`). Matches the names the
    /// previous enum's `Debug` spelling produced, so existing loadouts keep parsing.
    pub id: &'static str,
    /// BO2's pick-10 tier (1, 2, or 3).
    pub tier: u8,
    /// BO2's localize key for the perk's name (`PERKS_<key>`).
    pub key: &'static str,
    /// The perk's `statstable.csv` reference without `PERKS_` (Dead Silence's row is `NINJA`): the
    /// key of its weapon-table row.
    pub stats: &'static str,
}

const fn perk(id: &'static str, tier: u8, key: &'static str, stats: &'static str) -> PerkDef {
    PerkDef { id, tier, key, stats }
}

/// BO2's multiplayer perks (Pick 10), in tier order.
pub const PERKS: [PerkDef; 16] = [
    perk("Lightweight", 1, "LIGHTWEIGHT", "LIGHTWEIGHT"),
    perk("Hardline", 1, "HARDLINE", "HARDLINE"),
    perk("BlindEye", 1, "BLIND_EYE", "BLIND_EYE"),
    perk("FlakJacket", 1, "FLAK_JACKET", "FLAK_JACKET"),
    perk("Ghost", 1, "GHOST", "GHOST"),
    perk("Toughness", 2, "TOUGHNESS", "TOUGHNESS"),
    perk("ColdBlooded", 2, "COLD_BLOODED", "COLD_BLOODED"),
    perk("FastHands", 2, "FAST_HANDS", "FAST_HANDS"),
    perk("HardWired", 2, "HARD_WIRED", "HARD_WIRED"),
    perk("Scavenger", 2, "SCAVENGER", "SCAVENGER"),
    perk("Dexterity", 3, "DEXTERITY", "DEXTERITY"),
    perk("ExtremeConditioning", 3, "EXTREME_CONDITIONING", "EXTREME_CONDITIONING"),
    perk("Engineer", 3, "HACKER", "HACKER"),
    perk("TacticalMask", 3, "TACTICAL_MASK", "TACTICAL_MASK"),
    perk("DeadSilence", 3, "DEAD_SILENCE", "NINJA"),
    perk("Awareness", 3, "AWARENESS", "AWARENESS"),
];

/// A perk: an index into [`PERKS`]. Loadout specs reference the table ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Perk(pub(crate) u8);

impl Perk {
    pub const LIGHTWEIGHT: Perk = Perk(0);
    pub const HARDLINE: Perk = Perk(1);
    pub const BLIND_EYE: Perk = Perk(2);
    pub const FLAK_JACKET: Perk = Perk(3);
    pub const GHOST: Perk = Perk(4);
    pub const TOUGHNESS: Perk = Perk(5);
    pub const COLD_BLOODED: Perk = Perk(6);
    pub const FAST_HANDS: Perk = Perk(7);
    pub const HARD_WIRED: Perk = Perk(8);
    pub const SCAVENGER: Perk = Perk(9);
    pub const DEXTERITY: Perk = Perk(10);
    pub const EXTREME_CONDITIONING: Perk = Perk(11);
    pub const ENGINEER: Perk = Perk(12);
    pub const TACTICAL_MASK: Perk = Perk(13);
    pub const DEAD_SILENCE: Perk = Perk(14);
    pub const AWARENESS: Perk = Perk(15);

    /// Every perk, in BO2's pick-10 order (tier 1, 2, 3).
    pub const ALL: [Perk; 16] = [
        Perk::LIGHTWEIGHT,
        Perk::HARDLINE,
        Perk::BLIND_EYE,
        Perk::FLAK_JACKET,
        Perk::GHOST,
        Perk::TOUGHNESS,
        Perk::COLD_BLOODED,
        Perk::FAST_HANDS,
        Perk::HARD_WIRED,
        Perk::SCAVENGER,
        Perk::DEXTERITY,
        Perk::EXTREME_CONDITIONING,
        Perk::ENGINEER,
        Perk::TACTICAL_MASK,
        Perk::DEAD_SILENCE,
        Perk::AWARENESS,
    ];

    /// The perk's definition (id, tier, key, effect).
    pub fn def(self) -> &'static PerkDef {
        &PERKS[self.0 as usize]
    }

    /// BO2's localize key for the perk's name (`PERKS_<key>`).
    pub fn key(self) -> &'static str {
        self.def().key
    }

    /// The perk's `statstable.csv` row in the installed weapon table (the converted one, or the
    /// built-in stand-in rows).
    pub fn row(self) -> Option<&'static super::def::PerkRow> {
        super::catalog::catalog().perk(self.def().stats)
    }

    /// The specialties the perk grants (its row's column 4).
    pub fn specialties(self) -> Specialties {
        self.row().map(|r| r.specialties).unwrap_or(Specialties::NONE)
    }

    /// BO2's localized name (`UIExpression.GetItemName`): the row's, or the spec id spelled out
    /// (`FastHands` -> `Fast Hands`) when a row has none.
    pub fn name(self) -> String {
        match self.row().filter(|r| !r.name.is_empty()) {
            Some(r) => r.name.clone(),
            None => {
                let id = self.def().id;
                let mut out = String::with_capacity(id.len() + 2);
                for (i, ch) in id.chars().enumerate() {
                    if i > 0 && ch.is_ascii_uppercase() {
                        out.push(' ');
                    }
                    out.push(ch);
                }
                out
            }
        }
    }

    /// BO2's localized description (`UIExpression.GetItemDesc`, statstable column 7): what the
    /// perk grid's lower panel shows under the name (`cacperksp.lua`'s `perkDescription`).
    pub fn description(self) -> &'static str {
        self.row().map_or("", |r| r.description.as_str())
    }

    /// The perk whose spec id matches, if any.
    pub fn by_id(id: &str) -> Option<Perk> {
        PERKS.iter().position(|d| d.id == id).map(|i| Perk(i as u8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapons::Specialty;

    #[test]
    fn every_spec_id_round_trips_to_a_distinct_perk() {
        let mut seen = std::collections::HashSet::new();
        for perk in Perk::ALL {
            let d = perk.def();
            assert_eq!(Perk::by_id(d.id), Some(perk), "spec id {} must round-trip", d.id);
            assert!((1..=3).contains(&d.tier), "tier {} out of range", d.id);
            assert!(!d.key.is_empty(), "{} has a localize key", d.id);
            assert!(!perk.specialties().is_empty(), "{} grants something", d.id);
            assert!(!perk.description().is_empty(), "{} has a description", d.id);
            assert!(seen.insert(d.id), "{} appears twice", d.id);
        }
        assert_eq!(seen.len(), PERKS.len(), "the table covers every perk");
    }

    #[test]
    fn perks_grant_bo2s_specialty_sets() {
        // statstable.csv:151 Lightweight, :161 Dexterity, :164 Extreme Conditioning (G1).
        let l = Perk::LIGHTWEIGHT.specialties();
        assert!(l.has(Specialty::MoveFaster) && l.has(Specialty::FallHeight));
        let d = Perk::DEXTERITY.specialties();
        for s in [Specialty::FastMantle, Specialty::FastLadderClimb, Specialty::SprintRecovery, Specialty::FastMeleeRecovery] {
            assert!(d.has(s), "Dexterity grants {}", s.name());
        }
        assert!(!d.has(Specialty::FastAds), "Dexterity is not quicker aiming");
        assert!(Perk::EXTREME_CONDITIONING.specialties().has(Specialty::LongerSprint));
        assert_eq!(Perk::DEAD_SILENCE.def().stats, "NINJA");
    }

    #[test]
    fn perks_carry_bo2s_names_and_descriptions() {
        // statstable.csv:151-166 column 3 and 7, localized (the built-in rows).
        assert_eq!(Perk::ENGINEER.name(), "Engineer");
        assert_eq!(Perk::DEAD_SILENCE.name(), "Dead Silence");
        assert_eq!(Perk::HARDLINE.description(), "Earn Scorestreaks faster.");
        assert_eq!(Perk::LIGHTWEIGHT.description(), "Move faster.  Take no damage when falling.");
    }
}
