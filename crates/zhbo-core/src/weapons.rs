//! Black Ops II weapons, as BO2 plays them: fire types (auto, semi, burst, minigun spin-up, pellets,
//! bolt/pump cycling), magazines with BO2's reload timing (the round count lands at the add time,
//! shell-by-shell reloads for tube shotguns), raise/drop/first-raise, sprint-out, ADS in/out,
//! stance-based hip spread with bloom, view kick, the damage-over-range curve, and offhand
//! grenades with cooking.
//!
//! The numbers come from the player's own BO2 install: the launcher converts every multiplayer
//! weapon definition (`zhbo_formats::weapon`) into a table (`Data\ConquerOps\weapons.ini`) that the
//! host hands to [`install_catalog`] before play starts. Without a BO2 install a small built-in
//! table with the same fields stands in ([`BUILTIN`]).
//!
//! Everything here runs inside Zero Hour's lockstep logic, so randomness comes from [`Rng`], which is
//! seeded identically on every client, and the table must be identical on every client.

use crate::input::buttons;
use crate::units::{inches, LOGIC_DT};
use std::collections::HashMap;
use std::sync::OnceLock;

// ------------------------------------------------------------------------------------------------
// Definitions
// ------------------------------------------------------------------------------------------------

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
    fn name(self) -> &'static str {
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
    fn parse(s: &str) -> Option<Self> {
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
    fn name(self) -> &'static str {
        match self {
            Effect::Explode => "explode",
            Effect::Stun => "stun",
            Effect::Emp => "emp",
            Effect::Smoke => "smoke",
            Effect::Impact => "impact",
            Effect::None => "none",
        }
    }
    fn parse(s: &str) -> Option<Self> {
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
    pub projectile_object: String,
    pub fire_sound: String,
    pub explode_sound: String,
    /// Viewmodel index in the converted art, or -1.
    pub viewmodel: i32,
    /// Which BO2 viewmodel animation slots (`weapAnimFiles_t`) the weapon has, as bits.
    pub anims: u128,
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
            projectile_object: String::new(),
            fire_sound: String::new(),
            explode_sound: String::new(),
            viewmodel: -1,
            anims: 0,
        }
    }
}

/// BO2 view kick values are kick-physics inputs; this turns one into degrees of view per shot.
pub const KICK_TO_DEGREES: f32 = 1.0 / 80.0;

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

// ------------------------------------------------------------------------------------------------
// The table: text format shared with the converter
// ------------------------------------------------------------------------------------------------

/// Write weapon definitions in the table format [`parse_table`] reads (Zero Hour INI style: one
/// `Weapon <id>` block per weapon, `key = value` lines, `End`).
pub fn format_table(defs: &[WeaponDef]) -> String {
    let mut s = String::from("; Black Ops II multiplayer weapons, converted from the player's own install by zhbo-assets.\n; Private data: do not redistribute.\n\n");
    for d in defs {
        let pair = |a: (f32, f32)| format!("{} {}", a.0, a.1);
        let list = |v: &[f32]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" ");
        let fire = match d.fire {
            FireMode::Auto => "auto".to_string(),
            FireMode::Semi => "semi".to_string(),
            FireMode::Burst(n) => format!("burst {n}"),
            FireMode::Minigun => "minigun".to_string(),
        };
        let lines: Vec<(&str, String)> = vec![
            ("DisplayName", d.display_name.clone()),
            ("Class", d.class.name().into()),
            ("Fire", fire),
            ("FireTime", d.fire_time.to_string()),
            ("BurstDelay", d.burst_delay.to_string()),
            ("SpinUpTime", d.spin_up_time.to_string()),
            ("Pellets", d.pellets.to_string()),
            ("Clip", d.clip.to_string()),
            ("StartReserve", d.start_reserve.to_string()),
            ("MaxReserve", d.max_reserve.to_string()),
            ("Reload", format!("{} {} {} {}", d.reload_time, d.reload_empty_time, d.reload_add_time, d.reload_empty_add_time)),
            ("ReloadShells", format!("{} {} {}", d.reload_start_time, d.reload_end_time, d.reload_ammo_add)),
            ("Raise", format!("{} {} {} {} {}", d.raise_time, d.first_raise_time, d.drop_time, d.quick_raise_time, d.quick_drop_time)),
            ("SprintOutTime", d.sprint_out_time.to_string()),
            ("Ads", format!("{} {} {}", d.ads_in_time, d.ads_out_time, d.ads_fov)),
            ("Damage", list(&d.damage)),
            ("DamageRange", list(&d.damage_range)),
            ("HeadMultiplier", d.head_multiplier.to_string()),
            ("MaxRange", d.max_range.to_string()),
            ("HipSpread", format!("{} {} {}", pair(d.hip_spread[0]), pair(d.hip_spread[1]), pair(d.hip_spread[2]))),
            ("Spread", format!("{} {} {} {}", d.spread_decay, d.spread_fire_add, d.spread_move_add, d.ads_spread)),
            ("AdsKick", format!("{} {}", pair(d.ads_kick.pitch), pair(d.ads_kick.yaw))),
            ("HipKick", format!("{} {}", pair(d.hip_kick.pitch), pair(d.hip_kick.yaw))),
            ("Move", format!("{} {} {}", d.move_scale, d.ads_move_scale, d.sprint_duration_scale)),
            ("Melee", format!("{} {}", d.melee_damage, d.melee_time)),
            ("Effect", d.effect.name().into()),
            ("Fuse", format!("{} {}", d.fuse_time, d.cook as u8)),
            ("Explosion", format!("{} {} {} {}", d.explosion_radius, d.explosion_radius_min, d.explosion_inner_damage, d.explosion_outer_damage)),
            (
                "Projectile",
                format!("{} {} {} {} {}", d.projectile_speed, d.projectile_speed_up, d.projectile_gravity as u8, d.impact_explode as u8, d.sticky as u8),
            ),
            ("FxWeapon", d.fx_weapon.clone()),
            ("ExplosionFx", d.explosion_fx.clone()),
            ("ProjectileObject", d.projectile_object.clone()),
            ("FireSound", d.fire_sound.clone()),
            ("ExplodeSound", d.explode_sound.clone()),
            ("Viewmodel", d.viewmodel.to_string()),
            ("Anims", format!("{:x}", d.anims)),
        ];
        s.push_str(&format!("Weapon {}\n", d.id));
        for (k, v) in lines {
            s.push_str(&format!("  {k} = {v}\n"));
        }
        s.push_str("End\n\n");
    }
    s
}

