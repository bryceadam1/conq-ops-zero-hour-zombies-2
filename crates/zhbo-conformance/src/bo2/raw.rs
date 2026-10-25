//! Raw views of BO2's weapon structs in a loaded zone, read field by field through the generated
//! layouts (`zhbo_formats::t6zone::layouts`).
//!
//! The oracle deliberately does not use `zhbo_formats::weapon::read`: that parser already makes
//! choices for the converter (milliseconds to seconds, the overlay fallback, the base-parts merge),
//! and the expected side must apply BO2's own rules to BO2's own fields.

use zhbo_formats::t6zone::layouts::{WeaponAttachment as WA, WeaponAttachmentUnique as WU, WeaponDef as WD, WeaponVariantDef as WV};
use zhbo_formats::t6zone::Zone;

/// `NUM_WEAP_ANIMS`.
pub const ANIMS: usize = 0x58;
/// `ATTACHMENT_TYPE_COUNT`: the `eAttachment` kinds (index 30 is "unknown").
pub const KINDS: usize = 30;
/// The `attachmentUniques` array's length (`WeaponFullDef::attachmentUniques[95]`, OAT
/// `T6_Assets.h`): 30 single uniques by `eAttachment`, then the combined children.
pub const UNIQUES: u32 = 95;

fn opt_str(z: &Zone, field: u32) -> Option<String> {
    (z.u32(field) != 0).then(|| z.str_ptr(field)).filter(|s| !s.is_empty())
}

/// The name of the XModel/Material/FxEffectDef/TracerDef at the pointer in `field` (each starts
/// with its name), without the `,` BO2 prefixes on some material names.
fn named(z: &Zone, field: u32) -> Option<String> {
    let p = z.u32(field);
    (p != 0).then(|| z.str_ptr(p).trim_start_matches(',').to_string()).filter(|s| !s.is_empty())
}

fn vec3(z: &Zone, at: u32) -> [f32; 3] {
    [z.f32(at), z.f32(at + 4), z.f32(at + 8)]
}

/// A zero-terminated `hideTags` list of up to 32 script strings.
fn tags(z: &Zone, list: u32) -> Vec<String> {
    if list == 0 {
        return Vec::new();
    }
    (0..32).map(|i| z.u16(list + 2 * i)).take_while(|&t| t != 0).map(|t| z.script_string(t).to_string()).collect()
}

/// A `char*[NUM_WEAP_ANIMS]` table.
fn anim_table(z: &Zone, table: u32) -> Vec<Option<String>> {
    (0..ANIMS as u32).map(|i| if table == 0 { None } else { opt_str(z, table + 4 * i) }).collect()
}

/// A `WeaponVariantDef`.
#[derive(Clone, Copy)]
pub struct Variant<'z> {
    pub z: &'z Zone,
    pub at: u32,
}

/// A `WeaponDef`.
#[derive(Clone, Copy)]
pub struct Def<'z> {
    pub z: &'z Zone,
    pub at: u32,
}

/// A `WeaponAttachment`.
#[derive(Clone, Copy)]
pub struct Attachment<'z> {
    pub z: &'z Zone,
    pub at: u32,
}

/// A `WeaponAttachmentUnique`.
#[derive(Clone, Copy)]
pub struct Unique<'z> {
    pub z: &'z Zone,
    pub at: u32,
}

impl PartialEq for Unique<'_> {
    fn eq(&self, o: &Self) -> bool {
        self.at == o.at
    }
}

macro_rules! getters {
    ($ty:ident, $m:ident { $( $kind:ident $name:ident = $field:ident ; )* }) => {
        impl<'z> $ty<'z> {
            $( getters!(@one $kind $name $m::$field); )*
        }
    };
    (@one f32 $name:ident $f:path) => { pub fn $name(&self) -> f32 { self.z.f32(self.at + $f) } };
    (@one i32 $name:ident $f:path) => { pub fn $name(&self) -> i32 { self.z.u32(self.at + $f) as i32 } };
    (@one u32 $name:ident $f:path) => { pub fn $name(&self) -> u32 { self.z.u32(self.at + $f) } };
    (@one bool $name:ident $f:path) => { pub fn $name(&self) -> bool { self.z.u8(self.at + $f) != 0 } };
    (@one str $name:ident $f:path) => { pub fn $name(&self) -> Option<String> { opt_str(self.z, self.at + $f) } };
    (@one named $name:ident $f:path) => { pub fn $name(&self) -> Option<String> { named(self.z, self.at + $f) } };
    (@one vec3 $name:ident $f:path) => { pub fn $name(&self) -> [f32; 3] { vec3(self.z, self.at + $f) } };
}

