//! Black Ops II (T6) weapon definitions, read from a fully loaded zone (`t6zone::Zone`).
//!
//! Every multiplayer weapon is a `WeaponVariantDef` asset in `common_mp` (`an94_mp`, `fnp45_mp`,
//! `frag_grenade_mp`, ...) pointing at its `WeaponDef`: fire type and timings, clip and ammo, the
//! damage curve, spread, kick, movement, reload/raise/drop timings, grenade fuse and explosion, the
//! view/world models and the viewmodel animation for each `weapAnimFiles_t` slot. Field offsets
//! come from the generated `t6zone::layouts`. Times are converted from milliseconds to seconds.

use crate::t6zone::layouts::{
    WeaponAttachment as WA, WeaponAttachmentUnique as WU, WeaponCamo as WC, WeaponCamoMaterial as WCMat,
    WeaponCamoMaterialSet as WCMatSet, WeaponCamoSet as WCSet, WeaponDef as WD, WeaponVariantDef as WV,
};
use crate::t6zone::layouts::TracerDef as TD;
use crate::t6zone::Zone;
use crate::FormatError;

/// `weapAnimFiles_t`: the viewmodel animation slots (`szXAnims` index).
pub mod anim {
    pub const IDLE: usize = 0x01;
    pub const EMPTY_IDLE: usize = 0x02;
    pub const FIRE_INTRO: usize = 0x03;
    pub const FIRE: usize = 0x04;
    pub const HOLD_FIRE: usize = 0x05;
    pub const LASTSHOT: usize = 0x06;
    pub const FINALSHOT: usize = 0x07;
    pub const RECHAMBER: usize = 0x08;
    pub const MELEE: usize = 0x09;
    pub const MELEE_EMPTY: usize = 0x0D;
    pub const RELOAD: usize = 0x10;
    pub const RELOAD_EMPTY: usize = 0x12;
    pub const RELOAD_START: usize = 0x13;
    pub const RELOAD_END: usize = 0x14;
    pub const RAISE: usize = 0x17;
    pub const FIRST_RAISE: usize = 0x18;
    pub const DROP: usize = 0x19;
    pub const ALT_RAISE: usize = 0x1A;
    pub const ALT_DROP: usize = 0x1B;
    pub const QUICK_RAISE: usize = 0x1C;
    pub const QUICK_DROP: usize = 0x1D;
    pub const EMPTY_RAISE: usize = 0x1E;
    pub const EMPTY_DROP: usize = 0x1F;
    pub const SPRINT_IN: usize = 0x20;
    pub const SPRINT_LOOP: usize = 0x21;
    pub const SPRINT_OUT: usize = 0x22;
    pub const SPRINT_EMPTY_IN: usize = 0x23;
    pub const SPRINT_EMPTY_LOOP: usize = 0x24;
    pub const SPRINT_EMPTY_OUT: usize = 0x25;
    pub const DETONATE: usize = 0x3A;
    pub const ADS_FIRE: usize = 0x3D;
    pub const ADS_LASTSHOT: usize = 0x3E;
    pub const ADS_FIRE_INTRO: usize = 0x3F;
    pub const ADS_RECHAMBER: usize = 0x40;
    pub const ADS_UP: usize = 0x55;
    pub const ADS_DOWN: usize = 0x56;
    pub const ADS_UP_OTHER_SCOPE: usize = 0x57;
    pub const COUNT: usize = 0x58;
}

/// `weapType_t`.
pub mod kind {
    pub const BULLET: u32 = 0;
    pub const GRENADE: u32 = 1;
    pub const PROJECTILE: u32 = 2;
    pub const GAS: u32 = 4;
    pub const BOMB: u32 = 5;
    pub const MINE: u32 = 6;
    pub const MELEE: u32 = 7;
    pub const RIOTSHIELD: u32 = 8;
}

/// `weapClass_t`.
pub mod class {
    pub const RIFLE: u32 = 0;
    pub const MG: u32 = 1;
    pub const SMG: u32 = 2;
    pub const SPREAD: u32 = 3;
    pub const PISTOL: u32 = 4;
    pub const GRENADE: u32 = 5;
    pub const ROCKETLAUNCHER: u32 = 6;
    pub const TURRET: u32 = 7;
    pub const NON_PLAYER: u32 = 8;
    pub const GAS: u32 = 9;
    pub const ITEM: u32 = 10;
    pub const MELEE: u32 = 11;
    pub const KILLSTREAK_ALT_STORED_WEAPON: u32 = 12;
    pub const PISTOL_SPREAD: u32 = 13;
}

/// `weapInventoryType_t`.
pub mod inventory {
    pub const PRIMARY: u32 = 0;
    pub const OFFHAND: u32 = 1;
    pub const ITEM: u32 = 2;
    pub const ALTMODE: u32 = 3;
    pub const MELEE: u32 = 4;
    pub const DWLEFTHAND: u32 = 5;
}

/// `weapFireType_t`.
pub mod fire {
    pub const FULLAUTO: u32 = 0;
    pub const SINGLESHOT: u32 = 1;
    pub const BURST2: u32 = 2;
    pub const BURST3: u32 = 3;
    pub const BURST4: u32 = 4;
    pub const BURST5: u32 = 5;
    pub const STACKED: u32 = 6;
    pub const MINIGUN: u32 = 7;
    pub const CHARGESHOT: u32 = 8;
}

/// `OffhandClass`.
pub mod offhand {
    pub const NONE: u32 = 0;
    pub const FRAG: u32 = 1;
    pub const SMOKE: u32 = 2;
    pub const FLASH: u32 = 3;
    pub const GEAR: u32 = 4;
}

/// `eAttachment` count (`ATTACHMENT_TYPE_COUNT`).
pub const ATTACHMENT_KINDS: u32 = 30;

/// `hitLocation_t` count (`locationDamageMultipliers`).
pub const HIT_LOCATIONS: usize = 21;
/// `HITLOC_HEAD`.
pub const HIT_LOCATION_HEAD: usize = 2;

/// One `attachWorldModel` slot of a weapon variant: a base part of the third-person weapon. The
/// slot matters: BO2 drops slot 0 (the base attachment art) under `disableBaseWeaponAttachment`
/// and slot 6 (the clip) under `disableBaseWeaponClip` (`FUN_008ace30`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AttachWorldModel {
    pub slot: u32,
    pub model: String,
    /// `attachWorldModelTag[slot]`, `None` when BO2 leaves it empty (the default tag).
    pub tag: Option<String>,
    /// Inches, in the tag's frame.
    pub offset: [f32; 3],
    /// Degrees: pitch, yaw, roll.
    pub rotation: [f32; 3],
}

/// A bullet tracer (`TracerDef`, the `tracer` asset): the streak BO2 draws for every
/// `draw_interval`-th bullet (`FUN_007c65c0`) and how `Tracer_Spawn` (`FUN_007c6150`) draws it.
/// Distances are inches, speed inches per second (0: the beam spans the whole shot at once,
/// the `screw` kind), the five colours run from the beam's head to its tail (RGBA, 0..1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tracer {
    pub name: String,
    /// `tracerType_t`: 0 a straight beam flying at `speed`, 1 a corkscrew (`screw_*`).
    pub kind: u32,
    pub material: Option<String>,
    pub draw_interval: u32,
    pub speed: f32,
    pub beam_length: f32,
    pub beam_width: f32,
    pub screw_radius: f32,
    pub screw_dist: f32,
    pub fade_time: f32,
    pub fade_scale: f32,
    pub tex_repeat_rate: f32,
    pub colors: [[f32; 4]; 5],
}

/// The `TracerDef` at the pointer in `field`, if any. A comma-prefixed reference stub keeps its
/// name; [`bind_tracer`] resolves it against the later zones.
pub fn read_tracer(z: &Zone, field: u32) -> Option<Tracer> {
    read_tracer_at(z, z.u32(field))
}

/// Every `TracerDef` asset in a zone, by name.
pub fn tracers(z: &Zone) -> Vec<Tracer> {
    let Some(kind) = crate::fastfile::asset_type_id("tracer") else { return Vec::new() };
    let mut out = Vec::new();
    for a in z.assets.iter().filter(|a| a.kind == kind) {
        // The asset table holds the header's address: read it as a pointer field would.
        if let Some(t) = read_tracer_at(z, a.addr) {
            out.push(t);
        }
    }
    out
}

/// A parsed tracer as BO2's asset pool binds it: kept when it is the full asset, else (a
/// comma-prefixed reference stub) the first zone in `pool` holding the full asset of that name.
pub fn bind_tracer(t: Option<Tracer>, pool: &[&Zone]) -> Option<Tracer> {
    let t = t?;
    let Some(name) = t.name.strip_prefix(',') else { return Some(t) };
    pool.iter().find_map(|z| tracers(z).into_iter().find(|f| f.name == name)).or(Some(Tracer { name: name.to_string(), ..t }))
}