/// Read the weapon table. Unknown keys are ignored; a malformed value is an error.
pub fn parse_table(text: &str) -> Result<Vec<WeaponDef>, String> {
    let mut out = Vec::new();
    let mut cur: Option<WeaponDef> = None;
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let err = |what: &str| format!("weapons table line {}: {what}: {raw}", n + 1);
        if let Some(id) = line.strip_prefix("Weapon ") {
            cur = Some(WeaponDef { id: id.trim().to_string(), ..Default::default() });
            continue;
        }
        if line == "End" {
            out.push(cur.take().ok_or_else(|| err("End without Weapon"))?);
            continue;
        }
        let d = cur.as_mut().ok_or_else(|| err("value outside a Weapon block"))?;
        let (key, value) = line.split_once('=').ok_or_else(|| err("expected key = value"))?;
        let (key, value) = (key.trim(), value.trim());
        let nums: Vec<f32> = value.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        let need = |k: usize| if nums.len() >= k { Ok(()) } else { Err(err("too few numbers")) };
        match key {
            "DisplayName" => d.display_name = value.to_string(),
            "Class" => d.class = Class::parse(value).ok_or_else(|| err("unknown class"))?,
            "Fire" => {
                d.fire = match value.split_whitespace().next() {
                    Some("auto") => FireMode::Auto,
                    Some("semi") => FireMode::Semi,
                    Some("minigun") => FireMode::Minigun,
                    Some("burst") => FireMode::Burst(nums.first().copied().unwrap_or(3.0) as u8),
                    _ => return Err(err("unknown fire mode")),
                }
            }
            "FireTime" => d.fire_time = value.parse().map_err(|_| err("number"))?,
            "BurstDelay" => d.burst_delay = value.parse().map_err(|_| err("number"))?,
            "SpinUpTime" => d.spin_up_time = value.parse().map_err(|_| err("number"))?,
            "Pellets" => d.pellets = value.parse().map_err(|_| err("number"))?,
            "Clip" => d.clip = value.parse().map_err(|_| err("number"))?,
            "StartReserve" => d.start_reserve = value.parse().map_err(|_| err("number"))?,
            "MaxReserve" => d.max_reserve = value.parse().map_err(|_| err("number"))?,
            "Reload" => {
                need(4)?;
                (d.reload_time, d.reload_empty_time, d.reload_add_time, d.reload_empty_add_time) = (nums[0], nums[1], nums[2], nums[3]);
            }
            "ReloadShells" => {
                need(3)?;
                (d.reload_start_time, d.reload_end_time, d.reload_ammo_add) = (nums[0], nums[1], nums[2] as u16);
            }
            "Raise" => {
                need(5)?;
                (d.raise_time, d.first_raise_time, d.drop_time, d.quick_raise_time, d.quick_drop_time) = (nums[0], nums[1], nums[2], nums[3], nums[4]);
            }
            "SprintOutTime" => d.sprint_out_time = value.parse().map_err(|_| err("number"))?,
            "Ads" => {
                need(3)?;
                (d.ads_in_time, d.ads_out_time, d.ads_fov) = (nums[0], nums[1], nums[2]);
            }
            "Damage" => {
                need(6)?;
                d.damage.copy_from_slice(&nums[..6]);
            }
            "DamageRange" => {
                need(6)?;
                d.damage_range.copy_from_slice(&nums[..6]);
            }
            "HeadMultiplier" => d.head_multiplier = value.parse().map_err(|_| err("number"))?,
            "MaxRange" => d.max_range = value.parse().map_err(|_| err("number"))?,
            "HipSpread" => {
                need(6)?;
                d.hip_spread = [(nums[0], nums[1]), (nums[2], nums[3]), (nums[4], nums[5])];
            }
            "Spread" => {
                need(4)?;
                (d.spread_decay, d.spread_fire_add, d.spread_move_add, d.ads_spread) = (nums[0], nums[1], nums[2], nums[3]);
            }
            "AdsKick" | "HipKick" => {
                need(4)?;
                let k = Kick { pitch: (nums[0], nums[1]), yaw: (nums[2], nums[3]) };
                if key == "AdsKick" {
                    d.ads_kick = k;
                } else {
                    d.hip_kick = k;
                }
            }
            "Move" => {
                need(3)?;
                (d.move_scale, d.ads_move_scale, d.sprint_duration_scale) = (nums[0], nums[1], nums[2]);
            }
            "Melee" => {
                need(2)?;
                (d.melee_damage, d.melee_time) = (nums[0], nums[1]);
            }
            "Effect" => d.effect = Effect::parse(value).ok_or_else(|| err("unknown effect"))?,
            "Fuse" => {
                need(2)?;
                (d.fuse_time, d.cook) = (nums[0], nums[1] != 0.0);
            }
            "Explosion" => {
                need(4)?;
                (d.explosion_radius, d.explosion_radius_min, d.explosion_inner_damage, d.explosion_outer_damage) = (nums[0], nums[1], nums[2], nums[3]);
            }
            "Projectile" => {
                need(5)?;
                d.projectile_speed = nums[0];
                d.projectile_speed_up = nums[1];
                d.projectile_gravity = nums[2] != 0.0;
                d.impact_explode = nums[3] != 0.0;
                d.sticky = nums[4] != 0.0;
            }
            "FxWeapon" => d.fx_weapon = value.to_string(),
            "ExplosionFx" => d.explosion_fx = value.to_string(),
            "ProjectileObject" => d.projectile_object = value.to_string(),
            "FireSound" => d.fire_sound = value.to_string(),
            "ExplodeSound" => d.explode_sound = value.to_string(),
            "Viewmodel" => d.viewmodel = value.parse().map_err(|_| err("number"))?,
            "Anims" => d.anims = u128::from_str_radix(value, 16).map_err(|_| err("hex"))?,
            _ => {}
        }
    }
    if cur.is_some() {
        return Err("weapons table: last Weapon block has no End".into());
    }
    Ok(out)
}

// ------------------------------------------------------------------------------------------------
// The catalog
// ------------------------------------------------------------------------------------------------

/// All weapons by id.
pub struct Catalog {
    by_id: HashMap<String, &'static WeaponDef>,
    order: Vec<&'static WeaponDef>,
}

impl Catalog {
    pub fn new(defs: Vec<WeaponDef>) -> Catalog {
        let order: Vec<&'static WeaponDef> = defs.into_iter().map(|d| &*Box::leak(Box::new(d))).collect();
        let by_id = order.iter().map(|d| (d.id.clone(), *d)).collect();
        Catalog { by_id, order }
    }