impl<'z> Variant<'z> {
    pub fn name(&self) -> String {
        self.z.str_ptr(self.at + WV::szInternalName)
    }
    pub fn def(&self) -> Option<Def<'z>> {
        let d = self.z.u32(self.at + WV::weapDef);
        (d != 0).then_some(Def { z: self.z, at: d })
    }
    /// `attachments[k]` (indexed by `eAttachment`).
    pub fn attachment(&self, k: usize) -> Option<Attachment<'z>> {
        let arr = self.z.u32(self.at + WV::attachments);
        if arr == 0 || k >= KINDS {
            return None;
        }
        let a = self.z.u32(arr + 4 * k as u32);
        (a != 0).then_some(Attachment { z: self.z, at: a })
    }
    /// `attachmentUniques[i]`: `i < 30` is the single unique of `eAttachment` `i` (0 the base
    /// unique), higher indices the combined children (G14 §2).
    pub fn unique(&self, i: u32) -> Option<Unique<'z>> {
        let arr = self.z.u32(self.at + WV::attachmentUniques);
        if arr == 0 || i >= UNIQUES {
            return None;
        }
        let u = self.z.u32(arr + 4 * i);
        (u != 0).then_some(Unique { z: self.z, at: u })
    }
    pub fn anims(&self) -> Vec<Option<String>> {
        anim_table(self.z, self.z.u32(self.at + WV::szXAnims))
    }
    pub fn hide_tags(&self) -> Vec<String> {
        tags(self.z, self.z.u32(self.at + WV::hideTags))
    }
    /// `attachViewModel[i]` (XModel) and its tag, for `i < 8`.
    pub fn attach_view_model(&self, i: u32) -> (Option<String>, Option<String>) {
        let (m, t) = (self.z.u32(self.at + WV::attachViewModel), self.z.u32(self.at + WV::attachViewModelTag));
        (if m == 0 { None } else { named(self.z, m + 4 * i) }, if t == 0 { None } else { opt_str(self.z, t + 4 * i) })
    }
    pub fn attach_view_offset(&self, i: u32) -> [f32; 3] {
        vec3(self.z, self.at + WV::attachViewModelOffsets + 12 * i)
    }
    pub fn attach_view_rotation(&self, i: u32) -> [f32; 3] {
        vec3(self.z, self.at + WV::attachViewModelRotations + 12 * i)
    }
    pub fn attach_world_model(&self, i: u32) -> (Option<String>, Option<String>) {
        let (m, t) = (self.z.u32(self.at + WV::attachWorldModel), self.z.u32(self.at + WV::attachWorldModelTag));
        (if m == 0 { None } else { named(self.z, m + 4 * i) }, if t == 0 { None } else { opt_str(self.z, t + 4 * i) })
    }
    pub fn attach_world_offset(&self, i: u32) -> [f32; 3] {
        vec3(self.z, self.at + WV::attachWorldModelOffsets + 12 * i)
    }
    pub fn attach_world_rotation(&self, i: u32) -> [f32; 3] {
        vec3(self.z, self.at + WV::attachWorldModelRotations + 12 * i)
    }
    pub fn perks(&self) -> (u32, u32) {
        (self.z.u32(self.at + WV::perks), self.z.u32(self.at + WV::perks + 4))
    }
}

