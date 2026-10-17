//! The weapons converter's one manifest of BO2 name-keyed mappings (refactor plan 3.2): which
//! weapons are lethals or tacticals, what each offhand is once down, how it is used, and what its
//! shot lands as. Every mapping the converter decides by weapon name lives here instead of being
//! scattered as match arms over `weapons.rs`.

use zhbo_core::weapons::{Effect, Kind, Place};

/// Lethal equipment and tactical equipment in BO2's create-a-class (the rest of the offhands and
/// items are sorted by `OffhandClass`).
pub const LETHALS: &[&str] = &["frag_grenade_mp", "sticky_grenade_mp", "hatchet_mp", "bouncingbetty_mp", "claymore_mp", "satchel_charge_mp"];
pub const TACTICALS: &[&str] = &[
    "concussion_grenade_mp",
    "flash_grenade_mp",
    "willy_pete_mp",
    "emp_grenade_mp",
    "sensor_grenade_mp",
    "proximity_grenade_mp",
    "trophy_system_mp",
    "tactical_insertion_mp",
    "pda_hack_mp",
];

/// What each BO2 weapon is as equipment once down: the weapon table's `Equipment` block, written
/// here.
pub fn equipment_kind_of(name: &str) -> Kind {
    match name {
        "claymore_mp" => Kind::Claymore,
        "bouncingbetty_mp" => Kind::Betty,
        "satchel_charge_mp" => Kind::C4,
        "proximity_grenade_mp" => Kind::Shock,
        "trophy_system_mp" => Kind::Trophy,
        "tactical_insertion_mp" => Kind::Insertion,
        "sensor_grenade_mp" => Kind::Sensor,
        _ => Kind::Other,
    }
}

/// How an offhand item is used.
pub fn place_of(name: &str) -> Place {
    match name {
        "claymore_mp" | "bouncingbetty_mp" | "trophy_system_mp" | "tactical_insertion_mp" => Place::Plant,
        "pda_hack_mp" => Place::Hack,
        _ => Place::Throw,
    }
}

/// The effect a name-keyed weapon's shot lands as. Weapons not listed here fall back to what
/// their BO2 weapon type implies (see `weapons.rs`).
pub fn named_effect(name: &str) -> Option<Effect> {
    match name {
        "concussion_grenade_mp" | "flash_grenade_mp" | "proximity_grenade_mp" => Some(Effect::Stun),
        "emp_grenade_mp" => Some(Effect::Emp),
        "willy_pete_mp" => Some(Effect::Smoke),
        "hatchet_mp" | "knife_ballistic_mp" => Some(Effect::Impact),
        "sensor_grenade_mp" | "trophy_system_mp" | "tactical_insertion_mp" | "pda_hack_mp" => Some(Effect::None),
        _ => None,
    }
}

/// Thrown weapons that stick to what they hit.
pub const STICKY: &[&str] = &["sticky_grenade_mp", "crossbow_mp", "satchel_charge_mp", "proximity_grenade_mp"];

/// Weapons that exist in BO2's files but the mod never hands out (turrets and killstreak props
/// are filtered by name patterns; these two are ordinary guns BO2 never gives in multiplayer).
pub const EXCLUDED_WEAPONS: &[&str] = &["minigun_mp", "riotshield_mp"];

/// A few weapons' own display keys have no text; BO2's menus use these instead.
pub fn display_name_key(key: &str) -> &str {
    match key {
        "FLASH_GRENADE" => "WEAPON_FLASHBANG",
        "EMP_GRENADE" => "WEAPON_EMPGRENADE",
        "CONCUSSION_GRENADE" => "WEAPON_STUN_GRENADE",
        _ => key,
    }
}