    pub fn get(&self, id: &str) -> Option<&'static WeaponDef> {
        self.by_id.get(id).copied()
    }

    pub fn all(&self) -> &[&'static WeaponDef] {
        &self.order
    }
}

static CATALOG: OnceLock<Catalog> = OnceLock::new();

/// Install the converted BO2 table (once, before play). Returns the number of weapons, or why not.
pub fn install_catalog(text: &str) -> Result<usize, String> {
    let defs = parse_table(text)?;
    let n = defs.len();
    CATALOG.set(Catalog::new(defs)).map_err(|_| "weapon catalog already installed".to_string())?;
    Ok(n)
}

/// The installed catalog, or the built-in one.
pub fn catalog() -> &'static Catalog {
    CATALOG.get().unwrap_or_else(|| {
        static BUILT_IN: OnceLock<Catalog> = OnceLock::new();
        BUILT_IN.get_or_init(|| Catalog::new(parse_table(BUILTIN).expect("built-in weapon table")))
    })
}

/// The weapon with this id, falling back to the built-in table and then to its first entry.
pub fn weapon(id: &str) -> &'static WeaponDef {
    catalog().get(id).or_else(|| catalog().all().first().copied()).expect("weapon catalog is empty")
}

/// Stand-in table when BO2 is not converted: the default loadout's weapons with BO2's published
/// multiplayer values (rounded), in the same format as the converted table.
pub const BUILTIN: &str = "
Weapon an94_mp
  DisplayName = AN-94
  Class = rifle
  Fire = auto
  FireTime = 0.096
  Clip = 30
  StartReserve = 90
  MaxReserve = 210
  Reload = 2.47 3.11 1.3 1.9
  Raise = 0.6 1.1 0.4 0.35 0.3
  SprintOutTime = 0.25
  Ads = 0.25 0.2 50
  Damage = 40 33 33 0 0 0
  DamageRange = 216.7 216.8 375 0 0 0
  HeadMultiplier = 1.2
  HipSpread = 3 6.75 2.5 6 2 5
  Spread = 6 0.5 0.5 0
  AdsKick = 30 47.5 -50 50
  HipKick = 30 47.5 -50 50
  Move = 0.95 0.5 1
  FxWeapon = ZHBO_MTAR_FX
  Viewmodel = 0
  Anims = 60000ee003f007ff85005e
End

Weapon fnp45_mp
  DisplayName = Tac-45
  Class = pistol
  Fire = semi
  FireTime = 0.096
  Clip = 10
  StartReserve = 30
  MaxReserve = 70
  Reload = 1.48 1.8 1.1 1.4
  Raise = 0.25 0.6 0.25 0.2 0.2
  SprintOutTime = 0.15
  Ads = 0.12 0.1 60
  Damage = 55 45 45 0 0 0
  DamageRange = 58.3 58.4 125 0 0 0
  HeadMultiplier = 1.1
  HipSpread = 3 5 2.5 4.5 2 4
  Spread = 7 0.6 0.5 0.3
  AdsKick = 60 80 -40 40
  HipKick = 60 80 -40 40
  Move = 1 0.65 1
  FxWeapon = ZHBO_B23R_FX
  Viewmodel = 1
  Anims = 60007e60fff03fff85e256
End

Weapon frag_grenade_mp
  DisplayName = Grenade
  Class = lethal
  Fire = semi
  FireTime = 0.6
  Clip = 1
  StartReserve = 1
  MaxReserve = 2
  Effect = explode
  ExplosionFx = FX_BoobyTrapExplosion
  Fuse = 3.5 1
  Explosion = 42 0 150 50
  Projectile = 225 50 1 0 0
  Viewmodel = 2
  Anims = e800076
End

Weapon concussion_grenade_mp
  DisplayName = Concussion
  Class = tactical
  Fire = semi
  FireTime = 0.6
  Clip = 2
  StartReserve = 2
  MaxReserve = 3
  Effect = stun
  ExplosionFx = WeaponFX_RangerFlashBangGrenadeDetonation
  Fuse = 1.5 0
  Explosion = 67 0 4 2
  Projectile = 225 50 1 0 0
  Viewmodel = 3
  Anims = e800076
End

Weapon knife_held_mp
  DisplayName = Knife
  Class = melee
  Melee = 135 0.6
  Viewmodel = 4
  Anims = 60200e6003f007f3804256
End
";

// ------------------------------------------------------------------------------------------------
// Loadout and perks
// ------------------------------------------------------------------------------------------------

/// BO2's multiplayer perks (Pick 10). Effects that need systems Conquer Ops does not have yet
/// (scorestreaks, UAV, targeting) are carried but do nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Perk {
    // Tier 1
    Lightweight,
    Hardline,
    BlindEye,
    FlakJacket,
    Ghost,
    // Tier 2
    Toughness,
    ColdBlooded,
    FastHands,
    HardWired,
    Scavenger,
    // Tier 3
    Dexterity,
    ExtremeConditioning,
    Engineer,
    TacticalMask,
    DeadSilence,
    Awareness,
}

impl Perk {
    pub const ALL: [Perk; 16] = [
        Perk::Lightweight,
        Perk::Hardline,
        Perk::BlindEye,
        Perk::FlakJacket,
        Perk::Ghost,
        Perk::Toughness,
        Perk::ColdBlooded,
        Perk::FastHands,
        Perk::HardWired,
        Perk::Scavenger,
        Perk::Dexterity,
        Perk::ExtremeConditioning,
        Perk::Engineer,
        Perk::TacticalMask,
        Perk::DeadSilence,
        Perk::Awareness,
    ];

    /// BO2's localize key for the perk's name (`PERKS_<key>`).
    pub fn key(self) -> &'static str {
        match self {
            Perk::Lightweight => "LIGHTWEIGHT",
            Perk::Hardline => "HARDLINE",
            Perk::BlindEye => "BLIND_EYE",
            Perk::FlakJacket => "FLAK_JACKET",
            Perk::Ghost => "GHOST",
            Perk::Toughness => "TOUGHNESS",
            Perk::ColdBlooded => "COLD_BLOODED",
            Perk::FastHands => "FAST_HANDS",
            Perk::HardWired => "HARD_WIRED",
            Perk::Scavenger => "SCAVENGER",
            Perk::Dexterity => "DEXTERITY",
            Perk::ExtremeConditioning => "EXTREME_CONDITIONING",
            Perk::Engineer => "HACKER",
            Perk::TacticalMask => "TACTICAL_MASK",
            Perk::DeadSilence => "DEAD_SILENCE",
            Perk::Awareness => "AWARENESS",
        }
    }
}

