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

/// One blow as BO2's damage call takes it (`G_Damage`'s `mod`, `weapon` and `vDir`): the means of
/// death, the weapon that dealt it, and the direction it travelled (zero: none). The armour and the
/// sim's rules read the means; the weapon and the direction are the victim's death animation's
/// (`crate::death`, presentation only, read off `EventKind::KilledBy`).
#[derive(Clone, Copy, Debug)]
pub struct Blow {
    pub how: MeansOfDeath,
    pub weapon: Option<&'static WeaponDef>,
    pub dir: crate::math::Vec3,
}

impl PartialEq for Blow {
    /// The same means, the same weapon (by its id) and direction.
    fn eq(&self, o: &Blow) -> bool {
        self.how == o.how && self.weapon.map(|w| w.id.as_str()) == o.weapon.map(|w| w.id.as_str()) && self.dir == o.dir
    }
}

impl Blow {
    /// A blow by `weapon` travelling along `dir`.
    pub fn new(how: MeansOfDeath, weapon: &'static WeaponDef, dir: crate::math::Vec3) -> Blow {
        Blow { how, weapon: Some(weapon), dir }
    }

    /// A blow by `weapon` from `from` into a victim at `to` (BO2's radius damage: `vDir` is the
    /// victim's origin less the blast's).
    pub fn from_to(how: MeansOfDeath, weapon: &'static WeaponDef, from: crate::math::Vec3, to: crate::math::Vec3) -> Blow {
        Blow::new(how, weapon, to - from)
    }

    /// A blow by no weapon, from no direction (tests, and hosts that read only the means).
    pub fn bare(how: MeansOfDeath) -> Blow {
        Blow { how, weapon: None, dir: crate::math::Vec3::ZERO }
    }
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

/// The tuning pack's `[ZeroHourDamage]` table: the BO2 means of death each Zero Hour damage type
/// ([`ZH_DAMAGE_TYPES`]) stands for when Zero Hour's own weapons hurt a General (the engine's
/// damage hook, `Sim::engine_blow`): what his death animation and hurt read (`crate::death`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZhDamageTable {
    mods: [MeansOfDeath; ZH_DAMAGE_TYPES.len()],
}

impl Default for ZhDamageTable {
    /// The mod's defaults (the shipped `tuning.ini` must equal them), each Zero Hour type by what it
    /// is in BO2's terms:
    ///
    /// * rounds (`SMALL_ARMS`, `GATTLING`, `SNIPER`, `COMANCHE_VULCAN`): `MOD_RIFLE_BULLET`;
    /// * a blow (`MELEE`): `MOD_MELEE`;
    /// * blasts and shells (`EXPLOSION`, `ARMOR_PIERCING`, `AURORA_BOMB`, `LAND_MINE`):
    ///   `MOD_EXPLOSIVE`; missiles (`INFANTRY_MISSILE`, `JET_MISSILES`, `STEALTHJET_MISSILES`):
    ///   `MOD_PROJECTILE_SPLASH`;
    /// * crushing and a toppled structure: `MOD_CRUSH`; falling: `MOD_FALLING`;
    /// * fire, lasers, the particle beam and the microwave: `MOD_BURNED`; radiation and poison:
    ///   `MOD_GAS`; water: `MOD_DROWN`;
    /// * Zero Hour's unconditional kill (`UNRESISTABLE`): `MOD_TRIGGER_HURT` (BO2's world kill);
    /// * what does no harm to a soldier's health or is not a weapon (healing, the subdual types,
    ///   capture, hacking, disarming, deploying, penalties, status, the pilot's and the garrison's
    ///   kills): `MOD_UNKNOWN`.
    fn default() -> Self {
        use MeansOfDeath::*;
        let pick = |zh: &str| match zh {
            "SMALL_ARMS" | "GATTLING" | "SNIPER" | "COMANCHE_VULCAN" => RifleBullet,
            "MELEE" => Melee,
            "EXPLOSION" | "ARMOR_PIERCING" | "AURORA_BOMB" | "LAND_MINE" => Explosive,
            "INFANTRY_MISSILE" | "JET_MISSILES" | "STEALTHJET_MISSILES" => ProjectileSplash,
            "CRUSH" | "TOPPLING" => Crush,
            "FALLING" => Falling,
            "FLAME" | "MOLOTOV_COCKTAIL" | "LASER" | "PARTICLE_BEAM" | "MICROWAVE" => Burned,
            "RADIATION" | "POISON" => Gas,
            "WATER" => Drown,
            "UNRESISTABLE" => TriggerHurt,
            _ => Unknown,
        };
        ZhDamageTable { mods: std::array::from_fn(|i| pick(ZH_DAMAGE_TYPES[i])) }
    }
}