getters!(Variant, WV {
    str display_name = szDisplayName;
    str alt_weapon_name = szAltWeaponName;
    u32 alt_weapon_index = altWeaponIndex;
    u32 allowed_mask = iAttachments;
    bool ignore_attachments = bIgnoreAttachments;
    i32 clip_size = iClipSize;
    i32 reload_time = iReloadTime;
    i32 reload_empty_time = iReloadEmptyTime;
    i32 reload_quick_time = iReloadQuickTime;
    i32 reload_quick_empty_time = iReloadQuickEmptyTime;
    i32 ads_in_time = iAdsTransInTime;
    i32 ads_out_time = iAdsTransOutTime;
    i32 alt_raise_time = iAltRaiseTime;
    f32 ads_sway_horiz_scale = fAdsSwayHorizScale;
    f32 ads_sway_vert_scale = fAdsSwayVertScale;
    f32 ads_view_kick_center_speed = fAdsViewKickCenterSpeed;
    f32 hip_view_kick_center_speed = fHipViewKickCenterSpeed;
    f32 ads_zoom_fov1 = fAdsZoomFov1;
    f32 ads_zoom_fov2 = fAdsZoomFov2;
    f32 ads_zoom_fov3 = fAdsZoomFov3;
    f32 ads_zoom_in_frac = fAdsZoomInFrac;
    f32 ads_zoom_out_frac = fAdsZoomOutFrac;
    f32 overlay_alpha_scale = fOverlayAlphaScale;
    bool silenced = bSilenced;
    bool dual_mag = bDualMag;
    bool infrared = bInfraRed;
    named overlay_material = overlayMaterial;
    named overlay_material_low_res = overlayMaterialLowRes;
    named dpad_icon = dpadIcon;
    bool mms_weapon = mmsWeapon;
    f32 mms_max_dist = mmsMaxDist;
    vec3 ik_left_hand_offset = ikLeftHandOffset;
    vec3 ik_left_hand_rotation = ikLeftHandRotation;
});

impl<'z> Def<'z> {
    pub fn damage(&self) -> [i32; 6] {
        std::array::from_fn(|i| self.z.u32(self.at + WD::damage + 4 * i as u32) as i32)
    }
    pub fn damage_range(&self) -> [f32; 6] {
        std::array::from_fn(|i| self.z.f32(self.at + WD::damageRange + 4 * i as u32))
    }
    /// `locationDamageMultipliers[HITLOC_NUM]` (21), empty when the pointer is NULL.
    pub fn location_multipliers(&self) -> Vec<f32> {
        let p = self.z.u32(self.at + WD::locationDamageMultipliers);
        if p == 0 {
            return Vec::new();
        }
        (0..21).map(|i| self.z.f32(p + 4 * i)).collect()
    }
    /// `gunXModel[0]`.
    pub fn view_model(&self) -> Option<String> {
        let p = self.z.u32(self.at + WD::gunXModel);
        if p == 0 { None } else { named(self.z, p) }
    }
    /// `worldModel[0]`.
    pub fn world_model(&self) -> Option<String> {
        let p = self.z.u32(self.at + WD::worldModel);
        if p == 0 { None } else { named(self.z, p) }
    }
    /// A sound alias name field (`fireSound`, …) by its layout offset.
    pub fn sound(&self, field: u32) -> Option<String> {
        opt_str(self.z, self.at + field)
    }
    /// An `FxEffectDef*` field's effect name by its layout offset.
    pub fn effect(&self, field: u32) -> Option<String> {
        named(self.z, self.at + field)
    }
}