fn read_tracer_at(z: &Zone, t: u32) -> Option<Tracer> {
    if t == 0 {
        return None;
    }
    let mut colors = [[0.0; 4]; 5];
    for (i, c) in colors.iter_mut().enumerate() {
        let at = t + TD::colors + 16 * i as u32;
        *c = [z.f32(at), z.f32(at + 4), z.f32(at + 8), z.f32(at + 12)];
    }
    Some(Tracer {
        name: z.str_ptr(t + TD::name),
        kind: z.u32(t + TD::r#type),
        material: named(z, t + TD::material),
        draw_interval: z.u32(t + TD::drawInterval),
        speed: z.f32(t + TD::speed),
        beam_length: z.f32(t + TD::beamLength),
        beam_width: z.f32(t + TD::beamWidth),
        screw_radius: z.f32(t + TD::screwRadius),
        screw_dist: z.f32(t + TD::screwDist),
        fade_time: z.f32(t + TD::fadeTime),
        fade_scale: z.f32(t + TD::fadeScale),
        tex_repeat_rate: z.f32(t + TD::texRepeatRate),
        colors,
    })
}

/// One of a weapon's attachment combinations (`WeaponAttachmentUnique`): BO2 hangs the default
/// magazine and every attachment's view model this way.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AttachmentUnique {
    pub name: String,
    /// `eAttachment` (0: none, the weapon's base parts).
    pub kind: u32,
    pub view_model: Option<String>,
    pub view_model_tag: Option<String>,
    pub world_model: Option<String>,
    pub world_model_tag: Option<String>,
    /// Bits (`1 << eAttachment`) of the attachments this combination stands for.
    pub combined: u32,
    /// The unique's slot in the variant's `attachmentUniques[95]` (a single sits at its kind), and
    /// its `childLink`/`siblingLink` slots: the combined uniques under a single, which BO2 walks to
    /// resolve the entry a combination uses (`FUN_008eb690`). 0 ends a chain.
    pub index: u32,
    pub child: u32,
    pub sibling: u32,
    /// The attachment replaces the weapon's own magazine (extended or dual magazines).
    pub disable_base_clip: bool,
    /// The attachment replaces the weapon's base `attachViewModel` art — the AS50's ACOG takes
    /// the place of its scope. The two replacement channels are independent: a combination may
    /// replace the magazine, the attachment art, both or neither.
    pub disable_base_attachment: bool,
    /// The attachment moves the base attachment art (`viewModelOffsetBaseAttachment`).
    pub override_base_attachment_offsets: bool,
    /// The base attachment's new view offset, in inches (view frame; no rotation field exists).
    pub base_attachment_offset: [f32; 3],
    /// Viewmodel animations the combination overrides (`anim` slots), e.g. a grip's or extended
    /// magazine's reloads.
    pub anims: Vec<Option<String>>,
    /// Where the attachment's view model sits on the weapon (inches) and its rotation (degrees:
    /// pitch, yaw, roll), from `view_model_tag` or the weapon's origin.
    pub view_offset: [f32; 3],
    pub view_rotation: [f32; 3],
    /// A second model drawn with the attachment's own (`viewModelAdditional`): the dual-band IR
    /// optic's rail mount, at its own `viewModelAddOffsets`/`viewModelAddRotations`.
    pub view_model_additional: Option<String>,
    pub view_add_offset: [f32; 3],
    pub view_add_rotation: [f32; 3],
    pub world_offset: [f32; 3],
    pub world_rotation: [f32; 3],
    /// The third-person twin of `view_model_additional` (`worldModelAdditional`), at its own
    /// `worldModelAddOffsets`/`worldModelAddRotations`.
    pub world_model_additional: Option<String>,
    pub world_add_offset: [f32; 3],
    pub world_add_rotation: [f32; 3],
    /// The base attachment art's moved world offset (`worldModelOffsetBaseAttachment`), applied
    /// when `override_base_attachment_offsets` is set.
    pub world_base_attachment_offset: [f32; 3],
    /// Bones of the weapon hidden with this combination (iron sights under an optic).
    pub hide_tags: Vec<String>,
    /// The player's gunshot with this combination (suppressors), if it changes.
    pub fire_sound_player: Option<String>,
    pub fire_sound: Option<String>,
    /// The weapon the attachment toggles to (select fire, grenade launcher, dual optic).
    pub alt_weapon: Option<String>,
    pub dual_wield_weapon: Option<String>,
    /// The 2D screen-space ADS overlay material (`scope_overlay_<weapon>`), or empty.
    pub overlay_material: String,
    /// The low-resolution variant of [`Self::overlay_material`], or empty.
    pub overlay_material_low_res: String,
    /// `weapOverlayReticle_t`: which reticle the overlay draws (0: none).
    pub overlay_reticle: u32,
    /// The `WeaponCamo` asset the class camo uses on this attachment's own models (`weaponCamo`,
    /// `camo_reflex`, …); read from the single-attachment unique (`FUN_00529600`, G16).
    pub weapon_camo: Option<String>,
    /// The combination's own tracers (`tracerType`, `enemyTracerType`): the first single unique
    /// with one overrides the def's (`FUN_008eb420`, G14).
    pub tracer: Option<Tracer>,
    pub enemy_tracer: Option<Tracer>,
}

/// A BO2 attachment (`WeaponAttachment`): how it changes any weapon it goes on. Scales multiply the
/// weapon's own value (1 = unchanged, 0 = BO2 leaves the value alone).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Attachment {
    /// `reflex`, `extclip`, `silencer`, ...
    pub name: String,
    /// Localize key, e.g. `MPUI_REFLEX`.
    pub display_name: String,
    /// `eAttachment`: the bit in a weapon's allowed set ([`Weapon::allowed_attachments`]).
    pub kind: u32,
    /// `eAttachmentPoint`: optic, under-barrel, muzzle, ... (one attachment per point).
    pub point: u32,
    /// A fire type the attachment switches the weapon to (select fire), or `None`.
    pub fire_type: Option<u32>,
    pub silenced: bool,
    pub dual_mag: bool,
    pub laser_sight: bool,
    pub dual_wield: bool,
    pub damage_range_scale: f32,
    /// Zoomed field of view (0: the weapon's own).
    pub ads_zoom_fov: f32,
    pub ads_in_scale: f32,
    pub ads_out_scale: f32,
    pub ads_move_scale: f32,
    pub hip_spread_min_scale: f32,
    pub hip_spread_max_scale: f32,
    pub fire_time_scale: f32,
    pub reload_scale: f32,
    pub reload_empty_scale: f32,
    pub reload_add_scale: f32,
    pub clip_size_scale: f32,
    /// A fixed magazine size (0: none).
    pub clip_size: i32,
    /// ADS recoil reduction per shot and its limit (the grip).
    pub recoil_reduction: (f32, f32),
    /// `WeaponAttachment.perks`: the specialty bits the attachment grants (G1).
    pub perks: (u32, u32),
    /// `firstRaisePriority`: the first-raise slot's tie-break priority (G14 §5).
    pub first_raise_priority: i32,
    /// `bAltWeaponAdsOnly`: the alt-mode's ADS-only flag (G14 §5).
    pub alt_weapon_ads_only: bool,
}

/// Every attachment in a loaded zone.
pub fn attachments(z: &Zone) -> Result<Vec<Attachment>, FormatError> {
    let Some(kind) = crate::fastfile::asset_type_id("attachment") else { return Ok(Vec::new()) };
    z.assets
        .iter()
        .filter(|a| a.kind == kind)
        .map(|a| -> Result<Attachment, FormatError> {
            let at = a.addr;
            let fire = z.try_u32(at + WA::fireType)?;
            Ok(Attachment {
                name: z.try_str_ptr(at + WA::szInternalName)?,
                display_name: opt_str(z, at + WA::szDisplayName).unwrap_or_default(),
                kind: z.u32(at + WA::attachmentType),
                point: z.u32(at + WA::attachmentPoint),
                // `weapFireType_t` has no "unchanged" value: BO2 marks it with the type count.
                fire_type: (fire < 9 && fire != 0).then_some(fire),
                silenced: z.u8(at + WA::bSilenced) != 0,
                dual_mag: z.u8(at + WA::bDualMag) != 0,
                laser_sight: z.u8(at + WA::laserSight) != 0,
                dual_wield: z.u8(at + WA::bDualWield) != 0,
                damage_range_scale: z.f32(at + WA::fDamageRangeScale),
                ads_zoom_fov: z.f32(at + WA::fAdsZoomFov1),
                ads_in_scale: z.f32(at + WA::fAdsTransInTimeScale),
                ads_out_scale: z.f32(at + WA::fAdsTransOutTimeScale),
                ads_move_scale: z.f32(at + WA::adsMoveSpeedScale),
                hip_spread_min_scale: z.f32(at + WA::fHipSpreadMinScale),
                hip_spread_max_scale: z.f32(at + WA::fHipSpreadMaxScale),
                fire_time_scale: z.f32(at + WA::fFireTimeScale),
                reload_scale: z.f32(at + WA::fReloadTimeScale),
                reload_empty_scale: z.f32(at + WA::fReloadEmptyTimeScale),
                reload_add_scale: z.f32(at + WA::fReloadAddTimeScale),
                clip_size_scale: z.f32(at + WA::clipSizeScale),
                clip_size: int(z, at + WA::iClipSize),
                recoil_reduction: (z.f32(at + WA::fAdsRecoilReductionRate), z.f32(at + WA::fAdsRecoilReductionLimit)),
                perks: (z.u32(at + WA::perks), z.u32(at + WA::perks + 4)),
                first_raise_priority: z.u32(at + WA::firstRaisePriority) as i32,
                alt_weapon_ads_only: z.u8(at + WA::bAltWeaponAdsOnly) != 0,
            })
        })
        .collect()
}

/// Min/max pitch and yaw of a kick, degrees.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Kick {
    pub pitch: (f32, f32),
    pub yaw: (f32, f32),
}

