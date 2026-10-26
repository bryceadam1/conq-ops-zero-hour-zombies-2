//! The weapons table's columns: one row per BO2 weapon variant and attachment combination (the
//! bare weapon, every CaC-reachable combination, their alt modes), every fact both worlds have.
//!
//! Key: `weapon` is the BO2 variant (`an94_mp`, `gl_an94_mp` for the launcher mode), `combo` the
//! attachment names in `eAttachment` order joined by `+` (G14 §1), `""` for the bare weapon.
//!
//! Vocabulary is BO2's: enums by their `T6_Assets.h` names (lowercased, prefix dropped), times in
//! milliseconds, distances in inches, angles in degrees, sounds by alias, effects/models/materials
//! by asset name, specialties by bit index (the G1 table).
//!
//! Facts deliberately absent (meaningless in one world, owner decision 1): the viewmodel arms (the
//! faction hands are Zero Hour's), killcam sounds, AI/bot fields, scorestreak-only fields, network
//! and stat-tracking fields.

use crate::schema::{Kind, Schema, F32_EPS, INCH_EPS, MS_EPS, QUAT_EPS};

/// `weapAnimFiles_t` (OAT `T6_Assets.h`), `WEAP_ANIM_` dropped and lowercased; index = slot.
pub const ANIM_SLOTS: [&str; 0x58] = [
    "root", "idle", "empty_idle", "fire_intro", "fire", "hold_fire", "lastshot", "finalshot", "rechamber", "melee", "melee1",
    "melee2", "melee3", "melee_empty", "melee_charge", "melee_charge_empty", "reload", "reload_right", "reload_empty",
    "reload_start", "reload_end", "reload_quick", "reload_quick_empty", "raise", "first_raise", "drop", "alt_raise",
    "alt_drop", "quick_raise", "quick_drop", "empty_raise", "empty_drop", "sprint_in", "sprint_loop", "sprint_out",
    "sprint_empty_in", "sprint_empty_loop", "sprint_empty_out", "lowready_in", "lowready_loop", "lowready_out",
    "cont_fire_in", "cont_fire_loop", "cont_fire_out", "crawl_in", "crawl_forward", "crawl_back", "crawl_right",
    "crawl_left", "crawl_out", "crawl_empty_in", "crawl_empty_forward", "crawl_empty_back", "crawl_empty_right",
    "crawl_empty_left", "crawl_empty_out", "deploy", "breakdown", "detonate", "nightvision_wear", "nightvision_remove",
    "ads_fire", "ads_lastshot", "ads_fire_intro", "ads_rechamber", "dtp_in", "dtp_loop", "dtp_out", "dtp_empty_in",
    "dtp_empty_loop", "dtp_empty_out", "slide_in", "mantle", "camera_sprint_loop", "camera_dtp_in", "camera_dtp_loop",
    "camera_dtp_out", "camera_mantle", "fire_left", "lastshot_left", "finalshot_left", "idle_left", "empty_idle_left",
    "reload_empty_left", "reload_left", "ads_up", "ads_down", "ads_up_other_scope",
];

