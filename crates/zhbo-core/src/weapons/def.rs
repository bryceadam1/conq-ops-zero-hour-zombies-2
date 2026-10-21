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
    Melee,
    Lethal,
    Tactical,
    Equipment,
}

impl Class {
    pub fn is_gun(self) -> bool {
        !matches!(self, Class::Melee | Class::Lethal | Class::Tactical | Class::Equipment)
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
            Class::Melee => "melee",
            Class::Lethal => "lethal",
            Class::Tactical => "tactical",
            Class::Equipment => "equipment",
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
    /// Grenades and launchers.
    pub effect: Effect,
    pub fuse_time: f32,
    /// Hold to cook (the fuse runs from the pin pull).
    pub cook: bool,
    pub explosion_radius: f32,
    pub explosion_radius_min: f32,
    pub explosion_inner_damage: f32,
    pub explosion_outer_damage: f32,
    pub projectile_speed: f32,
    pub projectile_speed_up: f32,
    pub projectile_gravity: bool,
    pub impact_explode: bool,
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
}

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
            effect: Effect::None,
            equipment_kind: Kind::Other,
            fuse_time: 0.0,
            cook: false,
            explosion_radius: 0.0,
            explosion_radius_min: 0.0,
            explosion_inner_damage: 0.0,
            explosion_outer_damage: 0.0,
            projectile_speed: 0.0,
            projectile_speed_up: 0.0,
            projectile_gravity: true,
            impact_explode: false,
            sticky: false,
            fx_weapon: String::new(),
            explosion_fx: String::new(),
            view_flash_fx: String::new(),
            view_shell_fx: String::new(),
            view_last_shot_shell_fx: String::new(),
            projectile_object: String::new(),
            fire_sound: String::new(),
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
        }
    }
}

/// BO2 view kick values are kick-physics inputs; this turns one into degrees of view per shot.
pub const KICK_TO_DEGREES: f32 = 1.0 / 80.0;

pub const MELEE_DAMAGE: f32 = 135.0;
pub const MELEE_RANGE: f32 = inches(64.0);
pub const MELEE_FX_WEAPON: &str = "ZHBO_Knife_FX";

impl WeaponDef {
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