/// Perk tuning (BO2's engine defaults as documented by the community; BO2 sets them in script).
pub mod perk_values {
    /// Lightweight: "Move faster."
    pub const LIGHTWEIGHT_SPEED: f32 = 1.07;
    /// Extreme Conditioning: "Sprint for a longer duration."
    pub const EXTREME_CONDITIONING_SPRINT: f32 = 2.0;
    /// Dexterity: "Aim faster after sprinting."
    pub const DEXTERITY_SPRINT_OUT: f32 = 0.5;
    /// Flak Jacket: "Take less explosive damage."
    pub const FLAK_JACKET_EXPLOSIVE: f32 = 0.65;
    /// Toughness: "Flinch less when shot."
    pub const TOUGHNESS_FLINCH: f32 = 0.25;
    /// Tactical Mask: "Reduce the effects of Flashbangs, Concussion Grenades, and Shock Charges."
    pub const TACTICAL_MASK_STUN: f32 = 0.25;
}

/// What the General carries. Weapons are catalog ids, so they can later be picked up, swapped or
/// bought by changing the loadout.
#[derive(Clone, Debug, PartialEq)]
pub struct Loadout {
    pub primary: String,
    pub secondary: String,
    pub lethal: String,
    pub tactical: String,
    pub melee: String,
    pub perks: Vec<Perk>,
}

impl Default for Loadout {
    /// The General's kit: the AN-94 (the rifle the converted models hold), the Tac-45, a frag, a
    /// concussion, the knife; Lightweight, Fast Hands and Dexterity.
    fn default() -> Self {
        Loadout {
            primary: "an94_mp".into(),
            secondary: "fnp45_mp".into(),
            lethal: "frag_grenade_mp".into(),
            tactical: "concussion_grenade_mp".into(),
            melee: "knife_held_mp".into(),
            perks: vec![Perk::Lightweight, Perk::FastHands, Perk::Dexterity],
        }
    }
}

// ------------------------------------------------------------------------------------------------
// State
// ------------------------------------------------------------------------------------------------

/// xorshift32: tiny, deterministic, identical on every client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng(seed.max(1))
    }

    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform in [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Uniform in [-1, 1).
    pub fn signed(&mut self) -> f32 {
        self.next_f32() * 2.0 - 1.0
    }

    pub fn range(&mut self, r: (f32, f32)) -> f32 {
        r.0 + (r.1 - r.0) * self.next_f32()
    }
}

