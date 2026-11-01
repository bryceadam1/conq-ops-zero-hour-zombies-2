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
    // ---- The bullet (Bullet_Fire, FUN_005d0c20) --------------------------------------------------
    bulletrange: "bulletrange" = 8192.0, "How far a non-spread weapon's round is traced, inches (`bulletrange`, registered in `FUN_006e86b0`, default `0x00b12d08` = 8192, max 65536; read by `Bullet_Fire` `FUN_005d0c20`). Spread classes use their damage curve's last range.";
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
    cg_fov_min: "cg_fovMin" = 10.0, "The narrowest view FOV, degrees (`FUN_004a0920`).";
    cg_fov_scale: "cg_fovScale" = 1.0, "Scale on the view FOV after the ADS zoom (`FUN_004a0920`).";
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
    // ---- Hold breath (`FUN_008e4860`, `FUN_008dea30`, `FUN_0052e3d0`; dvars-t6mp.tsv) -------------
    player_breath_hold_time: "player_breath_hold_time" = 4.5, "The longest a breath holds, seconds.";
    player_breath_gasp_time: "player_breath_gasp_time" = 0.0, "Extra seconds of gasping once the breath runs out.";
    player_breath_gasp_scale: "player_breath_gasp_scale" = 2.5, "The idle sway's scale right after a breath, easing back to 1 as the breath recovers.";
    player_breath_hold_lerp: "player_breath_hold_lerp" = 1.5, "Rate the sway scale eases to 0 while holding (per second).";
    player_breath_gasp_lerp: "player_breath_gasp_lerp" = 4.0, "Rate the sway scale eases to its gasp target while breathing (per second).";
    player_breath_fire_delay: "player_breath_fire_delay" = 0.0, "Seconds of breath a shot costs (the shot also lets the breath go).";
    player_breath_snd_delay: "player_breath_snd_delay" = 1.0, "Seconds between the breath-in and breath-out sounds.";
    perk_extra_breath: "perk_extraBreath" = 5.0, "`holdbreath`: extra seconds of breath (G1).";
    bg_aqs: "bg_aqs" = 1.0, "Anti-quickscope on (1) or off (0) (`FUN_008dac10`, `FUN_008f0f00`).";
    bg_aqs_style: "bg_aqsStyle" = 2.0, "Anti-quickscope style: 1 scales the scope sway, 2 offsets it by the sway factor.";

    // ---- Optic displays (`t6mp` `FUN_004cc260` dvars; `sim::optics`) ---------------------------
    sonar_max_speed: "cg_sonarAttachmentMaxSpeed" = 2.2, "MMS: inches per BO2 frame at which a far character is gone from the scanner (`FUN_004e7b40`).";
    sonar_distance_factor: "cg_sonarAttachmentDistanceFactor" = 3.0, "MMS: added to the max speed as a character comes closer, at zero range (`FUN_004e7b40`).";
    sonar_speed_delay: "cg_sonarAttachmentSpeedDelay" = 0.8, "MMS: seconds of speed averaged (`FUN_004e7b40`).";
    sonar_speed_time_ratio: "cg_sonarAttachmentSpeedTimeRatio" = 0.0, "MMS: the lowest visibility a moving character keeps (`FUN_004e7b40`).";
    sonar_hide_friendlies: "cg_sonarAttachmentHideFriendlies" = 1.0, "MMS: teammates are left off the scanner (1) (`FUN_004e7b40`).";
    sonar_frames_per_second: "SonarFramesPerSecond" = 60.0, "MMS: BO2's frame for its per-frame speeds; the speed ring holds `SpeedDelay x 60` samples (`0x00b128e4`).";
    range_finder_min_enemy_spotting_dist: "cg_rangeFinderMinEnemySpottingDist" = -1.0, "Target finder: feet beyond which a spotted enemy does not turn the reticle; -1 no limit (`FUN_00528ec0`).";
    range_finder_display_ads: "RangeFinderDisplayAds" = 0.99, "Target finder: the ADS fraction above which its display draws (`DAT_00c73584`, `FUN_00526780`).";
    range_finder_mark_height: "RangeFinderMarkHeight" = 45.0, "Target finder: inches above a standing enemy's feet its diamond is centred (70 - 25, `FUN_00526780`).";
    range_finder_trace_dist: "RangeFinderTraceDist" = 8192.0, "Target finder: inches the spotting trace reaches (`_DAT_00b12d08`, `FUN_00528ec0`).";
    scope_overlay_width: "ScopeOverlayWidth" = 480.0, "The scope overlay's width in BO2's 640x480 virtual screen when a weapon table carries no `OverlaySize` (a stale table's 0): the `overlayWidth` every overlay weapon in BO2's install has (`WeaponDef+0x59c`, `FUN_004b4c70`).";
    scope_overlay_height: "ScopeOverlayHeight" = 480.0, "As above, the height (`overlayHeight`, `WeaponDef+0x5a0`): a height-fit square.";

    // ---- Bullet penetration (t6mp `FUN_005d0350`; the table is patch_mp's `info/bullet_penetration_mp`) ----
    penetration_count: "penetrationCount" = 5.0, "Most surfaces one bullet passes through (`penetrationCount` 5, 0..5; `FUN_006e86b0`).";
    perk_bullet_penetration_multiplier: "perk_bulletPenetrationMultiplier" = 2.0, "`bulletpenetration` (FMJ): every depth is multiplied by it (G1; `FUN_005d0350`).";
    bullet_penetration_min_fx_dist: "bullet_penetrationMinFxDist" = 30.0, "Inches a penetrated round must fly from an exit to the next surface before either plays its impact (`FUN_008b7bb0`).";
    pen_small_flesh: "small_flesh" = 32.0, "Penetration depth, inches: a `small` round through flesh (`info/bullet_penetration_mp`).";
    pen_small_metal: "small_metal" = 12.0, "Penetration depth, inches: a `small` round through metal (`info/bullet_penetration_mp`).";
    pen_small_concrete: "small_concrete" = 6.0, "Penetration depth, inches: a `small` round through concrete (`info/bullet_penetration_mp`).";
    pen_small_dirt: "small_dirt" = 0.0, "Penetration depth, inches: a `small` round through dirt (`info/bullet_penetration_mp`).";
    pen_small_grass: "small_grass" = 0.0, "Penetration depth, inches: a `small` round through grass (`info/bullet_penetration_mp`).";
    pen_small_asphalt: "small_asphalt" = 6.0, "Penetration depth, inches: a `small` round through asphalt (`info/bullet_penetration_mp`).";
    pen_small_sand: "small_sand" = 0.0, "Penetration depth, inches: a `small` round through sand (`info/bullet_penetration_mp`).";
    pen_small_snow: "small_snow" = 72.0, "Penetration depth, inches: a `small` round through snow (`info/bullet_penetration_mp`).";
    pen_small_rock: "small_rock" = 6.0, "Penetration depth, inches: a `small` round through rock (`info/bullet_penetration_mp`).";
    pen_small_wood: "small_wood" = 12.0, "Penetration depth, inches: a `small` round through wood (`info/bullet_penetration_mp`).";
    pen_small_water: "small_water" = 72.0, "Penetration depth, inches: a `small` round through water (`info/bullet_penetration_mp`).";
    pen_medium_flesh: "medium_flesh" = 64.0, "Penetration depth, inches: a `medium` round through flesh (`info/bullet_penetration_mp`).";
    pen_medium_metal: "medium_metal" = 16.0, "Penetration depth, inches: a `medium` round through metal (`info/bullet_penetration_mp`).";
    pen_medium_concrete: "medium_concrete" = 9.0, "Penetration depth, inches: a `medium` round through concrete (`info/bullet_penetration_mp`).";
    pen_medium_dirt: "medium_dirt" = 0.0, "Penetration depth, inches: a `medium` round through dirt (`info/bullet_penetration_mp`).";
    pen_medium_grass: "medium_grass" = 0.0, "Penetration depth, inches: a `medium` round through grass (`info/bullet_penetration_mp`).";
    pen_medium_asphalt: "medium_asphalt" = 8.0, "Penetration depth, inches: a `medium` round through asphalt (`info/bullet_penetration_mp`).";
    pen_medium_sand: "medium_sand" = 0.0, "Penetration depth, inches: a `medium` round through sand (`info/bullet_penetration_mp`).";
    pen_medium_snow: "medium_snow" = 72.0, "Penetration depth, inches: a `medium` round through snow (`info/bullet_penetration_mp`).";
    pen_medium_rock: "medium_rock" = 8.0, "Penetration depth, inches: a `medium` round through rock (`info/bullet_penetration_mp`).";
    pen_medium_wood: "medium_wood" = 8.0, "Penetration depth, inches: a `medium` round through wood (`info/bullet_penetration_mp`).";
    pen_medium_water: "medium_water" = 72.0, "Penetration depth, inches: a `medium` round through water (`info/bullet_penetration_mp`).";
    pen_large_flesh: "large_flesh" = 112.0, "Penetration depth, inches: a `large` round through flesh (`info/bullet_penetration_mp`).";
    pen_large_metal: "large_metal" = 20.0, "Penetration depth, inches: a `large` round through metal (`info/bullet_penetration_mp`).";
    pen_large_concrete: "large_concrete" = 12.0, "Penetration depth, inches: a `large` round through concrete (`info/bullet_penetration_mp`).";
    pen_large_dirt: "large_dirt" = 0.0, "Penetration depth, inches: a `large` round through dirt (`info/bullet_penetration_mp`).";
    pen_large_grass: "large_grass" = 0.0, "Penetration depth, inches: a `large` round through grass (`info/bullet_penetration_mp`).";
    pen_large_asphalt: "large_asphalt" = 12.0, "Penetration depth, inches: a `large` round through asphalt (`info/bullet_penetration_mp`).";
    pen_large_sand: "large_sand" = 0.0, "Penetration depth, inches: a `large` round through sand (`info/bullet_penetration_mp`).";
    pen_large_snow: "large_snow" = 72.0, "Penetration depth, inches: a `large` round through snow (`info/bullet_penetration_mp`).";
    pen_large_rock: "large_rock" = 12.0, "Penetration depth, inches: a `large` round through rock (`info/bullet_penetration_mp`).";
    pen_large_wood: "large_wood" = 16.0, "Penetration depth, inches: a `large` round through wood (`info/bullet_penetration_mp`).";
    pen_large_water: "large_water" = 72.0, "Penetration depth, inches: a `large` round through water (`info/bullet_penetration_mp`).";
    // ---- Melee and the knife's lunge (t6mp `FUN_008b7bb0` dvars; `FUN_008c3770`/`FUN_008c34f0` PM charge) ----
    player_melee_range: "player_meleeRange" = 55.0, "The melee's reach, inches (dvar default); past it, inside the weapon's `meleeChargeRange`, the melee lunges.";
    player_melee_charge_max_speed: "player_meleeChargeMaxSpeed" = 800.0, "The lunge's top speed, inches/s (dvar default; `FUN_008c34f0` clamps the charge velocity to it).";
    player_melee_charge_friction: "player_meleeChargeFriction" = 1200.0, "The lunge's deceleration, inches/s^2 (dvar default): it starts at sqrt(2 x friction x distance) so it stops at the reach (`FUN_008c3770`).";
    // ---- Equipment, concussion and EMP (BO2's MP scripts, `gsc-dump/mp/decompiled/t6/maps/mp/`) ----
    scr_weaponobject_radius: "scr_weaponobject_radius" = 192.0, "Claymore: the trigger radius, inches (`gametypes/_weaponobjects.gsc:34` default; the trigger at `:1502`). The weapon's own explosion radius also caps it (`shouldaffectweaponobject`, `:1562-1569`).";
    scr_weaponobject_coneangle: "scr_weaponobject_coneangle" = 70.0, "Claymore: half-angle of its front cone, degrees (`gametypes/_weaponobjects.gsc:31`; `detectiondot = cos(angle)`, `:282`).";
    scr_weaponobject_mindist: "scr_weaponobject_mindist" = 20.0, "Claymore: how far in front of it, along its facing, an enemy must be, inches (`gametypes/_weaponobjects.gsc:32`; `shouldaffectweaponobject`, `:1579`).";
    scr_weaponobject_graceperiod: "scr_weaponobject_graceperiod" = 0.6, "Claymore: seconds from the trip to the blast (`gametypes/_weaponobjects.gsc:33`; `wait( detectiongraceperiod )`, `:1548`).";
    betty_radius: "bettyradius" = 192.0, "Bouncing Betty: trigger radius, inches, any direction (`_bouncingbetty.gsc:19`, `ignoredirection`).";
    betty_grace_period: "bettygraceperiod" = 0.6, "Bouncing Betty: seconds from the trip to the jump (`_bouncingbetty.gsc:18`).";
    betty_jump_height: "bettyjumpheight" = 65.0, "Bouncing Betty: how high it jumps before it goes off, inches (`_bouncingbetty.gsc:24`, `bouncingbettyjumpandexplode`).";
    betty_jump_time: "bettyjumptime" = 0.65, "Bouncing Betty: seconds the jump takes; it goes off at the top (`_bouncingbetty.gsc:25`, `:140-145`).";
    proximity_grenade_detection_radius: "scr_proximityGrenadeDetectionRadius" = 150.0, "Shock Charge: trigger radius, inches (`_proximity_grenade.gsc:19`).";
    proximity_grenade_grace_period: "scr_proximityGrenadeGracePeriod" = 0.1, "Shock Charge: seconds from the trip to the shock (`_proximity_grenade.gsc:20`).";
    trophy_system_range: "trophy_system_range" = 512.0, "Trophy System: how near a projectile it shoots down, inches (`_trophy_system.gsc:171`, `distancesquared < 262144`).";
    trophy_system_ammo: "trophy_system_ammo" = 2.0, "Trophy System: projectiles it shoots down before it is spent (`_trophy_system.gsc:62`, `self.ammo = 2`).";
    perk_armorpiercing: "perk_armorpiercing" = 40.0, "`armorpiercing` (FMJ): percent extra bullet damage on scorestreaks — Zero Hour's vehicles and structures (`gametypes/_class.gsc:859`, `cac_get_dvar_int( \"perk_armorpiercing\", \"40\" ) / 100`; applied as `damage + int(damage * data)`, `killstreaks/_ai_tank.gsc:292-293`).";
    perk_delay_explosive_time: "perk_delayExplosiveTime" = 0.9, "`delayexplosive` (Engineer): extra seconds a tripped explosive waits (dvar default, `dvars-t6mp.tsv`; `gametypes/_weaponobjects.gsc:1550-1551`).";
    equipment_hacker_tool_time_ms: "equipmenthackertooltimems" = 50.0, "Black Hat: the lock on equipment, milliseconds of sight (`_hacker_tool.gsc:20`; the lock loop `:293-301`).";
    vehicle_hacker_tool_time_ms: "vehiclehackertooltimems" = 4000.0, "Black Hat: the lock on a vehicle, milliseconds of sight (`_hacker_tool.gsc:30`, `gethacktime`'s default for a vehicle); the mod's Zero Hour drones are vehicles.";
    hacker_tool_lock_start: "hackertoollockstart" = 0.1, "Black Hat: seconds from the pull-back to the lock starting: `watchhackertooluse`'s `wait 0.05` after `grenade_pullback` (`_hacker_tool.gsc:166-170`) and the target loop's first `wait 0.05` before it picks the target (`:245`, `:330-335`).";
    hacker_tool_max_equipment_distance: "hackertoolmaxequipmentdistance" = 2000.0, "Black Hat: the farthest equipment it locks on to, inches (`_hacker_tool.gsc:12`); the mod uses it for Zero Hour drones too.";
    concussion_time_min: "concussion_time_min" = 2.0, "Stun at the blast's edge, seconds: BO2's `time = 2 + 4 * scale` (`gametypes/_weapons.gsc:1372`).";
    concussion_time_scale: "concussion_time_scale" = 4.0, "Extra stun at the blast's centre, seconds (the `4` in `gametypes/_weapons.gsc:1372`; `scale` is 1 at the centre, 0 at the edge).";
    concussion_self_radius_scale: "concussion_self_radius_scale" = 0.5, "The thrower's own concussion: `scale` runs over this share of the radius (`gametypes/_weapons.gsc:1364-1365`: `radius * 0.5` when `self == eattacker`).";
    emp_grenade_duration: "empgrenade_duration" = 12.0, "EMP Grenade: seconds a vehicle stays disabled (`_empgrenade.gsc:102`, `empduration = 12`).";
    // ---- The mod's own ------------------------------------------------------------------------
    shield_blast_protection_30: "shieldBlastDamageProtection_30" = 0.4, "Assault Shield: share of a blast it takes away within a 30-degree arc of its forward axis (`FUN_00670940`; dvar default).";
    shield_blast_protection_60: "shieldBlastDamageProtection_60" = 0.2, "As above, within a 60-degree arc.";
    shield_blast_protection_120: "shieldBlastDamageProtection_120" = 0.1, "As above, within a 120-degree arc.";
    shield_blast_protection_180: "shieldBlastDamageProtection_180" = 0.0, "As above, within a 180-degree arc.";
    riotshield_deployed_health: "riotshield_deployed_health" = 300.0, "A deployed Assault Shield's health: the scaled damage it takes before it is destroyed (`FUN_00966680` dvar; `_riotshield.gsc` `watchdeployedriotshielddamage`).";
    riotshield_bullet_damage_scale: "riotshield_bullet_damage_scale" = 0.025, "Bullet damage scale on a deployed shield (dvar default).";
    riotshield_melee_damage_scale: "riotshield_melee_damage_scale" = 1.0, "Melee damage scale on a deployed shield (dvar default).";
    riotshield_explosive_damage_scale: "riotshield_explosive_damage_scale" = 1.0, "Explosive damage scale on a deployed shield (dvar default).";
    riotshield_projectile_damage_scale: "riotshield_projectile_damage_scale" = 0.01, "Impact (thrown/launched) damage scale on a deployed shield (dvar default).";
    riotshield_deploy_zdiff_max: "riotshield_deploy_zdiff_max" = 30.0, "Deploy: the most the ground may differ from the player's feet, inches (dvar default).";
    riotshield_deploy_pitch_max: "riotshield_deploy_pitch_max" = 20.0, "Deploy: the steepest ground pitch, degrees (dvar default).";
    riotshield_deploy_roll_max: "riotshield_deploy_roll_max" = 25.0, "Deploy: the steepest ground roll, degrees (dvar default).";
    riotshield_deploy_limit_radius: "riotshield_deploy_limit_radius" = 100.0, "Deploy: the least distance to any other deployed shield, inches (dvar default).";
    riotshield_destroyed_cleanup_time: "riotshield_destroyed_cleanup_time" = 10.0, "Seconds a destroyed shield's model stays (dvar default; it stops blocking at once).";
    riotshield_placement_zoffset: "riotshield_placement_zoffset" = 26.0, "Height of the deployed shield's origin over the ground, inches (`_riotshield.gsc` init).";
    regen_delay: "RegenDelay" = 5.0, "Seconds after the last hit before health regenerates (the mod's value).";
    regen_full_time: "RegenFullTime" = 3.0, "Seconds to regenerate from empty (the mod's value).";
    building_rate: "BuildingRate" = 1.0, "Build/repair rate multiplier (the mod's value).";
    projectile_bounce: "ProjectileBounce" = 0.3, "Grenade bounce: share of the speed into the surface kept, where the weapon def has no `perpendicularBounce` cell for the surface (the mod's value).";
    projectile_bounce_along: "ProjectileBounceAlong" = 0.6, "Grenade bounce: share of the speed along the surface kept, where the def has no `parallelBounce` cell (the mod's value).";
    projectile_rest_speed: "ProjectileRestSpeed" = 3.0, "A bouncing grenade below this speed comes to rest, Zero Hour units/s (the mod's value).";
    equipment_arm_time: "EquipmentArmTime" = 1.0, "Seconds after it is down before equipment does anything (the mod's stand-in for BO2's `waittillnotmoving` plus `activationdelay`).";
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

    #[test]
    fn bo2s_equipment_script_values() {
        // The MP scripts' `level.*` and `scr_*` defaults (gsc-dump, cited on each key).
        let t = Tuning::default();
        assert_eq!((t.scr_weaponobject_radius, t.scr_weaponobject_coneangle, t.scr_weaponobject_mindist, t.scr_weaponobject_graceperiod), (192.0, 70.0, 20.0, 0.6));
        assert_eq!((t.betty_radius, t.betty_grace_period, t.betty_jump_height, t.betty_jump_time), (192.0, 0.6, 65.0, 0.65));
        assert_eq!((t.proximity_grenade_detection_radius, t.proximity_grenade_grace_period), (150.0, 0.1));
        assert_eq!((t.trophy_system_range, t.trophy_system_ammo, t.perk_delay_explosive_time), (512.0, 2.0, 0.9));
        assert_eq!((t.hacker_tool_max_equipment_distance, t.concussion_time_min, t.concussion_time_scale, t.emp_grenade_duration), (2000.0, 2.0, 4.0, 12.0));
        assert_eq!((t.equipment_hacker_tool_time_ms, t.vehicle_hacker_tool_time_ms, t.hacker_tool_lock_start), (50.0, 4000.0, 0.1));
    }
}