/// The gun kick of one aim state (`f{Ads,Hip}GunKick*`, `{ads,hip}GunKickReducedKick*`; G5).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GunKick {
    pub pitch: (f32, f32),
    pub yaw: (f32, f32),
    pub accel: f32,
    pub speed_max: f32,
    pub speed_decay: f32,
    pub static_decay: f32,
    pub reduced_bullets: i32,
    pub reduced_percent: f32,
}

/// View sway (`sway*` / `adsSway*`; G4). BO2's ADS set has no horizontal/vertical scales.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sway {
    pub max_angle: f32,
    pub lerp_speed: f32,
    pub pitch_scale: f32,
    pub yaw_scale: f32,
    pub horiz_scale: f32,
    pub vert_scale: f32,
}

/// The viewmodel's stance and movement offsets and their rates (inches, degrees; G4).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewPos {
    pub stand_move: [f32; 3],
    pub stand_rot: [f32; 3],
    pub ducked_ofs: [f32; 3],
    pub ducked_move: [f32; 3],
    pub ducked_rot: [f32; 3],
    pub prone_ofs: [f32; 3],
    pub prone_move: [f32; 3],
    pub prone_rot: [f32; 3],
    pub strafe_move: [f32; 3],
    pub strafe_rot: [f32; 3],
    pub sprint_ofs: [f32; 3],
    pub sprint_rot: [f32; 3],
    pub sprint_bob: [f32; 2],
    pub move_rate: f32,
    pub prone_move_rate: f32,
    pub rot_rate: f32,
    pub prone_rot_rate: f32,
    pub stand_min_speed: f32,
    pub ducked_min_speed: f32,
    pub prone_min_speed: f32,
    pub ads_aim_pitch: f32,
    pub ads_bob_factor: f32,
}

/// The weapon's own crosshair (G6).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reticle {
    pub center: Option<String>,
    pub side: Option<String>,
    pub center_size: i32,
    pub side_size: i32,
    pub min_ofs: i32,
    pub side_pos: f32,
    pub ads_in_frac: f32,
    pub ads_out_frac: f32,
    pub keep_when_ads: bool,
    pub color_change: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Weapon {
    /// `an94_mp`.
    pub name: String,
    /// Localize key, e.g. `WEAPON_AN94`.
    pub display_name: String,
    pub weap_type: u32,
    pub class: u32,
    pub inventory: u32,
    pub fire_type: u32,
    pub offhand_class: u32,
    /// `gunXModel[0]`: the viewmodel weapon.
    pub view_model: Option<String>,
    pub world_model: Option<String>,
    pub projectile_model: Option<String>,
    /// Extra viewmodel parts (such as the magazine) and the tag each hangs from (empty when BO2
    /// leaves the part at the weapon root).
    pub attach_view_models: Vec<(String, String)>,
    /// The authored view offset of each `attach_view_models` entry, in inches.
    pub attach_view_offsets: Vec<[f32; 3]>,
    /// The third-person base parts (`attachWorldModel[8]`): the magazine, a sniper's scope, the
    /// ballistic knife's blade, each with its slot, tag and placement.
    pub attach_world_models: Vec<AttachWorldModel>,
    /// Attachment combinations; `kind == 0` entries are the base weapon's parts (its magazine).
    pub attachment_uniques: Vec<AttachmentUnique>,
    /// Bit `1 << eAttachment` for each attachment create-a-class offers on this weapon.
    pub allowed_attachments: u64,
    /// The `WeaponAttachment` this weapon uses for each kind it allows, e.g. (23, `silencer_smg`).
    pub attachment_variants: Vec<(u32, String)>,
    /// Bones hidden on the plain weapon.
    pub hide_tags: Vec<String>,
    /// Alt-mode weapon (the select-fire, launcher or second optic variant `sf_an94_mp`).
    pub alt_weapon: Option<String>,
    /// Viewmodel animation per `anim` slot.
    pub anims: Vec<Option<String>>,
    pub clip_size: i32,
    pub start_ammo: i32,
    pub max_ammo: i32,
    /// `start_ammo`/`max_ammo` count magazines (loaded one included), not rounds.
    pub ammo_clip_relative: bool,
    /// Pellets per shot (shotguns).
    pub shot_count: i32,
    pub shots_before_rechamber: i32,
    // Seconds.
    pub fire_time: f32,
    pub last_fire_time: f32,
    pub fire_delay: f32,
    pub burst_delay: f32,
    pub rechamber_time: f32,
    pub rechamber_bolt_time: f32,
    pub hold_fire_time: f32,
    pub reload_time: f32,
    pub reload_empty_time: f32,
    pub reload_add_time: f32,
    pub reload_empty_add_time: f32,
    pub reload_start_time: f32,
    pub reload_end_time: f32,
    pub reload_start_add_time: f32,
    pub reload_ammo_add: i32,
    pub raise_time: f32,
    pub first_raise_time: f32,
    pub drop_time: f32,
    pub quick_raise_time: f32,
    pub quick_drop_time: f32,
    pub sprint_in_time: f32,
    pub sprint_out_time: f32,
    pub ads_in_time: f32,
    pub ads_out_time: f32,
    pub melee_time: f32,
    pub melee_delay: f32,
    pub spin_up_time: f32,
    pub spin_down_time: f32,
    pub fuse_time: f32,
    pub detonate_time: f32,
    /// Damage at each of `damage_range` (inches), BO2's falloff curve.
    pub damage: [i32; 6],
    pub damage_range: [f32; 6],
    pub min_player_damage: i32,
    pub melee_damage: i32,
    pub location_multipliers: Vec<f32>,
    pub explosion_radius: i32,
    pub explosion_radius_min: i32,
    pub explosion_inner_damage: i32,
    pub explosion_outer_damage: i32,
    pub projectile_speed: i32,
    pub projectile_speed_up: i32,
    pub projectile_impact_explode: bool,
    /// Degrees.
    pub hip_spread_stand: (f32, f32),
    pub hip_spread_ducked: (f32, f32),
    pub hip_spread_prone: (f32, f32),
    pub hip_spread_decay: f32,
    pub hip_spread_fire_add: f32,
    pub hip_spread_move_add: f32,
    pub ads_spread: f32,
    pub ads_zoom_fov: f32,
    pub ads_view_kick: Kick,
    pub hip_view_kick: Kick,
    /// `f{Ads,Hip}ViewKickMinMagnitude`.
    pub view_kick_min: (f32, f32),
    /// The variant's `f{Ads,Hip}ViewKickCenterSpeed`.
    pub view_kick_center: (f32, f32),
    pub ads_gun_kick: GunKick,
    pub hip_gun_kick: GunKick,
    /// `fGunMaxPitch`, `fGunMaxYaw`.
    pub gun_max: (f32, f32),
    /// `fAdsRecoilReductionRate`, `fAdsRecoilReductionLimit`, `fAdsRecoilReturnRate`.
    pub recoil: (f32, f32, f32),
    pub hip_sway: Sway,
    pub ads_sway: Sway,
    pub view_pos: ViewPos,
    pub reticle: Reticle,
    /// `WeaponVariantDef.perks`: the specialty bits the variant grants (G1).
    pub perks: (u32, u32),
    pub move_speed_scale: f32,
    pub ads_move_speed_scale: f32,
    pub sprint_duration_scale: f32,
    pub max_range: f32,
    pub bolt_action: bool,
    pub cook_off_hold: bool,
    pub dual_wield: bool,
    pub clip_only: bool,
    pub rifle_bullet: bool,
    pub silenced: bool,
    pub fire_sound: Option<String>,
    pub fire_sound_player: Option<String>,
    pub fire_last_sound_player: Option<String>,
    pub empty_fire_sound_player: Option<String>,
    pub projectile_explosion_sound: Option<String>,
    pub pullback_sound_player: Option<String>,
    pub melee_swipe_sound_player: Option<String>,
    /// The `WeaponCamo` asset the class camo uses on the gun and its base `attachViewModel` parts
    /// (`WeaponDef.weaponCamo`: `camo_an94`, the INSAS's `camo_msmc`), or `None`.
    pub weapon_camo: Option<String>,
    /// HUD art (material names).
    pub hud_icon: Option<String>,
    /// The HUD icon's image in the `.ipak` packs (its material is inline in the weapon).
    pub hud_icon_hash: Option<u32>,
    pub dpad_icon: Option<String>,
    /// What a projectile does when it goes off (`projExplosionEffect`), or the impact table's
    /// effect for its `impactType` on dirt.
    pub explosion_effect: Option<crate::fx::Effect>,
    pub impact_type: u32,
    /// The muzzle flash others see (`worldFlashEffect`).
    pub world_flash_effect: Option<crate::fx::Effect>,
    /// The first-person muzzle flash (`viewFlashEffect`).
    pub view_flash_effect: Option<crate::fx::Effect>,
    /// The first-person shell eject (`viewShellEjectEffect`).
    pub view_shell_eject_effect: Option<crate::fx::Effect>,
    /// The first-person shell eject on the last round (`viewLastShotEjectEffect`).
    pub view_last_shot_eject_effect: Option<crate::fx::Effect>,
    /// The shell eject everyone else sees, at the world model's `tag_brass`
    /// (`worldShellEjectEffect`; `FUN_00531630`), and its last-round twin
    /// (`worldLastShotEjectEffect`, used when the shot empties the clip and both last fields are set).
    pub world_shell_eject_effect: Option<crate::fx::Effect>,
    pub world_last_shot_eject_effect: Option<crate::fx::Effect>,
    /// The def's tracers for a friendly and an enemy shooter (`tracerType`, `enemyTracerType`).
    pub tracer: Option<Tracer>,
    pub enemy_tracer: Option<Tracer>,
    /// The 2D screen-space ADS overlay material (`scope_overlay_<weapon>`), or empty.
    pub overlay_material: String,
    /// The low-resolution variant of [`Self::overlay_material`], or empty.
    pub overlay_material_low_res: String,
    /// `weapOverlayReticle_t`: which reticle the overlay draws (0: none).
    pub overlay_reticle: u32,
}