getters!(Def, WD {
    u32 weap_type = weapType;
    u32 weap_class = weapClass;
    u32 inventory = inventoryType;
    u32 fire_type = fireType;
    u32 offhand_class = offhandClass;
    u32 impact_type = impactType;
    u32 penetrate_type = penetrateTWeaponAttachmentype;
    i32 start_ammo = iStartAmmo;
    i32 max_ammo = iMaxAmmo;
    bool ammo_clip_relative = ammoCountClipRelative;
    i32 shot_count = shotCount;
    i32 shots_before_rechamber = iShotsBeforeRechamber;
    i32 fire_time = iFireTime;
    i32 last_fire_time = iLastFireTime;
    i32 fire_delay = iFireDelay;
    i32 burst_delay = iBurstDelayTime;
    i32 rechamber_time = iRechamberTime;
    i32 rechamber_bolt_time = iRechamberBoltTime;
    i32 hold_fire_time = iHoldFireTime;
    i32 reload_add_time = iReloadAddTime;
    i32 reload_empty_add_time = iReloadEmptyAddTime;
    i32 reload_quick_add_time = iReloadQuickAddTime;
    i32 reload_start_time = iReloadStartTime;
    i32 reload_end_time = iReloadEndTime;
    i32 reload_start_add = iReloadStartAdd;
    i32 reload_ammo_add = iReloadAmmoAdd;
    bool segmented_reload = bSegmentedReload;
    i32 raise_time = iRaiseTime;
    i32 first_raise_time = iFirstRaiseTime;
    i32 drop_time = iDropTime;
    i32 alt_drop_time = iAltDropTime;
    i32 quick_raise_time = quickRaiseTime;
    i32 quick_drop_time = quickDropTime;
    i32 empty_raise_time = iEmptyRaiseTime;
    i32 empty_drop_time = iEmptyDropTime;
    i32 sprint_in_time = sprintInTime;
    i32 sprint_out_time = sprintOutTime;
    i32 melee_time = iMeleeTime;
    i32 melee_delay = iMeleeDelay;
    i32 melee_damage = iMeleeDamage;
    i32 spin_up_time = iSpinUpTime;
    i32 spin_down_time = iSpinDownTime;
    i32 fuse_time = fuseTime;
    i32 detonate_time = iDetonateTime;
    i32 min_player_damage = minPlayerDamage;
    i32 explosion_radius = iExplosionRadius;
    i32 explosion_radius_min = iExplosionRadiusMin;
    i32 explosion_inner_damage = iExplosionInnerDamage;
    i32 explosion_outer_damage = iExplosionOuterDamage;
    i32 projectile_speed = iProjectileSpeed;
    i32 projectile_speed_up = iProjectileSpeedUp;
    bool projectile_impact_explode = bProjImpactExplode;
    named projectile_model = projectileModel;
    f32 hip_spread_stand_min = fHipSpreadStandMin;
    f32 hip_spread_stand_max = hipSpreadStandMax;
    f32 hip_spread_ducked_min = fHipSpreadDuckedMin;
    f32 hip_spread_ducked_max = hipSpreadDuckedMax;
    f32 hip_spread_prone_min = fHipSpreadProneMin;
    f32 hip_spread_prone_max = hipSpreadProneMax;
    f32 hip_spread_decay = fHipSpreadDecayRate;
    f32 hip_spread_fire_add = fHipSpreadFireAdd;
    f32 hip_spread_move_add = fHipSpreadMoveAdd;
    f32 ads_spread = fAdsSpread;
    f32 ads_view_kick_pitch_min = fAdsViewKickPitchMin;
    f32 ads_view_kick_pitch_max = fAdsViewKickPitchMax;
    f32 ads_view_kick_yaw_min = fAdsViewKickYawMin;
    f32 ads_view_kick_yaw_max = fAdsViewKickYawMax;
    f32 hip_view_kick_pitch_min = fHipViewKickPitchMin;
    f32 hip_view_kick_pitch_max = fHipViewKickPitchMax;
    f32 hip_view_kick_yaw_min = fHipViewKickYawMin;
    f32 hip_view_kick_yaw_max = fHipViewKickYawMax;
    f32 ads_view_kick_min = fAdsViewKickMinMagnitude;
    f32 hip_view_kick_min = fHipViewKickMinMagnitude;
    f32 ads_gun_kick_pitch_min = fAdsGunKickPitchMin;
    f32 ads_gun_kick_pitch_max = fAdsGunKickPitchMax;
    f32 ads_gun_kick_yaw_min = fAdsGunKickYawMin;
    f32 ads_gun_kick_yaw_max = fAdsGunKickYawMax;
    f32 ads_gun_kick_accel = fAdsGunKickAccel;
    f32 ads_gun_kick_speed_max = fAdsGunKickSpeedMax;
    f32 ads_gun_kick_speed_decay = fAdsGunKickSpeedDecay;
    f32 ads_gun_kick_static_decay = fAdsGunKickStaticDecay;
    i32 ads_gun_kick_reduced_bullets = adsGunKickReducedKickBullets;
    f32 ads_gun_kick_reduced_percent = adsGunKickReducedKickPercent;
    f32 hip_gun_kick_pitch_min = fHipGunKickPitchMin;
    f32 hip_gun_kick_pitch_max = fHipGunKickPitchMax;
    f32 hip_gun_kick_yaw_min = fHipGunKickYawMin;
    f32 hip_gun_kick_yaw_max = fHipGunKickYawMax;
    f32 hip_gun_kick_accel = fHipGunKickAccel;
    f32 hip_gun_kick_speed_max = fHipGunKickSpeedMax;
    f32 hip_gun_kick_speed_decay = fHipGunKickSpeedDecay;
    f32 hip_gun_kick_static_decay = fHipGunKickStaticDecay;
    i32 hip_gun_kick_reduced_bullets = hipGunKickReducedKickBullets;
    f32 hip_gun_kick_reduced_percent = hipGunKickReducedKickPercent;
    f32 gun_max_pitch = fGunMaxPitch;
    f32 gun_max_yaw = fGunMaxYaw;
    f32 ads_recoil_reduction_rate = fAdsRecoilReductionRate;
    f32 ads_recoil_reduction_limit = fAdsRecoilReductionLimit;
    f32 ads_recoil_return_rate = fAdsRecoilReturnRate;
    f32 sway_max_angle = swayMaxAngle;
    f32 sway_lerp_speed = swayLerpSpeed;
    f32 sway_pitch_scale = swayPitchScale;
    f32 sway_yaw_scale = swayYawScale;
    f32 sway_horiz_scale = swayHorizScale;
    f32 sway_vert_scale = swayVertScale;
    f32 ads_sway_max_angle = adsSwayMaxAngle;
    f32 ads_sway_lerp_speed = adsSwayLerpSpeed;
    f32 ads_sway_pitch_scale = adsSwayPitchScale;
    f32 ads_sway_yaw_scale = adsSwayYawScale;
    f32 ads_idle_amount = fAdsIdleAmount;
    f32 hip_idle_amount = fHipIdleAmount;
    vec3 stand_move = vStandMove;
    vec3 stand_rot = vStandRot;
    vec3 strafe_move = vStrafeMove;
    vec3 strafe_rot = vStrafeRot;
    vec3 ducked_ofs = vDuckedOfs;
    vec3 ducked_move = vDuckedMove;
    vec3 ducked_rot = vDuckedRot;
    vec3 prone_ofs = vProneOfs;
    vec3 prone_move = vProneMove;
    vec3 prone_rot = vProneRot;
    vec3 sprint_ofs = vSprintOfs;
    vec3 sprint_rot = vSprintRot;
    f32 pos_move_rate = fPosMoveRate;
    f32 pos_prone_move_rate = fPosProneMoveRate;
    f32 pos_rot_rate = fPosRotRate;
    f32 pos_prone_rot_rate = fPosProneRotRate;
    f32 ads_aim_pitch = fAdsAimPitch;
    f32 ads_bob_factor = fAdsBobFactor;
    f32 move_speed_scale = moveSpeedScale;
    f32 ads_move_speed_scale = adsMoveSpeedScale;
    f32 sprint_duration_scale = sprintDurationScale;
    f32 max_range = maxRange;
    f32 stack_fire_spread = stackFireSpread;
    f32 stack_fire_accuracy_decay = stackFireAccuracyDecay;
    f32 ads_dof_start = adsDofStart;
    f32 ads_dof_end = adsDofEnd;
    f32 alt_scope_ads_in_time = altScopeADSTransInTime;
    f32 alt_scope_ads_out_time = altScopeADSTransOutTime;
    bool bolt_action = bBoltAction;
    bool cook_off_hold = bCookOffHold;
    bool dual_wield = bDualWield;
    bool clip_only = bClipOnly;
    bool rifle_bullet = bRifleBullet;
    bool laser_sight = laserSight;
    bool shared_ammo = sharedAmmo;
    bool use_as_melee = bUseAsMelee;
    bool has_bayonet = bHasBayonet;
    bool alt_weapon_ads_only = bAltWeaponAdsOnly;
    bool alt_weapon_disable_switching = bAltWeaponDisableSwitching;
    u32 dual_wield_weapon_index = dualWieldWeaponIndex;
    str dual_wield_weapon_name = szDualWieldWeaponName;
    u32 overlay_reticle = overlayReticle;
    named reticle_center = reticleCenter;
    named reticle_side = reticleSide;
    i32 reticle_center_size = iReticleCenterSize;
    i32 reticle_side_size = iReticleSideSize;
    i32 reticle_min_ofs = iReticleMinOfs;
    f32 hip_reticle_side_pos = fHipReticleSidePos;
    f32 ads_crosshair_in_frac = fAdsCrosshairInFrac;
    f32 ads_crosshair_out_frac = fAdsCrosshairOutFrac;
    bool keep_crosshair_when_ads = bKeepCrosshairWhenADS;
    named hud_icon = hudIcon;
    named weapon_camo = weaponCamo;
    named tracer = tracerType;
    named enemy_tracer = enemyTracerType;
});