/// A reload in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reload {
    /// Magazine swap: seconds in, total length, when the rounds go in, whether they have.
    Magazine { t: f32, length: f32, add_at: f32, added: bool, empty: bool },
    /// Tube shotgun: start, then one cycle per batch of shells, then end.
    Shells { phase: ShellPhase, t: f32, stop: bool },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShellPhase {
    Start,
    Insert,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeaponState {
    pub def: &'static WeaponDef,
    pub ammo: u16,
    pub reserve: u16,
    /// Seconds until the next round may fire (can go negative within a frame for high RPM).
    pub cooldown: f32,
    pub reload: Option<Reload>,
    pub burst_left: u8,
    /// Hip spread bloom above the stance minimum, degrees.
    pub bloom: f32,
    /// Minigun barrel spin, seconds toward `spin_up_time`.
    pub spin: f32,
    /// Has been raised before (the first raise is BO2's longer "first raise").
    pub raised_once: bool,
}

impl WeaponState {
    pub fn new(def: &'static WeaponDef) -> Self {
        Self {
            def,
            ammo: def.clip,
            reserve: def.start_reserve,
            cooldown: 0.0,
            reload: None,
            burst_left: 0,
            bloom: 0.0,
            spin: 0.0,
            raised_once: false,
        }
    }

    pub fn is_reloading(&self) -> bool {
        self.reload.is_some()
    }

    /// Seconds left on a magazine reload (0 when none; shell reloads report their current phase).
    pub fn reload_left(&self) -> f32 {
        match self.reload {
            Some(Reload::Magazine { t, length, .. }) => (length - t).max(0.0),
            Some(Reload::Shells { .. }) => self.def.reload_time,
            None => 0.0,
        }
    }

    pub fn can_reload(&self) -> bool {
        !self.is_reloading() && self.ammo < self.def.clip && self.reserve > 0
    }

    fn start_reload(&mut self, speed: f32) {
        if !self.can_reload() {
            return;
        }
        let d = self.def;
        self.burst_left = 0;
        self.reload = Some(if d.shell_by_shell() {
            Reload::Shells { phase: ShellPhase::Start, t: 0.0, stop: false }
        } else {
            let empty = self.ammo == 0;
            let (length, add_at) = if empty { (d.reload_empty_time, d.reload_empty_add_time) } else { (d.reload_time, d.reload_add_time) };
            let add_at = if add_at > 0.0 { add_at } else { length };
            Reload::Magazine { t: 0.0, length: length / speed, add_at: add_at.min(length) / speed, added: false, empty }
        });
    }

    fn take(&mut self, n: u16) {
        let take = n.min(self.def.clip - self.ammo).min(self.reserve);
        self.ammo += take;
        self.reserve -= take;
    }

    /// Advance a reload; returns whether it finished this frame.
    fn step_reload(&mut self, dt: f32) -> bool {
        let d = self.def;
        let Some(r) = self.reload else { return false };
        match r {
            Reload::Magazine { t, length, add_at, added, empty } => {
                let t = t + dt;
                let mut added = added;
                if !added && t >= add_at {
                    self.take(d.clip);
                    added = true;
                }
                if t >= length {
                    self.reload = None;
                    return true;
                }
                self.reload = Some(Reload::Magazine { t, length, add_at, added, empty });
            }
            Reload::Shells { phase, t, stop } => {
                let t = t + dt;
                match phase {
                    ShellPhase::Start if t >= d.reload_start_time => self.reload = Some(Reload::Shells { phase: ShellPhase::Insert, t: 0.0, stop }),
                    ShellPhase::Insert if t >= d.reload_time => {
                        self.take(d.reload_ammo_add.max(1));
                        let full = self.ammo >= d.clip || self.reserve == 0;
                        let next = if full || stop { ShellPhase::End } else { ShellPhase::Insert };
                        self.reload = Some(Reload::Shells { phase: next, t: 0.0, stop });
                    }
                    ShellPhase::End if t >= d.reload_end_time => {
                        self.reload = None;
                        return true;
                    }
                    _ => self.reload = Some(Reload::Shells { phase, t, stop }),
                }
            }
        }
        false
    }

    /// Hip cone half-angle (degrees) for a stance (0 stand, 1 crouch, 2 prone) and ADS fraction.
    pub fn spread_deg(&self, stance: usize, ads: f32) -> f32 {
        let (min, max) = self.def.hip_spread[stance.min(2)];
        let hip = (min + self.bloom).min(max);
        hip + (self.def.ads_spread - hip) * ads
    }
}

/// A grenade or other offhand item and how many are left.
#[derive(Clone, Debug, PartialEq)]
pub struct Offhand {
    pub def: &'static WeaponDef,
    pub count: u16,
}

/// What the weapon layer wants the logic to do this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WeaponAction {
    /// Fire one round (one pellet for shotguns) with the given cone (degrees), random offsets in
    /// [-1, 1) and view kick (degrees pitch up, yaw).
    Fire { spread_deg: f32, jitter: (f32, f32), recoil: (f32, f32), pellet: u16 },
    /// Launch a projectile (launchers, crossbow, ballistic knife).
    Launch { def: &'static WeaponDef },
    /// Throw an offhand item; `fuse_left` is what remains after cooking.
    Throw { def: &'static WeaponDef, fuse_left: f32 },
    /// Cooked too long: it goes off in hand.
    CookedOff { def: &'static WeaponDef },
    /// Pin pulled (the throw animation's wind-up starts).
    OffhandReady { lethal: bool },
    Melee { damage: f32 },
    DryFire,
    ReloadStarted,
    ReloadFinished,
    Swapped,
}

/// Which raise animation is playing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaiseKind {
    First,
    Normal,
    Quick,
}

/// Offhand use in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OffhandUse {
    /// Pin pulled / wound up and held (`cooked` seconds of fuse used for cooking grenades).
    Holding { lethal: bool, held_for: f32 },
    /// Released: the throw plays out before the weapon comes back.
    Throwing { lethal: bool, t: f32 },
}

/// Seconds of BO2's offhand pull-back before a throw can leave the hand, and the throw itself.
pub const OFFHAND_PULLBACK: f32 = 0.35;
pub const OFFHAND_THROW: f32 = 0.45;

/// BO2 melee: a lunge up to this range (inches 64 plus the lunge), one hit on infantry.
pub const MELEE_DAMAGE: f32 = 135.0;
pub const MELEE_RANGE: f32 = inches(64.0);
pub const MELEE_FX_WEAPON: &str = "ZHBO_Knife_FX";

#[derive(Clone, Debug, PartialEq)]
pub struct Arsenal {
    pub loadout: Loadout,
    pub weapons: Vec<WeaponState>,
    pub active: usize,
    pub lethal: Offhand,
    pub tactical: Offhand,
    pub melee: &'static WeaponDef,
    /// Raising the active weapon (first raise, swap, after an offhand), out of `raise_total`.
    pub raise_left: f32,
    pub raise_total: f32,
    pub raise_kind: RaiseKind,
    /// Putting the previous weapon away before `raise_left` starts, out of `drop_total`.
    pub drop_left: f32,
    pub drop_total: f32,
    /// The weapon being put away.
    pub previous: usize,
    pub ads: f32,
    /// Seconds since sprinting stopped (weapon comes back up over `sprint_out_time`).
    pub since_sprint: f32,
    pub melee_left: f32,
    pub offhand: Option<OffhandUse>,
    held_last: u32,
}

impl Arsenal {
    pub fn new() -> Self {
        Self::with_loadout(Loadout::default())
    }

    pub fn with_loadout(loadout: Loadout) -> Self {
        let mut weapons: Vec<WeaponState> = [&loadout.primary, &loadout.secondary].iter().map(|id| WeaponState::new(weapon(id))).collect();
        if loadout.perks.contains(&Perk::Scavenger) {
            for w in &mut weapons {
                w.reserve = (w.reserve + w.def.clip).min(w.def.max_reserve.max(w.reserve));
            }
        }
        // Offhands start with BO2's start count (`start_reserve` for them).
        let offhand = |id: &str| {
            let def = weapon(id);
            Offhand { def, count: def.start_reserve.max(1) }
        };
        let raise = weapons[0].def.first_raise_time;
        weapons[0].raised_once = true;
        Self {
            lethal: offhand(&loadout.lethal),
            tactical: offhand(&loadout.tactical),
            melee: weapon(&loadout.melee),
            loadout,
            weapons,
            active: 0,
            raise_left: raise,
            raise_total: raise,
            raise_kind: RaiseKind::First,
            drop_left: 0.0,
            drop_total: 0.0,
            previous: 0,
            ads: 0.0,
            since_sprint: 10.0,
            melee_left: 0.0,
            offhand: None,
            held_last: 0,
        }
    }

    pub fn has(&self, perk: Perk) -> bool {
        self.loadout.perks.contains(&perk)
    }

    pub fn current(&self) -> &WeaponState {
        &self.weapons[self.active]
    }

    pub fn current_mut(&mut self) -> &mut WeaponState {
        &mut self.weapons[self.active]
    }

    /// The weapon can't shoot right now (raising, putting away, reloading, meleeing, throwing).
    pub fn is_busy(&self) -> bool {
        self.raise_left > 0.0 || self.drop_left > 0.0 || self.current().is_reloading() || self.melee_left > 0.0 || self.offhand.is_some()
    }

    /// Movement speed multiplier from the held weapon, ADS and perks.
    pub fn move_scale(&self) -> f32 {
        let d = self.current().def;
        let base = d.move_scale + (d.ads_move_scale - d.move_scale) * self.ads;
        base * if self.has(Perk::Lightweight) { perk_values::LIGHTWEIGHT_SPEED } else { 1.0 }
    }

    /// Sprint duration multiplier from the held weapon and perks.
    pub fn sprint_scale(&self) -> f32 {
        let d = self.current().def.sprint_duration_scale.max(0.1);
        d * if self.has(Perk::ExtremeConditioning) { perk_values::EXTREME_CONDITIONING_SPRINT } else { 1.0 }
    }

    /// Field of view multiplier when fully aimed (relative to BO2's 65 degrees).
    pub fn ads_fov_scale(&self) -> f32 {
        let fov = self.current().def.ads_fov;
        if fov > 0.0 {
            (fov / 65.0).clamp(0.1, 1.0)
        } else {
            1.0
        }
    }

    fn sprint_out_time(&self) -> f32 {
        self.current().def.sprint_out_time * if self.has(Perk::Dexterity) { perk_values::DEXTERITY_SPRINT_OUT } else { 1.0 }
    }

    fn switch_to(&mut self, index: usize) {
        let fast = self.has(Perk::FastHands);
        let from = self.current().def;
        self.current_mut().reload = None; // swapping cancels a reload, like BO2
        self.current_mut().burst_left = 0;
        self.drop_left = if fast { from.quick_drop_time } else { from.drop_time };
        self.drop_total = self.drop_left;
        self.previous = self.active;
        self.active = index;
        let to = self.current().def;
        let first = !self.current().raised_once;
        self.current_mut().raised_once = true;
        (self.raise_left, self.raise_kind) = if first {
            (to.first_raise_time, RaiseKind::First)
        } else if fast {
            (to.quick_raise_time, RaiseKind::Quick)
        } else {
            (to.raise_time, RaiseKind::Normal)
        };
        self.raise_total = self.raise_left;
        self.ads = 0.0;
    }

    /// Give a magazine (Scavenger) to every gun.
    pub fn scavenge(&mut self) {
        for w in &mut self.weapons {
            w.reserve = (w.reserve + w.def.clip).min(w.def.max_reserve);
        }
        self.lethal.count = self.lethal.count.max(1);
    }

    /// Advance one logic frame. `stance` is 0 stand, 1 crouch, 2 prone; `speed` the ground speed.
    pub fn step(&mut self, held: u32, pressed: u32, sprinting: bool, stance: usize, speed: f32, rng: &mut Rng, out: &mut Vec<WeaponAction>) {
        let dt = LOGIC_DT;
        let pressed = pressed | (held & !self.held_last);
        let released = self.held_last & !held;
        self.held_last = held;

        self.since_sprint = if sprinting { 0.0 } else { self.since_sprint + dt };
        self.melee_left = (self.melee_left - dt).max(0.0);
        if self.drop_left > 0.0 {
            self.drop_left = (self.drop_left - dt).max(0.0);
        } else {
            self.raise_left = (self.raise_left - dt).max(0.0);
        }
        for w in &mut self.weapons {
            let decay = w.def.spread_decay.max(0.1);
            w.bloom = (w.bloom - decay * dt).max(0.0);
        }
        let reload_speed = 1.0;

        // Offhands: hold to pull back (and cook), release to throw.
        if self.step_offhand(held, pressed, released, out) {
            return;
        }

        // Swap.
        if pressed & buttons::SWAP_WEAPON != 0 && self.weapons.len() > 1 && self.melee_left <= 0.0 {
            let next = (self.active + 1) % self.weapons.len();
            self.switch_to(next);
            out.push(WeaponAction::Swapped);
        }

        // Melee interrupts a reload and firing, not a raise.
        if pressed & buttons::MELEE != 0 && self.melee_left <= 0.0 && self.raise_left <= 0.0 && self.drop_left <= 0.0 {
            self.melee_left = self.melee.melee_time.max(0.3);
            let w = self.current_mut();
            w.burst_left = 0;
            w.reload = None;
            self.ads = 0.0;
            out.push(WeaponAction::Melee { damage: self.melee.melee_damage });
            return;
        }

        // Reload progress / requests.
        if self.current_mut().step_reload(dt) {
            out.push(WeaponAction::ReloadFinished);
        }
        if pressed & buttons::RELOAD != 0 && self.current().can_reload() && self.raise_left <= 0.0 && self.drop_left <= 0.0 {
            self.current_mut().start_reload(reload_speed);
            out.push(WeaponAction::ReloadStarted);
        }

        // ADS blend: BO2 aims in and out at the weapon's own rates, and not while sprinting or
        // until the weapon is back up from a sprint.
        let def = self.current().def;
        let sprint_ready = self.since_sprint >= self.sprint_out_time();
        let wants_ads = held & buttons::ADS != 0 && !sprinting && sprint_ready && self.raise_left <= 0.0 && self.drop_left <= 0.0 && !self.current().is_reloading();
        self.ads = if wants_ads {
            (self.ads + dt / def.ads_in_time.max(1e-3)).min(1.0)
        } else {
            (self.ads - dt / def.ads_out_time.max(1e-3)).max(0.0)
        };

        if def.class == Class::Melee || def.class.is_offhand() {
            return;
        }

        // Firing.
        let ads = self.ads;
        let blocked = sprinting || !sprint_ready || self.raise_left > 0.0 || self.drop_left > 0.0 || self.melee_left > 0.0;
        let w = self.current_mut();
        w.cooldown -= dt;
        // Minigun barrels spin while the trigger is held.
        if def.fire == FireMode::Minigun {
            w.spin = if held & buttons::FIRE != 0 && !blocked { (w.spin + dt).min(def.spin_up_time) } else { (w.spin - dt).max(0.0) };
        }
        // A tube-shotgun reload stops after the current shell when the trigger is pressed.
        if pressed & buttons::FIRE != 0 {
            if let Some(Reload::Shells { phase, t, .. }) = w.reload {
                if w.ammo > 0 {
                    w.reload = Some(Reload::Shells { phase, t, stop: true });
                }
            }
        }
        let trigger = match def.fire {
            FireMode::Auto => held & buttons::FIRE != 0,
            FireMode::Minigun => held & buttons::FIRE != 0 && w.spin >= def.spin_up_time,
            FireMode::Semi => pressed & buttons::FIRE != 0,
            FireMode::Burst(n) => {
                if pressed & buttons::FIRE != 0 && w.burst_left == 0 && w.cooldown <= 0.0 {
                    w.burst_left = n;
                }
                w.burst_left > 0
            }
        };
        if !trigger || blocked || w.is_reloading() {
            if w.cooldown < 0.0 {
                w.cooldown = 0.0;
            }
            if sprinting || w.is_reloading() {
                w.burst_left = 0;
            }
            return;
        }
        if w.ammo == 0 {
            w.burst_left = 0;
            if pressed & buttons::FIRE != 0 {
                if w.can_reload() {
                    w.start_reload(reload_speed);
                    out.push(WeaponAction::ReloadStarted);
                } else {
                    out.push(WeaponAction::DryFire);
                }
            }
            if w.cooldown < 0.0 {
                w.cooldown = 0.0;
            }
            return;
        }
        let kick = if ads > 0.5 { def.ads_kick } else { def.hip_kick };
        let mut fired_semi = false;
        while w.cooldown <= 0.0 && w.ammo > 0 {
            if def.fire == FireMode::Semi && fired_semi {
                break;
            }
            if def.class == Class::Launcher || (def.projectile_speed > 0.0 && def.class != Class::Shotgun && def.effect != Effect::None) {
                out.push(WeaponAction::Launch { def });
            } else {
                let spread = w.spread_deg(stance, ads) + if ads < 0.5 { def.spread_move_add * (speed / 30.0).min(1.0) } else { 0.0 };
                for pellet in 0..def.pellets.max(1) {
                    let jitter = (rng.signed(), rng.signed());
                    let recoil = if pellet == 0 {
                        (rng.range(kick.pitch) * KICK_TO_DEGREES, rng.range(kick.yaw) * KICK_TO_DEGREES)
                    } else {
                        (0.0, 0.0)
                    };
                    out.push(WeaponAction::Fire { spread_deg: spread, jitter, recoil, pellet });
                }
            }
            w.ammo -= 1;
            w.bloom += def.spread_fire_add;
            w.cooldown += def.fire_time;
            fired_semi = true;
            if let FireMode::Burst(_) = def.fire {
                w.burst_left = w.burst_left.saturating_sub(1);
                if w.burst_left == 0 {
                    w.cooldown += def.burst_delay;
                    break;
                }
            }
        }
        if w.ammo == 0 && w.reserve > 0 {
            // BO2 reloads by itself when the magazine runs dry.
            w.start_reload(reload_speed);
            out.push(WeaponAction::ReloadStarted);
        }
    }

    /// Lethal (G) and tactical (Q) use. Returns whether it owns this frame.
    fn step_offhand(&mut self, held: u32, pressed: u32, released: u32, out: &mut Vec<WeaponAction>) -> bool {
        let dt = LOGIC_DT;
        match self.offhand {
            None => {
                let lethal = pressed & buttons::LETHAL != 0;
                let tactical = pressed & buttons::TACTICAL != 0;
                // BO2 lets an offhand interrupt a raise, not a swap's put-away or a melee.
                if !(lethal || tactical) || self.drop_left > 0.0 || self.melee_left > 0.0 {
                    return false;
                }
                let slot = if lethal { &self.lethal } else { &self.tactical };
                if slot.count == 0 {
                    return false;
                }
                let w = self.current_mut();
                w.reload = None;
                w.burst_left = 0;
                self.ads = 0.0;
                self.offhand = Some(OffhandUse::Holding { lethal, held_for: 0.0 });
                out.push(WeaponAction::OffhandReady { lethal });
                true
            }
            Some(OffhandUse::Holding { lethal, held_for }) => {
                let held_for = held_for + dt;
                let button = if lethal { buttons::LETHAL } else { buttons::TACTICAL };
                let def = if lethal { self.lethal.def } else { self.tactical.def };
                let fuse_left = if def.cook { def.fuse_time - held_for } else { def.fuse_time };
                if def.cook && fuse_left <= 0.0 {
                    // Cooked too long.
                    self.use_offhand(lethal);
                    self.offhand = Some(OffhandUse::Throwing { lethal, t: 0.0 });
                    out.push(WeaponAction::CookedOff { def });
                    return true;
                }
                let let_go = released & button != 0 || held & button == 0;
                if let_go && held_for >= OFFHAND_PULLBACK {
                    self.use_offhand(lethal);
                    self.offhand = Some(OffhandUse::Throwing { lethal, t: 0.0 });
                    out.push(WeaponAction::Throw { def, fuse_left });
                } else {
                    self.offhand = Some(OffhandUse::Holding { lethal, held_for });
                }
                true
            }
            Some(OffhandUse::Throwing { lethal, t }) => {
                let t = t + dt;
                if t >= OFFHAND_THROW {
                    self.offhand = None;
                    let d = self.current().def;
                    (self.raise_left, self.raise_kind) =
                        if self.has(Perk::FastHands) { (d.quick_raise_time, RaiseKind::Quick) } else { (d.raise_time, RaiseKind::Normal) };
                    self.raise_total = self.raise_left;
                } else {
                    self.offhand = Some(OffhandUse::Throwing { lethal, t });
                }
                true
            }
        }
    }

    fn use_offhand(&mut self, lethal: bool) {
        let slot = if lethal { &mut self.lethal } else { &mut self.tactical };
        slot.count = slot.count.saturating_sub(1);
    }
}

impl Default for Arsenal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(f: impl FnOnce(&mut WeaponDef)) -> &'static WeaponDef {
        let mut d = WeaponDef { id: "test_mp".into(), ..Default::default() };
        f(&mut d);
        Box::leak(Box::new(d))
    }

    fn arsenal_with(primary: &'static WeaponDef) -> Arsenal {
        let mut a = Arsenal::new();
        a.weapons[0] = WeaponState::new(primary);
        a.raise_left = 0.0;
        a
    }

    fn frames(a: &mut Arsenal, held: u32, n: usize, rng: &mut Rng) -> Vec<WeaponAction> {
        let mut out = Vec::new();
        for _ in 0..n {
            a.step(held, 0, false, 0, 0.0, rng, &mut out);
        }
        out
    }

    fn shots(actions: &[WeaponAction]) -> usize {
        actions.iter().filter(|a| matches!(a, WeaponAction::Fire { pellet: 0, .. })).count()
    }

    #[test]
    fn table_round_trips() {
        let defs: Vec<WeaponDef> = catalog().all().iter().map(|d| (*d).clone()).collect();
        let again = parse_table(&format_table(&defs)).unwrap();
        assert_eq!(defs, again);
        assert!(catalog().get("an94_mp").is_some() && catalog().get("frag_grenade_mp").is_some());
    }

    #[test]
    fn auto_fire_matches_fire_time() {
        let d = def(|d| d.fire_time = 0.08); // 750 rpm
        let mut a = arsenal_with(d);
        let fired = shots(&frames(&mut a, buttons::FIRE, 60, &mut Rng::new(1))); // 2 s
        assert!((24..=26).contains(&fired), "fired {fired}");
    }

    #[test]
    fn magazine_reload_adds_rounds_at_the_add_time() {
        let d = def(|d| {
            d.clip = 30;
            d.start_reserve = 90;
            d.reload_empty_time = 2.0;
            d.reload_empty_add_time = 1.0;
        });
        let mut a = arsenal_with(d);
        let mut rng = Rng::new(2);
        let out = frames(&mut a, buttons::FIRE, 120, &mut rng);
        assert_eq!(shots(&out), 30);
        assert!(out.contains(&WeaponAction::ReloadStarted), "auto reload when empty");
        // Rounds land at the add time, before the reload animation ends.
        let mut a2 = a.clone();
        frames(&mut a2, 0, 1, &mut rng);
        assert!(a.current().is_reloading());
        let mut t = 0;
        while a.current().ammo == 0 {
            frames(&mut a, 0, 1, &mut rng);
            t += 1;
        }
        assert!(a.current().is_reloading(), "still animating after the rounds went in");
        assert_eq!(a.current().ammo, 30);
        assert_eq!(a.current().reserve, 60);
        assert!(t < 40);
    }

    #[test]
    fn bursts_fire_n_per_press() {
        let d = def(|d| {
            d.fire = FireMode::Burst(3);
            d.fire_time = 0.06;
            d.burst_delay = 0.2;
        });
        let mut a = arsenal_with(d);
        let mut rng = Rng::new(3);
        let mut out = Vec::new();
        a.step(buttons::FIRE, buttons::FIRE, false, 0, 0.0, &mut rng, &mut out);
        for _ in 0..20 {
            a.step(buttons::FIRE, 0, false, 0, 0.0, &mut rng, &mut out);
        }
        assert_eq!(shots(&out), 3, "one burst while holding");
    }

    #[test]
    fn shotguns_fire_pellets_and_reload_shell_by_shell() {
        let d = def(|d| {
            d.class = Class::Shotgun;
            d.fire = FireMode::Semi;
            d.pellets = 8;
            d.clip = 8;
            d.start_reserve = 16;
            d.reload_time = 0.5;
            d.reload_start_time = 0.3;
            d.reload_end_time = 0.3;
            d.reload_ammo_add = 1;
        });
        let mut a = arsenal_with(d);
        let mut rng = Rng::new(4);
        let mut out = Vec::new();
        a.step(buttons::FIRE, buttons::FIRE, false, 0, 0.0, &mut rng, &mut out);
        let pellets = out.iter().filter(|x| matches!(x, WeaponAction::Fire { .. })).count();
        assert_eq!(pellets, 8);
        // Reload one shell, then the trigger stops it after that shell.
        a.step(0, buttons::RELOAD, false, 0, 0.0, &mut rng, &mut out);
        frames(&mut a, 0, 30, &mut rng); // start (0.3) + one shell (0.5)
        assert_eq!(a.current().ammo, 8, "one shell went in");
    }

    #[test]
    fn grenades_cook_and_throw() {
        let mut a = Arsenal::new();
        a.raise_left = 0.0;
        let mut rng = Rng::new(5);
        let mut out = Vec::new();
        a.step(buttons::LETHAL, buttons::LETHAL, false, 0, 0.0, &mut rng, &mut out);
        assert!(out.contains(&WeaponAction::OffhandReady { lethal: true }));
        // Hold (cook) for one second, then let go.
        for _ in 0..30 {
            a.step(buttons::LETHAL, 0, false, 0, 0.0, &mut rng, &mut out);
        }
        a.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
        let throw = out.iter().find_map(|x| if let WeaponAction::Throw { fuse_left, .. } = x { Some(*fuse_left) } else { None });
        let fuse = a.lethal.def.fuse_time;
        let left = throw.expect("thrown");
        assert!((left - (fuse - 1.0)).abs() < 0.1, "cooked: {left} of {fuse}");
        assert_eq!(a.lethal.count, 0);
        // The weapon comes back after the throw.
        frames(&mut a, 0, 60, &mut rng);
        assert!(a.offhand.is_none() && !a.is_busy());
    }

    #[test]
    fn cooking_too_long_goes_off_in_hand() {
        let mut a = Arsenal::new();
        a.raise_left = 0.0;
        let mut out = Vec::new();
        let mut rng = Rng::new(6);
        a.step(buttons::LETHAL, buttons::LETHAL, false, 0, 0.0, &mut rng, &mut out);
        for _ in 0..200 {
            a.step(buttons::LETHAL, 0, false, 0, 0.0, &mut rng, &mut out);
        }
        assert!(out.iter().any(|x| matches!(x, WeaponAction::CookedOff { .. })));
    }

    #[test]
    fn sprinting_blocks_fire_and_ads_until_sprint_out() {
        let d = def(|d| d.sprint_out_time = 0.3);
        let mut a = arsenal_with(d);
        a.loadout.perks.clear();
        let mut rng = Rng::new(7);
        let mut out = Vec::new();
        for _ in 0..30 {
            a.step(buttons::FIRE | buttons::ADS, 0, true, 0, 0.0, &mut rng, &mut out);
        }
        assert_eq!(shots(&out), 0);
        assert_eq!(a.ads, 0.0);
        // Right after the sprint: still coming up.
        a.step(buttons::FIRE, 0, false, 0, 0.0, &mut rng, &mut out);
        assert_eq!(shots(&out), 0, "sprint-out time");
        frames(&mut a, buttons::FIRE, 12, &mut rng);
        let mut out = Vec::new();
        a.step(buttons::FIRE, 0, false, 0, 0.0, &mut rng, &mut out);
        assert!(shots(&frames(&mut a, buttons::FIRE, 5, &mut rng)) > 0);
    }

    #[test]
    fn swap_drops_then_raises_faster_with_fast_hands() {
        let mut a = Arsenal::new();
        a.raise_left = 0.0;
        a.loadout.perks.clear();
        let mut out = Vec::new();
        a.step(0, buttons::SWAP_WEAPON, false, 0, 0.0, &mut Rng::new(8), &mut out);
        let slow = a.drop_left + a.raise_left;
        let mut b = Arsenal::new();
        b.raise_left = 0.0;
        b.step(0, buttons::SWAP_WEAPON, false, 0, 0.0, &mut Rng::new(8), &mut out);
        let fast = b.drop_left + b.raise_left;
        assert!(fast < slow, "fast hands {fast} vs {slow}");
        assert_eq!(a.active, 1);
    }

    #[test]
    fn damage_curve_and_explosion_falloff() {
        let d = weapon("an94_mp");
        assert_eq!(d.damage_at(10.0), 40.0);
        assert_eq!(d.damage_at(1000.0), 33.0);
        let g = weapon("frag_grenade_mp");
        assert!(g.explosion_damage_at(0.0) > g.explosion_damage_at(g.explosion_radius * 0.9));
        assert_eq!(g.explosion_damage_at(g.explosion_radius + 1.0), 0.0);
    }

    #[test]
    fn perks_change_movement() {
        let mut a = Arsenal::new();
        let with = a.move_scale();
        a.loadout.perks.retain(|p| *p != Perk::Lightweight);
        assert!(with > a.move_scale());
        a.loadout.perks.push(Perk::ExtremeConditioning);
        assert!(a.sprint_scale() >= 2.0 * weapon("an94_mp").sprint_duration_scale - 1e-4);
    }
}