fn ms(z: &Zone, at: u32) -> f32 {
    z.u32(at) as i32 as f32 / 1000.0
}

fn int(z: &Zone, at: u32) -> i32 {
    z.u32(at) as i32
}

fn opt_str(z: &Zone, field: u32) -> Option<String> {
    (z.u32(field) != 0).then(|| z.str_ptr(field)).filter(|s| !s.is_empty())
}

/// The name of the XModel or Material at the pointer in `field` (both start with their name).
fn named(z: &Zone, field: u32) -> Option<String> {
    let p = z.u32(field);
    (p != 0).then(|| z.str_ptr(p).trim_start_matches(',').to_string()).filter(|s| !s.is_empty())
}

fn vec3(z: &Zone, at: u32) -> [f32; 3] {
    [z.f32(at), z.f32(at + 4), z.f32(at + 8)]
}

/// A `hideTags` list: up to 32 script strings, zero-terminated.
fn hide_tags(z: &Zone, at: u32) -> Vec<String> {
    if at == 0 {
        return Vec::new();
    }
    (0..32).map(|i| z.u16(at + 2 * i)).take_while(|&t| t != 0).map(|t| z.script_string(t).to_string()).filter(|t| !t.is_empty()).collect()
}

fn kick(z: &Zone, pitch_min: u32, pitch_max: u32, yaw_min: u32, yaw_max: u32) -> Kick {
    Kick { pitch: (z.f32(pitch_min), z.f32(pitch_max)), yaw: (z.f32(yaw_min), z.f32(yaw_max)) }
}

