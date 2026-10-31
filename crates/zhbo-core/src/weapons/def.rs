/*
** Conquer Ops - Zero Hour gameplay core: the weapon definition: BO2's WeaponDef, its enums and formatting helpers.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::feel::{GunKick, Reticle, Sway, ViewPos};
use super::specialty::Specialties;
use crate::units::inches;

/// BO2's base aim-down-sights walking speed, as a share of the normal walk. The weapon definition's
/// `ads_move_scale` stores the walk as this base times BO2's `adsMoveSpeedScale`; the attachment
/// replacement walk converts an attachment's raw scale with the same base.
pub const ADS_MOVE_BASE: f32 = 0.35;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    Rifle,
    Smg,
    Lmg,
    Sniper,
    Shotgun,
    Pistol,
    Launcher,
    Special,
    /// The Assault Shield (BO2 weapType `riotshield`): a primary that blocks, deploys and melees
    /// with itself, never fires a round.
    Shield,
    Melee,
    Lethal,
    Tactical,
    Equipment,
}

impl Class {
    pub fn is_gun(self) -> bool {
        !matches!(self, Class::Shield | Class::Melee | Class::Lethal | Class::Tactical | Class::Equipment)
    }
    /// A weapon of the primary/secondary slots: a gun or the Assault Shield.
    pub fn is_slot_weapon(self) -> bool {
        self.is_gun() || self == Class::Shield
    }
    pub fn is_offhand(self) -> bool {
        matches!(self, Class::Lethal | Class::Tactical | Class::Equipment)
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Class::Rifle => "rifle",
            Class::Smg => "smg",
            Class::Lmg => "lmg",
            Class::Sniper => "sniper",
            Class::Shotgun => "shotgun",
            Class::Pistol => "pistol",
            Class::Launcher => "launcher",
            Class::Special => "special",
            Class::Shield => "shield",
            Class::Melee => "melee",
            Class::Lethal => "lethal",
            Class::Tactical => "tactical",
            Class::Equipment => "equipment",
        }
    }
    /// BO2's Create-a-Class group for a class (`mp/statstable.csv` column 2); empty for melee.
    pub fn cac_group(self) -> &'static str {
        match self {
            Class::Rifle => "weapon_assault",
            Class::Smg => "weapon_smg",
            Class::Lmg => "weapon_lmg",
            Class::Sniper => "weapon_sniper",
            Class::Shotgun => "weapon_cqb",
            Class::Pistol => "weapon_pistol",
            Class::Launcher => "weapon_launcher",
            Class::Special | Class::Shield => "weapon_special",
            Class::Lethal | Class::Tactical | Class::Equipment => "weapon_grenade",
            Class::Melee => "",
        }
    }
    pub(crate) fn parse(s: &str) -> Option<Self> {
        [
            Class::Rifle,
            Class::Smg,
            Class::Lmg,
            Class::Sniper,
            Class::Shotgun,
            Class::Pistol,
            Class::Launcher,
            Class::Special,
            Class::Shield,
            Class::Melee,
            Class::Lethal,
            Class::Tactical,
            Class::Equipment,
        ]
        .into_iter()
        .find(|c| c.name() == s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireMode {
    Auto,
    Semi,
    Burst(u8),
    /// Full auto after a spin-up while the trigger is held.
    Minigun,
}

/// What a projectile does when it goes off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Explosion damage.
    Explode,
    /// Disables enemies in the radius (concussion, flash, shock charge): Zero Hour's paralysis.
    Stun,
    /// Disables vehicles and structures in the radius: Zero Hour's EMP.
    Emp,
    /// Smoke screen (visual only).
    Smoke,
    /// Damage to what it hits (ballistic knife, combat axe), no explosion.
    Impact,
    /// No gameplay effect yet (sensor, trophy, tactical insertion).
    None,
}

impl Effect {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Effect::Explode => "explode",
            Effect::Stun => "stun",
            Effect::Emp => "emp",
            Effect::Smoke => "smoke",
            Effect::Impact => "impact",
            Effect::None => "none",
        }
    }
    pub(crate) fn parse(s: &str) -> Option<Self> {
        [Effect::Explode, Effect::Stun, Effect::Emp, Effect::Smoke, Effect::Impact, Effect::None].into_iter().find(|e| e.name() == s)
    }
}

/// Min/max of a per-shot view kick, degrees.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Kick {
    pub pitch: (f32, f32),
    pub yaw: (f32, f32),
}

/// One BO2 weapon. Distances are Zero Hour units, times seconds, damage on BO2's 100 HP scale.
#[derive(Clone, Debug, PartialEq)]
pub struct WeaponDef {
    /// BO2's internal name, e.g. `an94_mp`.
    pub id: String,
    /// As BO2's HUD shows it, e.g. `AN-94`.
    pub display_name: String,
    pub class: Class,
    pub fire: FireMode,
    pub fire_time: f32,
    /// Extra wait after a burst before the next one.
    pub burst_delay: f32,
    pub spin_up_time: f32,
    pub pellets: u16,
    pub clip: u16,
    pub start_reserve: u16,
    pub max_reserve: u16,
    pub reload_time: f32,
    pub reload_empty_time: f32,
    /// When the rounds go in (a reload cancelled after this keeps them).
    pub reload_add_time: f32,
    pub reload_empty_add_time: f32,
    /// Shell-by-shell reloads (tube shotguns): start, rounds per `reload_time` cycle, end.
    pub reload_start_time: f32,
    pub reload_end_time: f32,
    pub reload_ammo_add: u16,
    pub raise_time: f32,
    pub first_raise_time: f32,
    pub drop_time: f32,
    pub quick_raise_time: f32,
    pub quick_drop_time: f32,
    pub sprint_out_time: f32,
    pub ads_in_time: f32,
    pub ads_out_time: f32,
    /// Horizontal field of view fully aimed (degrees at 4:3), 0 if the weapon has no zoom.
    pub ads_fov: f32,
    /// BO2's damage curve: damage at each range.
    pub damage: [f32; 6],
    pub damage_range: [f32; 6],
    pub head_multiplier: f32,
    pub max_range: f32,
    /// Hip spread cone (degrees) min/max standing, crouched, prone.
    pub hip_spread: [(f32, f32); 3],
    pub spread_decay: f32,
    pub spread_fire_add: f32,
    pub spread_move_add: f32,
    pub ads_spread: f32,
    pub ads_kick: Kick,
    pub hip_kick: Kick,
    pub move_scale: f32,
    pub ads_move_scale: f32,
    pub sprint_duration_scale: f32,
    pub melee_damage: f32,
    pub melee_time: f32,
    /// Seconds from the melee press to the strike (`iMeleeDelay`): BO2's swing starts on the press
    /// and the hit lands when this delay runs out.
    pub melee_delay: f32,
    /// Grenades and launchers.
    pub effect: Effect,
    pub fuse_time: f32,
    /// Hold to cook (the fuse runs from the end of the pull-back, `FUN_008e28a0`).
    pub cook: bool,
    /// BO2's offhand state machine (Ghidra `FUN_008e2710`..`FUN_008dcbe0`, MODLOG "Offhands run
    /// BO2's offhand states"): the pull-back (`iHoldFireTime`, state 0x17), the wait from the
    /// release to the item leaving the hand (`iFireDelay`, inside the `fire_time` of state 0x18),
    /// the detonate state (`iDetonateTime`, its charge going off at `iDetonateDelay`, 0x19) a
    /// `hasDetonator` item enters when the button is pressed with none left in hand, and
    /// `isHoldUseGrenade` (`hold_use`: held up while the button is, then put away in its quick drop,
    /// never thrown).
    pub hold_fire_time: f32,
    pub fire_delay: f32,
    pub detonate_time: f32,
    pub detonate_delay: f32,
    pub has_detonator: bool,
    pub hold_use: bool,
    /// `offhandHoldIsCancelable`: held up after the pull-back (0x1a), the weapon-switch button
    /// puts it away unused (`FUN_008e28e0` → `FUN_008dcbe0`; the client's weapnext sends the
    /// cancel instead of cycling, `FUN_00537c20`).
    pub hold_cancelable: bool,
    /// `plantable`: set down where it lands, so BO2's plant check (`FUN_008e1780`,
    /// [`crate::sim::can_plant`]) can refuse it ("Cannot Plant Here").
    pub plantable: bool,
    /// BO2's `inventoryType` ([`Inventory`]).
    pub inventory: Inventory,
    pub explosion_radius: f32,
    pub explosion_radius_min: f32,
    pub explosion_inner_damage: f32,
    pub explosion_outer_damage: f32,
    pub projectile_speed: f32,
    pub projectile_speed_up: f32,
    pub projectile_gravity: bool,
    pub impact_explode: bool,
    /// BO2's `WeaponDef.impactType` (+40): the impact-table bucket an impact-table lookup uses
    /// (`crates/zhbo-core/src/impacts.rs`); 0 when BO2 has none.
    pub impact_type: i32,
    pub sticky: bool,
    /// Zero Hour assets made for this weapon by the converter (empty when absent).
    pub fx_weapon: String,
    /// Zero Hour FXList for the blast (grenades, rockets).
    pub explosion_fx: String,
    /// Zero Hour FXList for the first-person muzzle flash (`viewFlashEffect`).
    pub view_flash_fx: String,
    /// Zero Hour FXList for the first-person shell eject (`viewShellEjectEffect`).
    pub view_shell_fx: String,
    /// Zero Hour FXList for the first-person shell eject on the last round.
    pub view_last_shot_shell_fx: String,
    pub projectile_object: String,
    pub fire_sound: String,
    /// The empty magazine's click in BO2's vocabulary (`emptyFireSoundPlayer`), as a converted event.
    pub empty_fire_sound: String,
    /// A melee weapon's own swing (BO2's `meleeSwipeSoundPlayer`) as a converted event; empty for
    /// the rest, which melee with the class melee weapon ([`super::melee_weapon`]).
    pub melee_sound: String,
    /// The suppressed shot's player event (`AttachmentUnique.fire_sound_player`).
    pub silenced_fire_sound: String,
    /// The suppressed shot's third-person event (`AttachmentUnique.fire_sound`).
    pub silenced_fire_sound_npc: String,
    /// The FX weapon whose INI block carries the suppressed shot and flash.
    pub silenced_fx_weapon: String,
    /// The shared suppressed first-person FXList (`weapon/muzzleflashes/fx_muz_silencer_md`).
    pub silenced_view_flash_fx: String,
    pub explode_sound: String,
    /// Viewmodel index in the converted art, or -1.
    pub viewmodel: i32,
    /// Which BO2 viewmodel animation slots (`weapAnimFiles_t`) the weapon has, as bits.
    pub anims: u128,
    /// Viewmodel clips are the BO2 slot plus this: an alt mode's clips sit after its weapon's on the
    /// same viewmodel ([`ALT_CLIPS`]).
    pub clip_base: i32,
    /// Third-person meshes of this weapon on the soldier model (`W{viewmodel:02}{k:02}`).
    pub world_meshes: u8,
    /// Attachments on this weapon (BO2's names: `grip`, `reflex`), sorted.
    pub attachments: Vec<String>,
    /// Base weapons: each attachment BO2's create-a-class offers, and which `Attachment` it uses
    /// on this weapon (`silencer` is `silencer_smg` on an SMG).
    pub allowed: Vec<(String, String)>,
    /// Base weapons: the alt-mode weapon an attachment adds (`gl` → `gl_an94_mp`).
    pub alt_modes: Vec<(String, String)>,
    /// The weapon this toggles to (its alt mode, or back from it); empty when there is none.
    pub alt: String,
    /// This alt mode keeps its own ammo (a grenade launcher), not the weapon's magazine.
    pub alt_own_ammo: bool,
    /// The Create-a-Class slot BO2 offers this weapon in (`mp/statstable.csv` column 13:
    /// `primary`, `secondary`, `primarygrenade`, `specialgrenade`); empty when it is not a
    /// Create-a-Class item (a negative allocation cost: the `_dw` pistols, the War Machine, the
    /// held knife). A combination inherits its base's.
    pub cac_slot: String,
    /// The Create-a-Class group BO2 lists this item under (`mp/statstable.csv` column 2:
    /// `weapon_assault`, `weapon_special`, ...); empty when it is not a Create-a-Class item. The
    /// editor's categories follow it ([`WeaponDef::cac_class`]); `class` stays the weapon's own
    /// (the Ballistic Knife is a pistol that BO2 lists under SPECIALS).
    pub cac_group: String,
    /// The left gun of a dual-wield weapon (BO2's `szDualWieldWeaponName`, `fnp45_lh_mp`); empty
    /// for a single gun.
    pub dual_wield_weapon: String,
    pub silenced: bool,
    pub laser: bool,
    /// FMJ: penetrates infantry, hits vehicles and structures harder.
    pub fmj: bool,
    /// Target finder: enemies are marked while aiming.
    pub target_finder: bool,
    /// ADS recoil reduction per shot fired, and its limit (the grip).
    pub recoil_reduction: (f32, f32),
    /// How a piece of equipment is used.
    pub place: Place,
    /// What the weapon is as equipment (claymore, C4, …); `Other` when it stays in the hand.
    pub equipment_kind: Kind,
    /// The specialties this weapon grants while it is the current weapon: its variant's own
    /// (`WeaponVariantDef.perks`) and its attachments' (`WeaponAttachment.perks`), unioned when
    /// the attachments are applied (Ghidra G1's HasSpecialty rule).
    pub specialties: Specialties,
    /// `f{Ads,Hip}ViewKickMinMagnitude`: the shortest view kick a shot gives, aimed and hip (G5).
    pub view_kick_min: (f32, f32),
    /// `f{Ads,Hip}ViewKickCenterSpeed`: how fast the view kick recentres (G5).
    pub view_kick_center: (f32, f32),
    pub ads_gun_kick: GunKick,
    pub hip_gun_kick: GunKick,
    /// `fGunMaxPitch`, `fGunMaxYaw`: the gun kick spring's limits (G5).
    pub gun_max: (f32, f32),
    /// `fAdsRecoilReturnRate`: how fast the grip's recoil reduction wears off between shots (G5).
    pub recoil_return_rate: f32,
    pub hip_sway: Sway,
    pub ads_sway: Sway,
    pub view_pos: ViewPos,
    pub reticle: Reticle,
    /// The camouflage options this weapon offers, as `camo_*` ids with `camo_none` first
    /// (`mp/attachmenttable.csv`'s `camo` rows).
    pub camo: Vec<String>,
    /// The weapon's own camo material (`camo_<weapon>`, `common_patch_mp`), empty when BO2 has none.
    pub camo_material: String,
    /// The 2D screen-space ADS overlay material (the snipers' `scope_overlay_<weapon>`), or empty.
    pub overlay_material: String,
    /// The low-resolution variant of [`Self::overlay_material`], or empty.
    pub overlay_material_low_res: String,
    /// `weapOverlayReticle_t`: which reticle the overlay draws (0: none).
    pub overlay_reticle: u32,
    /// An Assault Shield (BO2 weapType `riotshield`): its held and stowed collision boxes, posed on
    /// the body by the converter. `None` for every other weapon.
    pub shield: Option<ShieldGeometry>,
}

/// One posed shield collision box: BO2's riot-shield collmap (hit-location rank 101, above every
/// body box) as the model's bounds on its tag. Inches, in the body's model space (x forward,
/// y left, z up from the feet); `rot` (x, y, z, w) and `pos` place the box's own frame, whose x
/// axis is the tag's forward (the axis BO2's blast protection measures against, `FUN_0066f680`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShieldBox {
    pub rot: [f32; 4],
    pub pos: [f32; 3],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
}

impl ShieldBox {
    pub const NUMS: usize = 13;
    pub fn to_nums(&self) -> Vec<f32> {
        [&self.rot[..], &self.pos[..], &self.mins[..], &self.maxs[..]].concat()
    }
    pub fn from_nums(n: &[f32]) -> Self {
        let a = |i: usize| [n[i], n[i + 1], n[i + 2]];
        ShieldBox { rot: [n[0], n[1], n[2], n[3]], pos: a(4), mins: a(7), maxs: a(10) }
    }
}

/// The Assault Shield's two collision boxes (`_riotshield.gsc`): `t6_wpn_shield_carry_world` on
/// `tag_weapon_left` while it is the weapon up, `t6_wpn_shield_stow_world` on `tag_stowed_back`
/// while it is carried but not up.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShieldGeometry {
    pub held: ShieldBox,
    pub stowed: ShieldBox,
}

/// One BO2 camouflage option (`mp/attachmenttable.csv`'s rows with `camo` in column 1): the id
/// (`camo_none`, `camo_multicam`), the localized name the grid shows, the thumbnail image BO2's
/// `CamoGridButton` draws, and the table's sort key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CamoOption {
    pub id: String,
    pub name: String,
    pub image: String,
    pub sort: u32,
}

/// One BO2 reticle option (`reticle_0..10` from `mp/attachmenttable.csv`): the same id/name/image/
/// sort shape as a camo row; the art depends on the optic (`cacreticles.lua`).
pub type ReticleOption = CamoOption;

/// One BO2 reticle colour (`reticle_color_1..7`): the same shape; the name is the colour
/// (`MPUI_RETICLE_COLOR_1` is "Red").
pub type ReticleColorOption = CamoOption;

/// What a weapon is as equipment, once down (claymore, C4, …). The converter writes it from the
/// weapon table's `Equipment` block; `Kind::of` is just the field read (refactor plan 3.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Kind {
    Claymore,
    Betty,
    C4,
    Shock,
    Trophy,
    Insertion,
    Sensor,
    /// Not equipment that stays down.
    #[default]
    Other,
}

impl Kind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Kind::Claymore => "claymore",
            Kind::Betty => "betty",
            Kind::C4 => "c4",
            Kind::Shock => "shock",
            Kind::Trophy => "trophy",
            Kind::Insertion => "insertion",
            Kind::Sensor => "sensor",
            Kind::Other => "none",
        }
    }
    pub(crate) fn parse(s: &str) -> Option<Kind> {
        [Kind::Claymore, Kind::Betty, Kind::C4, Kind::Shock, Kind::Trophy, Kind::Insertion, Kind::Sensor, Kind::Other]
            .into_iter()
            .find(|k| k.name() == s)
    }
}

/// An alt mode's viewmodel clips are its BO2 slots plus this.
pub const ALT_CLIPS: i32 = 0x80;

/// BO2's `weapInventoryType` (`WeaponDef.inventoryType`), carried as data: no bg code branches on it
/// (only a client HUD path, `FUN_005efeb0`), and neither does the mod.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Inventory {
    #[default]
    Primary,
    Offhand,
    /// The placed equipment and the Black Hat.
    Item,
    AltMode,
    Melee,
    DwLeftHand,
}

impl Inventory {
    pub const ALL: [Inventory; 6] = [Inventory::Primary, Inventory::Offhand, Inventory::Item, Inventory::AltMode, Inventory::Melee, Inventory::DwLeftHand];
    /// BO2's name (`primary`, `offhand`, `item`, `altmode`, `melee`, `dwlefthand`).
    pub fn name(self) -> &'static str {
        match self {
            Inventory::Primary => "primary",
            Inventory::Offhand => "offhand",
            Inventory::Item => "item",
            Inventory::AltMode => "altmode",
            Inventory::Melee => "melee",
            Inventory::DwLeftHand => "dwlefthand",
        }
    }
    pub fn parse(s: &str) -> Option<Inventory> {
        Self::ALL.into_iter().find(|i| i.name() == s)
    }
    /// From `weapInventoryType_t`'s value (0 primary .. 5 dwlefthand).
    pub fn from_bo2(v: u32) -> Inventory {
        Self::ALL.get(v as usize).copied().unwrap_or_default()
    }
}

/// How an offhand item is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Place {
    /// Thrown (grenades, C4, Shock Charge, Sensor Grenade).
    #[default]
    Throw,
    /// Set down in front of the player (Claymore, Bouncing Betty, Trophy System, Tactical Insertion).
    Plant,
    /// Held up and aimed at enemy equipment or drones (Black Hat).
    Hack,
}

impl Place {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Place::Throw => "throw",
            Place::Plant => "plant",
            Place::Hack => "hack",
        }
    }
    pub(crate) fn parse(s: &str) -> Option<Place> {
        [Place::Throw, Place::Plant, Place::Hack].into_iter().find(|p| p.name() == s)
    }
}

impl Default for WeaponDef {
    fn default() -> Self {
        Self {
            id: String::new(),
            display_name: String::new(),
            class: Class::Rifle,
            fire: FireMode::Auto,
            fire_time: 0.1,
            burst_delay: 0.0,
            spin_up_time: 0.0,
            pellets: 1,
            clip: 30,
            start_reserve: 90,
            max_reserve: 210,
            reload_time: 2.0,
            reload_empty_time: 2.5,
            reload_add_time: 1.5,
            reload_empty_add_time: 2.0,
            reload_start_time: 0.0,
            reload_end_time: 0.0,
            reload_ammo_add: 0,
            raise_time: 0.5,
            first_raise_time: 1.0,
            drop_time: 0.4,
            quick_raise_time: 0.3,
            quick_drop_time: 0.25,
            sprint_out_time: 0.2,
            ads_in_time: 0.25,
            ads_out_time: 0.2,
            ads_fov: 55.0,
            damage: [40.0, 30.0, 0.0, 0.0, 0.0, 0.0],
            damage_range: [inches(1000.0), inches(2000.0), 0.0, 0.0, 0.0, 0.0],
            head_multiplier: 1.0,
            max_range: inches(8000.0),
            hip_spread: [(3.0, 6.75), (2.5, 6.0), (2.0, 5.0)],
            spread_decay: 6.0,
            spread_fire_add: 0.5,
            spread_move_add: 0.5,
            ads_spread: 0.0,
            ads_kick: Kick::default(),
            hip_kick: Kick::default(),
            move_scale: 1.0,
            ads_move_scale: 0.6,
            sprint_duration_scale: 1.0,
            melee_damage: MELEE_DAMAGE,
            melee_time: 0.6,
            melee_delay: 0.0,
            effect: Effect::None,
            equipment_kind: Kind::Other,
            fuse_time: 0.0,
            cook: false,
            hold_fire_time: 0.0,
            fire_delay: 0.0,
            detonate_time: 0.0,
            detonate_delay: 0.0,
            has_detonator: false,
            hold_use: false,
            hold_cancelable: false,
            plantable: false,
            inventory: Inventory::Primary,
            explosion_radius: 0.0,
            explosion_radius_min: 0.0,
            explosion_inner_damage: 0.0,
            explosion_outer_damage: 0.0,
            projectile_speed: 0.0,
            projectile_speed_up: 0.0,
            projectile_gravity: true,
            impact_explode: false,
            impact_type: 0,
            sticky: false,
            fx_weapon: String::new(),
            explosion_fx: String::new(),
            view_flash_fx: String::new(),
            view_shell_fx: String::new(),
            view_last_shot_shell_fx: String::new(),
            projectile_object: String::new(),
            fire_sound: String::new(),
            empty_fire_sound: String::new(),
            melee_sound: String::new(),
            silenced_fire_sound: String::new(),
            silenced_fire_sound_npc: String::new(),
            silenced_fx_weapon: String::new(),
            silenced_view_flash_fx: String::new(),
            explode_sound: String::new(),
            viewmodel: -1,
            anims: 0,
            clip_base: 0,
            world_meshes: 0,
            attachments: Vec::new(),
            allowed: Vec::new(),
            alt_modes: Vec::new(),
            alt: String::new(),
            alt_own_ammo: false,
            cac_slot: String::new(),
            cac_group: String::new(),
            dual_wield_weapon: String::new(),
            silenced: false,
            laser: false,
            fmj: false,
            target_finder: false,
            recoil_reduction: (0.0, 0.0),
            place: Place::Throw,
            specialties: Specialties::NONE,
            view_kick_min: (0.0, 0.0),
            view_kick_center: (0.0, 0.0),
            ads_gun_kick: GunKick::default(),
            hip_gun_kick: GunKick::default(),
            gun_max: (0.0, 0.0),
            recoil_return_rate: 0.0,
            hip_sway: Sway::default(),
            ads_sway: Sway::default(),
            view_pos: ViewPos::default(),
            reticle: Reticle::default(),
            camo: Vec::new(),
            camo_material: String::new(),
            overlay_material: String::new(),
            overlay_material_low_res: String::new(),
            overlay_reticle: 0,
            shield: None,
        }
    }
}

/// BO2 view kick values are kick-physics inputs; this turns one into degrees of view per shot.
pub const KICK_TO_DEGREES: f32 = 1.0 / 80.0;

pub const MELEE_DAMAGE: f32 = 135.0;
pub const MELEE_RANGE: f32 = inches(64.0);
pub const MELEE_FX_WEAPON: &str = "ZHBO_Knife_FX";

impl WeaponDef {
    /// An Assault Shield: a primary that blocks, deploys and melees with itself.
    pub fn is_shield(&self) -> bool {
        self.class == Class::Shield
    }

    /// The category Create-a-Class lists this weapon under: the gun class whose BO2 group is its
    /// statstable group ([`WeaponDef::cac_group`]), else its own class (not a CaC item, or a
    /// grenade-group item, which keeps its lethal/tactical/equipment class).
    pub fn cac_class(&self) -> Class {
        const GUNS: [Class; 8] = [Class::Rifle, Class::Smg, Class::Lmg, Class::Sniper, Class::Shotgun, Class::Pistol, Class::Launcher, Class::Special];
        GUNS.into_iter().find(|c| !self.cac_group.is_empty() && c.cac_group() == self.cac_group).unwrap_or(self.class)
    }

    /// Damage at `distance` (ZH units). BO2's curve has six slots: unused middle slots have no
    /// range, and the last is the minimum damage at the end of the curve. Damage interpolates
    /// linearly between the used points and stays at the last one beyond it.
    pub fn damage_at(&self, distance: f32) -> f32 {
        let points: Vec<(f32, f32)> = (0..6).filter(|&i| i == 0 || self.damage_range[i] > 0.0).map(|i| (self.damage_range[i], self.damage[i])).collect();
        if distance <= points[0].0 || points.len() == 1 {
            return points[0].1;
        }
        for w in points.windows(2) {
            let ((r0, d0), (r1, d1)) = (w[0], w[1]);
            if distance <= r1 {
                if r1 <= r0 {
                    return d0;
                }
                return d0 + (d1 - d0) * (distance - r0) / (r1 - r0);
            }
        }
        points[points.len() - 1].1
    }

    /// Explosion damage at `distance` from the blast (BO2: inner damage at the centre, falling to
    /// outer damage at the radius).
    pub fn explosion_damage_at(&self, distance: f32) -> f32 {
        if distance > self.explosion_radius {
            return 0.0;
        }
        let t = ((distance - self.explosion_radius_min) / (self.explosion_radius - self.explosion_radius_min).max(1e-3)).clamp(0.0, 1.0);
        self.explosion_inner_damage + (self.explosion_outer_damage - self.explosion_inner_damage) * t
    }

    pub fn rounds_per_minute(&self) -> f32 {
        60.0 / self.fire_time.max(1e-3)
    }

    pub fn shell_by_shell(&self) -> bool {
        self.reload_start_time > 0.0 && self.reload_ammo_add > 0
    }

    pub fn has_anim(&self, slot: usize) -> bool {
        slot < 128 && self.anims & (1u128 << slot) != 0
    }

    pub fn fire_mode_name(&self) -> &'static str {
        match self.fire {
            FireMode::Auto | FireMode::Minigun => "FULL-AUTO",
            FireMode::Semi => "SEMI-AUTO",
            FireMode::Burst(_) => "BURST",
        }
    }
}