impl<'z> Attachment<'z> {
    pub fn name(&self) -> String {
        self.z.str_ptr(self.at + WA::szInternalName)
    }
    pub fn perks(&self) -> (u32, u32) {
        (self.z.u32(self.at + WA::perks), self.z.u32(self.at + WA::perks + 4))
    }
    /// A float field by raw offset (the view-position override block, `G14 §4`).
    pub fn f32_at(&self, off: u32) -> f32 {
        self.z.f32(self.at + off)
    }
}

getters!(Attachment, WA {
    str display_name = szDisplayName;
    u32 kind = attachmentType;
    u32 point = attachmentPoint;
    u32 penetrate_type = penetrateType;
    u32 fire_type = fireType;
    i32 first_raise_priority = firstRaisePriority;
    f32 hip_idle_amount = fHipIdleAmount;
    bool alt_weapon_ads_only = bAltWeaponAdsOnly;
    bool alt_weapon_disable_switching = bAltWeaponDisableSwitching;
    f32 alt_scope_ads_in_time = altScopeADSTransInTime;
    f32 alt_scope_ads_out_time = altScopeADSTransOutTime;
    bool silenced = bSilenced;
    bool dual_mag = bDualMag;
    bool laser_sight = laserSight;
    bool infrared = bInfraRed;
    bool use_as_melee = bUseAsMelee;
    bool dual_wield = bDualWield;
    bool shared_ammo = sharedAmmo;
    f32 damage_range_scale = fDamageRangeScale;
    f32 ads_zoom_fov1 = fAdsZoomFov1;
    f32 ads_zoom_fov2 = fAdsZoomFov2;
    f32 ads_zoom_fov3 = fAdsZoomFov3;
    f32 ads_zoom_in_frac = fAdsZoomInFrac;
    f32 ads_zoom_out_frac = fAdsZoomOutFrac;
    f32 ads_in_scale = fAdsTransInTimeScale;
    f32 ads_out_scale = fAdsTransOutTimeScale;
    f32 ads_recoil_reduction_rate = fAdsRecoilReductionRate;
    f32 ads_recoil_reduction_limit = fAdsRecoilReductionLimit;
    f32 ads_view_kick_center_speed_scale = fAdsViewKickCenterSpeedScale;
    f32 ads_idle_amount_scale = fAdsIdleAmountScale;
    bool sway_override = swayOverride;
    bool ads_sway_override = adsSwayOverride;
    f32 ads_move_speed_scale = adsMoveSpeedScale;
    f32 hip_spread_min_scale = fHipSpreadMinScale;
    f32 hip_spread_max_scale = fHipSpreadMaxScale;
    f32 fire_time_scale = fFireTimeScale;
    f32 reload_time_scale = fReloadTimeScale;
    f32 reload_empty_time_scale = fReloadEmptyTimeScale;
    f32 reload_add_time_scale = fReloadAddTimeScale;
    f32 reload_quick_time_scale = fReloadQuickTimeScale;
    f32 reload_quick_empty_time_scale = fReloadQuickEmptyTimeScale;
    f32 reload_quick_add_time_scale = fReloadQuickAddTimeScale;
    bool mms_weapon = mmsWeapon;
    f32 mms_max_dist = mmsMaxDist;
    f32 clip_size_scale = clipSizeScale;
    i32 clip_size = iClipSize;
    f32 stack_fire_spread = stackFireSpread;
    f32 stack_fire_accuracy_decay = stackFireAccuracyDecay;
});