/// Read the `WeaponVariantDef` at `v`.
pub fn read(z: &Zone, v: u32) -> Result<Weapon, FormatError> {
    let d = z.try_u32(v + WV::weapDef)?;
    let anims_at = z.try_u32(v + WV::szXAnims)?;
    let anims = (0..anim::COUNT as u32)
        .map(|i| if anims_at == 0 { None } else { opt_str(z, anims_at + 4 * i) })
        .collect();
    let mut attach_view_models = Vec::new();
    let mut attach_view_offsets = Vec::new();
    let (models, tags) = (z.try_u32(v + WV::attachViewModel)?, z.try_u32(v + WV::attachViewModelTag)?);
    if models != 0 {
        for i in 0..8 {
            if let Some(m) = named(z, models + 4 * i) {
                // The magazine's slot has an empty tag: BO2 leaves it at the weapon root and
                // carries its placement in the inline `attachViewModelOffsets` float array.
                let t = if tags != 0 { opt_str(z, tags + 4 * i).unwrap_or_default() } else { String::new() };
                let at = v + WV::attachViewModelOffsets + 12 * i;
                let off = [z.f32(at), z.f32(at + 4), z.f32(at + 8)];
                attach_view_models.push((m, t));
                attach_view_offsets.push(off);
            }
        }
    }
    let mut attach_world_models = Vec::new();
    let (models, tags) = (z.try_u32(v + WV::attachWorldModel)?, z.try_u32(v + WV::attachWorldModelTag)?);
    if models != 0 {
        for slot in 0..8 {
            if let Some(model) = named(z, models + 4 * slot) {
                let tag = if tags != 0 { opt_str(z, tags + 4 * slot).filter(|t| !t.is_empty()) } else { None };
                let offset = vec3(z, v + WV::attachWorldModelOffsets + 12 * slot);
                let rotation = vec3(z, v + WV::attachWorldModelRotations + 12 * slot);
                attach_world_models.push(AttachWorldModel { slot, model, tag, offset, rotation });
            }
        }
    }
    let mut attachment_uniques = Vec::new();
    let uniques = z.try_u32(v + WV::attachmentUniques)?;
    if uniques != 0 {
        for i in 0..95 {
            let u = z.u32(uniques + 4 * i);
            if u == 0 {
                continue;
            }
            attachment_uniques.push(AttachmentUnique {
                name: z.str_ptr(u + WU::szInternalName),
                kind: z.u32(u + WU::attachmentType),
                view_model: named(z, u + WU::viewModel),
                view_model_tag: opt_str(z, u + WU::viewModelTag),
                world_model: named(z, u + WU::worldModel),
                world_model_tag: opt_str(z, u + WU::worldModelTag),
                combined: z.u32(u + WU::combinedAttachmentTypeMask),
                index: i,
                child: z.u32(u + WU::childLink),
                sibling: z.u32(u + WU::siblingLink),
                disable_base_clip: z.u8(u + WU::disableBaseWeaponClip) != 0,
                disable_base_attachment: z.u8(u + WU::disableBaseWeaponAttachment) != 0,
                override_base_attachment_offsets: z.u8(u + WU::overrideBaseWeaponAttachmentOffsets) != 0,
                base_attachment_offset: vec3(z, u + WU::viewModelOffsetBaseAttachment),
                anims: {
                    let at = z.u32(u + WU::szXAnims);
                    (0..anim::COUNT as u32).map(|i| if at == 0 { None } else { opt_str(z, at + 4 * i) }).collect()
                },
                view_offset: vec3(z, u + WU::viewModelOffsets),
                view_rotation: vec3(z, u + WU::viewModelRotations),
                view_model_additional: named(z, u + WU::viewModelAdditional),
                view_add_offset: vec3(z, u + WU::viewModelAddOffsets),
                view_add_rotation: vec3(z, u + WU::viewModelAddRotations),
                world_offset: vec3(z, u + WU::worldModelOffsets),
                world_rotation: vec3(z, u + WU::worldModelRotations),
                world_model_additional: named(z, u + WU::worldModelAdditional),
                world_add_offset: vec3(z, u + WU::worldModelAddOffsets),
                world_add_rotation: vec3(z, u + WU::worldModelAddRotations),
                world_base_attachment_offset: vec3(z, u + WU::worldModelOffsetBaseAttachment),
                hide_tags: hide_tags(z, z.u32(u + WU::hideTags)),
                fire_sound_player: opt_str(z, u + WU::fireSoundPlayer),
                fire_sound: opt_str(z, u + WU::fireSound),
                alt_weapon: opt_str(z, u + WU::szAltWeaponName),
                dual_wield_weapon: opt_str(z, u + WU::szDualWieldWeaponName),
                overlay_material: named(z, u + WU::overlayMaterial).unwrap_or_default(),
                overlay_material_low_res: named(z, u + WU::overlayMaterialLowRes).unwrap_or_default(),
                overlay_reticle: z.u32(u + WU::overlayReticle),
                weapon_camo: named(z, u + WU::weaponCamo),
                tracer: read_tracer(z, u + WU::tracerType),
                enemy_tracer: read_tracer(z, u + WU::enemyTracerType),
            });
        }
    }
    // The base unique (kind == 0) carries the weapon's overlay material; the raw attachment
    // struct has no overlay pointer. Fall back to the variant's own fields when the base unique
    // is absent.
    let variant_overlay_material = named(z, v + WV::overlayMaterial).unwrap_or_default();
    let variant_overlay_material_low_res = named(z, v + WV::overlayMaterialLowRes).unwrap_or_default();
    let variant_overlay_reticle = 0;
    let base_unique = attachment_uniques.iter().find(|u| u.kind == 0);
    let overlay_material = base_unique
        .map(|u| u.overlay_material.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| variant_overlay_material.clone());
    let overlay_material_low_res = base_unique
        .map(|u| u.overlay_material_low_res.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| variant_overlay_material_low_res.clone());
    let overlay_reticle = base_unique.map_or(variant_overlay_reticle, |u| u.overlay_reticle);
    let mut w = Weapon {
        attachment_uniques,
        name: z.str_ptr(v + WV::szInternalName),
        display_name: opt_str(z, v + WV::szDisplayName).unwrap_or_default(),
        anims,
        attach_view_models,
        attach_view_offsets,
        attach_world_models,
        clip_size: int(z, v + WV::iClipSize),
        reload_time: ms(z, v + WV::iReloadTime),
        reload_empty_time: ms(z, v + WV::iReloadEmptyTime),
        ads_in_time: ms(z, v + WV::iAdsTransInTime),
        ads_out_time: ms(z, v + WV::iAdsTransOutTime),
        ads_zoom_fov: z.f32(v + WV::fAdsZoomFov1),
        silenced: z.u8(v + WV::bSilenced) != 0,
        allowed_attachments: z.u32(v + WV::iAttachments) as u64,
        attachment_variants: {
            let at = z.try_u32(v + WV::attachments)?;
            let mut out = Vec::new();
            if at != 0 {
                for i in 1..ATTACHMENT_KINDS {
                    let a = z.u32(at + 4 * i);
                    if a != 0 {
                        out.push((i, z.str_ptr(a + WA::szInternalName)));
                    }
                }
            }
            out
        },
        hide_tags: hide_tags(z, z.try_u32(v + WV::hideTags)?),
        alt_weapon: opt_str(z, v + WV::szAltWeaponName),
        dpad_icon: named(z, v + WV::dpadIcon),
        perks: (z.u32(v + WV::perks), z.u32(v + WV::perks + 4)),
        view_kick_center: (z.f32(v + WV::fAdsViewKickCenterSpeed), z.f32(v + WV::fHipViewKickCenterSpeed)),
        overlay_material,
        overlay_material_low_res,
        overlay_reticle,
        ..Default::default()
    };
    if d == 0 {
        return Ok(w);
    }
    let guns = z.try_u32(d + WD::gunXModel)?;
    w.view_model = if guns != 0 { named(z, guns) } else { None };
    // Like `gunXModel`, `worldModel` is an array of model pointers (one per attachment variant).
    let worlds = z.try_u32(d + WD::worldModel)?;
    w.world_model = if worlds != 0 { named(z, worlds) } else { None };
    w.projectile_model = named(z, d + WD::projectileModel);
    w.weap_type = z.u32(d + WD::weapType);
    w.class = z.u32(d + WD::weapClass);
    w.inventory = z.u32(d + WD::inventoryType);
    w.fire_type = z.u32(d + WD::fireType);
    w.offhand_class = z.u32(d + WD::offhandClass);
    w.start_ammo = int(z, d + WD::iStartAmmo);
    w.max_ammo = int(z, d + WD::iMaxAmmo);
    w.ammo_clip_relative = z.u8(d + WD::ammoCountClipRelative) != 0;
    w.shot_count = int(z, d + WD::shotCount);
    w.shots_before_rechamber = int(z, d + WD::iShotsBeforeRechamber);
    w.fire_time = ms(z, d + WD::iFireTime);
    w.last_fire_time = ms(z, d + WD::iLastFireTime);
    w.fire_delay = ms(z, d + WD::iFireDelay);
    w.burst_delay = ms(z, d + WD::iBurstDelayTime);
    w.rechamber_time = ms(z, d + WD::iRechamberTime);
    w.rechamber_bolt_time = ms(z, d + WD::iRechamberBoltTime);
    w.hold_fire_time = ms(z, d + WD::iHoldFireTime);
    w.reload_add_time = ms(z, d + WD::iReloadAddTime);
    w.reload_empty_add_time = ms(z, d + WD::iReloadEmptyAddTime);
    w.reload_start_time = ms(z, d + WD::iReloadStartTime);
    w.reload_end_time = ms(z, d + WD::iReloadEndTime);
    w.reload_start_add_time = ms(z, d + WD::iReloadStartAddTime);
    w.reload_ammo_add = int(z, d + WD::iReloadAmmoAdd);
    w.raise_time = ms(z, d + WD::iRaiseTime);
    w.first_raise_time = ms(z, d + WD::iFirstRaiseTime);
    w.drop_time = ms(z, d + WD::iDropTime);
    w.quick_raise_time = ms(z, d + WD::quickRaiseTime);
    w.quick_drop_time = ms(z, d + WD::quickDropTime);
    w.sprint_in_time = ms(z, d + WD::sprintInTime);
    w.sprint_out_time = ms(z, d + WD::sprintOutTime);
    w.melee_time = ms(z, d + WD::iMeleeTime);
    w.melee_delay = ms(z, d + WD::iMeleeDelay);
    w.spin_up_time = ms(z, d + WD::iSpinUpTime);
    w.spin_down_time = ms(z, d + WD::iSpinDownTime);
    w.fuse_time = ms(z, d + WD::fuseTime);
    w.detonate_time = ms(z, d + WD::iDetonateTime);
    for i in 0..6u32 {
        w.damage[i as usize] = int(z, d + WD::damage + 4 * i);
        w.damage_range[i as usize] = z.f32(d + WD::damageRange + 4 * i);
    }
    w.min_player_damage = int(z, d + WD::minPlayerDamage);
    w.melee_damage = int(z, d + WD::iMeleeDamage);
    let mults = z.try_u32(d + WD::locationDamageMultipliers)?;
    w.location_multipliers = if mults != 0 { (0..HIT_LOCATIONS as u32).map(|i| z.f32(mults + 4 * i)).collect() } else { Vec::new() };
    w.explosion_radius = int(z, d + WD::iExplosionRadius);
    w.explosion_radius_min = int(z, d + WD::iExplosionRadiusMin);
    w.explosion_inner_damage = int(z, d + WD::iExplosionInnerDamage);
    w.explosion_outer_damage = int(z, d + WD::iExplosionOuterDamage);
    w.projectile_speed = int(z, d + WD::iProjectileSpeed);
    w.projectile_speed_up = int(z, d + WD::iProjectileSpeedUp);
    w.projectile_impact_explode = z.u8(d + WD::bProjImpactExplode) != 0;
    w.hip_spread_stand = (z.f32(d + WD::fHipSpreadStandMin), z.f32(d + WD::hipSpreadStandMax));
    w.hip_spread_ducked = (z.f32(d + WD::fHipSpreadDuckedMin), z.f32(d + WD::hipSpreadDuckedMax));
    w.hip_spread_prone = (z.f32(d + WD::fHipSpreadProneMin), z.f32(d + WD::hipSpreadProneMax));
    w.hip_spread_decay = z.f32(d + WD::fHipSpreadDecayRate);
    w.hip_spread_fire_add = z.f32(d + WD::fHipSpreadFireAdd);
    w.hip_spread_move_add = z.f32(d + WD::fHipSpreadMoveAdd);
    w.ads_spread = z.f32(d + WD::fAdsSpread);
    w.ads_view_kick = kick(z, d + WD::fAdsViewKickPitchMin, d + WD::fAdsViewKickPitchMax, d + WD::fAdsViewKickYawMin, d + WD::fAdsViewKickYawMax);
    w.hip_view_kick = kick(z, d + WD::fHipViewKickPitchMin, d + WD::fHipViewKickPitchMax, d + WD::fHipViewKickYawMin, d + WD::fHipViewKickYawMax);
    w.view_kick_min = (z.f32(d + WD::fAdsViewKickMinMagnitude), z.f32(d + WD::fHipViewKickMinMagnitude));
    w.ads_gun_kick = GunKick {
        pitch: (z.f32(d + WD::fAdsGunKickPitchMin), z.f32(d + WD::fAdsGunKickPitchMax)),
        yaw: (z.f32(d + WD::fAdsGunKickYawMin), z.f32(d + WD::fAdsGunKickYawMax)),
        accel: z.f32(d + WD::fAdsGunKickAccel),
        speed_max: z.f32(d + WD::fAdsGunKickSpeedMax),
        speed_decay: z.f32(d + WD::fAdsGunKickSpeedDecay),
        static_decay: z.f32(d + WD::fAdsGunKickStaticDecay),
        reduced_bullets: int(z, d + WD::adsGunKickReducedKickBullets),
        reduced_percent: z.f32(d + WD::adsGunKickReducedKickPercent),
    };
    w.hip_gun_kick = GunKick {
        pitch: (z.f32(d + WD::fHipGunKickPitchMin), z.f32(d + WD::fHipGunKickPitchMax)),
        yaw: (z.f32(d + WD::fHipGunKickYawMin), z.f32(d + WD::fHipGunKickYawMax)),
        accel: z.f32(d + WD::fHipGunKickAccel),
        speed_max: z.f32(d + WD::fHipGunKickSpeedMax),
        speed_decay: z.f32(d + WD::fHipGunKickSpeedDecay),
        static_decay: z.f32(d + WD::fHipGunKickStaticDecay),
        reduced_bullets: int(z, d + WD::hipGunKickReducedKickBullets),
        reduced_percent: z.f32(d + WD::hipGunKickReducedKickPercent),
    };
    w.gun_max = (z.f32(d + WD::fGunMaxPitch), z.f32(d + WD::fGunMaxYaw));
    w.recoil = (z.f32(d + WD::fAdsRecoilReductionRate), z.f32(d + WD::fAdsRecoilReductionLimit), z.f32(d + WD::fAdsRecoilReturnRate));
    w.hip_sway = Sway {
        max_angle: z.f32(d + WD::swayMaxAngle),
        lerp_speed: z.f32(d + WD::swayLerpSpeed),
        pitch_scale: z.f32(d + WD::swayPitchScale),
        yaw_scale: z.f32(d + WD::swayYawScale),
        horiz_scale: z.f32(d + WD::swayHorizScale),
        vert_scale: z.f32(d + WD::swayVertScale),
    };
    w.ads_sway = Sway {
        max_angle: z.f32(d + WD::adsSwayMaxAngle),
        lerp_speed: z.f32(d + WD::adsSwayLerpSpeed),
        pitch_scale: z.f32(d + WD::adsSwayPitchScale),
        yaw_scale: z.f32(d + WD::adsSwayYawScale),
        horiz_scale: 0.0,
        vert_scale: 0.0,
    };
    let v2 = |at: u32| [z.f32(at), z.f32(at + 4)];
    w.view_pos = ViewPos {
        stand_move: vec3(z, d + WD::vStandMove),
        stand_rot: vec3(z, d + WD::vStandRot),
        ducked_ofs: vec3(z, d + WD::vDuckedOfs),
        ducked_move: vec3(z, d + WD::vDuckedMove),
        ducked_rot: vec3(z, d + WD::vDuckedRot),
        prone_ofs: vec3(z, d + WD::vProneOfs),
        prone_move: vec3(z, d + WD::vProneMove),
        prone_rot: vec3(z, d + WD::vProneRot),
        strafe_move: vec3(z, d + WD::vStrafeMove),
        strafe_rot: vec3(z, d + WD::vStrafeRot),
        sprint_ofs: vec3(z, d + WD::vSprintOfs),
        sprint_rot: vec3(z, d + WD::vSprintRot),
        sprint_bob: v2(d + WD::vSprintBob),
        move_rate: z.f32(d + WD::fPosMoveRate),
        prone_move_rate: z.f32(d + WD::fPosProneMoveRate),
        rot_rate: z.f32(d + WD::fPosRotRate),
        prone_rot_rate: z.f32(d + WD::fPosProneRotRate),
        stand_min_speed: z.f32(d + WD::fStandMoveMinSpeed),
        ducked_min_speed: z.f32(d + WD::fDuckedMoveMinSpeed),
        prone_min_speed: z.f32(d + WD::fProneMoveMinSpeed),
        ads_aim_pitch: z.f32(d + WD::fAdsAimPitch),
        ads_bob_factor: z.f32(d + WD::fAdsBobFactor),
    };
    w.reticle = Reticle {
        center: named(z, d + WD::reticleCenter),
        side: named(z, d + WD::reticleSide),
        center_size: int(z, d + WD::iReticleCenterSize),
        side_size: int(z, d + WD::iReticleSideSize),
        min_ofs: int(z, d + WD::iReticleMinOfs),
        side_pos: z.f32(d + WD::fHipReticleSidePos),
        ads_in_frac: z.f32(d + WD::fAdsCrosshairInFrac),
        ads_out_frac: z.f32(d + WD::fAdsCrosshairOutFrac),
        keep_when_ads: z.u8(d + WD::bKeepCrosshairWhenADS) != 0,
        color_change: z.u8(d + WD::crosshairColorChange) != 0,
    };
    w.move_speed_scale = z.f32(d + WD::moveSpeedScale);
    w.ads_move_speed_scale = z.f32(d + WD::adsMoveSpeedScale);
    w.sprint_duration_scale = z.f32(d + WD::sprintDurationScale);
    w.max_range = z.f32(d + WD::maxRange);
    w.bolt_action = z.u8(d + WD::bBoltAction) != 0;
    w.cook_off_hold = z.u8(d + WD::bCookOffHold) != 0;
    w.dual_wield = z.u8(d + WD::bDualWield) != 0;
    w.clip_only = z.u8(d + WD::bClipOnly) != 0;
    w.rifle_bullet = z.u8(d + WD::bRifleBullet) != 0;
    w.fire_sound = opt_str(z, d + WD::fireSound);
    w.fire_sound_player = opt_str(z, d + WD::fireSoundPlayer);
    w.fire_last_sound_player = opt_str(z, d + WD::fireLastSoundPlayer);
    w.empty_fire_sound_player = opt_str(z, d + WD::emptyFireSoundPlayer);
    w.projectile_explosion_sound = opt_str(z, d + WD::projExplosionSound);
    w.pullback_sound_player = opt_str(z, d + WD::pullbackSoundPlayer);
    w.melee_swipe_sound_player = opt_str(z, d + WD::meleeSwipeSoundPlayer);
    w.impact_type = z.u32(d + WD::impactType);
    let flash = z.try_u32(d + WD::worldFlashEffect)?;
    w.world_flash_effect = (flash != 0).then(|| crate::fx::read(z, flash)).filter(|e| !e.elems.is_empty());
    let view_flash = z.try_u32(d + WD::viewFlashEffect)?;
    w.view_flash_effect = (view_flash != 0).then(|| crate::fx::read(z, view_flash)).filter(|e| !e.elems.is_empty());
    let shell = z.try_u32(d + WD::viewShellEjectEffect)?;
    w.view_shell_eject_effect = (shell != 0).then(|| crate::fx::read(z, shell)).filter(|e| !e.elems.is_empty());
    let last_shell = z.try_u32(d + WD::viewLastShotEjectEffect)?;
    w.view_last_shot_eject_effect = (last_shell != 0).then(|| crate::fx::read(z, last_shell)).filter(|e| !e.elems.is_empty());
    // The world shells are bound against the pool by `all_in` (several are reference stubs here).
    w.world_shell_eject_effect = crate::fx::read_or_stub(z, z.try_u32(d + WD::worldShellEjectEffect)?);
    w.world_last_shot_eject_effect = crate::fx::read_or_stub(z, z.try_u32(d + WD::worldLastShotEjectEffect)?);
    w.tracer = read_tracer(z, d + WD::tracerType);
    w.enemy_tracer = read_tracer(z, d + WD::enemyTracerType);
    let fx = z.try_u32(d + WD::projExplosionEffect)?;
    w.explosion_effect = (fx != 0).then(|| crate::fx::read(z, fx)).filter(|e| !e.elems.is_empty()).or_else(|| crate::fx::explosion_entry(w.impact_type).and_then(|e| crate::fx::impact(z, e, crate::fx::surface::DIRT)));
    w.hud_icon = named(z, d + WD::hudIcon);
    w.weapon_camo = named(z, d + WD::weaponCamo);
    let icon = z.try_u32(d + WD::hudIcon)?;
    w.hud_icon_hash = (icon != 0)
        .then(|| crate::xmodel::read_material(z, icon))
        .and_then(|m| m.textures.iter().find_map(|t| t.image.as_ref().map(|i| i.stream_hash)))
        .filter(|h| *h != 0);
    Ok(w)
}

