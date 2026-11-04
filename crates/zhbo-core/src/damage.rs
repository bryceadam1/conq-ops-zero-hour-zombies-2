/*
** Conquer Ops - Zero Hour gameplay core: BO2's means of death and its damage rules on Zero Hour's
** objects.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! Every hit the sim deals carries BO2's **means of death** (`MOD_*`), chosen where BO2's engine
//! chooses it, and reaches Zero Hour as the `DamageType` the tuning pack's `[MeansOfDeath]` table
//! names for it, so each Zero Hour armour scales it by its own row for that kind
//! (docs/CONTRACT.md, "Damage").
//!
//! * **Bullets** (t6mp `FUN_005cfc60`): `MOD_PISTOL_BULLET` from a pistol, `MOD_RIFLE_BULLET`
//!   otherwise; a soldier struck in the head or helmet takes `MOD_HEAD_SHOT`
//!   (`_globallogic_utils.gsc:356` `isheadshot`, applied at `_globallogic_player.gsc:802`).
//! * **Melee**: `MOD_MELEE`.
//! * **A missile's strike** (`FUN_00608c10`): a dud (`FUN_00604280`) or a def without
//!   `bProjImpactExplode` (a frag, a Semtex, the crossbow's bolt, a ballistic knife, a combat
//!   axe) strikes for `MOD_IMPACT`; an impact fuse's direct hit is its missile handler's direct
//!   means — a grenade's (`FUN_00607730`, handler 10) `MOD_GRENADE`, a rocket's (`FUN_0060d330`,
//!   handler 12) `MOD_PROJECTILE` (the handler table at `0x00c8c0e4`, `+0x14`).
//! * **Radius damage**: the handler's splash means (`+0x18`, `FUN_00604240`): `MOD_GRENADE_SPLASH`,
//!   `MOD_PROJECTILE_SPLASH` (a grenade whose `projExplosion` is `heavy explosive` would be
//!   `MOD_EXPLOSIVE`; no multiplayer weapon is, which the install test holds).
//!
//! On anything but a soldier (BO2's vehicles) the means of death picks BO2's vehicle scalars
//! ([`vehicle_scale`]); on a soldier (BO2's player) it decides the perks (Flak Jacket's
//! `isexplosivedamage`, the shellshock's causes; `sim::guard`).

use crate::tuning::Tuning;
use crate::weapons::{WeapClass, WeapType, WeaponDef};

/// BO2's `meansOfDeath_t`, in t6mp's order (the script strings registered by `FUN_00678440`, the
/// names the callbacks compare against).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MeansOfDeath {
    #[default]
    Unknown,
    PistolBullet,
    RifleBullet,
    Grenade,
    GrenadeSplash,
    Projectile,
    ProjectileSplash,
    Melee,
    Bayonet,
    HeadShot,
    Crush,
    Telefrag,
    Falling,
    Suicide,
    TriggerHurt,
    Explosive,
    Impact,
    Burned,
    HitByObject,
    Drown,
    Gas,
}

/// How many means of death BO2 has (`0x15`: `FUN_0063a1a0` pushes `badMOD` past it).
pub const MOD_COUNT: usize = 21;

impl MeansOfDeath {
    pub const ALL: [MeansOfDeath; MOD_COUNT] = [
        MeansOfDeath::Unknown,
        MeansOfDeath::PistolBullet,
        MeansOfDeath::RifleBullet,
        MeansOfDeath::Grenade,
        MeansOfDeath::GrenadeSplash,
        MeansOfDeath::Projectile,
        MeansOfDeath::ProjectileSplash,
        MeansOfDeath::Melee,
        MeansOfDeath::Bayonet,
        MeansOfDeath::HeadShot,
        MeansOfDeath::Crush,
        MeansOfDeath::Telefrag,
        MeansOfDeath::Falling,
        MeansOfDeath::Suicide,
        MeansOfDeath::TriggerHurt,
        MeansOfDeath::Explosive,
        MeansOfDeath::Impact,
        MeansOfDeath::Burned,
        MeansOfDeath::HitByObject,
        MeansOfDeath::Drown,
        MeansOfDeath::Gas,
    ];

    /// BO2's script name (`MOD_IMPACT`).
    pub fn name(self) -> &'static str {
        match self {
            MeansOfDeath::Unknown => "MOD_UNKNOWN",
            MeansOfDeath::PistolBullet => "MOD_PISTOL_BULLET",
            MeansOfDeath::RifleBullet => "MOD_RIFLE_BULLET",
            MeansOfDeath::Grenade => "MOD_GRENADE",
            MeansOfDeath::GrenadeSplash => "MOD_GRENADE_SPLASH",
            MeansOfDeath::Projectile => "MOD_PROJECTILE",
            MeansOfDeath::ProjectileSplash => "MOD_PROJECTILE_SPLASH",
            MeansOfDeath::Melee => "MOD_MELEE",
            MeansOfDeath::Bayonet => "MOD_BAYONET",
            MeansOfDeath::HeadShot => "MOD_HEAD_SHOT",
            MeansOfDeath::Crush => "MOD_CRUSH",
            MeansOfDeath::Telefrag => "MOD_TELEFRAG",
            MeansOfDeath::Falling => "MOD_FALLING",
            MeansOfDeath::Suicide => "MOD_SUICIDE",
            MeansOfDeath::TriggerHurt => "MOD_TRIGGER_HURT",
            MeansOfDeath::Explosive => "MOD_EXPLOSIVE",
            MeansOfDeath::Impact => "MOD_IMPACT",
            MeansOfDeath::Burned => "MOD_BURNED",
            MeansOfDeath::HitByObject => "MOD_HIT_BY_OBJECT",
            MeansOfDeath::Drown => "MOD_DROWN",
            MeansOfDeath::Gas => "MOD_GAS",
        }
    }

    /// The means of death BO2 calls `name` (case-insensitively).
    pub fn parse(name: &str) -> Option<MeansOfDeath> {
        Self::ALL.into_iter().find(|m| m.name().eq_ignore_ascii_case(name))
    }

    /// `_class.gsc:1058` `isexplosivedamage`'s means (Flak Jacket's): a grenade's and a blast's,
    /// not a rocket's direct hit nor an impact.
    pub fn is_explosive_damage(self) -> bool {
        matches!(self, MeansOfDeath::Explosive | MeansOfDeath::Grenade | MeansOfDeath::GrenadeSplash | MeansOfDeath::ProjectileSplash)
    }

    /// Whether a player killed by this means drops a Scavenger pack (`_globallogic_player.gsc:1913`:
    /// every means but `MOD_GRENADE`, `MOD_GRENADE_SPLASH`, `MOD_EXPLOSIVE`,
    /// `MOD_EXPLOSIVE_SPLASH` — not in t6mp's `meansOfDeath_t` — and `MOD_PROJECTILE_SPLASH`).
    pub fn drops_scavenger(self) -> bool {
        !matches!(self, MeansOfDeath::Grenade | MeansOfDeath::GrenadeSplash | MeansOfDeath::Explosive | MeansOfDeath::ProjectileSplash)
    }

    /// The causes `_shellshock.gsc:19` shakes a player for (`damage > 10`).
    pub fn shellshocks(self) -> bool {
        matches!(self, MeansOfDeath::Explosive | MeansOfDeath::Grenade | MeansOfDeath::GrenadeSplash | MeansOfDeath::Projectile | MeansOfDeath::ProjectileSplash)
    }

    /// Radius damage (`callback_vehicleradiusdamage`'s splash means, `_globallogic_vehicle.gsc:224`).
    pub fn is_splash(self) -> bool {
        matches!(self, MeansOfDeath::ProjectileSplash | MeansOfDeath::GrenadeSplash | MeansOfDeath::Explosive)
    }

    /// A bullet (`_class.gsc` `isprimarydamage`, and the headshot).
    pub fn is_bullet(self) -> bool {
        matches!(self, MeansOfDeath::PistolBullet | MeansOfDeath::RifleBullet | MeansOfDeath::HeadShot)
    }
}

/// Zero Hour's `DamageType`s by their INI names (`DamageTypeFlags::s_bitNameList`, Zero Hour's
/// build), in the order of the ABI's codes (`ZHBO_DAMAGE_*` in `host/zhbo/zhbo_core.h`): the
/// three the ABI has carried since its start first, then the rest in Zero Hour's order. The bridge
/// maps each code to its `DamageType` by name (`ZhboLogicApi.cpp`).
pub const ZH_DAMAGE_TYPES: [&str; 38] = [
    "SMALL_ARMS",
    "MELEE",
    "EXPLOSION",
    "CRUSH",
    "ARMOR_PIERCING",
    "GATTLING",
    "RADIATION",
    "FLAME",
    "LASER",
    "SNIPER",
    "POISON",
    "HEALING",
    "UNRESISTABLE",
    "WATER",
    "DEPLOY",
    "SURRENDER",
    "HACK",
    "KILL_PILOT",
    "PENALTY",
    "FALLING",
    "DISARM",
    "HAZARD_CLEANUP",
    "PARTICLE_BEAM",
    "TOPPLING",
    "INFANTRY_MISSILE",
    "AURORA_BOMB",
    "LAND_MINE",
    "JET_MISSILES",
    "STEALTHJET_MISSILES",
    "MOLOTOV_COCKTAIL",
    "COMANCHE_VULCAN",
    "SUBDUAL_MISSILE",
    "SUBDUAL_VEHICLE",
    "SUBDUAL_BUILDING",
    "SUBDUAL_UNRESISTABLE",
    "MICROWAVE",
    "KILL_GARRISONED",
    "STATUS",
];

/// The ABI code of the Zero Hour damage type `name` (case-insensitively).
pub fn zh_code(name: &str) -> Option<u8> {
    ZH_DAMAGE_TYPES.iter().position(|n| n.eq_ignore_ascii_case(name)).map(|i| i as u8)
}

/// The tuning pack's `[MeansOfDeath]` table: the Zero Hour damage type (its ABI code,
/// [`ZH_DAMAGE_TYPES`]) each BO2 means of death reaches the engine as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeansOfDeathTable {
    codes: [u8; MOD_COUNT],
}

impl Default for MeansOfDeathTable {
    /// The mod's defaults (the shipped `tuning.ini` must equal them). The armour row each kind
    /// picks stands in for BO2's per-means treatment of its vehicles and players:
    ///
    /// * bullets are cut down on BO2's vehicles (the AI tank x0.3, `_ai_tank.gsc:288-299`; the
    ///   helicopter's `heli_armor_bulletdamage`; the turrets' `bulletdamagereduction`) and taken
    ///   in full by its players: Zero Hour's `SMALL_ARMS` (tank armour 25 %, infantry 100 %);
    /// * a blast, a grenade's and a rocket's direct hit are taken in full or more by every BO2
    ///   vehicle (`_ai_tank.gsc:302-303` x1.5, the turrets' `explosivedamagereduction`, the
    ///   callback's own scalars, [`vehicle_scale`]): `EXPLOSION`, which every vehicle, aircraft
    ///   and structure armour and the soldiers take at 70-130 %;
    /// * an impact (a dud, a thrown or stuck piece striking) has no rule in
    ///   `callback_vehicledamage` and falls to every killstreak's full-damage branch
    ///   (`_turret_killstreak.gsc:922`, the helicopter's and the drones' defaults): `EXPLOSION`
    ///   too, the kind Zero Hour's vehicle armours take in full (`SMALL_ARMS` would cut a rocket's
    ///   dud on a tank to a quarter);
    /// * a blow is the one BO2's turrets ignore (`_turret_killstreak.gsc:907-912`): `MELEE`,
    ///   which Zero Hour's vehicles and structures shrug off and its soldiers take in full;
    /// * the rest (no weapon of the mod deals them) by their Zero Hour namesakes: crushing,
    ///   falling, burning, drowning, gas; the script's unconditional kills `UNRESISTABLE`.
    fn default() -> Self {
        use MeansOfDeath::*;
        let pick = |m: MeansOfDeath| match m {
            PistolBullet | RifleBullet | HeadShot => "SMALL_ARMS",
            Grenade | GrenadeSplash | Projectile | ProjectileSplash | Explosive | Impact => "EXPLOSION",
            Melee | Bayonet => "MELEE",
            Crush | HitByObject => "CRUSH",
            Falling => "FALLING",
            Burned => "FLAME",
            Drown => "WATER",
            Gas => "POISON",
            Unknown | Telefrag | Suicide | TriggerHurt => "UNRESISTABLE",
        };
        let mut codes = [0; MOD_COUNT];
        for m in MeansOfDeath::ALL {
            codes[m as usize] = zh_code(pick(m)).expect("a Zero Hour damage type");
        }
        MeansOfDeathTable { codes }
    }
}

impl MeansOfDeathTable {
    /// The ABI code `m` reaches Zero Hour as.
    pub fn code(&self, m: MeansOfDeath) -> i32 {
        self.codes[m as usize] as i32
    }

    /// The Zero Hour damage type's name `m` reaches Zero Hour as.
    pub fn zh_name(&self, m: MeansOfDeath) -> &'static str {
        ZH_DAMAGE_TYPES[self.codes[m as usize] as usize]
    }

    /// Set `m`'s Zero Hour damage type by name; false (and nothing set) for a name Zero Hour lacks.
    pub fn set(&mut self, m: MeansOfDeath, zh: &str) -> bool {
        match zh_code(zh) {
            Some(c) => {
                self.codes[m as usize] = c;
                true
            }
            None => false,
        }
    }

    /// Read the `MOD_* = <Zero Hour damage type>` lines of a tuning file over `self` (other lines
    /// and unknown names are ignored, as the pack's unknown keys are).
    pub fn parse_over(&mut self, text: &str) {
        for line in text.lines() {
            let Some((k, v)) = line.split(';').next().unwrap_or("").split_once('=') else { continue };
            if let Some(m) = MeansOfDeath::parse(k.trim()) {
                self.set(m, v.trim());
            }
        }
    }

    /// Every means of death with its Zero Hour damage type's name, in BO2's order.
    pub fn entries(&self) -> impl Iterator<Item = (MeansOfDeath, &'static str)> + '_ {
        MeansOfDeath::ALL.into_iter().map(|m| (m, self.zh_name(m)))
    }
}

/// The means of death of `def`'s bullet on a soldier (`soldier`) struck at `location`, or anything
/// else: BO2's bullet fire picks pistol or rifle by the weapon's class, and the player damage
/// callback renames a head or helmet hit `MOD_HEAD_SHOT`.
pub fn bullet(def: &WeaponDef, soldier: bool, location: u8) -> MeansOfDeath {
    if soldier && crate::hitloc::is_head(location) {
        MeansOfDeath::HeadShot
    } else if matches!(def.weap_class, WeapClass::Pistol | WeapClass::PistolSpread) {
        MeansOfDeath::PistolBullet
    } else {
        MeansOfDeath::RifleBullet
    }
}

/// The direct means of `def`'s missile handler: a grenade's (`weapType` grenade) `MOD_GRENADE`,
/// anything launched `MOD_PROJECTILE` (t6mp's handler table, `+0x14`).
pub fn direct(def: &WeaponDef) -> MeansOfDeath {
    if def.traits.weap_type == WeapType::Grenade {
        MeansOfDeath::Grenade
    } else {
        MeansOfDeath::Projectile
    }
}

/// The means of `def`'s radius damage (its handler's, `+0x18`, through `FUN_00604240`).
pub fn splash(def: &WeaponDef) -> MeansOfDeath {
    if def.traits.weap_type == WeapType::Grenade {
        MeansOfDeath::GrenadeSplash
    } else {
        MeansOfDeath::ProjectileSplash
    }
}

/// The means of `def`'s missile striking something (`FUN_00608c10`): `MOD_IMPACT` for a dud or a
/// def that does not go off on impact, else its handler's direct means.
pub fn strike(def: &WeaponDef, dud: bool) -> MeansOfDeath {
    if dud || !def.impact_explode {
        MeansOfDeath::Impact
    } else {
        direct(def)
    }
}

/// BO2's scale on damage `m` from `weapon` (its name) to a vehicle — what the mod's Zero Hour
/// vehicles, aircraft and structures take (`_globallogic_vehicle.gsc`):
///
/// * radius damage (`MOD_PROJECTILE_SPLASH`, `MOD_GRENADE_SPLASH`, `MOD_EXPLOSIVE`) first passes
///   `callback_vehicleradiusdamage`: x `getvehicleprojectilesplashscalar` (`:224-228`);
/// * then every hit passes `callback_vehicledamage` (the radius damage through
///   `finishvehicleradiusdamage`, t6mp `FUN_0066a3e0` -> `G_Damage` -> `FUN_005f0810`):
///   `MOD_PROJECTILE`/`MOD_GRENADE` x `getvehicleprojectilescalar` (`:56-62`), `MOD_GRENADE_SPLASH`
///   x `getvehicleunderneathsplashscalar` (`:63-68`, C4's 30), and everything x
///   `level.vehicledamagescalar` (`:70`).
pub fn vehicle_scale(t: &Tuning, weapon: &str, m: MeansOfDeath) -> f32 {
    use crate::tuning::{VEHICLE_PROJECTILE_SCALAR, VEHICLE_PROJECTILE_SPLASH_SCALAR, VEHICLE_UNDERNEATH_SPLASH_SCALAR};
    let radius = if m.is_splash() { t.per_weapon(VEHICLE_PROJECTILE_SPLASH_SCALAR, weapon) } else { 1.0 };
    let callback = match m {
        MeansOfDeath::Projectile | MeansOfDeath::Grenade => t.per_weapon(VEHICLE_PROJECTILE_SCALAR, weapon),
        MeansOfDeath::GrenadeSplash => t.per_weapon(VEHICLE_UNDERNEATH_SPLASH_SCALAR, weapon),
        _ => 1.0,
    };
    radius * callback * t.vehicle_damage_scalar
}

/// [`vehicle_scale`] for a victim: 1 on a soldier (BO2's player), BO2's vehicle rule on
/// anything else.
pub fn scale_on(t: &Tuning, weapon: &str, m: MeansOfDeath, victim: &crate::host::ObjInfo) -> f32 {
    if victim.has(crate::host::objflags::INFANTRY) {
        1.0
    } else {
        vehicle_scale(t, weapon, m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_means_of_death_are_bo2s_in_its_order() {
        // t6mp FUN_00678440 registers them in this order; the callbacks index the table by it.
        let names: Vec<&str> = MeansOfDeath::ALL.iter().map(|m| m.name()).collect();
        assert_eq!(names[0], "MOD_UNKNOWN");
        assert_eq!(names[5], "MOD_PROJECTILE");
        assert_eq!(names[16], "MOD_IMPACT");
        assert_eq!(names[20], "MOD_GAS");
        for (i, m) in MeansOfDeath::ALL.into_iter().enumerate() {
            assert_eq!(m as usize, i);
            assert_eq!(MeansOfDeath::parse(m.name()), Some(m));
        }
    }

    #[test]
    fn every_means_of_death_reaches_zero_hour_as_a_damage_type_it_has() {
        let t = MeansOfDeathTable::default();
        for (m, zh) in t.entries() {
            assert!(zh_code(zh).is_some(), "{} -> {zh}", m.name());
        }
        // The report's case: a strike that is not a blast meets a tank's armour as a blast does,
        // not as a rifle round (TankArmor's SMALL_ARMS 25 %).
        assert_eq!(t.zh_name(MeansOfDeath::Impact), "EXPLOSION");
        assert_eq!(t.zh_name(MeansOfDeath::Projectile), "EXPLOSION");
        assert_eq!(t.zh_name(MeansOfDeath::RifleBullet), "SMALL_ARMS");
        assert_eq!(t.zh_name(MeansOfDeath::HeadShot), "SMALL_ARMS");
        assert_eq!(t.zh_name(MeansOfDeath::Melee), "MELEE");
    }

    #[test]
    fn the_abi_codes_keep_the_first_three_kinds() {
        use crate::host::damage_kind;
        assert_eq!(zh_code("SMALL_ARMS"), Some(damage_kind::SMALL_ARMS as u8));
        assert_eq!(zh_code("MELEE"), Some(damage_kind::MELEE as u8));
        assert_eq!(zh_code("EXPLOSION"), Some(damage_kind::EXPLOSION as u8));
    }

    /// `host/zhbo/zhbo_core.h` defines every code by its Zero Hour name, and the bridge's map has
    /// one `DamageType` per code (the static assert there counts them).
    #[test]
    fn the_header_names_every_code() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let header = std::fs::read_to_string(root.join("../../host/zhbo/zhbo_core.h")).expect("host/zhbo/zhbo_core.h is checked in");
        for (i, name) in ZH_DAMAGE_TYPES.iter().enumerate() {
            let line = format!("#define ZHBO_DAMAGE_{name} {i}");
            assert!(header.lines().any(|l| l.trim() == line), "the header has `{line}`");
        }
        let count = format!("#define ZHBO_DAMAGE_COUNT {}", ZH_DAMAGE_TYPES.len());
        assert!(header.lines().any(|l| l.trim() == count), "the header has `{count}`");
        let bridge = std::fs::read_to_string(root.join("../../host/zhbo/ZhboLogicApi.cpp")).expect("the bridge is checked in");
        for name in ZH_DAMAGE_TYPES {
            assert!(bridge.contains(&format!("DAMAGE_{} /* ZHBO_DAMAGE_{name} */", name.replace("KILL_PILOT", "KILLPILOT"))), "the bridge maps {name}");
        }
    }

    #[test]
    fn the_table_reads_the_packs_lines() {
        let mut t = MeansOfDeathTable::default();
        t.parse_over("[MeansOfDeath]\nMOD_IMPACT = SMALL_ARMS ; a comment\nmod_melee=explosion\nMOD_GAS = NOT_A_TYPE\ng_speed = 190");
        assert_eq!(t.zh_name(MeansOfDeath::Impact), "SMALL_ARMS");
        assert_eq!(t.zh_name(MeansOfDeath::Melee), "EXPLOSION");
        assert_eq!(t.zh_name(MeansOfDeath::Gas), "POISON", "an unknown name keeps the default");
    }

    fn def(weap_type: WeapType, impact_explode: bool) -> WeaponDef {
        let mut d = WeaponDef { impact_explode, ..Default::default() };
        d.traits.weap_type = weap_type;
        d
    }

    #[test]
    fn a_strike_is_an_impact_unless_it_goes_off_on_impact() {
        // FUN_00608c10: a frag off a head, a Semtex sticking, a bolt or an axe: MOD_IMPACT.
        assert_eq!(strike(&def(WeapType::Grenade, false), false), MeansOfDeath::Impact);
        assert_eq!(strike(&def(WeapType::Projectile, false), false), MeansOfDeath::Impact);
        // A rocket's direct hit: its handler's direct means; a dud: an impact.
        assert_eq!(strike(&def(WeapType::Projectile, true), false), MeansOfDeath::Projectile);
        assert_eq!(strike(&def(WeapType::Projectile, true), true), MeansOfDeath::Impact);
        assert_eq!(strike(&def(WeapType::Grenade, true), false), MeansOfDeath::Grenade);
        assert_eq!(splash(&def(WeapType::Grenade, false)), MeansOfDeath::GrenadeSplash);
        assert_eq!(splash(&def(WeapType::Projectile, true)), MeansOfDeath::ProjectileSplash);
    }

    #[test]
    fn bullets_are_pistol_or_rifle_and_a_soldiers_head_a_headshot() {
        let mut d = WeaponDef::default();
        assert_eq!(bullet(&d, true, crate::hitloc::TORSO_UPPER), MeansOfDeath::RifleBullet);
        assert_eq!(bullet(&d, true, crate::hitloc::HELMET), MeansOfDeath::HeadShot);
        assert_eq!(bullet(&d, false, crate::hitloc::HEAD), MeansOfDeath::RifleBullet, "only a soldier has a head");
        d.weap_class = WeapClass::Pistol;
        assert_eq!(bullet(&d, true, crate::hitloc::NECK), MeansOfDeath::PistolBullet);
    }

    #[test]
    fn bo2s_vehicle_scalars_by_means_of_death() {
        let t = Tuning::default();
        use MeansOfDeath::*;
        // getvehicleprojectilescalar: only a direct hit.
        assert_eq!(vehicle_scale(&t, "smaw_mp", Projectile), 0.2);
        assert_eq!(vehicle_scale(&t, "smaw_mp", Impact), 1.0, "a dud is not MOD_PROJECTILE");
        assert_eq!(vehicle_scale(&t, "smaw_mp", ProjectileSplash), 1.0);
        // getvehicleunderneathsplashscalar: C4's blast x30 on any vehicle it reaches.
        assert_eq!(vehicle_scale(&t, "satchel_charge_mp", GrenadeSplash), 30.0);
        assert_eq!(vehicle_scale(&t, "satchel_charge_mp", Grenade), 1.0, "stuck to it: its direct means");
        assert_eq!(vehicle_scale(&t, "frag_grenade_mp", GrenadeSplash), 1.0);
        assert_eq!(vehicle_scale(&t, "an94_mp", RifleBullet), 1.0);
    }
}
