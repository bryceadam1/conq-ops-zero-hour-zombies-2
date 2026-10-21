//! The gameplay tuning pack (refactor plan 3.1): the numbers BO2/bridge code used to scatter
//! across ~10 files, in one place. Loaded at init from `Data/ConquerOps/tuning.ini` (part of the
//! mod's BIG file, like weapons.ini — every player must have the identical file); the built-in
//! defaults below keep tests and the fallback working when the file is absent, and a test holds
//! them equal to the shipped file.
//!
//! BO2's own numbers are kept in BO2's units (inches, inches per second, milliseconds where the
//! engine counts milliseconds) under the engine's dvar names, so each one can be checked against
//! its source: the engine defaults read from `t6mp` by Ghidra (`~/Documents/BO2-reference/ghidra/
//! answers/G1.md`, `G2.md`, `G4.md`, `G6.md`, `G7.md`; the full list is `dvars-t6mp.tsv` there).
//! Code converts at the point of use ([`crate::units::inches`]).

/*
** Conquer Ops - Zero Hour gameplay core: the gameplay tuning pack.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use std::collections::HashMap;
use std::sync::Mutex;

macro_rules! tuning {
    ($($field:ident: $key:literal = $default:expr, $doc:literal;)*) => {
        /// The gameplay tuning. Each field's doc names its source.
        #[derive(Clone, Debug, PartialEq)]
        pub struct Tuning {
            $(#[doc = $doc] pub $field: f32,)*
        }

        impl Default for Tuning {
            fn default() -> Self {
                Tuning { $($field: $default,)* }
            }
        }

        impl Tuning {
            /// The ini key of every field, in order (the dvar's name for BO2's numbers).
            pub const KEYS: &'static [&'static str] = &[$($key,)*];

            fn set(&mut self, key: &str, v: f32) -> bool {
                $(if key == normalize($key) { self.$field = v; return true; })*
                false
            }

            /// Every field's value, in [`Tuning::KEYS`] order.
            pub fn values(&self) -> Vec<f32> {
                vec![$(self.$field,)*]
            }
        }
    };
}

tuning! {
    // ---- Movement (G2) -------------------------------------------------------------------------
    g_speed: "g_speed" = 190.0, "Run speed, inches/s (`g_speed`; G2).";
    player_sprint_time: "player_sprintTime" = 4.0, "Base sprint length, seconds, before the weapon's `sprintDurationScale` (`player_sprintTime`; G2, medium: the engine's per-player base is filled from it).";
    player_sprint_speed_scale: "player_sprintSpeedScale" = 1.5, "Sprint speed multiplier (G2).";
    player_sprint_strafe_speed_scale: "player_sprintStrafeSpeedScale" = 0.667, "Strafe scale while sprinting (G2).";
    player_back_speed_scale: "player_backSpeedScale" = 0.7, "Backwards scale (G2).";
    player_strafe_speed_scale: "player_strafeSpeedScale" = 0.8, "Strafe scale (G2).";
    player_sprint_forward_minimum: "player_sprintForwardMinimum" = 105.0, "Forward stick (of 127) needed to sprint (G2).";
    player_sprint_recharge_pause: "player_sprintRechargePause" = 0.0, "Seconds of recharge lost after running dry (`player_sprintRechargePause` 0 ms; G2).";
    sprint_restart_minimum: "SprintRestartMinimum" = 1.0, "A sprint starts only with more than this many seconds in the meter (hard-coded 1.0 x 1000 ms in `FUN_008c5840`; G2).";
    sprint_max: "SprintMax" = 16.383, "The sprint meter's cap, seconds (`min(t, 0x3fff)` ms in `FUN_008bcfe0`; G2).";
    stance_crouch_scale: "StanceCrouchScale" = 0.65, "Crouched speed scale (hard-coded, `0x00b17c50`; G2).";
    stance_prone_scale: "StanceProneScale" = 0.15, "Prone speed scale (hard-coded, `0x00b11688`; G2).";
    stance_prone_down_time: "StanceProneDownTime" = 0.6, "Seconds the speed scale blends from crouched to prone going down (600 ms; G2).";
    stance_prone_up_time: "StanceProneUpTime" = 0.4, "Seconds the speed scale blends from prone to crouched getting up (400 ms; G2).";
    jump_height: "jump_height" = 39.0, "Jump height, inches (G2).";
    bg_gravity: "bg_gravity" = 800.0, "Gravity, inches/s^2 (G2).";
    friction: "friction" = 5.5, "Ground friction (G2).";
    stopspeed: "stopspeed" = 100.0, "Friction's stop speed, inches/s (G2).";
    jump_step_size: "jump_stepSize" = 18.0, "Step-up height, inches (G2).";
    // ---- Perk multipliers (G1; applied where each specialty is tested) --------------------------
    perk_speed_multiplier: "perk_speedMultiplier" = 1.07, "`movefaster`: run speed (G1).";
    perk_sprint_multiplier: "perk_sprintMultiplier" = 2.0, "`longersprint`: sprint length (G1).";
    perk_sprint_recovery_multiplier: "perk_sprintRecoveryMultiplier" = 0.3, "`sprintrecovery`: sprint-out time (G1).";
    perk_mantle_reduction: "perk_mantleReduction" = 0.4, "`fastmantle`: mantle time (G1; no mantling in this sim yet).";
    perk_fast_ladder_climb_multiplier: "perk_fastLadderClimbMultiplier" = 1.75, "`fastladderclimb`: ladder speed (G1; no ladders on Zero Hour maps).";
    perk_weap_switch_multiplier: "perk_weapSwitchMultiplier" = 0.5, "`fastweaponswitch`: raise/drop states last this share (G1, G3).";
    perk_weap_reload_multiplier: "perk_weapReloadMultiplier" = 0.5, "`fastreload`: reload states (G1, G3).";
    perk_weap_rate_multiplier: "perk_weapRateMultiplier" = 0.75, "`rof`: fire/rechamber states (G1, G3).";
    perk_weap_ads_multiplier: "perk_weapAdsMultiplier" = 0.5, "`fastads`: the ADS rate is divided by it (G1, G3).";
    perk_weap_toss_multiplier: "perk_weapTossMultiplier" = 0.5, "`fasttoss`: grenade states (G1, G3).";
    perk_weap_equipment_use_multiplier: "perk_weapEquipmentUseMultiplier" = 0.6, "`fastequipmentuse`: equipment states (G1, G3).";
    perk_weap_melee_multiplier: "perk_weapMeleeMultiplier" = 0.5, "`fastmeleerecovery`: melee state (G1, G3).";
    perk_weap_spread_multiplier: "perk_weapSpreadMultiplier" = 0.65, "`bulletaccuracy`: hip spread min and max (G1, G6).";
    perk_damage_kick_reduction: "perk_damageKickReduction" = 0.25, "`bulletflinch`: damage flinch (G1, G7).";
    perk_flak_jacket: "perk_flakJacket" = 35.0, "`flakjacket`: percent of explosive damage taken (G1).";
    stun_protection_scale: "StunProtectionScale" = 0.1, "`stunprotection`: a concussion's duration share (script, `_weapons.gsc:1375-1376`).";
    // ---- Damage flinch (G7) --------------------------------------------------------------------
    bg_view_kick_scale: "bg_viewKickScale" = 0.175, "Flinch degrees per point of damage (G7).";
    bg_view_kick_min: "bg_viewKickMin" = 0.5, "Flinch floor, degrees (G7).";
    bg_view_kick_max: "bg_viewKickMax" = 90.0, "Flinch cap, degrees (G7).";
    flinch_time: "FlinchTime" = 0.5, "Seconds a flinch lasts (`cg->damageTime = now + 500`; G7).";
    // ---- Spread and the view (G4, G6) ----------------------------------------------------------
    player_standing_view_height: "player_standingViewHeight" = 60.0, "Standing eye height, inches: spread blends crouched->standing up to it (G2, G6).";
    view_height_crouch: "ViewHeightCrouch" = 40.0, "Crouched eye height, inches (the engine's 40; G6).";
    view_height_prone: "ViewHeightProne" = 11.0, "Prone eye height, inches (the engine's 11; G6).";
    cg_fov: "cg_fov" = 65.0, "Field of view, degrees (G4).";
    bg_weapon_bob_max: "bg_weaponBobMax" = 8.0, "Weapon bob amplitude cap (G4).";
    bg_weapon_bob_lag: "bg_weaponBobLag" = 0.25, "Weapon bob phase lag behind the view, x pi (G4).";
    bob_standing_h: "bg_weaponBobAmplitudeStanding.x" = 0.055, "Standing bob, horizontal (G4).";
    bob_standing_v: "bg_weaponBobAmplitudeStanding.y" = 0.025, "Standing bob, vertical (G4).";
    bob_ducked_h: "bg_weaponBobAmplitudeDucked.x" = 0.045, "Crouched bob, horizontal (G4).";
    bob_ducked_v: "bg_weaponBobAmplitudeDucked.y" = 0.025, "Crouched bob, vertical (G4).";
    bob_prone_h: "bg_weaponBobAmplitudeProne.x" = 0.02, "Prone bob, horizontal (G4).";
    bob_prone_v: "bg_weaponBobAmplitudeProne.y" = 0.005, "Prone bob, vertical (G4).";
    bob_sprinting_h: "bg_weaponBobAmplitudeSprinting.x" = 0.02, "Sprinting bob, horizontal, x the weapon's `vSprintBob` (G4).";
    bob_sprinting_v: "bg_weaponBobAmplitudeSprinting.y" = 0.014, "Sprinting bob, vertical (G4).";
    bob_roll: "bg_weaponBobAmplitudeRoll" = 1.5, "Bob roll multiplier (G4).";
    // ---- The mod's own ------------------------------------------------------------------------
    regen_delay: "RegenDelay" = 5.0, "Seconds after the last hit before health regenerates (the mod's value).";
    regen_full_time: "RegenFullTime" = 3.0, "Seconds to regenerate from empty (the mod's value).";
    building_rate: "BuildingRate" = 1.0, "Build/repair rate multiplier (the mod's value).";
    projectile_bounce: "ProjectileBounce" = 0.3, "Grenade bounce restitution (the mod's value).";
}

/// Keys match case-insensitively, ignoring `_` and `.` (`perk_weapSwitchMultiplier`).
fn normalize(key: &str) -> String {
    key.chars().filter(|c| *c != '_' && *c != '.').flat_map(char::to_lowercase).collect()
}

/// Parse a tuning file over the defaults. Unknown keys are tolerated.
pub fn parse(text: &str) -> Tuning {
    let mut t = Tuning::default();
    let nums: HashMap<String, f32> = text
        .lines()
        .map(|l| l.split(';').next().unwrap_or("").trim())
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            Some((normalize(k.trim()), v.trim().parse().ok()?))
        })
        .collect();
    for (k, v) in nums {
        t.set(&k, v);
    }
    t
}

/// The tuning loaded at init; the defaults until `load` runs.
static TUNING: Mutex<Option<Tuning>> = Mutex::new(None);

/// Load the tuning pack (the bridge passes `data/ConquerOps/tuning.ini`'s text at init). The
/// values must be identical on every client: the file is part of the mod.
pub fn load(text: &str) {
    *TUNING.lock().unwrap_or_else(|e| e.into_inner()) = Some(parse(text));
}

/// The tuning in force; the built-in defaults before init (tests, the shell).
pub fn get() -> Tuning {
    TUNING.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIPPED: &str = include_str!("../../../data/ConquerOps/tuning.ini");

    #[test]
    fn parses_the_tuning_file() {
        let t = parse("; tuning.ini\ng_speed = 200\nperk_weapSwitchMultiplier = 0.4\nRegenDelay = 4 \nbg_weaponBobAmplitudeStanding.x = 0.1");
        assert_eq!(t.g_speed, 200.0);
        assert_eq!(t.perk_weap_switch_multiplier, 0.4);
        assert_eq!(t.regen_delay, 4.0);
        assert_eq!(t.bob_standing_h, 0.1);
        // Unknown keys are tolerated; absent keys keep the defaults.
        assert_eq!(t.projectile_bounce, 0.3);
    }

    #[test]
    fn the_shipped_file_sets_every_key_to_its_default() {
        // The defaults are the fallback when the file is missing: they must be the file.
        let shipped = parse(SHIPPED);
        assert_eq!(shipped, Tuning::default());
        let present: Vec<String> =
            SHIPPED.lines().filter_map(|l| l.split(';').next()?.split_once('=')).map(|(k, _)| normalize(k.trim())).collect();
        for key in Tuning::KEYS {
            assert!(present.contains(&normalize(key)), "{key} is in tuning.ini");
        }
    }

    #[test]
    fn bo2s_engine_defaults() {
        // G1 / G2 / G7 dvar defaults, spot-checked.
        let t = Tuning::default();
        assert_eq!((t.g_speed, t.jump_height, t.bg_gravity, t.friction), (190.0, 39.0, 800.0, 5.5));
        assert_eq!((t.perk_speed_multiplier, t.perk_sprint_multiplier, t.perk_sprint_recovery_multiplier), (1.07, 2.0, 0.3));
        assert_eq!((t.bg_view_kick_scale, t.bg_view_kick_min, t.bg_view_kick_max), (0.175, 0.5, 90.0));
    }
}
