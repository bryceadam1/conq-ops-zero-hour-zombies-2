/*
** Conquer Ops - Zero Hour gameplay core: the weapon's handling fields beyond the core ones: the fire cycle's extra times, the zoom levels and fractions, and BO2's per-weapon traits.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

/// The fire cycle's and reload's extra times (seconds), as BO2's `WeaponDef`/`WeaponVariantDef`
/// author them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Timing {
    /// `iLastFireTime`: the fire state's length for the magazine's last round.
    pub last_fire_time: f32,
    /// `iFireDelay`: the wait between the trigger and the shot (grenade throws, launchers).
    pub fire_delay: f32,
    /// `iRechamberTime`: the rechamber after a shot (pump and bolt actions).
    pub rechamber_time: f32,
    /// `iRechamberBoltTime`: when in the rechamber the bolt is worked.
    pub rechamber_bolt_time: f32,
    /// `iHoldFireTime`: how long a held throw/fire is held before it lets go.
    pub hold_fire_time: f32,
    /// `iShotsBeforeRechamber`: rounds fired before a rechamber (0: every shot).
    pub shots_before_rechamber: u16,
    /// `bBoltAction`: the weapon rechambers between shots.
    pub bolt_action: bool,
    /// `iReloadQuickTime`, `iReloadQuickEmptyTime` (the variant's) and `iReloadQuickAddTime`: the
    /// quick reload, each scaled by its attachments' `fReloadQuick*TimeScale` product.
    pub reload_quick_time: f32,
    pub reload_quick_empty_time: f32,
    pub reload_quick_add_time: f32,
    /// `iReloadStartAdd`: rounds the segmented reload's start adds.
    pub reload_start_add: u16,
    /// `iSpinDownTime`: the minigun barrels' spin-down.
    pub spin_down_time: f32,
    /// `iDetonateTime`: the detonator's press.
    pub detonate_time: f32,
    /// `sprintInTime`: lowering the weapon into a sprint.
    pub sprint_in_time: f32,
}

/// The aimed view: the zoom levels, how the zoom follows the ADS lerp, depth of field, idle sway.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Zoom {
    /// `fAdsZoomFov2`, `fAdsZoomFov3`: the second and third zoom levels (the variable zoom); the
    /// first is the def's `ads_fov`.
    pub fov2: f32,
    pub fov3: f32,
    /// `fAdsZoomInFrac`, `fAdsZoomOutFrac`: where in the ADS lerp the zoom starts (in) and ends
    /// (out).
    pub in_frac: f32,
    pub out_frac: f32,
    /// `altScopeADSTransInTime`, `altScopeADSTransOutTime`: the second optic's in and out (s).
    pub alt_scope_in_time: f32,
    pub alt_scope_out_time: f32,
    /// `adsDofStart`, `adsDofEnd`: the aimed depth of field (inches).
    pub dof: (f32, f32),
    /// `fOverlayAlphaScale`: the scope overlay's opacity.
    pub overlay_alpha_scale: f32,
    /// `fAdsIdleAmount` (× the attachments' `fAdsIdleAmountScale`) and `fHipIdleAmount`: the idle
    /// sway, aimed and from the hip.
    pub idle_amount: (f32, f32),
}

/// BO2's `PenetrateType`: how far a bullet goes through surfaces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Penetrate {
    #[default]
    None,
    Small,
    Medium,
    Large,
}

impl Penetrate {
    pub const ALL: [Penetrate; 4] = [Penetrate::None, Penetrate::Small, Penetrate::Medium, Penetrate::Large];

    /// BO2's enum value (`PenetrateType`), out of range → `None`.
    pub fn from_bo2(v: u32) -> Penetrate {
        Self::ALL.get(v as usize).copied().unwrap_or_default()
    }
    pub fn name(self) -> &'static str {
        match self {
            Penetrate::None => "none",
            Penetrate::Small => "small",
            Penetrate::Medium => "medium",
            Penetrate::Large => "large",
        }
    }
    pub fn parse(s: &str) -> Option<Penetrate> {
        Self::ALL.into_iter().find(|p| p.name() == s)
    }
}

/// BO2's `OffhandClass`: which kind of offhand a weapon is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OffhandClass {
    #[default]
    None,
    Frag,
    Smoke,
    Flash,
    Gear,
    SupplyDropMarker,
}

impl OffhandClass {
    pub const ALL: [OffhandClass; 6] =
        [OffhandClass::None, OffhandClass::Frag, OffhandClass::Smoke, OffhandClass::Flash, OffhandClass::Gear, OffhandClass::SupplyDropMarker];

    /// BO2's enum value, out of range → `None`.
    pub fn from_bo2(v: u32) -> OffhandClass {
        Self::ALL.get(v as usize).copied().unwrap_or_default()
    }
    /// BO2's names (`frag_grenade`, `gear`).
    pub fn name(self) -> &'static str {
        match self {
            OffhandClass::None => "none",
            OffhandClass::Frag => "frag_grenade",
            OffhandClass::Smoke => "smoke_grenade",
            OffhandClass::Flash => "flash_grenade",
            OffhandClass::Gear => "gear",
            OffhandClass::SupplyDropMarker => "supplydrop_marker",
        }
    }
    pub fn parse(s: &str) -> Option<OffhandClass> {
        Self::ALL.into_iter().find(|p| p.name() == s)
    }
}

/// BO2's per-weapon traits: flags (each the variant's or def's, OR-ed with its attachments'),
/// stack-fire spread, the damage floor, penetration and the offhand class.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Traits {
    /// `bClipOnly`: no reserve; the magazine is all the ammo.
    pub clip_only: bool,
    /// `bDualMag`: taped magazines (the variant's, or any attachment's unless it ignores them).
    pub dual_mag: bool,
    /// `bInfraRed`: the thermal optic.
    pub infrared: bool,
    /// `mmsWeapon`: the millimetre-wave scanner.
    pub mms: bool,
    /// `bUseAsMelee` / `bHasBayonet` (or the tactical knife): the melee uses the weapon.
    pub use_as_melee: bool,
    /// `bRifleBullet`.
    pub rifle_bullet: bool,
    /// `bAltWeaponAdsOnly`: the alt mode is only reachable while aiming (the second optic).
    pub alt_ads_only: bool,
    /// `bAltWeaponDisableSwitching`.
    pub alt_disable_switching: bool,
    /// `bIgnoreAttachments`: the variant's flags ignore its attachments' (G14 §4).
    pub ignore_attachments: bool,
    /// `stackFireSpread`, `stackFireAccuracyDecay` (the tri-bolt's spread).
    pub stack_fire: (f32, f32),
    /// `minPlayerDamage`: the least a hit on a player does.
    pub min_player_damage: f32,
    pub penetrate: Penetrate,
    pub offhand: OffhandClass,
}