impl<'z> Unique<'z> {
    pub fn name(&self) -> String {
        self.z.str_ptr(self.at + WU::szInternalName)
    }
    pub fn hide_tags(&self) -> Vec<String> {
        tags(self.z, self.z.u32(self.at + WU::hideTags))
    }
    pub fn anims(&self) -> Vec<Option<String>> {
        anim_table(self.z, self.z.u32(self.at + WU::szXAnims))
    }
    /// Whether `animationOverrides` has slot `s`.
    pub fn overrides_anim(&self, s: usize) -> bool {
        self.z.u32(self.at + WU::animationOverrides + 4 * (s as u32 >> 5)) & (1 << (s & 31)) != 0
    }
    pub fn sound_overrides(&self) -> u32 {
        self.z.u32(self.at + WU::soundOverrides)
    }
    pub fn effect_overrides(&self) -> u32 {
        self.z.u32(self.at + WU::effectOverrides)
    }
    /// The `i`th overridable sound (G14 §4: `0x100 + 4i`).
    pub fn override_sound(&self, i: u32) -> Option<String> {
        opt_str(self.z, self.at + WU::fireSound + 4 * i)
    }
    /// The `i`th overridable effect (bit 0 view flash, bit 1 world flash).
    pub fn override_effect(&self, i: u32) -> Option<String> {
        named(self.z, self.at + WU::viewFlashEffect + 4 * i)
    }
    pub fn location_multipliers(&self) -> Vec<f32> {
        let p = self.z.u32(self.at + WU::locationDamageMultipliers);
        if p == 0 {
            return Vec::new();
        }
        (0..21).map(|i| self.z.f32(p + 4 * i)).collect()
    }
    /// Raw 32-bit field (to compare a field with the base unique's, G14 §4).
    pub fn u32_at(&self, off: u32) -> u32 {
        self.z.u32(self.at + off)
    }
}

