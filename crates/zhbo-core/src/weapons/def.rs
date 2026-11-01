/*
** Conquer Ops - Zero Hour gameplay core: the weapon definition: BO2's WeaponDef, its enums and formatting helpers.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::feel::{GunKick, Reticle, Sway, ViewPos};
use super::handling::{CycleTimes, Traits, WeapType, Zoom};
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
    /// The gun classes, each one Create-a-Class weapon group ([`Class::cac_group`]).
    pub const ALL_GUNS: [Class; 8] = [Class::Rifle, Class::Smg, Class::Lmg, Class::Sniper, Class::Shotgun, Class::Pistol, Class::Launcher, Class::Special];

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

/// BO2's `weapClass_t`, carried from the weapon's def. The engine branches on it where the mod's
/// [`Class`] (a CaC/HUD grouping) would be wrong: a spread class fires `shotCount` pellets that
/// fly only to the curve's last range, every other class one round to `bulletrange`
/// (`Bullet_Fire`, `FUN_005d0c20`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WeapClass {
    #[default]
    Rifle,
    Mg,
    Smg,
    Spread,
    Pistol,
    Grenade,
    RocketLauncher,
    Turret,
    NonPlayer,
    Gas,
    Item,
    Melee,
    KillstreakAltStoredWeapon,
    PistolSpread,
}

impl WeapClass {
    /// In BO2's enum order.
    pub const ALL: [WeapClass; 14] = [
        WeapClass::Rifle,
        WeapClass::Mg,
        WeapClass::Smg,
        WeapClass::Spread,
        WeapClass::Pistol,
        WeapClass::Grenade,
        WeapClass::RocketLauncher,
        WeapClass::Turret,
        WeapClass::NonPlayer,
        WeapClass::Gas,
        WeapClass::Item,
        WeapClass::Melee,
        WeapClass::KillstreakAltStoredWeapon,
        WeapClass::PistolSpread,
    ];

    /// BO2's enum value; out of range → `Rifle` (BO2's 0).
    pub fn from_bo2(v: u32) -> WeapClass {
        Self::ALL.get(v as usize).copied().unwrap_or_default()
    }
    /// BO2's names (`rocketlauncher`, `pistol_spread`).
    pub fn name(self) -> &'static str {
        match self {
            WeapClass::Rifle => "rifle",
            WeapClass::Mg => "mg",
            WeapClass::Smg => "smg",
            WeapClass::Spread => "spread",
            WeapClass::Pistol => "pistol",
            WeapClass::Grenade => "grenade",
            WeapClass::RocketLauncher => "rocketlauncher",
            WeapClass::Turret => "turret",
            WeapClass::NonPlayer => "non_player",
            WeapClass::Gas => "gas",
            WeapClass::Item => "item",
            WeapClass::Melee => "melee",
            WeapClass::KillstreakAltStoredWeapon => "killstreak_alt_stored_weapon",
            WeapClass::PistolSpread => "pistol_spread",
        }
    }
    pub fn parse(s: &str) -> Option<WeapClass> {
        Self::ALL.into_iter().find(|c| c.name() == s)
    }
    /// `spread` and `pistol_spread`: the classes `Bullet_Fire` fires `shotCount` pellets for.
    pub fn is_spread(self) -> bool {
        matches!(self, WeapClass::Spread | WeapClass::PistolSpread)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireMode {
    Auto,
    Semi,
    Burst(u8),
    /// Full auto after a spin-up while the trigger is held.
    Minigun,
    /// BO2's `stacked` (the tri-bolt): a pull fires the whole loaded stack (`FUN_008e1320`
    /// keeps firing while `FUN_008dc0c0` says the stack is part-spent, the fire delay zeroed).
    Stacked,
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
    /// BO2's `weapClass_t` (the def's own, data).
    pub weap_class: WeapClass,
    /// The third-person body's animation set (`zhbo_native::thirdperson::state`): the converter's
    /// group of the weapons BO2's `mp/playeranim.script` animates alike, by `weapClass` and
    /// `playerAnimType` (0, the rifleman's, for every weapon the script gives no clip of its own).
    /// A combination and an alt mode keep their own def's.
    pub body_anims: u8,
    pub fire: FireMode,
    pub fire_time: f32,
    /// Extra wait after a burst before the next one.
    pub burst_delay: f32,
    pub spin_up_time: f32,
    pub pellets: u16,
    pub clip: u16,
    /// Spare rounds at spawn, beside the loaded magazine (BO2's `iStartAmmo` minus a magazine).
    pub start_reserve: u16,
    /// Spare rounds at most, beside the magazine: BO2's stock cap `iMaxAmmo` (`FUN_005e08d0`).
    /// Scavenger and refills top the reserve up to it; the HUD shows the reserve.
    pub max_reserve: u16,
    pub reload_time: f32,
    pub reload_empty_time: f32,
    /// When the rounds go in (a reload cancelled after this keeps them).
    pub reload_add_time: f32,
    /// BO2's `iReloadEmptyAddTime`, raw: when an empty reload's rounds go in; 0 means at
    /// `reload_add_time` (`FUN_008dbc00`/`FUN_008dec90`). No attachment scales it.
    pub reload_empty_add_time: f32,
    /// Segmented reloads (tube shotguns, `Traits::segmented_reload`): start, rounds per
    /// `reload_time` cycle, end.
    pub reload_start_time: f32,
    pub reload_end_time: f32,
    pub reload_ammo_add: u16,
    pub raise_time: f32,
    pub first_raise_time: f32,
    pub drop_time: f32,
    /// `iEmptyRaiseTime`, `iEmptyDropTime`: a swap raises / puts away a weapon with an empty clip
    /// over these instead (`FUN_008df320` — the empty raise beats the first raise — and
    /// `FUN_008db500`).
    pub empty_raise_time: f32,
    pub empty_drop_time: f32,
    pub quick_raise_time: f32,
    pub quick_drop_time: f32,
    /// An alt toggle's raise into this mode (BO2's `iAltRaiseTime` of the variant, `FUN_008df320`;
    /// no attachment scales it).
    pub alt_raise_time: f32,
    /// An alt toggle's drop out of this weapon (`FUN_008ed1e0`: the first of the combination's
    /// attachment uniques whose `iAltDropTime` differs from the base unique's, else the def's own;
    /// an alt mode always takes its own).
    pub alt_drop_time: f32,
    pub sprint_out_time: f32,
    pub ads_in_time: f32,
    pub ads_out_time: f32,
    /// Horizontal field of view fully aimed (degrees at 4:3), 0 if the weapon has no zoom.
    pub ads_fov: f32,
    /// BO2's damage curve: damage at each range.
    pub damage: [f32; 6],
    /// The curve's ranges, BO2's inches (× the attachments' range scale). Kept in BO2's unit,
    /// converted where a ZH distance is compared ([`WeaponDef::damage_at`]): its "no falloff"
    /// sentinel, 999999.1 in, does not survive a trip through ZH units in an `f32`.
    pub damage_range: [f32; 6],
    /// BO2's `locationDamageMultipliers`, one per `hitLocation_t` ([`crate::hitloc`]): a bullet's
    /// damage is multiplied by its location's (t6mp `FUN_005f05c0` via `FUN_005f0490` ->
    /// `FUN_008ed780`). A combination's are resolved by [`crate::attachments::apply`].
    pub location_multipliers: [f32; crate::hitloc::COUNT],
    /// Base weapons: the location multipliers an attachment's unique sets differently from the base
    /// unique's (`short`, `[(location, multiplier)]`, in `eAttachment` order). `FUN_008ed780` takes,
    /// per location, the first mounted attachment's that differs, else the def's.
    pub attachment_location_multipliers: Vec<(String, Vec<(u8, f32)>)>,
    /// BO2's `maxRange`: a turret's AI engagement range (read only by the turret spawn,
    /// `FUN_0067ee90`), 0 on every MP weapon. Not the bullet's reach: see
    /// [`WeaponDef::bullet_range`].
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
    /// The lunge: how far away an enemy may be for the melee to charge at him (`meleeChargeRange`,
    /// Zero Hour units; 0 for no lunge), and the charge melee's time and delay
    /// (`iMeleeChargeTime`/`iMeleeChargeDelay`, seconds), which replace `melee_time`/`melee_delay`.
    pub melee_charge_range: f32,
    pub melee_charge_time: f32,
    pub melee_charge_delay: f32,
    /// Grenades and launchers.
    pub effect: Effect,
    pub fuse_time: f32,
    /// Hold to cook (the fuse runs from the end of the pull-back, `FUN_008e28a0`).
    pub cook: bool,
    /// BO2's offhand state machine (Ghidra `FUN_008e2710`..`FUN_008dcbe0`, MODLOG "Offhands run
    /// BO2's offhand states"): the pull-back (`timing.hold_fire_time`, `iHoldFireTime`, state 0x17),
    /// the wait from the release to the item leaving the hand (`timing.fire_delay`, `iFireDelay`,
    /// inside the `fire_time` of state 0x18), the detonate state (`timing.detonate_time`,
    /// `iDetonateTime`, its charge going off at `iDetonateDelay`, 0x19) a `hasDetonator` item
    /// enters when the button is pressed with none left in hand, and `isHoldUseGrenade`
    /// (`hold_use`: held up while the button is, then put away in its quick drop, never thrown).
    /// The three times are the uncarried-fields `CycleTimes` (one copy of each BO2 field).
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
    /// Sticks where it strikes and waits for its fuse or lands as equipment (BO2's `stickiness`
    /// `ALL`/`ALL_ORIENT`); an impact weapon's hit is its damage, so it never waits.
    pub sticky: bool,
    /// The grenade weapon this projectile becomes where it strikes, stuck to what it hit (BO2's
    /// `projExplosion` `bolt` with `szSpawnedGrenadeWeaponName`, t6mp `FUN_00608c10`): the
    /// crossbow's bolt deals its own `damage[0]` on the hit and leaves `explosive_bolt_mp`, which
    /// goes off on that def's fuse. Empty for everything else.
    pub spawns: String,
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
    /// A dual-wield weapon's left gun's shot (its own def's `fireSoundPlayer`, `wpn_<x>_fire_plr_lh`),
    /// as a converted event; empty for a single gun (and the left gun then plays `fire_sound`).
    pub left_fire_sound: String,
    /// What everyone else hears when an offhand is thrown or set down: its third-person
    /// `fireSound` (`wpn_grenade_throw`, `fly_claymore_plant_npc`), as a converted event; empty
    /// for a gun (others hear a gun's shot through its FX weapon).
    pub npc_fire_sound: String,
    /// The suppressed shot's player event (`AttachmentUnique.fire_sound_player`).
    pub silenced_fire_sound: String,
    /// The suppressed shot's third-person event (`AttachmentUnique.fire_sound`).
    pub silenced_fire_sound_npc: String,
    /// The FX weapon whose INI block carries the suppressed shot and flash.
    pub silenced_fx_weapon: String,
    /// The shared suppressed first-person FXList (`weapon/muzzleflashes/fx_muz_silencer_md`).
    pub silenced_view_flash_fx: String,
    /// The raise's BO2 sound event (`raiseSoundPlayer`), played as a raise begins.
    pub raise_sound: String,
    /// The first raise's event (`firstRaiseSoundPlayer`).
    pub first_raise_sound: String,
    /// The alt toggle's event (`altSwitchSoundPlayer`), played by the mode coming up as the toggle
    /// begins (BO2's EV_WEAPON_ALT; the alt raise itself is silent).
    pub alt_switch_sound: String,
    /// The putaway's event (`putawaySoundPlayer`), played as the drop begins.
    pub putaway_sound: String,
    /// The zoom-in event as aiming starts (`adsZoomSound`; scoped weapons).
    pub ads_zoom_sound: String,
    /// The melee swing with this weapon in hand (`meleeSwipeSoundPlayer`).
    pub melee_swipe_sound: String,
    /// The melee landing on someone (`meleeHitSound`).
    pub melee_hit_sound: String,
    /// The fire string's opening (`fireStartSoundPlayer`: a burst's recording).
    pub fire_start_sound: String,
    /// The last round's shot in place of the shot (`fireLastSoundPlayer`).
    pub fire_last_sound: String,
    /// The offhand's pull-back (`pullbackSoundPlayer`).
    pub pullback_sound: String,
    /// The fire string's opening with the silencer (its unique's override, `FUN_008ebe80`).
    pub silenced_fire_start_sound: String,
    /// Base weapons: the player's shot another attachment's unique overrides (`short=event`, in
    /// `eAttachment` order), when the base unique's `soundOverrides` lets it (`FUN_008ebe80`): the
    /// SIG556/XM8 select fire's full shot in place of the burst gun's low-end-only one. The
    /// silencer's override is [`Self::silenced_fire_sound`], which wins (G14 §4).
    pub attachment_fire_sounds: Vec<(String, String)>,
    /// Base weapons: the fire string's opening another attachment overrides (`short=event`, same
    /// rule; Zero Hour's `NONE` when overridden to nothing): the select fire's `null_sound`.
    pub attachment_fire_start_sounds: Vec<(String, String)>,
    pub explode_sound: String,
    /// A bounce's sound per surface (`surface=event`, the mod's slot names): BO2's `bounceSound`
    /// array cell for the surface struck (t6mp `FUN_004c1590` event 0x47); `NONE` or absent where
    /// the install has no such alias (silent, as BO2's hash miss).
    pub bounce_sounds: Vec<(String, String)>,
    /// A bounce off each surface (the mod's slot names): the share of the speed kept along it and
    /// into it, BO2's `parallelBounce[surface]` / `perpendicularBounce[surface]`. A surface not
    /// listed bounces by the tuning pack's `ProjectileBounceAlong`/`ProjectileBounce`.
    pub bounce: Vec<(String, f32, f32)>,
    /// `iProjectileActivateDist` (ZH units): a projectile that strikes something before flying this
    /// far is a dud — no blast (t6mp `FUN_00604280`, the path length `G_RunMissile` accumulates).
    /// 0 = always armed.
    pub activate_dist: f32,
    /// A dud's own effect and sound (`projDudEffect`, `projDudSound`), on top of the impact table's
    /// `projectile_dud` cell and the large bullet's hit sound (event 0x51).
    pub dud_fx: String,
    pub dud_sound: String,
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
    /// on this weapon (`silencer` is `silencer_smg` on an SMG), in the attachment grid's order (the
    /// statstable item's column 8, set by the converter).
    pub allowed: Vec<(String, String)>,
    /// BO2's create-a-class bars for this weapon, 0..100, in [`crate::classui`]'s order: fire
    /// rate, damage, range, accuracy (`mp/attributestable.csv` columns 6, 3, 4, 2, read by
    /// `weaponattributelist.lua`; MP labels its "mobility" bar FIRE RATE).
    pub attributes: [f32; 4],
    /// Base weapons: each attachment's deltas to those bars on this weapon (the table's
    /// `weapon_attachment` rows, keyed by attachment and weapon), same order.
    pub attachment_attributes: Vec<(String, [f32; 4])>,
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
    /// The Create-a-Class item's localized name (`mp/statstable.csv` column 3, `GetItemName`,
    /// `cacweaponsp.lua:893`): what the class menus call it. BO2's HUD shows the variant's own
    /// `displayName` (`display_name`) instead — the patched R870's is "Remington 870 MCS", its
    /// item "R870 MCS". Empty when it is not an item; [`WeaponDef::menu_name`] then falls back.
    pub cac_name: String,
    /// Listed locked in Create-a-Class: visible, never selectable, and dropped from a stored class
    /// (`classrepair`). The converter locks an item whose view model's art did not convert — BO2
    /// shows a DLC item the install lacks the pack for this way (`IsItemLocked`,
    /// `cacweaponsp.lua:928-945`, `c-primary/007`: the Peacekeeper without Revolution).
    pub cac_locked: bool,
    /// The locked item's hint (`GetUnlockStringForItemIndex`, `codbase.lua:3741-3770`: the
    /// localized `MENU_<statstable column 15>_REQUIRED_HINT`, "Revolution DLC Pack required.").
    pub cac_lock_hint: String,
    /// The Create-a-Class item's localized description (`mp/statstable.csv` column 7,
    /// `GetItemDesc`), the hint the weapon picker shows under the card (`cacweaponsp.lua:540-590`);
    /// empty when it is not an item. A combination inherits its base's.
    pub description: String,
    /// The item's place in its Create-a-Class list: the statstable's sort key (column 9) and item
    /// index (column 0). BO2 sorts a list by the sort key, then the index (G17, `FUN_008cdde0`);
    /// both 0 for a non-item, which keeps the table's order.
    pub cac_sort: i32,
    pub cac_index: i32,
    /// The left gun of a dual-wield weapon (BO2's `szDualWieldWeaponName`, `fnp45_lh_mp`); empty
    /// for a single gun.
    pub dual_wield_weapon: String,
    pub silenced: bool,
    pub laser: bool,
    /// Target finder: enemies are marked while aiming.
    pub target_finder: bool,
    /// BO2's `mmsMaxDist` (inches): how far the millimetre scanner's screen shows characters
    /// behind walls (`sim::optics`); whether the weapon is a scanner is `traits.mms`.
    pub mms_max_dist: f32,
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
    /// The variant's `f{Ads,Hip}ViewKickCenterSpeed`: the view kick's recentre acceleration
    /// (deg/s², `view_kick::centre_speed`), unscaled.
    pub view_kick_center: (f32, f32),
    /// Π `fAdsViewKickCenterSpeedScale` over the attachments (`FUN_008ecf70`; 1 bare), multiplied
    /// in when the attachments are applied. Only the aimed recentre takes it.
    pub ads_kick_center_scale: f32,
    /// `fAdsViewKickCenter{Ducked,Prone}Scale`: the aimed recentre speed's crouched and prone
    /// scales (`FUN_0051f150`).
    pub kick_center_stance: (f32, f32),
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
    /// The overlay's width and height in BO2's 640×480 virtual screen (`overlayWidth`,
    /// `overlayHeight`), placed by [`crate::weapons::scope::overlay_rect`].
    pub overlay_size: (f32, f32),
    /// The fire cycle's and reload's extra times (`handling::CycleTimes`).
    pub timing: CycleTimes,
    /// The zoom levels and fractions, depth of field and idle sway (`handling::Zoom`).
    pub zoom: Zoom,
    /// BO2's per-weapon flags, stack-fire spread, damage floor, penetration, offhand class.
    pub traits: Traits,
    /// `guidedMissileType_t`: a TV-guided weapon (6) keeps the HUD's scoped-hidden widgets under
    /// its overlay ([`crate::weapons::scope::scoped`]).
    pub guided_missile_type: u32,
    /// BO2's `WeaponVariantDef.dpadIcon`: the action-slot material (`hud_gl_select` on a grenade
    /// launcher's alt mode, `hud_select_fire_bullet_stack` on select fire), empty when none. The HUD's
    /// alt-fire slot draws it (`hud::ammo::alt_slot_icon`).
    pub dpad_icon: String,
    /// The third-person shell eject (BO2's `worldShellEjectEffect` at the world model's
    /// `tag_brass`, `FUN_00531630`): the Zero Hour FX weapon whose `FireFX` plays it, fired once
    /// per shot at the shooter. Empty: the gun ejects nothing others see.
    pub world_shell_fx_weapon: String,
    /// The same for the round that empties the clip (`worldLastShotEjectEffect`).
    pub world_last_shot_shell_fx_weapon: String,
    /// The FX weapon drawing the gun's bullet tracer (`tracerType`, the first single unique's
    /// before the def's, `FUN_008eb420`); empty: no tracer.
    pub tracer_fx_weapon: String,
    /// The same for a viewer the shooter is not friendly to (`enemyTracerType`, `FUN_008eb420`'s
    /// enemy walk; `FUN_004bbf10` decides per viewer). Empty: the friendly one.
    pub enemy_tracer_fx_weapon: String,
    /// Every how-many bullets a tracer is drawn (`TracerDef.drawInterval`; `FUN_007c65c0`).
    pub tracer_interval: u32,
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
/// `CamoGridButton` draws, the table's sort key, and which personalize tab offers it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CamoOption {
    pub id: String,
    pub name: String,
    pub image: String,
    pub sort: u32,
    /// The personalize tab the option sits on ([`OptionTab::of`]).
    pub tab: OptionTab,
    /// A store (MTX) option: column 16 of its row names an `mtx_weapon*` item. Its class-preview
    /// swatch is the whole `<image>_32` (`classpreview.lua`'s `SetupMTXCamoSwatch`).
    pub mtx: bool,
}

/// BO2's personalize tabs for a weapon option (`caccamomenu.lua`'s `AddCamos`,
/// `cacreticles.lua`'s twin): the base tab holds the challenge-unlocked rows; DLC the
/// `digital_unlock` rows (`mp/statsmilestones*.csv` column 4) and the store rows without a DLC2 sort
/// key; DLC2 the store rows with one (column 5 > 0), in that key's order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OptionTab {
    #[default]
    Base,
    Dlc,
    Dlc2,
}

impl OptionTab {
    /// The tab `AddCamos` puts a row on: `mtx` from column 16, `dlc2_sort` column 5, and whether a
    /// `digital_unlock` milestone names the option.
    pub fn of(mtx: bool, dlc2_sort: u32, digital_unlock: bool) -> OptionTab {
        match (mtx, dlc2_sort > 0, digital_unlock) {
            (true, true, _) => OptionTab::Dlc2,
            (true, false, _) | (false, _, true) => OptionTab::Dlc,
            (false, _, false) => OptionTab::Base,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            OptionTab::Base => "base",
            OptionTab::Dlc => "dlc",
            OptionTab::Dlc2 => "dlc2",
        }
    }

    pub fn parse(s: &str) -> Option<OptionTab> {
        [OptionTab::Base, OptionTab::Dlc, OptionTab::Dlc2].into_iter().find(|t| t.name() == s)
    }
}

/// One BO2 bonus card (a Create-a-Class wildcard): a `bonuscard` row of `mp/statstable.csv`. The
/// reference (column 4, `bonuscard_overkill`) is the card's stable identity — what the editor's
/// rules key on ([`crate::classmenu::Wildcard::reference`]); the rest is what the grid shows and
/// charges: the localized name (column 3) and description (column 7), the card image (column 6),
/// the loadout slot `Engine.GetLoadoutSlotForItem` answers (column 13, `bonuscard1..7`), the
/// allocation cost (column 12), the sort key (column 9), the item index (column 0) and the unlock
/// level (column 10). The grid lists the group's cards by sort key, then index (G17).
/// One of BO2's localized UI strings the core draws, from the player's own english zones
/// (`UiText <key>` blocks): the converter writes each of [`UI_TEXT_KEYS`] the install has.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiText {
    /// The localize key (`MPUI_SECOND_SPECIAL_GRENADE_CAPS`).
    pub key: String,
    /// Its english text (`SECOND TACTICAL`).
    pub text: String,
}

/// The localize keys the core draws from the install: the class editor's offhand slot titles
/// (`cacgrenadesbutton.lua:104-126`, `cacgrenades.lua:85-104`, `classpreview.lua:3640-3660`:
/// `MPUI_<slot>_CAPS`, and the lethal slot's under Tactician) and the HUD's weapon hints
/// (`WEAPON_CANT_PLANT_EQUIPMENT`, `sim::hint_text`). The reticle grid's per-optic cell names
/// (`MPUI_RETICLE_<OPTIC>_<N>`, [`super::reticle_name_key`]) are written beside these, every one
/// the install has.
pub const UI_TEXT_KEYS: &[&str] = &[
    "MPUI_PRIMARY_GRENADE_CAPS", "MPUI_SPECIAL_GRENADE_CAPS", "MPUI_SECOND_SPECIAL_GRENADE_CAPS",
    "WEAPON_CANT_PLANT_EQUIPMENT", "MPUI_PRIMARY_CAPS", "MPUI_SECONDARY_CAPS",
    "MPUI_ATTACHMENTS_CAPS", "MENU_WILDCARDS_CAPS", "MPUI_ATTACHMENTS_NOT_AVAILABLE",
    "MPUI_CHOOSE_CLASS_CAPS", "MPUI_FIRE_RATE_CAPS", "MPUI_DAMAGE_CAPS", "MPUI_RANGE_CAPS",
    "MPUI_ACCURACY_CAPS", "MENU_CLASS_OPTIONS", "MENU_CLASS_SET_OPTIONS", "MENU_RENAME",
    "MENU_COPY_CAPS", "MENU_RESET_TO_DEFAULT", "MENU_CLEAR_LOADOUT_CAPS", "MENU_BACK",
    "MENU_SELECT", "MPUI_PERSONALIZE_KNIFE", "MPUI_WEAPON_SMG_CAPS", "MPUI_WEAPON_ASSAULT_CAPS",
    "MPUI_WEAPON_CQB_CAPS", "MPUI_WEAPON_LMG_CAPS", "MPUI_WEAPON_SNIPER_CAPS",
    "MPUI_WEAPON_SPECIAL_CAPS", "MPUI_WEAPON_LAUNCHER_CAPS", "MPUI_WEAPON_PISTOL_CAPS",
    "MPUI_CAMO_CAPS", "MPUI_RETICLES_CAPS", "MPUI_RETICLE_COLOR_CAPS", "MPUI_MAP_CATEGORY_DLC_CAPS",
    "MPUI_DLC2_CAPS", "MENU_CUSTOMIZED_CAPS", "MENU_SCOREBOARD_CAPS", "MPUI_SCORE_CAPS",
    "MPUI_KILLS_CAPS", "MPUI_DEATHS_CAPS", "MPUI_ASSISTS_CAPS", "WEAPON_NO_AMMO",
    "MENU_SELECT_RETICLE", "MENU_X_OPTICS",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BonusCard {
    pub reference: String,
    pub name: String,
    pub description: String,
    pub image: String,
    pub slot: String,
    pub cost: i32,
    pub sort: i32,
    pub index: i32,
    pub unlock_level: i32,
}

/// One BO2 perk: a `specialty` row of `mp/statstable.csv`. Column 3 is both the reference and the
/// name key (`PERKS_LIGHTWEIGHT`); without `PERKS_` (`LIGHTWEIGHT`, Dead Silence's `NINJA`) it is
/// the row's identity ([`super::perks::PerkDef::stats`]). The localized name (column 3) and
/// description (column 7, `PERKS_*_DESC`) are what the perk grid's lower panel shows
/// (`cacperksp.lua`'s `perkTitle` / `perkDescription`); the specialties (column 4, `|`-separated)
/// are what the perk does (Ghidra G1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PerkRow {
    pub stats: String,
    pub name: String,
    pub description: String,
    pub specialties: super::specialty::Specialties,
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
            weap_class: WeapClass::Rifle,
            body_anims: 0,
            fire: FireMode::Auto,
            fire_time: 0.1,
            burst_delay: 0.0,
            spin_up_time: 0.0,
            pellets: 1,
            clip: 30,
            start_reserve: 90,
            max_reserve: 240,
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
            empty_raise_time: 0.5,
            empty_drop_time: 0.4,
            quick_raise_time: 0.3,
            quick_drop_time: 0.25,
            alt_raise_time: 0.0,
            alt_drop_time: 0.0,
            sprint_out_time: 0.2,
            ads_in_time: 0.25,
            ads_out_time: 0.2,
            ads_fov: 55.0,
            damage: [40.0, 30.0, 0.0, 0.0, 0.0, 0.0],
            damage_range: [1000.0, 2000.0, 0.0, 0.0, 0.0, 0.0],
            location_multipliers: [1.0; crate::hitloc::COUNT],
            attachment_location_multipliers: Vec::new(),
            max_range: 0.0,
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
            melee_charge_range: 0.0,
            melee_charge_time: 0.0,
            melee_charge_delay: 0.0,
            effect: Effect::None,
            equipment_kind: Kind::Other,
            fuse_time: 0.0,
            cook: false,
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
            spawns: String::new(),
            fx_weapon: String::new(),
            explosion_fx: String::new(),
            view_flash_fx: String::new(),
            view_shell_fx: String::new(),
            view_last_shot_shell_fx: String::new(),
            projectile_object: String::new(),
            fire_sound: String::new(),
            empty_fire_sound: String::new(),
            left_fire_sound: String::new(),
            npc_fire_sound: String::new(),
            silenced_fire_sound: String::new(),
            silenced_fire_sound_npc: String::new(),
            silenced_fx_weapon: String::new(),
            silenced_view_flash_fx: String::new(),
            raise_sound: String::new(),
            first_raise_sound: String::new(),
            alt_switch_sound: String::new(),
            putaway_sound: String::new(),
            ads_zoom_sound: String::new(),
            melee_swipe_sound: String::new(),
            melee_hit_sound: String::new(),
            fire_start_sound: String::new(),
            fire_last_sound: String::new(),
            pullback_sound: String::new(),
            silenced_fire_start_sound: String::new(),
            attachment_fire_sounds: Vec::new(),
            attachment_fire_start_sounds: Vec::new(),
            explode_sound: String::new(),
            bounce_sounds: Vec::new(),
            bounce: Vec::new(),
            activate_dist: 0.0,
            dud_fx: String::new(),
            dud_sound: String::new(),
            viewmodel: -1,
            anims: 0,
            clip_base: 0,
            world_meshes: 0,
            attachments: Vec::new(),
            allowed: Vec::new(),
            attributes: [0.0; 4],
            attachment_attributes: Vec::new(),
            alt_modes: Vec::new(),
            alt: String::new(),
            alt_own_ammo: false,
            cac_slot: String::new(),
            cac_group: String::new(),
            cac_name: String::new(),
            cac_locked: false,
            cac_lock_hint: String::new(),
            description: String::new(),
            cac_sort: 0,
            cac_index: 0,
            dual_wield_weapon: String::new(),
            silenced: false,
            laser: false,
            target_finder: false,
            mms_max_dist: 0.0,
            recoil_reduction: (0.0, 0.0),
            place: Place::Throw,
            specialties: Specialties::NONE,
            view_kick_min: (0.0, 0.0),
            view_kick_center: (0.0, 0.0),
            ads_kick_center_scale: 1.0,
            kick_center_stance: (1.0, 1.0),
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
            overlay_size: (0.0, 0.0),
            // The stand-in sprint-in, as `sprint_out_time` above (BO2 rifles author 200-300 ms).
            timing: CycleTimes { sprint_in_time: 0.25, ..CycleTimes::default() },
            zoom: Zoom::default(),
            traits: Traits::default(),
            guided_missile_type: 0,
            dpad_icon: String::new(),
            world_shell_fx_weapon: String::new(),
            world_last_shot_shell_fx_weapon: String::new(),
            tracer_fx_weapon: String::new(),
            enemy_tracer_fx_weapon: String::new(),
            tracer_interval: 0,
            shield: None,
        }
    }
}

pub const MELEE_DAMAGE: f32 = 135.0;
/// The melee's reach past the victim's hit radius: BO2's `player_meleeRange` 55 in (the tuning
/// pack's `player_meleeRange`, held equal by a test; a constant because the converter bakes it into
/// the RTS knife's `AttackRange`). Past it the knife lunges ([`WeaponDef::melee_charge_range`]).
pub const MELEE_RANGE: f32 = inches(55.0);
pub const MELEE_FX_WEAPON: &str = "ZHBO_Knife_FX";

impl WeaponDef {
    /// The name the class menus show: the Create-a-Class item's (`cac_name`, statstable column 3),
    /// else the HUD's `display_name` (a weapon no item names).
    pub fn menu_name(&self) -> &str {
        if self.cac_name.is_empty() { &self.display_name } else { &self.cac_name }
    }

    /// Whether a pull of this gun launches a projectile ([`super::WeaponAction::Launch`]) rather
    /// than firing bullets: the launchers, and a gun whose shot is a projectile with an effect (the
    /// crossbow, the ballistic knife).
    pub fn launches(&self) -> bool {
        self.class == Class::Launcher || (self.projectile_speed > 0.0 && self.class != Class::Shotgun && self.effect != Effect::None)
    }

    /// Rounds one full pull puts out: a stacked weapon's whole magazine; a spread class's `shotCount`
    /// pellets (`Bullet_Fire`); one for everything else (BO2's `shotCount` is 0 there).
    pub fn rounds_per_shot(&self) -> u16 {
        if self.fire == FireMode::Stacked {
            self.clip.max(1)
        } else if self.weap_class.is_spread() {
            self.pellets.max(1)
        } else {
            1
        }
    }

    /// How far a round is traced (ZH units), BO2's `Bullet_Fire` rule (`FUN_005d0c20`): a spread
    /// class's pellets fly to the damage curve's last range (`damageRange[5]` × the attachments'
    /// range scale, already in `damage_range`); every other weapon's round flies `bulletrange`
    /// (the `bulletrange` dvar in ZH units, passed in from the tuning pack).
    pub fn bullet_range(&self, bulletrange: f32) -> f32 {
        if self.weap_class.is_spread() {
            inches(self.damage_range[5])
        } else {
            bulletrange
        }
    }

    /// An Assault Shield: a primary that blocks, deploys and melees with itself.
    pub fn is_shield(&self) -> bool {
        self.class == Class::Shield
    }

    /// The category Create-a-Class lists this weapon under: the gun class whose BO2 group is its
    /// statstable group ([`WeaponDef::cac_group`]), else its own class (not a CaC item, or a
    /// grenade-group item, which keeps its lethal/tactical/equipment class).
    pub fn cac_class(&self) -> Class {
        Class::ALL_GUNS.into_iter().find(|c| !self.cac_group.is_empty() && c.cac_group() == self.cac_group).unwrap_or(self.class)
    }

    /// Take the converter's sound and FX assets from `from` (the same combination derived from its
    /// fully converted base): the events and FXLists the converter fills on the base defs after the
    /// table's combinations were built. One list, so a new converted asset field is inherited by
    /// every combination the moment it is added here.
    pub fn take_converted_assets(&mut self, from: &WeaponDef) {
        self.fx_weapon.clone_from(&from.fx_weapon);
        self.explosion_fx.clone_from(&from.explosion_fx);
        self.view_flash_fx.clone_from(&from.view_flash_fx);
        self.view_shell_fx.clone_from(&from.view_shell_fx);
        self.view_last_shot_shell_fx.clone_from(&from.view_last_shot_shell_fx);
        self.fire_sound.clone_from(&from.fire_sound);
        self.empty_fire_sound.clone_from(&from.empty_fire_sound);
        self.npc_fire_sound.clone_from(&from.npc_fire_sound);
        self.silenced_fire_sound.clone_from(&from.silenced_fire_sound);
        self.silenced_fire_sound_npc.clone_from(&from.silenced_fire_sound_npc);
        self.silenced_fx_weapon.clone_from(&from.silenced_fx_weapon);
        self.silenced_view_flash_fx.clone_from(&from.silenced_view_flash_fx);
        self.explode_sound.clone_from(&from.explode_sound);
        self.raise_sound.clone_from(&from.raise_sound);
        self.first_raise_sound.clone_from(&from.first_raise_sound);
        self.alt_switch_sound.clone_from(&from.alt_switch_sound);
        self.putaway_sound.clone_from(&from.putaway_sound);
        self.ads_zoom_sound.clone_from(&from.ads_zoom_sound);
        self.melee_swipe_sound.clone_from(&from.melee_swipe_sound);
        self.melee_hit_sound.clone_from(&from.melee_hit_sound);
        self.fire_start_sound.clone_from(&from.fire_start_sound);
        self.fire_last_sound.clone_from(&from.fire_last_sound);
        self.pullback_sound.clone_from(&from.pullback_sound);
        self.silenced_fire_start_sound.clone_from(&from.silenced_fire_start_sound);
        self.attachment_fire_sounds.clone_from(&from.attachment_fire_sounds);
        self.attachment_fire_start_sounds.clone_from(&from.attachment_fire_start_sounds);
        self.bounce_sounds.clone_from(&from.bounce_sounds);
        self.bounce.clone_from(&from.bounce);
        self.dud_fx.clone_from(&from.dud_fx);
        self.dud_sound.clone_from(&from.dud_sound);
    }

    /// The bounce off `surface` ([`Self::bounce`]): (along, into) kept, if the def has it.
    pub fn bounce_off(&self, surface: crate::impacts::Surface) -> Option<(f32, f32)> {
        self.bounce.iter().find(|(s, ..)| s == surface.name()).map(|&(_, a, p)| (a, p))
    }

    /// The bounce sound for `surface` ([`Self::bounce_sounds`]), empty when there is none.
    pub fn bounce_sound(&self, surface: crate::impacts::Surface) -> &str {
        self.bounce_sounds.iter().find(|(s, _)| s == surface.name()).map_or("", |(_, e)| e.as_str())
    }

    /// Every def-level sound event this def plays (`names::def_sound_event`), with whether it is
    /// positional (the blast) rather than the player's own; empty ones included.
    pub fn def_sound_events(&self) -> impl Iterator<Item = (&str, bool)> + '_ {
        [
            (&self.fire_sound, false),
            (&self.raise_sound, false),
            (&self.first_raise_sound, false),
            (&self.alt_switch_sound, false),
            (&self.putaway_sound, false),
            (&self.ads_zoom_sound, false),
            (&self.melee_swipe_sound, false),
            (&self.melee_hit_sound, false),
            (&self.fire_start_sound, false),
            (&self.fire_last_sound, false),
            (&self.pullback_sound, false),
            (&self.silenced_fire_start_sound, false),
            (&self.explode_sound, true),
            (&self.dud_sound, true),
            (&self.npc_fire_sound, true),
        ]
        .into_iter()
        .map(|(e, world)| (e.as_str(), world))
        .chain(self.attachment_fire_start_sounds.iter().map(|(_, e)| (e.as_str(), false)))
        .chain(self.bounce_sounds.iter().map(|(_, e)| (e.as_str(), true)))
    }

    /// Damage at `distance` (ZH units). BO2's curve has six slots: unused middle slots have no
    /// range, and the last is the minimum damage at the end of the curve. Damage interpolates
    /// linearly between the used points and stays at the last one beyond it.
    pub fn damage_at(&self, distance: f32) -> f32 {
        self.damage_at_inches(distance / inches(1.0))
    }

    /// [`WeaponDef::damage_at`] for a distance in BO2's inches (the curve's own unit).
    pub fn damage_at_inches(&self, distance: f32) -> f32 {
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

    /// Whether a reload from `ammo` rounds is BO2's empty reload: a bullet weapon with an empty
    /// clip (`FUN_008dec90`); any other weapon plays its plain reload.
    pub fn empty_reload(&self, ammo: u16) -> bool {
        ammo == 0 && self.traits.weap_type == WeapType::Bullet
    }

    /// BO2's segmented reload (`bSegmentedReload`): start, rounds per cycle, end.
    /// A dual-wield weapon: it names its left gun (BO2's `bDualWield` with `szDualWieldWeaponName`).
    /// It fires two guns, one per trigger, and has no sights (`aimDownSight` 0 on every BO2 `_dw`).
    pub fn is_dual_wield(&self) -> bool {
        !self.dual_wield_weapon.is_empty()
    }

    pub fn shell_by_shell(&self) -> bool {
        self.traits.segmented_reload
    }

    pub fn has_anim(&self, slot: usize) -> bool {
        slot < 128 && self.anims & (1u128 << slot) != 0
    }
}
