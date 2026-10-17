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

/// The effect kinds the sim implements. Perks needing systems Conquer Ops does not have
/// (UAV, targeting, AI support, flinch) are carried inert, exactly as the old enum did.
///
/// Multiplier kinds are paired with their magnitude in [`PerkDef`]; the magnitude lives in the
/// perk table (the single source), not next to the system that reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PerkEffect {
    /// Lightweight: movement speed multiplier.
    Speed(f32),
    /// Extreme Conditioning: sprint duration multiplier.
    SprintTime(f32),
    /// Dexterity: sprint-out (aim-up) time multiplier.
    SprintOut(f32),
    /// Flak Jacket: explosive damage multiplier.
    ExplosiveResist(f32),
    /// Tactical Mask: stun duration multiplier.
    StunResist(f32),
    /// Engineer: see and hack enemy equipment.
    Engineer,
    /// No effect implemented in this sim; carried inert (BO2 fidelity of the loadout).
    Inert,
}

/// One BO2 perk's definition: spec id, tier, localize key, and the effect the sim implements.
///
/// This table is the single source for every perk fact (refactor plan 3.3): the old
/// `enum Perk` + `key()` match + `perk_values` constants were three hand-synced copies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerkDef {
    /// The name `ZHBO_LOADOUT` specs use (`perks=Lightweight,...`). Matches the names the
    /// previous enum's `Debug` spelling produced, so existing loadouts keep parsing.
    pub id: &'static str,
    /// BO2's pick-10 tier (1, 2, or 3).
    pub tier: u8,
    /// BO2's localize key for the perk's name (`PERKS_<key>`).
    pub key: &'static str,
    pub effect: PerkEffect,
}

/// BO2's multiplayer perks (Pick 10), in tier order. Effects that need systems Conquer Ops does
/// not have yet (UAV, targeting, AI support, flinch) are carried inert.
pub const PERKS: [PerkDef; 16] = [
    PerkDef { id: "Lightweight", tier: 1, key: "LIGHTWEIGHT", effect: PerkEffect::Speed(1.07) },
    PerkDef { id: "Hardline", tier: 1, key: "HARDLINE", effect: PerkEffect::Inert },
    PerkDef { id: "BlindEye", tier: 1, key: "BLIND_EYE", effect: PerkEffect::Inert },
    PerkDef { id: "FlakJacket", tier: 1, key: "FLAK_JACKET", effect: PerkEffect::ExplosiveResist(0.65) },
    PerkDef { id: "Ghost", tier: 1, key: "GHOST", effect: PerkEffect::Inert },
    PerkDef { id: "Toughness", tier: 2, key: "TOUGHNESS", effect: PerkEffect::Inert },
    PerkDef { id: "ColdBlooded", tier: 2, key: "COLD_BLOODED", effect: PerkEffect::Inert },
    PerkDef { id: "FastHands", tier: 2, key: "FAST_HANDS", effect: PerkEffect::Inert },
    PerkDef { id: "HardWired", tier: 2, key: "HARD_WIRED", effect: PerkEffect::Inert },
    PerkDef { id: "Scavenger", tier: 2, key: "SCAVENGER", effect: PerkEffect::Inert },
    PerkDef { id: "Dexterity", tier: 3, key: "DEXTERITY", effect: PerkEffect::SprintOut(0.5) },
    PerkDef { id: "ExtremeConditioning", tier: 3, key: "EXTREME_CONDITIONING", effect: PerkEffect::SprintTime(2.0) },
    PerkDef { id: "Engineer", tier: 3, key: "HACKER", effect: PerkEffect::Engineer },
    PerkDef { id: "TacticalMask", tier: 3, key: "TACTICAL_MASK", effect: PerkEffect::StunResist(0.25) },
    PerkDef { id: "DeadSilence", tier: 3, key: "DEAD_SILENCE", effect: PerkEffect::Inert },
    PerkDef { id: "Awareness", tier: 3, key: "AWARENESS", effect: PerkEffect::Inert },
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

    /// The magnitude the sim implements for the perk's effect (1.0 when the effect has none).
    pub fn magnitude(self) -> f32 {
        match self.def().effect {
            PerkEffect::Speed(v)
            | PerkEffect::SprintTime(v)
            | PerkEffect::SprintOut(v)
            | PerkEffect::ExplosiveResist(v)
            | PerkEffect::StunResist(v) => v,
            _ => 1.0,
        }
    }

    /// The perk whose spec id matches, if any.
    pub fn by_id(id: &str) -> Option<Perk> {
        PERKS.iter().position(|d| d.id == id).map(|i| Perk(i as u8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spec_id_round_trips_to_a_distinct_perk() {
        let mut seen = std::collections::HashSet::new();
        for perk in Perk::ALL {
            let d = perk.def();
            assert_eq!(Perk::by_id(d.id), Some(perk), "spec id {} must round-trip", d.id);
            assert!((1..=3).contains(&d.tier), "tier {} out of range", d.id);
            assert!(!d.key.is_empty(), "{} has a localize key", d.id);
            assert!(seen.insert(d.id), "{} appears twice", d.id);
        }
        assert_eq!(seen.len(), PERKS.len(), "the table covers every perk");
    }

    #[test]
    fn magnitudes_match_bo2s_documented_defaults() {
        assert_eq!(Perk::LIGHTWEIGHT.magnitude(), 1.07, "Lightweight: move faster");
        assert_eq!(Perk::EXTREME_CONDITIONING.magnitude(), 2.0, "Extreme Conditioning: sprint longer");
        assert_eq!(Perk::DEXTERITY.magnitude(), 0.5, "Dexterity: aim faster after sprinting");
        assert_eq!(Perk::FLAK_JACKET.magnitude(), 0.65, "Flak Jacket: less explosive damage");
        assert_eq!(Perk::TACTICAL_MASK.magnitude(), 0.25, "Tactical Mask: less stun");
        // Unimplemented kinds carry the neutral multiplier.
        assert_eq!(Perk::HARDLINE.magnitude(), 1.0);
    }
}