/// `weapType_t`.
pub const WEAP_TYPE: &[&str] = &["bullet", "grenade", "projectile", "binoculars", "gas", "bomb", "mine", "melee", "riotshield"];
/// `weapClass_t`.
pub const WEAP_CLASS: &[&str] = &[
    "rifle", "mg", "smg", "spread", "pistol", "grenade", "rocketlauncher", "turret", "non_player", "gas", "item", "melee",
    "killstreak_alt_stored_weapon", "pistol_spread",
];
/// `weapInventoryType_t`.
pub const INVENTORY: &[&str] = &["primary", "offhand", "item", "altmode", "melee", "dwlefthand"];
/// `weapFireType_t`.
pub const FIRE_TYPE: &[&str] = &["fullauto", "singleshot", "burstfire2", "burstfire3", "burstfire4", "burstfire5", "stacked", "minigun", "chargeshot", "jetgun"];
/// `OffhandClass`.
pub const OFFHAND_CLASS: &[&str] = &["none", "frag_grenade", "smoke_grenade", "flash_grenade", "gear", "supplydrop_marker"];
/// `ImpactType`.
pub const IMPACT_TYPE: &[&str] = &[
    "none", "bullet_small", "bullet_large", "bullet_ap", "bullet_xtreme", "shotgun", "grenade_bounce", "grenade_explode",
    "rifle_grenade", "rocket_explode", "rocket_explode_xtreme", "projectile_dud", "mortar_shell", "tank_shell", "bolt", "blade",
];
/// `OffhandSlot`.
pub const OFFHAND_SLOT: &[&str] = &["none", "lethal_grenade", "tactical_grenade", "equipment", "specific_use"];
/// `PenetrateType`.
pub const PENETRATE_TYPE: &[&str] = &["none", "small", "medium", "large"];
/// `weapOverlayReticle_t`.
pub const OVERLAY_RETICLE: &[&str] = &["none", "crosshair"];
/// `eAttachment` (the exe's own name table, G14 §1).
pub const ATTACHMENTS: [&str; 30] = [
    "none", "acog", "dualclip", "dualoptic", "dw", "extbarrel", "extclip", "extramags", "fastads", "fastreload", "fmj", "gl",
    "grip", "holo", "ir", "is", "longbreath", "mk", "mms", "rangefinder", "reflex", "rf", "sf", "silencer", "stackfire",
    "stalker", "steadyaim", "swayreduc", "tacknife", "vzoom",
];

/// Whether `weapAnimFiles_t` slot `s` is one of the dual-wield left gun's (`FIRE_LEFT` ..
/// `RELOAD_LEFT`): a column only a dual-wield weapon fills — BO2 binds the slots on a single gun
/// too (`FUN_0052d3e0`), but nothing ever plays them.
pub fn is_left_slot(s: usize) -> bool {
    (0x4e..=0x54).contains(&s)
}

/// The order both sides list parts in: by model, then tag. Which part an engine composes first is
/// not a fact either game shows, so the part columns hold the parts sorted (models, tags, offsets
/// and rotations stay index-aligned).
pub fn part_order(model: &str, tag: &str) -> (String, String) {
    (model.to_string(), tag.to_string())
}

/// An enum value's name, or its number when out of range (so an unknown value still diffs).
pub fn enum_name(table: &[&str], v: u32) -> String {
    table.get(v as usize).map_or_else(|| format!("#{v}"), |s| s.to_string())
}

/// The overridable sounds of a unique (G14 §4, bit `i` ↔ `0x100 + 4i`), as columns.
pub const OVERRIDE_SOUNDS: [&str; 12] = [
    "fire_sound", "fire_sound_player", "fire_loop_sound", "fire_loop_sound_player", "fire_loop_end_sound",
    "fire_loop_end_sound_player", "fire_start_sound", "fire_stop_sound", "fire_start_sound_player", "fire_stop_sound_player",
    "fire_last_sound", "fire_last_sound_player",
];

/// Sounds only the weapon def carries (no attachment override), as columns.
pub const DEF_SOUNDS: [&str; 12] = [
    "empty_fire_sound_player", "pullback_sound_player", "melee_swipe_sound_player", "melee_hit_sound", "rechamber_sound_player",
    "reload_sound_player", "reload_empty_sound_player", "raise_sound_player", "first_raise_sound_player", "putaway_sound_player",
    "ads_zoom_sound", "projectile_explosion_sound",
];

/// Effects as columns: the two overridable ones (G14 §4) then the def's own.
pub const EFFECTS: [&str; 7] = [
    "view_flash_effect", "world_flash_effect", "view_shell_eject_effect", "world_shell_eject_effect",
    "view_last_shot_eject_effect", "world_last_shot_eject_effect", "projectile_explosion_effect",
];