getters!(Unique, WU {
    u32 kind = attachmentType;
    i32 sibling = siblingLink;
    i32 child = childLink;
    u32 combined_mask = combinedAttachmentTypeMask;
    str alt_weapon_name = szAltWeaponName;
    u32 alt_weapon_index = altWeaponIndex;
    str dual_wield_weapon_name = szDualWieldWeaponName;
    u32 dual_wield_weapon_index = dualWieldWeaponIndex;
    named view_model = viewModel;
    named view_model_additional = viewModelAdditional;
    named view_model_ads = viewModelADS;
    named world_model = worldModel;
    named world_model_additional = worldModelAdditional;
    str view_model_tag = viewModelTag;
    str world_model_tag = worldModelTag;
    vec3 view_offset = viewModelOffsets;
    vec3 world_offset = worldModelOffsets;
    vec3 view_rotation = viewModelRotations;
    vec3 world_rotation = worldModelRotations;
    vec3 view_add_offset = viewModelAddOffsets;
    vec3 world_add_offset = worldModelAddOffsets;
    vec3 view_add_rotation = viewModelAddRotations;
    vec3 world_add_rotation = worldModelAddRotations;
    named weapon_camo = weaponCamo;
    bool disable_base_attachment = disableBaseWeaponAttachment;
    bool disable_base_clip = disableBaseWeaponClip;
    bool override_base_attachment_offsets = overrideBaseWeaponAttachmentOffsets;
    vec3 view_offset_base_attachment = viewModelOffsetBaseAttachment;
    vec3 world_offset_base_attachment = worldModelOffsetBaseAttachment;
    named overlay_material = overlayMaterial;
    named overlay_material_low_res = overlayMaterialLowRes;
    u32 overlay_reticle = overlayReticle;
    i32 first_raise_time = iFirstRaiseTime;
    i32 alt_raise_time = iAltRaiseTime;
    i32 alt_drop_time = iAltDropTime;
    i32 reload_ammo_add = iReloadAmmoAdd;
    i32 reload_start_add = iReloadStartAdd;
    bool segmented_reload = bSegmentedReload;
    named tracer = tracerType;
    named enemy_tracer = enemyTracerType;
    f32 ads_dof_start = adsDofStart;
    f32 ads_dof_end = adsDofEnd;
    bool override_left_hand_ik = bOverrideLeftHandIK;
    vec3 ik_left_hand_offset = ikLeftHandOffset;
    vec3 ik_left_hand_rotation = ikLeftHandRotation;
});