impl ZhDamageTable {
    /// The means of death Zero Hour's damage of ABI kind `kind` stands for (`MOD_UNKNOWN` for a
    /// kind out of range).
    pub fn means(&self, kind: i32) -> MeansOfDeath {
        usize::try_from(kind).ok().and_then(|k| self.mods.get(k)).copied().unwrap_or(MeansOfDeath::Unknown)
    }

    /// Read the `<Zero Hour damage type> = MOD_*` lines of a tuning file over `self` (other lines
    /// and unknown names are ignored).
    pub fn parse_over(&mut self, text: &str) {
        for line in text.lines() {
            let Some((k, v)) = line.split(';').next().unwrap_or("").split_once('=') else { continue };
            if let (Some(code), Some(m)) = (zh_code(k.trim()), MeansOfDeath::parse(v.trim())) {
                self.mods[code as usize] = m;
            }
        }
    }

    /// Every Zero Hour damage type's name with its means of death, in Zero Hour's order.
    pub fn entries(&self) -> impl Iterator<Item = (&'static str, MeansOfDeath)> + '_ {
        ZH_DAMAGE_TYPES.iter().copied().zip(self.mods.iter().copied())
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

/// The hit alert BO2 plays the attacker for a hit (`_damagefeedback.gsc` `updatedamagefeedback`):
/// none for `MOD_CRUSH`, `MOD_GRENADE_SPLASH` and `MOD_HIT_BY_OBJECT` or a call with no means at
/// all (`_weaponobjects.gsc:652`, a broken piece of equipment); `mpl_hit_alert` when the inflictor
/// is a player (`soundmod "player"`, `_battlechatter_mp.gsc:164`); the Shock Charge's
/// `mpl_hit_alert_taser_spike` (`_proximity_grenade.gsc:172-177`), which goes through
/// `playhitsound` and so plays at most once per 0.05 s; and `mpl_hit_alert_low` for an inflictor
/// with no `soundmod` (a grenade, a rocket, a bolt). Presentation only: the client plays it
/// ([`crate::client`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HitAlert {
    #[default]
    Player,
    Low,
    TaserSpike,
    Silent,
}

impl HitAlert {
    /// The means `updatedamagefeedback` plays nothing for.
    fn silent(how: MeansOfDeath) -> bool {
        matches!(how, MeansOfDeath::Crush | MeansOfDeath::GrenadeSplash | MeansOfDeath::HitByObject)
    }

    /// A hit dealt by the player himself (a round, a melee strike): his `soundmod`.
    pub fn by_player(how: MeansOfDeath) -> HitAlert {
        if Self::silent(how) { HitAlert::Silent } else { HitAlert::Player }
    }

    /// A hit dealt by an entity of his with no `soundmod` (a grenade, a rocket, a piece of
    /// equipment), or the Shock Charge's (`taser`).
    pub fn by_entity(how: MeansOfDeath, taser: bool) -> HitAlert {
        if Self::silent(how) {
            HitAlert::Silent
        } else if taser {
            HitAlert::TaserSpike
        } else {
            HitAlert::Low
        }
    }
}

/// The hit marker's perk variant (`_globallogic_player.gsc:1166` `doperkfeedback`): the victim's
/// Tactical Mask took a flash or stun (`damage_feedback_tac`), or his Flak Jacket an explosion that
/// is not an AI killstreak's (`damage_feedback_flak`). Presentation only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerkFeedback {
    FlakJacket,
    TacticalMask,
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

/// BO2's damage as its player's damage callback receives it: an integer. The engine truncates
/// every hit's damage before it reaches a player — the range curve (t6mp `FUN_008edb80` returns
/// `(int)`), the bullet's multiplier (`FUN_005cfc60`: `(int)(damage * mult)`), the hit location's
/// (`FUN_005f05c0` via `FUN_00a24ad0`), the radius damage (`FUN_005f2130`: `(int)` at each
/// entity) — and a hit under 1 does nothing (`FUN_005f05c0` returns for `iDamage < 1`). Whole
/// numbers pass unchanged.
pub fn idamage(bo2: f32) -> f32 {
    let d = bo2.trunc();
    if d >= 1.0 { d } else { 0.0 }
}

/// The damage callback's own rounding after its perks scaled a hit (Flak Jacket's
/// `cac_modified_damage`): `int(idamage)` and at least 1 (`_globallogic_player.gsc:779`,
/// `:1026-1027`).
pub fn callback_idamage(bo2: f32) -> f32 {
    bo2.trunc().max(1.0)
}

/// The Zero Hour damage a BO2 hit of `bo2` (BO2 health, after BO2's own rules for the victim)
/// deals `victim`:
///
/// * a **General** is BO2's player: he loses the hit's share of BO2's `playerMaxHealth` of his
///   own Zero Hour health, the hit first rounded as BO2's callback receives it ([`idamage`]).
///   Every BO2 weapon kills him in BO2's own shots whatever his Zero Hour health is (the converter
///   sets it as tough as Zero Hour's heroes, the tuning pack's `GeneralHeroToughness`), his
///   veterancy included;
/// * anything else (Zero Hour's infantry, vehicles, aircraft and structures) takes it times
///   [`crate::units::BO2_TO_ZH_DAMAGE`], the mod's exchange rate between BO2's weapons and Zero
///   Hour's own units.
pub fn to_zh(t: &Tuning, victim: &crate::host::ObjInfo, bo2: f32) -> f32 {
    if is_player(victim) {
        general_zh(t, bo2, victim.max_health)
    } else {
        bo2 * crate::units::BO2_TO_ZH_DAMAGE
    }
}

/// [`to_zh`] for a General known by his full Zero Hour health `max_health` rather than by his
/// object (the sim's own blows on a seat's General: a landing's fall damage, the Shock Charge's
/// ticks): the hit's share of `playerMaxHealth`, rounded as BO2's callback receives it. A General
/// with no health known (`0`) takes the exchange rate, as anything else does.
pub fn general_zh(t: &Tuning, bo2: f32, max_health: f32) -> f32 {
    if max_health > 0.0 {
        idamage(bo2) * max_health / t.player_max_health.max(1e-3) * (1.0 + LETHAL_MARGIN)
    } else {
        bo2 * crate::units::BO2_TO_ZH_DAMAGE
    }
}

/// A bullet's Zero Hour damage on `victim` (none known: anything but a General): the range
/// curve's damage `range` (BO2 health) times the round's multiplier `mult` (a penetrated round's),
/// the location's `at`, FMJ's `pierce` and BO2's vehicle rule `rule`. On a General each stage is
/// BO2's integer (`FUN_008edb80`, `FUN_005cfc60` `(int)(damage * mult)`, `FUN_005f05c0`'s location
/// multiplier, floored at 1), then [`to_zh`].
pub fn bullet_damage(t: &Tuning, victim: Option<&crate::host::ObjInfo>, range: f32, mult: f32, at: f32, pierce: f32, rule: f32) -> f32 {
    match victim {
        Some(v) if is_player(v) => {
            let hit = idamage(idamage(range) * mult);
            let located = if hit >= 1.0 { (hit * at * pierce * rule).trunc().max(1.0) } else { 0.0 };
            to_zh(t, v, located)
        }
        _ => range * mult * at * pierce * rule * crate::units::BO2_TO_ZH_DAMAGE,
    }
}

/// How much over its exact share [`to_zh`] makes a General's hit: BO2's hits are whole numbers
/// of health, so hits that add up to `playerMaxHealth` must take all of his Zero Hour health even
/// where the share is not a whole number in floats (`n x (max / n)` can fall a hair short and
/// leave him alive at 0.00001). Hits a whole point short of a kill stay short by 1 % of his
/// health, which this never reaches.
pub const LETHAL_MARGIN: f32 = 1e-4;

/// [`to_zh`] backwards: the BO2 health `zh` Zero Hour damage on `victim` stands for (a General's
/// share of his health in BO2's `playerMaxHealth`, else ÷ [`crate::units::BO2_TO_ZH_DAMAGE`]). A
/// General's hit comes back with its [`LETHAL_MARGIN`], a hair over the whole number, so BO2's
/// truncation of it keeps the number.
pub fn to_bo2(t: &Tuning, victim: &crate::host::ObjInfo, zh: f32) -> f32 {
    if is_player(victim) {
        general_bo2(t, zh, victim.max_health)
    } else {
        zh / crate::units::BO2_TO_ZH_DAMAGE
    }
}

/// Zero Hour health `zh` of a General whose full health is `max_health`, in BO2 health (his
/// share of `playerMaxHealth`): what his hurts, flinches, assists and regeneration count.
pub fn general_bo2(t: &Tuning, zh: f32, max_health: f32) -> f32 {
    if max_health > 0.0 {
        zh * t.player_max_health / max_health
    } else {
        0.0
    }
}

/// A General's Zero Hour damage `zh` scaled by a perk of BO2's damage callback (`scale`, Flak
/// Jacket's): BO2 scales the integer hit and rounds it again ([`callback_idamage`]). Anything else
/// scales as is.
pub fn callback_scaled(t: &Tuning, victim: &crate::host::ObjInfo, zh: f32, scale: f32) -> f32 {
    if is_player(victim) && scale != 1.0 && zh > 0.0 {
        to_zh(t, victim, callback_idamage(to_bo2(t, victim, zh) * scale))
    } else {
        zh * scale
    }
}

/// Whether `victim` is BO2's player: a General with health (the bridge flags him by his object).
fn is_player(victim: &crate::host::ObjInfo) -> bool {
    victim.has(crate::host::objflags::GENERAL) && victim.max_health > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_general(max_health: f32) -> crate::host::ObjInfo {
        crate::host::ObjInfo { flags: crate::host::objflags::INFANTRY | crate::host::objflags::GENERAL, health: max_health, max_health, ..Default::default() }
    }

    /// Hits of `zh` each until he is dead (health at or under 0, as Zero Hour's body dies).
    fn hits_to_kill(max_health: f32, zh: f32) -> u32 {
        let mut health = max_health;
        let mut n = 0;
        while health > 0.0 && n < 10_000 {
            health -= zh;
            n += 1;
        }
        n
    }

    /// BO2's integer hit (`FUN_008edb80`, `FUN_005cfc60`, `FUN_005f2130`: truncated; under 1
    /// nothing) and the callback's (`int`, at least 1).
    #[test]
    fn bo2_hits_are_whole_numbers() {
        assert_eq!([idamage(40.9), idamage(40.0), idamage(1.0), idamage(0.99), idamage(-3.0)], [40.0, 40.0, 1.0, 0.0, 0.0]);
        assert_eq!([callback_idamage(13.86), callback_idamage(0.4)], [13.0, 1.0]);
    }

    /// Every BO2 hit from 1 to 300 kills a General in BO2's shots against `playerMaxHealth`
    /// (`ceil(100 / damage)`), whatever his Zero Hour health — a hero's, a veteran's, the old 500,
    /// shares that are no whole number in floats — and hits a point short of a kill leave him
    /// standing. Anything that is not a General takes the exchange rate.
    #[test]
    fn a_general_dies_in_bo2s_shots_at_any_health() {
        let t = Tuning::default();
        for max in [100.0, 150.0, 200.0, 230.0, 240.0, 333.3, 500.0, 1234.5] {
            let g = a_general(max);
            for d in 1..=300 {
                let want = (t.player_max_health / d as f32).ceil().max(1.0) as u32;
                assert_eq!(hits_to_kill(max, to_zh(&t, &g, d as f32)), want, "{d} damage on {max} health");
            }
            // 99 then 1 kills; 99 alone does not.
            assert!(max - to_zh(&t, &g, 99.0) > 0.0 && max - to_zh(&t, &g, 99.0) - to_zh(&t, &g, 1.0) <= 0.0, "{max}");
            // A fraction is BO2's integer.
            assert_eq!(to_zh(&t, &g, 40.9), to_zh(&t, &g, 40.0));
            assert_eq!(to_zh(&t, &g, 0.5), 0.0, "under 1 does nothing");
            assert!((to_bo2(&t, &g, to_zh(&t, &g, 40.0)) - 40.0).abs() < 0.01);
            assert!((general_bo2(&t, max * 0.4, max) - 40.0).abs() < 1e-3, "his health in BO2's");
        }
        let ranger = crate::host::ObjInfo { flags: crate::host::objflags::INFANTRY, health: 180.0, max_health: 180.0, ..Default::default() };
        assert_eq!(to_zh(&t, &ranger, 40.5), 40.5 * crate::units::BO2_TO_ZH_DAMAGE, "Zero Hour's own infantry: the exchange rate");
        assert_eq!(to_bo2(&t, &ranger, 60.0), 40.0);
        assert_eq!(to_zh(&t, &crate::host::ObjInfo { flags: crate::host::objflags::GENERAL, ..Default::default() }, 40.0), 60.0, "no health: no share to take");
    }

    /// A bullet on a General is BO2's integer at each of its engine's stages: the range curve's
    /// damage, times the round's multiplier, times the location's (at least 1).
    #[test]
    fn a_bullet_on_a_general_truncates_where_bo2s_engine_does() {
        let t = Tuning::default();
        let g = a_general(200.0);
        // 37.6 -> 37 at range; x1.1 = 40.7 -> 40 (unrounded it would be 41.36).
        assert_eq!(bullet_damage(&t, Some(&g), 37.6, 1.0, 1.1, 1.0, 1.0), to_zh(&t, &g, 40.0));
        // A penetrated round's multiplier first: 40 x 0.5 = 20, then the head's 1.5 = 30.
        assert_eq!(bullet_damage(&t, Some(&g), 40.0, 0.5, 1.5, 1.0, 1.0), to_zh(&t, &g, 30.0));
        // A location multiplier under 1 floors at 1 (`FUN_005f05c0`).
        assert_eq!(bullet_damage(&t, Some(&g), 2.0, 1.0, 0.1, 1.0, 1.0), to_zh(&t, &g, 1.0));
        // Anything else: the product, at the exchange rate.
        assert_eq!(bullet_damage(&t, None, 37.6, 1.0, 1.1, 1.0, 1.0), 37.6 * 1.1 * crate::units::BO2_TO_ZH_DAMAGE);
    }

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

    /// Every Zero Hour damage type stands for a BO2 means of death, and so for a `dmgType` the
    /// death script reads; the pack's lines override it; a kind past the list is MOD_UNKNOWN.
    #[test]
    fn every_zero_hour_damage_type_is_a_means_of_death() {
        use crate::death::DmgType as D;
        let t = ZhDamageTable::default();
        let want = |zh: &str| match zh {
            "SMALL_ARMS" | "GATTLING" | "SNIPER" | "COMANCHE_VULCAN" => (MeansOfDeath::RifleBullet, D::Normal),
            "MELEE" => (MeansOfDeath::Melee, D::Melee),
            "EXPLOSION" | "ARMOR_PIERCING" | "AURORA_BOMB" | "LAND_MINE" => (MeansOfDeath::Explosive, D::Explosive),
            "INFANTRY_MISSILE" | "JET_MISSILES" | "STEALTHJET_MISSILES" => (MeansOfDeath::ProjectileSplash, D::Explosive),
            "CRUSH" | "TOPPLING" => (MeansOfDeath::Crush, D::Normal),
            "FALLING" => (MeansOfDeath::Falling, D::Falling),
            "FLAME" | "MOLOTOV_COCKTAIL" | "LASER" | "PARTICLE_BEAM" | "MICROWAVE" => (MeansOfDeath::Burned, D::Fire),
            "RADIATION" | "POISON" => (MeansOfDeath::Gas, D::Normal),
            "WATER" => (MeansOfDeath::Drown, D::Normal),
            "UNRESISTABLE" => (MeansOfDeath::TriggerHurt, D::Normal),
            "HEALING" | "DEPLOY" | "SURRENDER" | "HACK" | "KILL_PILOT" | "PENALTY" | "DISARM" | "HAZARD_CLEANUP" | "SUBDUAL_MISSILE" | "SUBDUAL_VEHICLE"
            | "SUBDUAL_BUILDING" | "SUBDUAL_UNRESISTABLE" | "KILL_GARRISONED" | "STATUS" => (MeansOfDeath::Unknown, D::Normal),
            other => panic!("{other} has no expectation"),
        };
        assert_eq!(t.entries().count(), ZH_DAMAGE_TYPES.len());
        for (code, zh) in ZH_DAMAGE_TYPES.iter().enumerate() {
            let m = t.means(code as i32);
            // Airborne for the falling one (BO2's falling death is in the air).
            assert_eq!((m, crate::death::DmgType::of(m, None, true)), want(zh), "{zh}");
        }
        assert_eq!(t.means(-1), MeansOfDeath::Unknown);
        assert_eq!(t.means(ZH_DAMAGE_TYPES.len() as i32), MeansOfDeath::Unknown);
        let mut o = t;
        o.parse_over("SNIPER = MOD_HEAD_SHOT\nMOD_HEAD_SHOT = SNIPER\nBOGUS = MOD_MELEE\nLASER = MOD_NOPE\n");
        assert_eq!(o.means(zh_code("SNIPER").unwrap() as i32), MeansOfDeath::HeadShot);
        assert_eq!(o.means(zh_code("LASER").unwrap() as i32), MeansOfDeath::Burned);
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