/// The weapons table's schema.
pub fn schema() -> Schema {
    let mut s = Schema::new("weapons", ["weapon", "combo"]);
    let ms = Kind::Float(MS_EPS);
    let inch = Kind::Float(INCH_EPS);
    let f = Kind::Float(F32_EPS);
    let v3 = Kind::Floats(F32_EPS);

    // Identity and names.
    s.col("display_name", "names", Kind::Text, "the weapon's localized name (szDisplayName through the install's english zones)")
        .col("attachment_names", "names", Kind::List, "each attachment's localized display name, in combo order")
        .col("weap_type", "identity", Kind::Text, "weapType_t")
        .col("weap_class", "identity", Kind::Text, "weapClass_t")
        .col("inventory", "identity", Kind::Text, "weapInventoryType_t")
        .col("offhand_class", "identity", Kind::Text, "OffhandClass")
        .col("offhand_slot", "identity", Kind::Text, "OffhandSlot (WeaponDef.offhandSlot): which offhand button the weapon is on")
        .col("fire_type", "fire", Kind::Text, "weapFireType_t; stackfire's replaces the def's (G14 §4, FUN_008ecd90)")
        .col("shot_count", "fire", Kind::Int, "pellets per shot")
        .col("shots_before_rechamber", "fire", Kind::Int, "iShotsBeforeRechamber")
        .col("bolt_action", "fire", Kind::Bool, "bBoltAction");

    // Timing (ms; the combo's scale products are truncated to int like the engine, G14 §4).
    for (c, doc) in [
        ("fire_time", "iFireTime × Π fFireTimeScale (FUN_008ec740)"),
        ("last_fire_time", "iLastFireTime"),
        ("fire_delay", "iFireDelay"),
        ("burst_delay", "iBurstDelayTime"),
        ("rechamber_time", "iRechamberTime"),
        ("rechamber_bolt_time", "iRechamberBoltTime"),
        ("hold_fire_time", "iHoldFireTime"),
        ("reload_time", "iReloadTime × Π fReloadTimeScale (FUN_008ec7a0)"),
        ("reload_empty_time", "iReloadEmptyTime × Π fReloadEmptyTimeScale (FUN_008ec800)"),
        ("reload_add_time", "iReloadAddTime × Π fReloadAddTimeScale (FUN_008ec860)"),
        ("reload_empty_add_time", "iReloadEmptyAddTime"),
        ("reload_quick_time", "iReloadQuickTime × Π fReloadQuickTimeScale (FUN_008ec8c0)"),
        ("reload_quick_empty_time", "iReloadQuickEmptyTime × Π fReloadQuickEmptyTimeScale (FUN_008ec920)"),
        ("reload_quick_add_time", "iReloadQuickAddTime × Π fReloadQuickAddTimeScale (FUN_008ec980)"),
        ("reload_start_time", "iReloadStartTime"),
        ("reload_end_time", "iReloadEndTime"),
        ("raise_time", "iRaiseTime"),
        ("first_raise_time", "first unique differing from the base unique, else iFirstRaiseTime (FUN_008ed100)"),
        ("drop_time", "iDropTime"),
        ("alt_raise_time", "the variant's iAltRaiseTime"),
        ("alt_drop_time", "first unique differing from the base unique, else iAltDropTime (FUN_008ed1e0)"),
        ("quick_raise_time", "quickRaiseTime"),
        ("quick_drop_time", "quickDropTime"),
        ("empty_raise_time", "iEmptyRaiseTime"),
        ("empty_drop_time", "iEmptyDropTime"),
        ("sprint_in_time", "sprintInTime"),
        ("sprint_out_time", "sprintOutTime"),
        ("ads_in_time", "iAdsTransInTime × Π fAdsTransInTimeScale (FUN_008ecca0)"),
        ("ads_out_time", "iAdsTransOutTime × Π fAdsTransOutTimeScale (FUN_008ecca0)"),
        ("melee_time", "iMeleeTime of the weapon a melee uses: the class's knife_mp, a melee/riot-shield weapon itself (_class.gsc:586, _scoreevents.gsc:375)"),
        ("melee_delay", "iMeleeDelay of the same melee weapon"),
        ("spin_up_time", "iSpinUpTime"),
        ("spin_down_time", "iSpinDownTime"),
        ("fuse_time", "fuseTime"),
        ("detonate_time", "iDetonateTime"),
    ] {
        s.col(c, "timing", ms, doc);
    }
    s.col("reload_ammo_add", "timing", Kind::Int, "first unique differing from the base unique, else the def's (FUN_008ed250)")
        .col("reload_start_add", "timing", Kind::Int, "same rule (FUN_008ed2c0)")
        .col("segmented_reload", "timing", Kind::Bool, "the def's OR any unique's (FUN_008ebc70)");

    // Ammo.
    s.col("clip_size", "ammo", Kind::Int, "stackfire's iClipSize, else round(iClipSize × Π clipSizeScale) (FUN_008e5e50)")
        .col("start_ammo_rounds", "ammo", Kind::Int, "rounds given at spawn, the loaded clip included: clip × iStartAmmo when ammoCountClipRelative (FUN_005e0910); the mod's magazine plus start reserve (an offhand's start count)")
        .col("max_ammo_rounds", "ammo", Kind::Int, "the stock cap: the spare rounds carried beside the magazine, clip × iMaxAmmo when ammoCountClipRelative (FUN_005e08d0); iMaxAmmo excludes the loaded clip (crossbow_mp: iStartAmmo 6, clip 3, iMaxAmmo 3); the mod's max reserve")
        .col("clip_only", "ammo", Kind::Bool, "bClipOnly")
        .col("dual_mag", "ammo", Kind::Bool, "the variant's OR any attachment's (FUN_008eba90)")
        .col("shared_ammo", "ammo", Kind::Bool, "the def's OR any attachment's (FUN_008ebc00)");

    // Damage.
    s.col("damage", "damage", Kind::List, "damage[6]")
        .col("damage_range", "damage", Kind::Floats(INCH_EPS), "damageRange[6] × Π fDamageRangeScale (FUN_008edb80), inches")
        .col("min_player_damage", "damage", Kind::Int, "minPlayerDamage")
        .col("melee_damage", "damage", Kind::Int, "iMeleeDamage of the weapon a melee uses (see melee_time)")
        .col("location_multipliers", "damage", Kind::Floats(F32_EPS), "21 hit locations: first unique differing from the base unique, else the def's (FUN_008ed780)")
        .col("max_range", "damage", inch, "maxRange")
        .col("penetrate_type", "damage", Kind::Text, "PenetrateType: the first attachment's non-zero penetrateType, else the def's (FUN_008ecec0)")
        .col("explosion_radius", "damage", inch, "iExplosionRadius")
        .col("explosion_radius_min", "damage", inch, "iExplosionRadiusMin")
        .col("explosion_inner_damage", "damage", Kind::Int, "iExplosionInnerDamage")
        .col("explosion_outer_damage", "damage", Kind::Int, "iExplosionOuterDamage")
        .col("projectile_speed", "damage", inch, "iProjectileSpeed (inches/s)")
        .col("projectile_speed_up", "damage", inch, "iProjectileSpeedUp")
        .col("projectile_impact_explode", "damage", Kind::Bool, "bProjImpactExplode")
        .col("projectile_model", "damage", Kind::Text, "projectileModel")
        .col("impact_type", "damage", Kind::Text, "ImpactType");

    // Spread.
    s.col("hip_spread", "spread", v3, "stand/ducked/prone min,max × Π fHipSpreadMin/MaxScale (FUN_008ed330)")
        .col("hip_spread_decay", "spread", f, "fHipSpreadDecayRate")
        .col("hip_spread_fire_add", "spread", f, "fHipSpreadFireAdd")
        .col("hip_spread_move_add", "spread", f, "fHipSpreadMoveAdd")
        .col("ads_spread", "spread", f, "fAdsSpread")
        .col("stack_fire_spread", "spread", f, "first positive attachment value, else the def's (FUN_008ece60)")
        .col("stack_fire_accuracy_decay", "spread", f, "same rule (FUN_008ece00)");

    // ADS.
    for (i, c) in ["ads_zoom_fov1", "ads_zoom_fov2", "ads_zoom_fov3"].into_iter().enumerate() {
        s.col(c, "ads", f, format!("fAdsZoomFov{}: the last attachment differing from defaultattachment wins (FUN_008ec3f0)", i + 1));
    }
    s
        .col("ads_zoom_in_frac", "ads", f, "first attachment differing from defaultattachment, else the variant's (FUN_008ec4b0)")
        .col("ads_zoom_out_frac", "ads", f, "same rule (FUN_008ec520)")
        .col("ads_move_speed_scale", "ads", f, "the def's if > 0 else 1, then the last attachment differing from defaultattachment replaces it (FUN_008ed080); the mod's side is its stored ADS walk divided by the converter's 0.35 base")
        .col("move_speed_scale", "ads", f, "moveSpeedScale")
        .col("sprint_duration_scale", "ads", f, "sprintDurationScale")
        .col("alt_scope_ads_in_time", "ads", f, "first attachment differing from defaultattachment, else the def's (FUN_008ecbc0)")
        .col("alt_scope_ads_out_time", "ads", f, "same rule (FUN_008ecc30)")
        .col("ads_dof", "ads", Kind::Floats(F32_EPS), "adsDofStart, adsDofEnd: first unique differing from the base unique, else the def's (FUN_008ebdb0)")
        .col("overlay_alpha_scale", "ads", f, "the variant's fOverlayAlphaScale");

    // Kick and recoil.
    s.col("ads_view_kick", "kick", v3, "fAdsViewKick pitch min,max, yaw min,max")
        .col("hip_view_kick", "kick", v3, "fHipViewKick pitch min,max, yaw min,max")
        .col("view_kick_min", "kick", v3, "fAds/HipViewKickMinMagnitude")
        .col("view_kick_center", "kick", v3, "ADS centre speed × Π fAdsViewKickCenterSpeedScale (FUN_008ecf70), hip centre speed")
        .col("ads_gun_kick", "kick", v3, "pitch min,max, yaw min,max, accel, speed max, speed decay, static decay, reduced percent")
        .col("ads_gun_kick_reduced_bullets", "kick", Kind::Int, "adsGunKickReducedKickBullets")
        .col("hip_gun_kick", "kick", v3, "the same block for the hip")
        .col("hip_gun_kick_reduced_bullets", "kick", Kind::Int, "hipGunKickReducedKickBullets")
        .col("gun_max", "kick", v3, "fGunMaxPitch, fGunMaxYaw")
        .col("ads_recoil_reduction", "kick", v3, "rate, limit: the last attachment differing from defaultattachment, else the def's (FUN_008ecd00)")
        .col("ads_recoil_return_rate", "kick", f, "fAdsRecoilReturnRate");

    // Sway and the gun's position.
    s.col("hip_sway", "sway", v3, "max angle, lerp speed, pitch, yaw, horiz, vert: the first attachment with swayOverride supplies all (FUN_008ec590)")
        .col("ads_sway", "sway", v3, "max angle, lerp speed, pitch, yaw, horiz, vert: the first with adsSwayOverride, else the def's and the variant's horiz/vert (FUN_008ec660)")
        .col("idle_amount", "sway", v3, "ADS (fAdsIdleAmount × Π fAdsIdleAmountScale, FUN_008ecfd0), hip (first differing from defaultattachment, FUN_008ed3a0)")
        .col("stand_move", "viewpos", v3, "vStandMove (attachment overrides per G14 §4, FUN_008ed410)")
        .col("stand_rot", "viewpos", v3, "vStandRot (FUN_008ed410)")
        .col("strafe_move", "viewpos", v3, "vStrafeMove")
        .col("strafe_rot", "viewpos", v3, "vStrafeRot (FUN_008ed4f0)")
        .col("ducked_ofs", "viewpos", v3, "vDuckedOfs")
        .col("ducked_move", "viewpos", v3, "vDuckedMove")
        .col("ducked_rot", "viewpos", v3, "vDuckedRot")
        .col("prone_ofs", "viewpos", v3, "vProneOfs")
        .col("prone_move", "viewpos", v3, "vProneMove")
        .col("prone_rot", "viewpos", v3, "vProneRot")
        .col("sprint_ofs", "viewpos", v3, "vSprintOfs")
        .col("sprint_rot", "viewpos", v3, "vSprintRot")
        .col("pos_rates", "viewpos", v3, "fPosMoveRate, fPosProneMoveRate, fPosRotRate, fPosProneRotRate")
        .col("ads_aim_pitch", "viewpos", f, "fAdsAimPitch")
        .col("ads_bob_factor", "viewpos", f, "fAdsBobFactor");

    // Crosshair and the scope overlay.
    s.col("reticle_center", "reticle", Kind::Text, "reticleCenter material")
        .col("reticle_side", "reticle", Kind::Text, "reticleSide material")
        .col("reticle_sizes", "reticle", Kind::List, "iReticleCenterSize, iReticleSideSize, iReticleMinOfs")
        .col("reticle_fracs", "reticle", v3, "fHipReticleSidePos, fAdsCrosshairInFrac, fAdsCrosshairOutFrac")
        .col("keep_crosshair_when_ads", "reticle", Kind::Bool, "bKeepCrosshairWhenADS")
        .col("overlay_material", "overlay", Kind::Text, "first unique differing from the base unique, else the variant's (FUN_008ed570)")
        .col("overlay_material_low_res", "overlay", Kind::Text, "same rule (FUN_008ed5f0)")
        .col("overlay_reticle", "overlay", Kind::Text, "weapOverlayReticle_t, same rule over the def's (FUN_008ed670)")
        .col("hud_icon", "overlay", Kind::Text, "hudIcon material")
        .col("dpad_icon", "overlay", Kind::Text, "the variant's dpadIcon material");

    // Alt mode, dual wield, flags, specialties.
    s.col("alt_weapon", "alt", Kind::Text, "the first single unique's alt weapon, else the variant's (FUN_008ec9e0)")
        .col("alt_weapon_ads_only", "alt", Kind::Bool, "the def's OR any attachment's (FUN_008ecb00)")
        .col("alt_weapon_disable_switching", "alt", Kind::Bool, "the def's OR any attachment's (FUN_008ecb60)")
        .col("dual_wield", "dualwield", Kind::Bool, "the def's OR any attachment's (FUN_008eb9c0)")
        .col("dual_wield_weapon", "dualwield", Kind::Text, "the left gun: the first single unique's, else the def's (FUN_008eca70)")
        .col("dual_wield_view_model", "dualwield", Kind::Text, "the left gun's gunXModel[0]")
        .col("silenced", "flags", Kind::Bool, "the variant's OR any attachment's (FUN_008eba20)")
        .col("laser_sight", "flags", Kind::Bool, "the def's OR any attachment's (FUN_008ebb00)")
        .col("infrared", "flags", Kind::Bool, "the variant's OR any attachment's (FUN_008ebb70)")
        .col("mms_weapon", "flags", Kind::Bool, "the variant's OR any attachment's (FUN_008ebce0)")
        .col("use_as_melee", "flags", Kind::Bool, "bUseAsMelee/bHasBayonet OR any attachment's (FUN_008da950)")
        .col("cook_off_hold", "flags", Kind::Bool, "bCookOffHold")
        .col("rifle_bullet", "flags", Kind::Bool, "bRifleBullet")
        .col("specialties", "perks", Kind::Set, "specialty bit indices: the variant's perks OR every attachment's (FUN_008ecf10, G1)");

    // The first-person parts (G14 §3) and what they draw.
    s.col("view_model", "parts", Kind::Text, "gunXModel[0]")
        .col("view_part_models", "parts", Kind::List, "hip draw: base attachViewModel slots then each attachment's resolved unique (and its additional model)")
        .col("view_part_tags", "parts", Kind::List, "each part's tag (`j_gun` when the authored one is empty)")
        .col("view_part_offsets", "parts", Kind::Floats(INCH_EPS), "each part's offset, x,y,z flattened, inches")
        .col("view_part_rotations", "parts", Kind::Floats(QUAT_EPS), "each part's rotation as a unit quaternion x,y,z,w (w >= 0), flattened; BO2's pitch,yaw,roll degrees through the engine's AnglesToAxis")
        .col("view_ads_part_models", "parts", Kind::List, "ADS draw: the same list with attachViewModel[7] / viewModelADS swaps")
        .col("view_hide_tags", "hide", Kind::Set, "the merged hideTags (G14 §4, FUN_008ec1a0)")
        .col("view_part_materials", "materials", Kind::List, "each hip part's LOD0 surface materials, `;`-joined per part")
        .col("world_model", "worldparts", Kind::Text, "worldModel[0]")
        .col("world_part_models", "worldparts", Kind::List, "third person: base attachWorldModel slots then each attachment's resolved unique (FUN_008ace30)")
        .col("world_part_tags", "worldparts", Kind::List, "each world part's tag")
        .col("world_part_offsets", "worldparts", Kind::Floats(INCH_EPS), "each world part's offset, flattened, inches")
        .col("world_part_rotations", "worldparts", Kind::Floats(QUAT_EPS), "each world part's rotation as a unit quaternion x,y,z,w (w >= 0), flattened");

    // The animation each slot plays (G14 §5; an empty slot plays IDLE).
    for name in &ANIM_SLOTS[1..] {
        let doc = if is_left_slot(ANIM_SLOTS.iter().position(|n| n == name).unwrap_or(0)) {
            "the dual-wield left gun's xanim for this slot (G14 §5); empty for a single gun"
        } else {
            "the xanim this weapAnimFiles_t slot plays (G14 §5)"
        };
        s.col(format!("anim_{name}"), "anims", Kind::Text, doc);
    }

    // Sounds and effects.
    for name in OVERRIDE_SOUNDS {
        s.col(name, "sounds", Kind::Text, "the def's alias unless the base unique's soundOverrides bit lets an attachment replace it (FUN_008ebe80)");
    }
    for name in DEF_SOUNDS {
        s.col(name, "sounds", Kind::Text, "the def's alias");
    }
    for name in EFFECTS {
        s.col(name, "effects", Kind::Text, "the FxEffectDef; flash effects follow effectOverrides (FUN_008ec0c0)");
    }
    s.col("tracer", "effects", Kind::Text, "the first unique's tracerType, else the def's (FUN_008eb420)");

    // Camouflage and the CaC context of the row.
    s.col("view_part_camos", "camo", Kind::List, "the WeaponCamo a class camo uses on the gun and each hip part: the def's for the gun and base parts, the attachment's single unique's for its parts (FUN_00529600)")
        .col("cac_group", "cac", Kind::Text, "the CaC group the weapon lists under (statstable column 2: weapon_assault, weapon_smg, ...); `` for weapons CaC never offers")
        .col("cac_slot", "cac", Kind::Text, "statstable column 13: primary, secondary, primarygrenade, specialgrenade; `` for weapons CaC never offers")
        .col("cac_requires", "cac", Kind::Set, "the wildcards a class needs for this combination (G15: primarygunfighter for a primary's third attachment, secondarygunfighter for a secondary's second)");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_builds_and_names_every_anim_slot_once() {
        let s = schema();
        assert_eq!(s.columns.iter().filter(|c| c.area == "anims").count(), 0x57);
        assert!(s.index("anim_ads_up").is_some() && s.index("anim_root").is_none());
        assert_eq!(ANIMS_CHECK, ANIM_SLOTS.len());
    }

    const ANIMS_CHECK: usize = 0x58;
}