/// Every weapon in a loaded zone.
pub fn all(z: &Zone) -> Result<Vec<Weapon>, FormatError> {
    all_in(z, &[])
}

/// The weapons of `z` with their world shell effects and tracers bound as BO2's asset pool binds
/// them: a def pointing at a reference stub (`common_mp`'s `,weapon/shellejects/fx_rifle`) uses
/// the full asset of that name from a later zone (`later`, latest-loaded first — `patch_mp`),
/// then from `z` itself; a stub nothing defines is dropped (effects) or kept by name (tracers).
pub fn all_in(z: &Zone, later: &[&Zone]) -> Result<Vec<Weapon>, FormatError> {
    let Some(kind) = crate::fastfile::asset_type_id("weapon") else { return Ok(Vec::new()) };
    let mut pool: Vec<&Zone> = later.to_vec();
    pool.push(z);
    let mut out: Vec<Weapon> = z.assets.iter().filter(|a| a.kind == kind).map(|a| read(z, a.addr)).collect::<Result<_, _>>()?;
    for w in &mut out {
        for e in [&mut w.world_shell_eject_effect, &mut w.world_last_shot_eject_effect] {
            *e = crate::fx::bind(e.take(), &pool);
        }
        for t in [&mut w.tracer, &mut w.enemy_tracer] {
            *t = bind_tracer(t.take(), &pool);
        }
        for u in &mut w.attachment_uniques {
            for t in [&mut u.tracer, &mut u.enemy_tracer] {
                *t = bind_tracer(t.take(), &pool);
            }
        }
    }
    Ok(out)
}

/// One `WeaponCamo` asset (`camo_<weapon>` in `common_patch_mp`, the `camo` type in the asset
/// dumps): the per-weapon camouflage recipes. `sets` is the global pattern list (an id slot per
/// camo: its solid swatch and tiling pattern, plus this weapon's UV transform); `materials` is one
/// material-replacement recipe per style camo (gold, diamond, …).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WeaponCamo {
    pub name: String,
    pub solid_base_image: Option<crate::xmodel::Image>,
    pub pattern_base_image: Option<crate::xmodel::Image>,
    pub sets: Vec<CamoSet>,
    pub materials: Vec<CamoMaterialSet>,
}

/// One `WeaponCamoSet`: the solid and pattern images and the pattern's UV transform.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CamoSet {
    pub solid_image: Option<crate::xmodel::Image>,
    pub pattern_image: Option<crate::xmodel::Image>,
    pub offset: [f32; 2],
    pub scale: f32,
}

/// One `WeaponCamoMaterialSet`: one style camo's whole recipe for a weapon (1–2 entries).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CamoMaterialSet {
    pub entries: Vec<CamoMaterial>,
}

/// One `WeaponCamoMaterial`: parallel `base`/`camo` material-name lists (each pair replaces one base
/// surface material with one camo material) and the entry's shader constants. `replaceFlags` is
/// carried uninterpreted (research §5.5).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CamoMaterial {
    pub replace_flags: u16,
    pub base: Vec<String>,
    pub camo: Vec<String>,
    pub shader_consts: [f32; 8],
}

