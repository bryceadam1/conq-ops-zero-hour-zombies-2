/*
** Conquer Ops - Zero Hour gameplay core: BO2's specialties, the bits perks and weapons grant.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! BO2's engine never sees perk names: a perk is a set of `specialty_*` bits (`mp/statstable.csv`
//! column e), and each system tests one bit (Ghidra G1, `~/Documents/BO2-reference/ghidra/
//! answers/G1.md`). A player has a specialty when the class grants it, **or** the weapon in hand
//! grants it (`WeaponVariantDef.perks`), **or** one of that weapon's attachments does
//! (`WeaponAttachment.perks`) — which is how fast mag (`fastreload`), quickdraw (`fastads`), the
//! laser (`bulletaccuracy`) and rapid fire (`rof`) work. The indices are the engine's: the
//! 62-entry name table at `t6mp` `0x00f6b918`, alphabetical, cross-checked at eight consumers.

macro_rules! specialties {
    ($($v:ident = $name:literal,)*) => {
        /// One `specialty_*` bit, by the engine's index.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[repr(u8)]
        pub enum Specialty { $($v,)* }

        impl Specialty {
            /// Every specialty, in index order.
            pub const ALL: &'static [Specialty] = &[$(Specialty::$v,)*];

            /// The name after `specialty_` (`movefaster`).
            pub const fn name(self) -> &'static str {
                match self { $(Specialty::$v => $name,)* }
            }
        }
    };
}

specialties! {
    AdditionalPrimaryWeapon = "additionalprimaryweapon",
    ArmorPiercing = "armorpiercing",
    ArmorVest = "armorvest",
    BulletAccuracy = "bulletaccuracy",
    BulletDamage = "bulletdamage",
    BulletFlinch = "bulletflinch",
    BulletPenetration = "bulletpenetration",
    Deadshot = "deadshot",
    DelayExplosive = "delayexplosive",
    DetectExplosive = "detectexplosive",
    DisarmExplosive = "disarmexplosive",
    EarnMoreMomentum = "earnmoremomentum",
    ExplosiveDamage = "explosivedamage",
    ExtraAmmo = "extraammo",
    FallHeight = "fallheight",
    FastAds = "fastads",
    FastEquipmentUse = "fastequipmentuse",
    FastLadderClimb = "fastladderclimb",
    FastMantle = "fastmantle",
    FastMeleeRecovery = "fastmeleerecovery",
    FastReload = "fastreload",
    FastToss = "fasttoss",
    FastWeaponSwitch = "fastweaponswitch",
    FinalStand = "finalstand",
    Fireproof = "fireproof",
    FlakJacket = "flakjacket",
    FlashProtection = "flashprotection",
    GpsJammer = "gpsjammer",
    GrenadePullDeath = "grenadepulldeath",
    HealthRegen = "healthregen",
    HoldBreath = "holdbreath",
    ImmuneCounterUav = "immunecounteruav",
    ImmuneEmp = "immuneemp",
    ImmuneMms = "immunemms",
    ImmuneNvThermal = "immunenvthermal",
    ImmuneRangefinder = "immunerangefinder",
    Killstreak = "killstreak",
    LongerSprint = "longersprint",
    LoudEnemies = "loudenemies",
    Marksman = "marksman",
    MoveFaster = "movefaster",
    NoMotionSensor = "nomotionsensor",
    NoName = "noname",
    NotTargetedByAirSupport = "nottargetedbyairsupport",
    NoKillstreakReticle = "nokillstreakreticle",
    NotTargettedBySentry = "nottargettedbysentry",
    PinBack = "pin_back",
    PistolDeath = "pistoldeath",
    ProximityProtection = "proximityprotection",
    QuickRevive = "quickrevive",
    Quieter = "quieter",
    Reconnaissance = "reconnaissance",
    Rof = "rof",
    Scavenger = "scavenger",
    ShowEnemyEquipment = "showenemyequipment",
    StunProtection = "stunprotection",
    Shellshock = "shellshock",
    SprintRecovery = "sprintrecovery",
    ShowOnRadar = "showonradar",
    Stalker = "stalker",
    TwoGrenades = "twogrenades",
    TwoPrimaries = "twoprimaries",
}

impl Specialty {
    /// The engine's bit index.
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// By name, with or without the `specialty_` prefix.
    pub fn parse(s: &str) -> Option<Specialty> {
        let s = s.trim();
        let s = s.strip_prefix("specialty_").unwrap_or(s);
        Specialty::ALL.iter().copied().find(|v| v.name() == s)
    }
}

/// A set of specialties: the engine's two 32-bit perk words (`perks[2]`) as one 64-bit set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Specialties(pub u64);

impl Specialties {
    pub const NONE: Specialties = Specialties(0);

    /// From the engine's two perk words (`WeaponVariantDef.perks`, `WeaponAttachment.perks`).
    pub const fn from_words(lo: u32, hi: u32) -> Specialties {
        Specialties(lo as u64 | (hi as u64) << 32)
    }

    pub fn of(list: &[Specialty]) -> Specialties {
        list.iter().fold(Specialties::NONE, |s, v| s.with(*v))
    }

    pub const fn with(self, s: Specialty) -> Specialties {
        Specialties(self.0 | 1 << s.index())
    }

    pub const fn has(self, s: Specialty) -> bool {
        self.0 & (1 << s.index()) != 0
    }

    pub const fn union(self, other: Specialties) -> Specialties {
        Specialties(self.0 | other.0)
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The set's members, in index order. Bits without a name (index 62, the engine's unnamed
    /// "unlimited sprint" test) are not listed.
    pub fn iter(self) -> impl Iterator<Item = Specialty> {
        Specialty::ALL.iter().copied().filter(move |s| self.has(*s))
    }

    /// Space-separated names, the weapon table's format (`fastreload rof`).
    pub fn format(self) -> String {
        self.iter().map(Specialty::name).collect::<Vec<_>>().join(" ")
    }

    /// The table format (`|` separators, as `statstable.csv` writes them, are accepted too).
    pub fn parse(s: &str) -> Result<Specialties, String> {
        s.split(|c: char| c.is_whitespace() || c == '|')
            .filter(|w| !w.is_empty())
            .try_fold(Specialties::NONE, |set, w| Specialty::parse(w).map(|v| set.with(v)).ok_or_else(|| format!("unknown specialty {w}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indices_are_the_engines() {
        // G1's cross-checked consumers.
        assert_eq!(Specialty::ALL.len(), 62);
        assert_eq!(Specialty::BulletAccuracy.index(), 3);
        assert_eq!(Specialty::BulletFlinch.index(), 5);
        assert_eq!(Specialty::FastAds.index(), 15);
        assert_eq!(Specialty::FastLadderClimb.index(), 17);
        assert_eq!(Specialty::FastMantle.index(), 18);
        assert_eq!(Specialty::LongerSprint.index(), 37);
        assert_eq!(Specialty::MoveFaster.index(), 40);
        assert_eq!(Specialty::SprintRecovery.index(), 57);
        assert_eq!(Specialty::TwoPrimaries.index(), 61);
    }

    #[test]
    fn sets_round_trip_through_both_formats() {
        let s = Specialties::parse("specialty_movefaster|specialty_fallheight").unwrap();
        assert!(s.has(Specialty::MoveFaster) && s.has(Specialty::FallHeight) && !s.has(Specialty::Rof));
        assert_eq!(Specialties::parse(&s.format()).unwrap(), s);
        assert_eq!(Specialties::from_words(1 << 20, 0), Specialties::of(&[Specialty::FastReload]));
        assert_eq!(Specialties::from_words(0, 1 << (40 - 32)), Specialties::of(&[Specialty::MoveFaster]));
        assert!(Specialties::parse("specialty_nonsense").is_err());
        assert_eq!(Specialties::parse("").unwrap(), Specialties::NONE);
    }
}
