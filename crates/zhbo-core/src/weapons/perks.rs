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

use super::specialty::{Specialties, Specialty};
use std::collections::HashMap;
use std::sync::Mutex;

/// One BO2 perk's definition: spec id, tier, localize key, and the specialties it grants.
///
/// This table is the single source for every perk fact the UI needs (refactor plan 3.3). What a
/// perk *does* is its specialty set (Ghidra G1): the converted weapon table carries BO2's own sets
/// from the player's `mp/statstable.csv` (`Perk` blocks, see [`install_converted`]); the built-in
/// sets below are those same rows (`lua-dump/patch_mp/mp/statstable.csv:151-166`) so tests and the
/// no-install fallback behave identically.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerkDef {
    /// The name `ZHBO_LOADOUT` specs use (`perks=Lightweight,...`). Matches the names the
    /// previous enum's `Debug` spelling produced, so existing loadouts keep parsing.
    pub id: &'static str,
    /// BO2's pick-10 tier (1, 2, or 3).
    pub tier: u8,
    /// BO2's localize key for the perk's name (`PERKS_<key>`).
    pub key: &'static str,
    /// The perk's `statstable.csv` reference without `PERKS_` (Dead Silence's row is `NINJA`).
    pub stats: &'static str,
    /// The specialties BO2 grants (`statstable.csv` column e), the built-in copy.
    pub specialties: &'static [Specialty],
}

const fn perk(id: &'static str, tier: u8, key: &'static str, stats: &'static str, specialties: &'static [Specialty]) -> PerkDef {
    PerkDef { id, tier, key, stats, specialties }
}

use Specialty as S;

/// BO2's multiplayer perks (Pick 10), in tier order.
pub const PERKS: [PerkDef; 16] = [
    perk("Lightweight", 1, "LIGHTWEIGHT", "LIGHTWEIGHT", &[S::MoveFaster, S::FallHeight]),
    perk("Hardline", 1, "HARDLINE", "HARDLINE", &[S::EarnMoreMomentum]),
    perk("BlindEye", 1, "BLIND_EYE", "BLIND_EYE", &[S::NotTargetedByAirSupport]),
    perk("FlakJacket", 1, "FLAK_JACKET", "FLAK_JACKET", &[S::FlakJacket]),
    perk("Ghost", 1, "GHOST", "GHOST", &[S::GpsJammer]),
    perk("Toughness", 2, "TOUGHNESS", "TOUGHNESS", &[S::BulletFlinch]),
    perk(
        "ColdBlooded",
        2,
        "COLD_BLOODED",
        "COLD_BLOODED",
        &[S::ImmuneNvThermal, S::NoName, S::ImmuneRangefinder, S::NoKillstreakReticle, S::ImmuneMms, S::NoMotionSensor],
    ),
    perk("FastHands", 2, "FAST_HANDS", "FAST_HANDS", &[S::FastWeaponSwitch, S::PinBack, S::FastToss, S::FastEquipmentUse]),
    perk("HardWired", 2, "HARD_WIRED", "HARD_WIRED", &[S::ImmuneCounterUav, S::ImmuneEmp]),
    perk("Scavenger", 2, "SCAVENGER", "SCAVENGER", &[S::Scavenger]),
    perk("Dexterity", 3, "DEXTERITY", "DEXTERITY", &[S::FastMantle, S::FastLadderClimb, S::SprintRecovery, S::FastMeleeRecovery]),
    perk("ExtremeConditioning", 3, "EXTREME_CONDITIONING", "EXTREME_CONDITIONING", &[S::LongerSprint]),
    perk("Engineer", 3, "HACKER", "HACKER", &[S::ShowEnemyEquipment, S::DelayExplosive]),
    perk("TacticalMask", 3, "TACTICAL_MASK", "TACTICAL_MASK", &[S::StunProtection, S::FlashProtection, S::ProximityProtection]),
    perk("DeadSilence", 3, "DEAD_SILENCE", "NINJA", &[S::Quieter]),
    perk("Awareness", 3, "AWARENESS", "AWARENESS", &[S::LoudEnemies]),
];

/// The converted sets (`statstable.csv` reference -> specialties), when the converted weapon table
/// is installed.
static CONVERTED: Mutex<Option<HashMap<String, Specialties>>> = Mutex::new(None);

/// Install the converted perk rows (the weapon table's `Perk` blocks). Rows for perks the mod
/// does not offer are kept but unused.
pub fn install_converted(rows: &[(String, Specialties)]) {
    let map = rows.iter().map(|(k, v)| (k.to_ascii_uppercase(), *v)).collect();
    *CONVERTED.lock().unwrap_or_else(|e| e.into_inner()) = Some(map);
}

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

    /// The specialties the perk grants: the converted `statstable.csv` row when installed, else
    /// the built-in copy of it.
    pub fn specialties(self) -> Specialties {
        let d = self.def();
        CONVERTED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .and_then(|m| m.get(d.stats).copied())
            .unwrap_or_else(|| Specialties::of(d.specialties))
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
            assert!(!d.specialties.is_empty(), "{} grants something", d.id);
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
}