/// Read the `WeaponCamo` at `a`.
pub fn read_camo(z: &Zone, a: u32) -> Result<WeaponCamo, FormatError> {
    // A set image is a GfxImage* (its name at +72), not a char*; the base images are the same.
    let image = |field: u32| {
        let p = z.u32(field);
        (p != 0).then(|| crate::xmodel::read_image(z, p))
    };
    let sets_at = z.try_u32(a + WC::camoSets)?;
    let n_sets = z.try_u32(a + WC::numCamoSets)?.min(64);
    let mut sets = Vec::new();
    if sets_at != 0 {
        for i in 0..n_sets {
            let s = sets_at + i * WCSet::SIZE;
            sets.push(CamoSet {
                solid_image: image(s + WCSet::solidCamoImage),
                pattern_image: image(s + WCSet::patternCamoImage),
                offset: [z.f32(s + WCSet::patternOffset), z.f32(s + WCSet::patternOffset + 4)],
                scale: z.f32(s + WCSet::patternScale),
            });
        }
    }
    let mats_at = z.try_u32(a + WC::camoMaterials)?;
    let n_mats = z.try_u32(a + WC::numCamoMaterials)?.min(64);
    let mut materials = Vec::new();
    if mats_at != 0 {
        for i in 0..n_mats {
            let set = mats_at + i * WCMatSet::SIZE;
            let n = z.u32(set + WCMatSet::numMaterials).min(64);
            let at = z.u32(set + WCMatSet::materials);
            let mut entries = Vec::new();
            if at != 0 {
                for j in 0..n {
                    let m = at + j * WCMat::SIZE;
                    // `numBaseMaterials` is the length of BOTH parallel Material* arrays; the
                    // fields themselves are pointers to those arrays, not names.
                    let count = z.u16(m + WCMat::numBaseMaterials) as u32;
                    let names = |field: u32| {
                        let arr = z.u32(field);
                        if arr == 0 {
                            return Vec::new();
                        }
                        (0..count)
                            .filter_map(|k| {
                                let p = z.u32(arr + 4 * k);
                                (p != 0).then(|| z.str_ptr(p).trim_start_matches(',').to_string())
                            })
                            .collect::<Vec<_>>()
                    };
                    entries.push(CamoMaterial {
                        replace_flags: z.u16(m + WCMat::replaceFlags),
                        base: names(m + WCMat::baseMaterials),
                        camo: names(m + WCMat::camoMaterials),
                        shader_consts: std::array::from_fn(|k| z.f32(m + WCMat::shaderConsts + 4 * k as u32)),
                    });
                }
            }
            materials.push(CamoMaterialSet { entries });
        }
    }
    Ok(WeaponCamo {
        name: z.try_str_ptr(a + WC::name)?,
        solid_base_image: image(a + WC::solidBaseImage),
        pattern_base_image: image(a + WC::patternBaseImage),
        sets,
        materials,
    })
}

/// Every `WeaponCamo` asset in a loaded zone (`common_patch_mp` in a stock install).
pub fn camo_all(z: &Zone) -> Result<Vec<WeaponCamo>, FormatError> {
    let Some(kind) = crate::fastfile::asset_type_id("weaponcamo") else { return Ok(Vec::new()) };
    z.assets.iter().filter(|a| a.kind == kind).map(|a| read_camo(z, a.addr)).collect()
}

impl Weapon {
    /// The base weapon's extra view parts (the default magazine) and their tags.
    pub fn base_view_parts(&self) -> Vec<(String, String)> {
        let mut parts: Vec<(String, String)> = self.attach_view_models.clone();
        for u in self.attachment_uniques.iter().filter(|u| u.kind == 0) {
            if let Some(m) = &u.view_model {
                parts.push((m.clone(), u.view_model_tag.clone().unwrap_or_else(|| "tag_clip".into())));
            }
        }
        parts.dedup();
        parts
    }

    pub fn anim(&self, slot: usize) -> Option<&str> {
        self.anims.get(slot).and_then(|a| a.as_deref())
    }

    /// Damage at `distance` inches: BO2 interpolates linearly between the curve's points.
    pub fn damage_at(&self, distance: f32) -> f32 {
        let n = self.damage_range.iter().take_while(|r| **r > 0.0).count().max(1);
        if distance <= self.damage_range[0] || n == 1 {
            return self.damage[0] as f32;
        }
        for i in 1..n {
            if distance <= self.damage_range[i] {
                let (r0, r1) = (self.damage_range[i - 1], self.damage_range[i]);
                let t = (distance - r0) / (r1 - r0).max(1e-3);
                return self.damage[i - 1] as f32 + (self.damage[i] - self.damage[i - 1]) as f32 * t;
            }
        }
        self.damage[n - 1] as f32
    }

    /// Rounds per burst (1 for everything that is not burst fire).
    pub fn burst_count(&self) -> u32 {
        match self.fire_type {
            fire::BURST2 => 2,
            fire::BURST3 => 3,
            fire::BURST4 => 4,
            fire::BURST5 => 5,
            _ => 1,
        }
    }

