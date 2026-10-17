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

/// BO2's multiplayer perks (Pick 10). Effects that need systems Conquer Ops does not have yet
/// (UAV, targeting, AI support) are carried but do nothing.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Perk {
    // Tier 1
    Lightweight,
    Hardline,
    BlindEye,
    FlakJacket,
    Ghost,
    // Tier 2
    Toughness,
    ColdBlooded,
    FastHands,
    HardWired,
    Scavenger,
    // Tier 3
    Dexterity,
    ExtremeConditioning,
    Engineer,
    TacticalMask,
    DeadSilence,
    Awareness,
}

impl Perk {
    pub const ALL: [Perk; 16] = [
        Perk::Lightweight,
        Perk::Hardline,
        Perk::BlindEye,
        Perk::FlakJacket,
        Perk::Ghost,
        Perk::Toughness,
        Perk::ColdBlooded,
        Perk::FastHands,
        Perk::HardWired,
        Perk::Scavenger,
        Perk::Dexterity,
        Perk::ExtremeConditioning,
        Perk::Engineer,
        Perk::TacticalMask,
        Perk::DeadSilence,
        Perk::Awareness,
    ];

    /// BO2's localize key for the perk's name (`PERKS_<key>`).
    pub fn key(self) -> &'static str {
        match self {
            Perk::Lightweight => "LIGHTWEIGHT",
            Perk::Hardline => "HARDLINE",
            Perk::BlindEye => "BLIND_EYE",
            Perk::FlakJacket => "FLAK_JACKET",
            Perk::Ghost => "GHOST",
            Perk::Toughness => "TOUGHNESS",
            Perk::ColdBlooded => "COLD_BLOODED",
            Perk::FastHands => "FAST_HANDS",
            Perk::HardWired => "HARD_WIRED",
            Perk::Scavenger => "SCAVENGER",
            Perk::Dexterity => "DEXTERITY",
            Perk::ExtremeConditioning => "EXTREME_CONDITIONING",
            Perk::Engineer => "HACKER",
            Perk::TacticalMask => "TACTICAL_MASK",
            Perk::DeadSilence => "DEAD_SILENCE",
            Perk::Awareness => "AWARENESS",
        }
    }
}

/// Perk tuning (BO2's engine defaults as documented by the community; BO2 sets them in script).
pub mod perk_values {
    /// Lightweight: "Move faster."
    pub const LIGHTWEIGHT_SPEED: f32 = 1.07;
    /// Extreme Conditioning: "Sprint for a longer duration."
    pub const EXTREME_CONDITIONING_SPRINT: f32 = 2.0;
    /// Dexterity: "Aim faster after sprinting."
    pub const DEXTERITY_SPRINT_OUT: f32 = 0.5;
    /// Flak Jacket: "Take less explosive damage."
    pub const FLAK_JACKET_EXPLOSIVE: f32 = 0.65;
    /// Toughness: "Flinch less when shot."
    pub const TOUGHNESS_FLINCH: f32 = 0.25;
    /// Tactical Mask: "Reduce the effects of Flashbangs, Concussion Grenades, and Shock Charges."
    pub const TACTICAL_MASK_STUN: f32 = 0.25;
}