    /// A gun the player carries as a primary or secondary.
    pub fn is_gun(&self) -> bool {
        matches!(self.weap_type, kind::BULLET | kind::PROJECTILE)
            && self.inventory == inventory::PRIMARY
            && !matches!(self.class, class::TURRET | class::NON_PLAYER | class::KILLSTREAK_ALT_STORED_WEAPON)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_curve_interpolates_like_bo2() {
        let w = Weapon { damage: [40, 30, 0, 0, 0, 0], damage_range: [1000.0, 2000.0, 0.0, 0.0, 0.0, 0.0], ..Default::default() };
        assert_eq!(w.damage_at(500.0), 40.0);
        assert_eq!(w.damage_at(1500.0), 35.0);
        assert_eq!(w.damage_at(5000.0), 30.0);
    }

    fn zone(name: &str) -> Zone {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR");
        let file = std::fs::read(std::path::Path::new(&dir).join(format!("zone/all/{name}.ff"))).unwrap();
        crate::t6zone::load(&crate::fastfile::read(&file).unwrap()).unwrap()
    }

    /// Needs the player's BO2 install (with Multiplayer).
    #[test]
    #[ignore]
    fn reads_the_tracers_and_world_shells() {
        let common = zone("common_mp");
        let patch = zone("patch_mp");
        let all = all_in(&common, &[&patch]).unwrap();
        let an94 = all.iter().find(|w| w.name == "an94_mp").expect("an94_mp");
        let t = an94.tracer.as_ref().expect("the AN-94's tracer");
        assert_eq!(t.name, "assaultrifle");
        assert!(t.draw_interval > 0 && t.speed > 0.0 && t.beam_length > 0.0 && t.beam_width > 0.0, "{t:?}");
        assert!(t.colors.iter().any(|c| c[3] > 0.0), "{t:?}");
        assert_eq!(an94.world_shell_eject_effect.as_ref().map(|e| e.name.as_str()), Some("weapon/shellejects/fx_rifle"), "bound from patch_mp");
        assert!(!an94.world_shell_eject_effect.as_ref().unwrap().elems.is_empty());
        if std::env::var_os("ZHBO_PROBE").is_some() {
            let mut seen = std::collections::BTreeSet::new();
            for w in &all {
                for t in [&w.tracer, &w.enemy_tracer].into_iter().flatten().chain(w.attachment_uniques.iter().flat_map(|u| [&u.tracer, &u.enemy_tracer]).flatten()) {
                    if seen.insert(t.name.clone()) {
                        eprintln!("{t:?}");
                    }
                }
                eprintln!(
                    "{} tracer={:?} enemy={:?} shell={:?} last={:?} uniq={:?}",
                    w.name,
                    w.tracer.as_ref().map(|t| &t.name),
                    w.enemy_tracer.as_ref().map(|t| &t.name),
                    w.world_shell_eject_effect.as_ref().map(|e| (&e.name, e.elems.len())),
                    w.world_last_shot_eject_effect.as_ref().map(|e| &e.name),
                    w.attachment_uniques.iter().filter_map(|u| u.tracer.as_ref().map(|t| (&u.name, &t.name))).collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    #[ignore]
    fn reads_the_an94() {
        let z = zone("common_mp");
        let w = all(&z).unwrap().into_iter().find(|w| w.name == "an94_mp").expect("an94_mp");
        assert_eq!(w.clip_size, 30);
        assert_eq!(w.class, class::RIFLE);
        assert!(w.fire_time > 0.03 && w.fire_time < 0.2, "{}", w.fire_time);
        assert_eq!(w.view_model.as_deref(), Some("t6_wpn_ar_an94_view"));
        assert!(w.anim(anim::RELOAD).is_some() && w.anim(anim::FIRE).is_some());
        assert!(w.damage[0] > w.damage[1] && w.damage[1] > 0, "{:?}", w.damage);
        for name in ["qcw05_mp", "fnp45_mp", "870mcs_mp", "frag_grenade_mp", "knife_mp"] {
            let o = all(&z).unwrap().into_iter().find(|w| w.name == name).unwrap();
            assert!(o.view_model.is_some(), "{name} has a view model");
        }
    }

    #[test]
    fn reads_a_synthetic_camo_asset() {
        // One pattern set and one style entry of two material pairs. The set images are GfxImage
        // pointers (name at +72); the pair fields are Material* arrays. The old reader stringifies
        // the image struct address and the array pointer, so this test fails on it: images come back
        // None and the pair names are mojibake.
        let mut mem = vec![0u8; 0x500];
        let put32 = |m: &mut Vec<u8>, at: usize, v: u32| m[at..at + 4].copy_from_slice(&v.to_le_bytes());
        let put16 = |m: &mut Vec<u8>, at: usize, v: u16| m[at..at + 2].copy_from_slice(&v.to_le_bytes());
        let putf = |m: &mut Vec<u8>, at: usize, v: f32| m[at..at + 4].copy_from_slice(&v.to_le_bytes());
        let put_str = |m: &mut Vec<u8>, at: usize, s: &str| {
            m[at..at + s.len()].copy_from_slice(s.as_bytes());
            m[at + s.len()] = 0;
        };
        // The asset at 0x40.
        put_str(&mut mem, 0x300, "camo_an94");
        put32(&mut mem, 0x40, 0x300);
        // The set's solid image at 0x80 (name at +72 -> 0x320) and pattern at 0xD0 (-> 0x340).
        put_str(&mut mem, 0x320, "camo_off_solid");
        put32(&mut mem, 0x80 + 72, 0x320);
        put16(&mut mem, 0x80 + 20, 32);
        put16(&mut mem, 0x80 + 22, 32);
        put32(&mut mem, 0x80 + 40, 0x1111);
        put_str(&mut mem, 0x340, "t6_camo_atacs_pattern");
        put32(&mut mem, 0xD0 + 72, 0x340);
        put16(&mut mem, 0xD0 + 20, 128);
        put16(&mut mem, 0xD0 + 22, 128);
        put32(&mut mem, 0xD0 + 40, 0x2222);
        // The set array at 0x140: one 20-byte set.
        put32(&mut mem, 0x40 + 12, 0x140); // camoSets
        put32(&mut mem, 0x40 + 16, 1); // numCamoSets
        put32(&mut mem, 0x140, 0x80); // solidCamoImage
        put32(&mut mem, 0x144, 0xD0); // patternCamoImage
        putf(&mut mem, 0x148, 0.25); // patternOffset.x
        putf(&mut mem, 0x14C, 0.5); // patternOffset.y
        putf(&mut mem, 0x150, 2.0); // patternScale
        // The material set array at 0x180: one set with one 44-byte entry at 0x190.
        put32(&mut mem, 0x40 + 20, 0x180); // camoMaterials
        put32(&mut mem, 0x40 + 24, 1); // numCamoMaterials
        put32(&mut mem, 0x180, 1); // numMaterials
        put32(&mut mem, 0x184, 0x190); // materials
        put16(&mut mem, 0x190, 6); // replaceFlags
        put16(&mut mem, 0x192, 2); // numBaseMaterials
        put32(&mut mem, 0x194, 0x1D0); // baseMaterials
        put32(&mut mem, 0x198, 0x1E0); // camoMaterials
        putf(&mut mem, 0x19C, 10.0); // shaderConsts[0]
        // The two parallel arrays: base then camo, each two Material* pointers. A Material's name is
        // its first field (a char*), so each entry at 0x400/0x410/0x420/0x430 names a string.
        put32(&mut mem, 0x1D0, 0x400);
        put32(&mut mem, 0x1D4, 0x410);
        put32(&mut mem, 0x1E0, 0x420);
        put32(&mut mem, 0x1E4, 0x430);
        put_str(&mut mem, 0x480, "mc/mtl_t6_wpn_ar_an94_camo1");
        put_str(&mut mem, 0x4A0, "mc/mtl_t6_wpn_ar_an94_camo3");
        put_str(&mut mem, 0x4C0, "mc/mtl_weapon_camo_gold");
        put_str(&mut mem, 0x4E0, "mc/mtl_weapon_camo_gold_1");
        put32(&mut mem, 0x400, 0x480);
        put32(&mut mem, 0x410, 0x4A0);
        put32(&mut mem, 0x420, 0x4C0);
        put32(&mut mem, 0x430, 0x4E0);
        // The base images the asset states it supersedes.
        put32(&mut mem, 0x40 + 4, 0x80);
        put32(&mut mem, 0x40 + 8, 0xD0);
        let zone = crate::t6zone::synthetic_zone("camo", mem, Vec::new(), Vec::new());
        let c = read_camo(&zone, 0x40).expect("the camo reads");
        assert_eq!(c.name, "camo_an94");
        assert_eq!(c.solid_base_image.as_ref().map(|i| i.name.as_str()), Some("camo_off_solid"));
        assert_eq!(c.pattern_base_image.as_ref().map(|i| i.name.as_str()), Some("t6_camo_atacs_pattern"));
        assert_eq!(c.sets.len(), 1);
        assert_eq!(c.sets[0].solid_image.as_ref().map(|i| i.name.as_str()), Some("camo_off_solid"));
        assert_eq!(c.sets[0].pattern_image.as_ref().map(|i| i.name.as_str()), Some("t6_camo_atacs_pattern"));
        assert_eq!(c.sets[0].offset, [0.25, 0.5]);
        assert_eq!(c.sets[0].scale, 2.0);
        assert_eq!(c.materials.len(), 1, "the set boundary survives");
        let e = &c.materials[0].entries[0];
        assert_eq!(e.replace_flags, 6);
        assert_eq!(e.base, ["mc/mtl_t6_wpn_ar_an94_camo1", "mc/mtl_t6_wpn_ar_an94_camo3"]);
        assert_eq!(e.camo, ["mc/mtl_weapon_camo_gold", "mc/mtl_weapon_camo_gold_1"]);
        assert_eq!(e.shader_consts[0], 10.0);
    }

    /// Needs the player's BO2 install with Multiplayer.
    #[test]
    #[ignore]
    fn reads_the_an94_camo() {
        let z = zone("common_patch_mp");
        let all = camo_all(&z).unwrap();
        assert_eq!(all.len(), 48, "common_patch_mp's camo_<weapon> assets (research §1.1)");
        let c = all.iter().find(|c| c.name == "camo_an94").expect("camo_an94");
        assert_eq!(c.sets.len(), 40, "the patch zone's pattern slots");
        assert_eq!(c.materials.len(), 15, "gold/carbon/diamond/… style recipes");
        assert_eq!(c.solid_base_image.as_ref().map(|i| i.name.as_str()), Some("camo_off_solid"));
        assert_eq!(c.pattern_base_image.as_ref().map(|i| i.name.as_str()), Some("camo_off_pattern"));
        // The set list is the global slot list: devgru first, atacs second (research §1.2).
        assert_eq!(c.sets[0].pattern_image.as_ref().map(|i| i.name.as_str()), Some("t6_camo_devgru_pattern"));
        assert_eq!(c.sets[1].pattern_image.as_ref().map(|i| i.name.as_str()), Some("t6_camo_atacs_pattern"));
        assert_eq!(c.sets[1].solid_image.as_ref().map(|i| i.name.as_str()), Some("t6_camo_atacs_solid"));
        assert!(c.sets.iter().all(|s| s.pattern_image.is_some() && s.scale > 0.0));
        // Gold: two entries, the pattern-kind surfaces then the solid-kind ones (research §1.3).
        let gold = &c.materials[0];
        assert_eq!(gold.entries.len(), 2);
        let e = &gold.entries[0];
        assert_eq!(e.replace_flags, 0x6);
        assert_eq!(e.base, ["mc/mtl_t6_wpn_ar_an94_camo1", "mc/mtl_t6_wpn_ar_an94_camo3"]);
        assert_eq!(e.camo, ["mc/mtl_weapon_camo_gold", "mc/mtl_weapon_camo_gold_1"]);
        assert_eq!(e.shader_consts[0], 10.0);
        assert_eq!(gold.entries[1].base, ["mc/mtl_t6_wpn_ar_an94_camo2", "mc/mtl_t6_wpn_ar_an94_camo4"]);
        for set in &c.materials {
            for e in &set.entries {
                assert_eq!(e.base.len(), e.camo.len(), "{e:?}");
                assert!(e.base.iter().all(|n| n.starts_with("mc/mtl_") || n.starts_with("mtl_")), "{e:?}");
            }
        }
    }

    #[test]
    fn the_base_unique_carries_the_overlay_material() {
        // The base unique's kind == 0 entry is the weapon's base configuration. The synthetic zone's
        // base unique points at `scope_overlay_xpr50`; the weapon surfaces it.
        let mut mem = vec![0u8; 512];
        let put32 = |mem: &mut Vec<u8>, at: usize, v: u32| {
            mem[at..at + 4].copy_from_slice(&v.to_le_bytes());
        };
        let put_str = |mem: &mut Vec<u8>, at: usize, s: &str| {
            let b = s.as_bytes();
            mem[at..at + b.len()].copy_from_slice(b);
            mem[at + b.len()] = 0;
        };
        put_str(&mut mem, 16, "as50_mp");
        put32(&mut mem, 0, 16);
        put_str(&mut mem, 32, "AS50");
        put32(&mut mem, 4, 32);
        // The base unique array at 64: one pointer to 128.
        put32(&mut mem, 64, 128);
        put32(&mut mem, 68, 0);
        put32(&mut mem, 28, 64); // WeaponVariantDef.attachmentUniques = 64
        // The AttachmentUnique at 128: the name is a pointer at offset 0.
        put32(&mut mem, 128, 148);
        put_str(&mut mem, 148, "au_as50_none");
        // attachmentType at offset 4 is 0.
        // overlayMaterial at offset 196 points to a Material at 300 whose name pointer is at 340.
        put32(&mut mem, 300, 340);
        put_str(&mut mem, 340, "scope_overlay_xpr50");
        put32(&mut mem, 128 + 196, 300);
        // overlayMaterialLowRes at offset 200 is 0.
        // overlayReticle at offset 204 is 1.
        put32(&mut mem, 128 + 204, 1);
        let zone = crate::t6zone::synthetic_zone("t", mem, Vec::new(), Vec::new());
        let w = read(&zone, 0).expect("the weapon reads");
        assert_eq!(w.overlay_material, "scope_overlay_xpr50");
        assert_eq!(w.overlay_reticle, 1);
    }
}
