/*
** Conquer Ops - Zero Hour gameplay core: the weapon table: BO2's WeaponDef rows as `key = value` text, and the schema it is generated from.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::def::{Class, Effect, FireMode, Kind, Kick, Place, WeaponDef};
use super::feel::{nums, v3, GunKick, Sway};
use super::specialty::Specialties;
use crate::attachments::AttachmentDef;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    pub weapons: Vec<WeaponDef>,
    pub attachments: Vec<AttachmentDef>,
    /// Third-person meshes of the scorestreak tablet on the soldier model (`TB{k:02}`).
    pub tablet_meshes: u8,
    /// BO2's perk rows (`mp/statstable.csv`): the reference without `PERKS_` (`LIGHTWEIGHT`,
    /// `NINJA`) and the specialties it grants (`Perk <ref>` blocks).
    pub perks: Vec<(String, Specialties)>,
}

pub(crate) fn pairs(v: &[(String, String)]) -> String {
    v.iter().map(|(a, b)| format!("{a}={b}")).collect::<Vec<_>>().join(" ")
}

pub(crate) fn parse_pairs(s: &str) -> Vec<(String, String)> {
    s.split_whitespace().filter_map(|p| p.split_once('=')).map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

pub(crate) fn fire_name(f: FireMode) -> String {
    match f {
        FireMode::Auto => "auto".to_string(),
        FireMode::Semi => "semi".to_string(),
        FireMode::Burst(n) => format!("burst {n}"),
        FireMode::Minigun => "minigun".to_string(),
    }
}

pub(crate) fn parse_fire(value: &str) -> Option<FireMode> {
    let mut words = value.split_whitespace();
    Some(match words.next()? {
        "auto" => FireMode::Auto,
        "semi" => FireMode::Semi,
        "minigun" => FireMode::Minigun,
        "burst" => FireMode::Burst(words.next().and_then(|n| n.parse().ok()).unwrap_or(3)),
        _ => return None,
    })
}

/// Write the table in the format [`parse_table`] reads (Zero Hour INI style: `Weapon <id>` and
/// `Attachment <id>` blocks of `key = value` lines, each closed by `End`).
pub fn format_table(defs: &[WeaponDef]) -> String {
    format_full_table(&Table { weapons: defs.to_vec(), ..Default::default() })
}

pub fn format_full_table(t: &Table) -> String {
    let mut s = String::from("; Black Ops II multiplayer weapons, converted from the player's own install by zhbo-assets.\n; Private data: do not redistribute.\n\n");
    s.push_str(&format!("TabletMeshes = {}\n\n", t.tablet_meshes));
    for (stats, specialties) in &t.perks {
        s.push_str(&format!("Perk {stats}\n  Specialties = {}\nEnd\n\n", specialties.format()));
    }
    for a in &t.attachments {
        write_block(&mut s, "Attachment ", &a.id, ATTACHMENT_FIELDS, a);
    }
    for d in &t.weapons {
        write_block(&mut s, "Weapon ", &d.id, WEAPON_FIELDS, d);
    }
    s
}

/// Read the weapon table's weapons (see [`parse_full_table`]).
pub fn parse_table(text: &str) -> Result<Vec<WeaponDef>, String> {
    parse_full_table(text).map(|t| t.weapons)
}

/// Read the weapon table. Unknown keys are ignored; a malformed value is an error. The keys and
/// their formats live in [`crate::schema`], shared with the formatter.
pub fn parse_full_table(text: &str) -> Result<Table, String> {
    enum Block {
        Weapon(Box<WeaponDef>),
        Attachment(Box<AttachmentDef>),
        Perk(String, Specialties),
    }
    let mut table = Table::default();
    let mut cur: Option<Block> = None;
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let err = |what: &str| format!("weapons table line {}: {what}: {raw}", n + 1);
        if let Some(id) = line.strip_prefix("Weapon ") {
            cur = Some(Block::Weapon(Box::new(WeaponDef { id: id.trim().to_string(), ..Default::default() })));
            continue;
        }
        if let Some(id) = line.strip_prefix("Attachment ") {
            cur = Some(Block::Attachment(Box::new(AttachmentDef { id: id.trim().to_string(), ..Default::default() })));
            continue;
        }
        if let Some(id) = line.strip_prefix("Perk ") {
            cur = Some(Block::Perk(id.trim().to_string(), Specialties::NONE));
            continue;
        }
        if line == "End" {
            match cur.take().ok_or_else(|| err("End without a block"))? {
                Block::Weapon(w) => table.weapons.push(*w),
                Block::Attachment(a) => table.attachments.push(*a),
                Block::Perk(stats, specialties) => table.perks.push((stats, specialties)),
            }
            continue;
        }
        let (key, value) = line.split_once('=').ok_or_else(|| err("expected key = value"))?;
        let (key, value) = (key.trim(), value.trim());
        let nums: Vec<f32> = value.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        let values = Values { text: value, nums: &nums };
        match cur.as_mut() {
            None => match key {
                "TabletMeshes" => table.tablet_meshes = values.text_num().map_err(|e| err(&e))? as u8,
                _ => return Err(err("value outside a block")),
            },
            Some(Block::Attachment(a)) => set_field(ATTACHMENT_FIELDS, a, key, values).map_err(|e| err(&e))?,
            Some(Block::Perk(_, specialties)) => {
                if key == "Specialties" {
                    *specialties = Specialties::parse(value).map_err(|e| err(&e))?;
                }
            }
            Some(Block::Weapon(d)) => set_field(WEAPON_FIELDS, d, key, values).map_err(|e| err(&e))?,
        }
    }
    if cur.is_some() {
        return Err("weapons table: last block has no End".into());
    }
    Ok(table)
}

// ------------------------------------------------------------------------------------------------
// The table: text format shared with the converter
// ------------------------------------------------------------------------------------------------

/// The shape of a field's value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// One number.
    Num,
    /// `n` space-separated numbers, all required (bools are `0`/`1`).
    Nums(usize),
    /// A string, verbatim (trimmed).
    Str,
    /// Space-separated words (a weapon's attachment shorts).
    Words,
    /// Lowercase hex, no prefix (the viewmodel anims bitmask).
    Hex,
    /// `a=b c=d` pairs (attachments and the alt modes they add).
    Pairs,
    /// One of the table's enums, by name.
    Enum,
}

/// The value half of a `key = value` line: the trimmed text and the numbers parsed out of it.
pub struct Values<'a> {
    pub text: &'a str,
    pub nums: &'a [f32],
}

impl Values<'_> {
    /// The first `n` numbers must be present.
    pub fn need(&self, n: usize) -> Result<(), String> {
        if self.nums.len() >= n {
            Ok(())
        } else {
            Err("too few numbers".into())
        }
    }
    pub fn num(&self, i: usize) -> f32 {
        self.nums[i]
    }
    pub fn flag(&self, i: usize) -> bool {
        self.nums[i] != 0.0
    }
    /// The whole value as one number (the single-number kinds parse the text, so `30.0` is as
    /// valid as `30` there).
    pub fn text_num(&self) -> Result<f32, String> {
        self.text.parse().map_err(|_| "number".to_string())
    }
}

/// One `key = value` line of a block.
pub struct Field<T> {
    /// The line's key, as the converter writes it.
    pub key: &'static str,
    pub doc: &'static str,
    pub kind: FieldKind,
    /// The formatted value; must reproduce the table format byte for byte.
    pub get: fn(&T) -> String,
    /// Parse the value; the error is a short reason (the caller adds the line context).
    pub set: fn(&mut T, Values) -> Result<(), String>,
}

macro_rules! field {
    ($key:literal, $doc:literal, $kind:expr, $get:expr, $set:expr) => {
        Field { key: $key, doc: $doc, kind: $kind, get: $get, set: $set }
    };
}

/// Every `Attachment` block line, in table order.
pub const ATTACHMENT_FIELDS: &[Field<AttachmentDef>] = &[
    field!("Short", "The attachment's two-letter short (`gl`, `reflex`).", FieldKind::Str,
        |a: &AttachmentDef| a.short.clone(),
        |a: &mut AttachmentDef, v: Values| { a.short = v.text.into(); Ok(()) }),
    field!("DisplayName", "As BO2's create-a-class shows it.", FieldKind::Str,
        |a: &AttachmentDef| a.display_name.clone(),
        |a: &mut AttachmentDef, v: Values| { a.display_name = v.text.into(); Ok(()) }),
    field!("Point", "The viewmodel attachment point (BO2's attachment point id).", FieldKind::Num,
        |a: &AttachmentDef| a.point.to_string(),
        |a: &mut AttachmentDef, v: Values| { a.point = v.text_num()? as u8; Ok(()) }),
    field!("Flags", "silenced, laser.", FieldKind::Nums(2),
        |a: &AttachmentDef| format!("{} {}", a.silenced as u8, a.laser as u8),
        |a: &mut AttachmentDef, v: Values| {
            v.need(2)?;
            (a.silenced, a.laser) = (v.flag(0), v.flag(1));
            Ok(())
        }),
    field!("Fire", "The fire mode it forces (`none` keeps the weapon's).", FieldKind::Enum,
        |a: &AttachmentDef| a.fire.map_or("none".into(), fire_name),
        |a: &mut AttachmentDef, v: Values| { a.fire = parse_fire(v.text); Ok(()) }),
    field!("Clip", "Rounds in the magazine when this optic or mag is on (0 keeps the weapon's).", FieldKind::Num,
        |a: &AttachmentDef| a.clip.to_string(),
        |a: &mut AttachmentDef, v: Values| { a.clip = v.text_num()? as u16; Ok(()) }),
    field!("DamageRangeScale", "Scales the damage curve's ranges.", FieldKind::Num,
        |a: &AttachmentDef| a.damage_range_scale.to_string(),
        |a: &mut AttachmentDef, v: Values| { a.damage_range_scale = v.text_num()?; Ok(()) }),
    field!("Ads", "ADS field of view, in/out scale and move scale.", FieldKind::Nums(4),
        |a: &AttachmentDef| format!("{} {} {} {}", a.ads_fov, a.ads_in_scale, a.ads_out_scale, a.ads_move_scale),
        |a: &mut AttachmentDef, v: Values| {
            v.need(4)?;
            (a.ads_fov, a.ads_in_scale, a.ads_out_scale, a.ads_move_scale) = (v.num(0), v.num(1), v.num(2), v.num(3));
            Ok(())
        }),
    field!("HipSpreadScale", "Scales the hip spread's min and max.", FieldKind::Nums(2),
        |a: &AttachmentDef| format!("{} {}", a.hip_spread_min_scale, a.hip_spread_max_scale),
        |a: &mut AttachmentDef, v: Values| {
            v.need(2)?;
            (a.hip_spread_min_scale, a.hip_spread_max_scale) = (v.num(0), v.num(1));
            Ok(())
        }),
    field!("FireTimeScale", "Scales the time between shots.", FieldKind::Num,
        |a: &AttachmentDef| a.fire_time_scale.to_string(),
        |a: &mut AttachmentDef, v: Values| { a.fire_time_scale = v.text_num()?; Ok(()) }),
    field!("ReloadScale", "Scales the reload, empty-reload and add times.", FieldKind::Nums(3),
        |a: &AttachmentDef| format!("{} {} {}", a.reload_scale, a.reload_empty_scale, a.reload_add_scale),
        |a: &mut AttachmentDef, v: Values| {
            v.need(3)?;
            (a.reload_scale, a.reload_empty_scale, a.reload_add_scale) = (v.num(0), v.num(1), v.num(2));
            Ok(())
        }),
    field!("ClipScale", "Scales the clip size (rounded).", FieldKind::Num,
        |a: &AttachmentDef| a.clip_scale.to_string(),
        |a: &mut AttachmentDef, v: Values| { a.clip_scale = v.text_num()?; Ok(()) }),
    field!("RecoilReduction", "Per-shot recoil reduction and its limit.", FieldKind::Nums(2),
        |a: &AttachmentDef| format!("{} {}", a.recoil_reduction.0, a.recoil_reduction.1),
        |a: &mut AttachmentDef, v: Values| {
            v.need(2)?;
            a.recoil_reduction = (v.num(0), v.num(1));
            Ok(())
        }),
    field!("Specialties", "The specialties it grants (`WeaponAttachment.perks`), by name.", FieldKind::Words,
        |a: &AttachmentDef| a.specialties.format(),
        |a: &mut AttachmentDef, v: Values| { a.specialties = Specialties::parse(v.text)?; Ok(()) }),
];

/// Every `Weapon` block line, in table order.
pub const WEAPON_FIELDS: &[Field<WeaponDef>] = &[
    field!("DisplayName", "As BO2's HUD shows it.", FieldKind::Str,
        |d: &WeaponDef| d.display_name.clone(),
        |d: &mut WeaponDef, v: Values| { d.display_name = v.text.into(); Ok(()) }),
    field!("Class", "The weapon class (smg, ar, sniper, launcher, pistol, melee, offhand).", FieldKind::Enum,
        |d: &WeaponDef| d.class.name().into(),
        |d: &mut WeaponDef, v: Values| { d.class = Class::parse(v.text).ok_or("unknown class")?; Ok(()) }),
    field!("Fire", "auto, semi, minigun, or `burst n`.", FieldKind::Enum,
        |d: &WeaponDef| fire_name(d.fire),
        |d: &mut WeaponDef, v: Values| { d.fire = parse_fire(v.text).ok_or("unknown fire mode")?; Ok(()) }),
    field!("FireTime", "Seconds between shots.", FieldKind::Num,
        |d: &WeaponDef| d.fire_time.to_string(),
        |d: &mut WeaponDef, v: Values| { d.fire_time = v.text_num()?; Ok(()) }),
    field!("BurstDelay", "Extra wait after a burst before the next one.", FieldKind::Num,
        |d: &WeaponDef| d.burst_delay.to_string(),
        |d: &mut WeaponDef, v: Values| { d.burst_delay = v.text_num()?; Ok(()) }),
    field!("SpinUpTime", "Minigun spin-up.", FieldKind::Num,
        |d: &WeaponDef| d.spin_up_time.to_string(),
        |d: &mut WeaponDef, v: Values| { d.spin_up_time = v.text_num()?; Ok(()) }),
    field!("Pellets", "Projectiles per shot (shotguns).", FieldKind::Num,
        |d: &WeaponDef| d.pellets.to_string(),
        |d: &mut WeaponDef, v: Values| { d.pellets = v.text.parse().map_err(|_| "number")?; Ok(()) }),
    field!("Clip", "Rounds in the magazine.", FieldKind::Num,
        |d: &WeaponDef| d.clip.to_string(),
        |d: &mut WeaponDef, v: Values| { d.clip = v.text.parse().map_err(|_| "number")?; Ok(()) }),
    field!("StartReserve", "Spare rounds at spawn.", FieldKind::Num,
        |d: &WeaponDef| d.start_reserve.to_string(),
        |d: &mut WeaponDef, v: Values| { d.start_reserve = v.text.parse().map_err(|_| "number")?; Ok(()) }),
    field!("MaxReserve", "Spare rounds at most.", FieldKind::Num,
        |d: &WeaponDef| d.max_reserve.to_string(),
        |d: &mut WeaponDef, v: Values| { d.max_reserve = v.text.parse().map_err(|_| "number")?; Ok(()) }),
    field!("Reload", "Reload, empty reload, rounds-in time, empty rounds-in time.", FieldKind::Nums(4),
        |d: &WeaponDef| format!("{} {} {} {}", d.reload_time, d.reload_empty_time, d.reload_add_time, d.reload_empty_add_time),
        |d: &mut WeaponDef, v: Values| {
            v.need(4)?;
            (d.reload_time, d.reload_empty_time, d.reload_add_time, d.reload_empty_add_time) = (v.num(0), v.num(1), v.num(2), v.num(3));
            Ok(())
        }),
    field!("ReloadShells", "Shell-by-shell reload (tube shotguns): start, end, rounds per cycle.", FieldKind::Nums(3),
        |d: &WeaponDef| format!("{} {} {}", d.reload_start_time, d.reload_end_time, d.reload_ammo_add),
        |d: &mut WeaponDef, v: Values| {
            v.need(3)?;
            (d.reload_start_time, d.reload_end_time, d.reload_ammo_add) = (v.num(0), v.num(1), v.num(2) as u16);
            Ok(())
        }),
    field!("Raise", "Raise, first raise, drop, quick raise, quick drop.", FieldKind::Nums(5),
        |d: &WeaponDef| format!("{} {} {} {} {}", d.raise_time, d.first_raise_time, d.drop_time, d.quick_raise_time, d.quick_drop_time),
        |d: &mut WeaponDef, v: Values| {
            v.need(5)?;
            (d.raise_time, d.first_raise_time, d.drop_time, d.quick_raise_time, d.quick_drop_time) =
                (v.num(0), v.num(1), v.num(2), v.num(3), v.num(4));
            Ok(())
        }),
    field!("SprintOutTime", "Raising from a sprint.", FieldKind::Num,
        |d: &WeaponDef| d.sprint_out_time.to_string(),
        |d: &mut WeaponDef, v: Values| { d.sprint_out_time = v.text_num()?; Ok(()) }),
    field!("Ads", "ADS in and out time, and the aimed field of view (0: no zoom).", FieldKind::Nums(3),
        |d: &WeaponDef| format!("{} {} {}", d.ads_in_time, d.ads_out_time, d.ads_fov),
        |d: &mut WeaponDef, v: Values| {
            v.need(3)?;
            (d.ads_in_time, d.ads_out_time, d.ads_fov) = (v.num(0), v.num(1), v.num(2));
            Ok(())
        }),
    field!("Damage", "BO2's damage curve: damage at each range step.", FieldKind::Nums(6),
        |d: &WeaponDef| d.damage.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" "),
        |d: &mut WeaponDef, v: Values| {
            v.need(6)?;
            d.damage.copy_from_slice(&v.nums[..6]);
            Ok(())
        }),
    field!("DamageRange", "The ranges (Zero Hour units) of the damage curve.", FieldKind::Nums(6),
        |d: &WeaponDef| d.damage_range.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" "),
        |d: &mut WeaponDef, v: Values| {
            v.need(6)?;
            d.damage_range.copy_from_slice(&v.nums[..6]);
            Ok(())
        }),
    field!("HeadMultiplier", "Headshot damage multiplier.", FieldKind::Num,
        |d: &WeaponDef| d.head_multiplier.to_string(),
        |d: &mut WeaponDef, v: Values| { d.head_multiplier = v.text_num()?; Ok(()) }),
    field!("MaxRange", "Bullets fly this far (Zero Hour units).", FieldKind::Num,
        |d: &WeaponDef| d.max_range.to_string(),
        |d: &mut WeaponDef, v: Values| { d.max_range = v.text_num()?; Ok(()) }),
    field!("HipSpread", "Hip spread cone (degrees) min/max: standing, crouched, prone.", FieldKind::Nums(6),
        |d: &WeaponDef| d.hip_spread.iter().flat_map(|p| [p.0, p.1]).map(|x| x.to_string()).collect::<Vec<_>>().join(" "),
        |d: &mut WeaponDef, v: Values| {
            v.need(6)?;
            d.hip_spread = [(v.num(0), v.num(1)), (v.num(2), v.num(3)), (v.num(4), v.num(5))];
            Ok(())
        }),
    field!("Spread", "Spread decay, per-shot add, move add, and the ADS spread.", FieldKind::Nums(4),
        |d: &WeaponDef| format!("{} {} {} {}", d.spread_decay, d.spread_fire_add, d.spread_move_add, d.ads_spread),
        |d: &mut WeaponDef, v: Values| {
            v.need(4)?;
            (d.spread_decay, d.spread_fire_add, d.spread_move_add, d.ads_spread) = (v.num(0), v.num(1), v.num(2), v.num(3));
            Ok(())
        }),
    field!("AdsKick", "Per-shot view kick aimed: pitch min/max, yaw min/max.", FieldKind::Nums(4),
        |d: &WeaponDef| format!("{} {} {} {}", d.ads_kick.pitch.0, d.ads_kick.pitch.1, d.ads_kick.yaw.0, d.ads_kick.yaw.1),
        |d: &mut WeaponDef, v: Values| {
            v.need(4)?;
            d.ads_kick = Kick { pitch: (v.num(0), v.num(1)), yaw: (v.num(2), v.num(3)) };
            Ok(())
        }),
    field!("HipKick", "Per-shot view kick from the hip: pitch min/max, yaw min/max.", FieldKind::Nums(4),
        |d: &WeaponDef| format!("{} {} {} {}", d.hip_kick.pitch.0, d.hip_kick.pitch.1, d.hip_kick.yaw.0, d.hip_kick.yaw.1),
        |d: &mut WeaponDef, v: Values| {
            v.need(4)?;
            d.hip_kick = Kick { pitch: (v.num(0), v.num(1)), yaw: (v.num(2), v.num(3)) };
            Ok(())
        }),
    field!("Move", "Move scale, ADS move scale, sprint duration scale.", FieldKind::Nums(3),
        |d: &WeaponDef| format!("{} {} {}", d.move_scale, d.ads_move_scale, d.sprint_duration_scale),
        |d: &mut WeaponDef, v: Values| {
            v.need(3)?;
            (d.move_scale, d.ads_move_scale, d.sprint_duration_scale) = (v.num(0), v.num(1), v.num(2));
            Ok(())
        }),
    field!("Melee", "Bayonet/stock melee damage and time.", FieldKind::Nums(2),
        |d: &WeaponDef| format!("{} {}", d.melee_damage, d.melee_time),
        |d: &mut WeaponDef, v: Values| {
            v.need(2)?;
            (d.melee_damage, d.melee_time) = (v.num(0), v.num(1));
            Ok(())
        }),
    field!("Effect", "How the shot lands: explode, stun, emp, smoke, impact, none.", FieldKind::Enum,
        |d: &WeaponDef| d.effect.name().into(),
        |d: &mut WeaponDef, v: Values| { d.effect = Effect::parse(v.text).ok_or("unknown effect")?; Ok(()) }),
    field!("Fuse", "Fuse time, and whether holding cooks it.", FieldKind::Nums(2),
        |d: &WeaponDef| format!("{} {}", d.fuse_time, d.cook as u8),
        |d: &mut WeaponDef, v: Values| {
            v.need(2)?;
            (d.fuse_time, d.cook) = (v.num(0), v.flag(1));
            Ok(())
        }),
    field!("Explosion", "Blast radius, inner radius, inner damage, outer damage.", FieldKind::Nums(4),
        |d: &WeaponDef| format!("{} {} {} {}", d.explosion_radius, d.explosion_radius_min, d.explosion_inner_damage, d.explosion_outer_damage),
        |d: &mut WeaponDef, v: Values| {
            v.need(4)?;
            (d.explosion_radius, d.explosion_radius_min, d.explosion_inner_damage, d.explosion_outer_damage) =
                (v.num(0), v.num(1), v.num(2), v.num(3));
            Ok(())
        }),
    field!("Projectile", "Speed, speed when aimed up, gravity, explode on impact, sticky.", FieldKind::Nums(5),
        |d: &WeaponDef| format!("{} {} {} {} {}", d.projectile_speed, d.projectile_speed_up, d.projectile_gravity as u8, d.impact_explode as u8, d.sticky as u8),
        |d: &mut WeaponDef, v: Values| {
            v.need(5)?;
            d.projectile_speed = v.num(0);
            d.projectile_speed_up = v.num(1);
            d.projectile_gravity = v.flag(2);
            d.impact_explode = v.flag(3);
            d.sticky = v.flag(4);
            Ok(())
        }),
    field!("FxWeapon", "Zero Hour assets made for this weapon by the converter (empty when absent).", FieldKind::Str,
        |d: &WeaponDef| d.fx_weapon.clone(),
        |d: &mut WeaponDef, v: Values| { d.fx_weapon = v.text.into(); Ok(()) }),
    field!("ExplosionFx", "Zero Hour FXList for the blast (grenades, rockets).", FieldKind::Str,
        |d: &WeaponDef| d.explosion_fx.clone(),
        |d: &mut WeaponDef, v: Values| { d.explosion_fx = v.text.into(); Ok(()) }),
    field!("ProjectileObject", "The thrown/projectiled Zero Hour object.", FieldKind::Str,
        |d: &WeaponDef| d.projectile_object.clone(),
        |d: &mut WeaponDef, v: Values| { d.projectile_object = v.text.into(); Ok(()) }),
    field!("FireSound", "The shot's BO2 sound event.", FieldKind::Str,
        |d: &WeaponDef| d.fire_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.fire_sound = v.text.into(); Ok(()) }),
    field!("ExplodeSound", "The blast's BO2 sound event.", FieldKind::Str,
        |d: &WeaponDef| d.explode_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.explode_sound = v.text.into(); Ok(()) }),
    field!("Viewmodel", "Viewmodel index in the converted art, or -1.", FieldKind::Num,
        |d: &WeaponDef| d.viewmodel.to_string(),
        |d: &mut WeaponDef, v: Values| { d.viewmodel = v.text.parse().map_err(|_| "number")?; Ok(()) }),
    field!("Anims", "Which BO2 viewmodel animation slots the weapon has, as hex bits.", FieldKind::Hex,
        |d: &WeaponDef| format!("{:x}", d.anims),
        |d: &mut WeaponDef, v: Values| { d.anims = u128::from_str_radix(v.text, 16).map_err(|_| "hex")?; Ok(()) }),
    field!("ClipBase", "Viewmodel clip base (an alt mode's clips sit after its weapon's).", FieldKind::Num,
        |d: &WeaponDef| d.clip_base.to_string(),
        |d: &mut WeaponDef, v: Values| { d.clip_base = v.text.parse().map_err(|_| "number")?; Ok(()) }),
    field!("WorldMeshes", "Third-person meshes on the soldier model (`W{vm}{k}`).", FieldKind::Num,
        |d: &WeaponDef| d.world_meshes.to_string(),
        |d: &mut WeaponDef, v: Values| { d.world_meshes = v.text.parse().map_err(|_| "number")?; Ok(()) }),
    field!("Attachments", "The attachment shorts on this variant, sorted.", FieldKind::Words,
        |d: &WeaponDef| d.attachments.join(" "),
        |d: &mut WeaponDef, v: Values| { d.attachments = v.text.split_whitespace().map(str::to_string).collect(); Ok(()) }),
    field!("Allowed", "Base weapons: each attachment BO2 offers (`short=attachment`).", FieldKind::Pairs,
        |d: &WeaponDef| pairs(&d.allowed),
        |d: &mut WeaponDef, v: Values| { d.allowed = parse_pairs(v.text); Ok(()) }),
    field!("AltModes", "Base weapons: the alt-mode weapon an attachment adds (`short=alt`).", FieldKind::Pairs,
        |d: &WeaponDef| pairs(&d.alt_modes),
        |d: &mut WeaponDef, v: Values| { d.alt_modes = parse_pairs(v.text); Ok(()) }),
    field!("Alt", "The weapon this toggles to (its alt mode, or back from it).", FieldKind::Str,
        |d: &WeaponDef| d.alt.clone(),
        |d: &mut WeaponDef, v: Values| { d.alt = v.text.into(); Ok(()) }),
    field!("Flags", "alt keeps its own ammo, silenced, laser, FMJ, target finder.", FieldKind::Nums(5),
        |d: &WeaponDef| format!("{} {} {} {} {}", d.alt_own_ammo as u8, d.silenced as u8, d.laser as u8, d.fmj as u8, d.target_finder as u8),
        |d: &mut WeaponDef, v: Values| {
            v.need(5)?;
            d.alt_own_ammo = v.flag(0);
            d.silenced = v.flag(1);
            d.laser = v.flag(2);
            d.fmj = v.flag(3);
            d.target_finder = v.flag(4);
            Ok(())
        }),
    field!("RecoilReduction", "Per-shot recoil reduction and its limit (the grip).", FieldKind::Nums(2),
        |d: &WeaponDef| format!("{} {}", d.recoil_reduction.0, d.recoil_reduction.1),
        |d: &mut WeaponDef, v: Values| {
            v.need(2)?;
            d.recoil_reduction = (v.num(0), v.num(1));
            Ok(())
        }),
    field!("Place", "How the equipment is used: throw, plant, hack.", FieldKind::Enum,
        |d: &WeaponDef| d.place.name().into(),
        |d: &mut WeaponDef, v: Values| { d.place = Place::parse(v.text).ok_or("unknown placement")?; Ok(()) }),
    field!("Equipment", "What the weapon is as equipment once down: claymore, betty, c4, shock, trophy, insertion, sensor, or none.", FieldKind::Enum,
        |d: &WeaponDef| d.equipment_kind.name().into(),
        |d: &mut WeaponDef, v: Values| { d.equipment_kind = Kind::parse(v.text).ok_or("unknown equipment kind")?; Ok(()) }),
    field!("Specialties", "The specialties the weapon grants while in hand (`WeaponVariantDef.perks`, with its attachments'), by name.", FieldKind::Words,
        |d: &WeaponDef| d.specialties.format(),
        |d: &mut WeaponDef, v: Values| { d.specialties = Specialties::parse(v.text)?; Ok(()) }),
    field!("ViewKickMin", "The shortest view kick, aimed and hip (`f{Ads,Hip}ViewKickMinMagnitude`).", FieldKind::Nums(2),
        |d: &WeaponDef| nums(&[d.view_kick_min.0, d.view_kick_min.1]),
        |d: &mut WeaponDef, v: Values| { v.need(2)?; d.view_kick_min = (v.num(0), v.num(1)); Ok(()) }),
    field!("ViewKickCenter", "View kick recentre speed, aimed and hip (`f{Ads,Hip}ViewKickCenterSpeed`).", FieldKind::Nums(2),
        |d: &WeaponDef| nums(&[d.view_kick_center.0, d.view_kick_center.1]),
        |d: &mut WeaponDef, v: Values| { v.need(2)?; d.view_kick_center = (v.num(0), v.num(1)); Ok(()) }),
    field!("AdsGunKick", "Aimed gun kick: pitch min/max, yaw min/max, accel, speed max, speed decay, static decay, reduced-kick bullets and percent.", FieldKind::Nums(10),
        |d: &WeaponDef| d.ads_gun_kick.format(),
        |d: &mut WeaponDef, v: Values| { v.need(10)?; d.ads_gun_kick = GunKick::from_nums(v.nums); Ok(()) }),
    field!("HipGunKick", "Hip gun kick, as AdsGunKick.", FieldKind::Nums(10),
        |d: &WeaponDef| d.hip_gun_kick.format(),
        |d: &mut WeaponDef, v: Values| { v.need(10)?; d.hip_gun_kick = GunKick::from_nums(v.nums); Ok(()) }),
    field!("GunMax", "The gun kick's pitch and yaw limits (`fGunMaxPitch`, `fGunMaxYaw`).", FieldKind::Nums(2),
        |d: &WeaponDef| nums(&[d.gun_max.0, d.gun_max.1]),
        |d: &mut WeaponDef, v: Values| { v.need(2)?; d.gun_max = (v.num(0), v.num(1)); Ok(()) }),
    field!("RecoilReturn", "How fast the grip's recoil reduction wears off (`fAdsRecoilReturnRate`).", FieldKind::Num,
        |d: &WeaponDef| d.recoil_return_rate.to_string(),
        |d: &mut WeaponDef, v: Values| { d.recoil_return_rate = v.text_num()?; Ok(()) }),
    field!("HipSway", "Hip sway: max angle, lerp speed, pitch, yaw, horizontal and vertical scales.", FieldKind::Nums(6),
        |d: &WeaponDef| d.hip_sway.format(),
        |d: &mut WeaponDef, v: Values| { v.need(6)?; d.hip_sway = Sway::from_nums(v.nums); Ok(()) }),
    field!("AdsSway", "Aimed sway, as HipSway (BO2 has no aimed horizontal/vertical scales).", FieldKind::Nums(6),
        |d: &WeaponDef| d.ads_sway.format(),
        |d: &mut WeaponDef, v: Values| { v.need(6)?; d.ads_sway = Sway::from_nums(v.nums); Ok(()) }),
    field!("ViewStand", "Viewmodel move offset and rotation standing (`vStandMove`, `vStandRot`).", FieldKind::Nums(6),
        |d: &WeaponDef| { let p = &d.view_pos; nums(&[p.stand_move, p.stand_rot].concat()) },
        |d: &mut WeaponDef, v: Values| { v.need(6)?; let p = &mut d.view_pos; (p.stand_move, p.stand_rot) = (v3(v.nums), v3(&v.nums[3..])); Ok(()) }),
    field!("ViewDucked", "Crouched offset, move and rotation (`vDuckedOfs`, `vDuckedMove`, `vDuckedRot`).", FieldKind::Nums(9),
        |d: &WeaponDef| { let p = &d.view_pos; nums(&[p.ducked_ofs, p.ducked_move, p.ducked_rot].concat()) },
        |d: &mut WeaponDef, v: Values| { v.need(9)?; let p = &mut d.view_pos; (p.ducked_ofs, p.ducked_move, p.ducked_rot) = (v3(v.nums), v3(&v.nums[3..]), v3(&v.nums[6..])); Ok(()) }),
    field!("ViewProne", "Prone offset, move and rotation (`vProneOfs`, `vProneMove`, `vProneRot`).", FieldKind::Nums(9),
        |d: &WeaponDef| { let p = &d.view_pos; nums(&[p.prone_ofs, p.prone_move, p.prone_rot].concat()) },
        |d: &mut WeaponDef, v: Values| { v.need(9)?; let p = &mut d.view_pos; (p.prone_ofs, p.prone_move, p.prone_rot) = (v3(v.nums), v3(&v.nums[3..]), v3(&v.nums[6..])); Ok(()) }),
    field!("ViewStrafe", "Strafe move and rotation (`vStrafeMove`, `vStrafeRot`).", FieldKind::Nums(6),
        |d: &WeaponDef| { let p = &d.view_pos; nums(&[p.strafe_move, p.strafe_rot].concat()) },
        |d: &mut WeaponDef, v: Values| { v.need(6)?; let p = &mut d.view_pos; (p.strafe_move, p.strafe_rot) = (v3(v.nums), v3(&v.nums[3..])); Ok(()) }),
    field!("ViewSprint", "Sprint offset, rotation and bob (`vSprintOfs`, `vSprintRot`, `vSprintBob`).", FieldKind::Nums(8),
        |d: &WeaponDef| { let p = &d.view_pos; nums(&[&p.sprint_ofs[..], &p.sprint_rot[..], &p.sprint_bob[..]].concat()) },
        |d: &mut WeaponDef, v: Values| {
            v.need(8)?;
            let p = &mut d.view_pos;
            (p.sprint_ofs, p.sprint_rot, p.sprint_bob) = (v3(v.nums), v3(&v.nums[3..]), [v.num(6), v.num(7)]);
            Ok(())
        }),
    field!("ViewRates", "Position and rotation blend rates (standing, prone) and the stand/crouch/prone move min speeds.", FieldKind::Nums(7),
        |d: &WeaponDef| { let p = &d.view_pos; nums(&[p.move_rate, p.prone_move_rate, p.rot_rate, p.prone_rot_rate, p.stand_min_speed, p.ducked_min_speed, p.prone_min_speed]) },
        |d: &mut WeaponDef, v: Values| {
            v.need(7)?;
            let p = &mut d.view_pos;
            (p.move_rate, p.prone_move_rate, p.rot_rate, p.prone_rot_rate) = (v.num(0), v.num(1), v.num(2), v.num(3));
            (p.stand_min_speed, p.ducked_min_speed, p.prone_min_speed) = (v.num(4), v.num(5), v.num(6));
            Ok(())
        }),
    field!("ViewAds", "Aimed pitch offset and bob factor (`fAdsAimPitch`, `fAdsBobFactor`).", FieldKind::Nums(2),
        |d: &WeaponDef| nums(&[d.view_pos.ads_aim_pitch, d.view_pos.ads_bob_factor]),
        |d: &mut WeaponDef, v: Values| { v.need(2)?; (d.view_pos.ads_aim_pitch, d.view_pos.ads_bob_factor) = (v.num(0), v.num(1)); Ok(()) }),
    field!("Reticle", "Crosshair: centre and side size, min offset, side position, ADS in/out fade fractions, keep when aimed, colour change.", FieldKind::Nums(8),
        |d: &WeaponDef| d.reticle.format(),
        |d: &mut WeaponDef, v: Values| { v.need(8)?; d.reticle.set_nums(v.nums); Ok(()) }),
    field!("ReticleArt", "The crosshair's centre and side materials (`-` for none).", FieldKind::Words,
        |d: &WeaponDef| d.reticle.format_art(),
        |d: &mut WeaponDef, v: Values| { d.reticle.set_art(v.text); Ok(()) }),
];

/// Apply one schema line to `t`. Unknown keys are tolerated (the parser skips them).
pub fn set_field<T>(fields: &[Field<T>], t: &mut T, key: &str, values: Values) -> Result<(), String> {
    for f in fields {
        if f.key == key {
            return (f.set)(t, values);
        }
    }
    Ok(())
}

/// Append one block's lines.
pub fn write_block<T>(out: &mut String, header: &str, id: &str, fields: &[Field<T>], t: &T) {
    out.push_str(header);
    out.push_str(id);
    out.push('\n');
    for f in fields {
        out.push_str(&format!("  {} = {}\n", f.key, (f.get)(t)));
    }
    out.push_str("End\n\n");
}

/// The keys a block's formatting writes, in order (tests walk this to prove coverage).
pub fn keys<T>(fields: &[Field<T>]) -> impl Iterator<Item = &'static str> + '_ {
    fields.iter().map(|f| f.key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_keys_are_unique_and_ordered() {
        // The table format is positional: a duplicated key within a block would make parse
        // ambiguity, and the order here is what the converter has always written. (Weapon and
        // Attachment blocks are separate namespaces: they may share keys.)
        fn check<T>(fields: &[Field<T>]) {
            let mut seen = std::collections::HashSet::new();
            for k in keys(fields) {
                assert!(seen.insert(k), "duplicate key {k}");
            }
        }
        check(WEAPON_FIELDS);
        check(ATTACHMENT_FIELDS);
        assert_eq!(WEAPON_FIELDS.len(), 64);
        assert_eq!(ATTACHMENT_FIELDS.len(), 14);
    }

    #[test]
    fn equipment_block_round_trips() {
        let mut d = WeaponDef { id: "claymore_mp".into(), equipment_kind: Kind::Claymore, ..Default::default() };
        let text = format_table(&[d.clone()]);
        assert!(text.contains("Equipment = claymore"));
        let back = parse_table(&text).unwrap();
        assert_eq!(back[0].equipment_kind, Kind::Claymore);
        d.equipment_kind = Kind::Other;
        let text = format_table(&[d]);
        assert!(text.contains("Equipment = none"));
        assert_eq!(parse_table(&text).unwrap()[0].equipment_kind, Kind::Other);
    }

    #[test]
    fn specialties_and_perk_rows_round_trip() {
        use super::super::specialty::Specialty;
        let d = WeaponDef { id: "mp7_mp+fastmag".into(), specialties: Specialties::of(&[Specialty::FastReload]), ..Default::default() };
        let a = AttachmentDef { id: "rf".into(), specialties: Specialties::of(&[Specialty::Rof]), ..Default::default() };
        let t = Table {
            weapons: vec![d],
            attachments: vec![a],
            perks: vec![("LIGHTWEIGHT".into(), Specialties::of(&[Specialty::MoveFaster, Specialty::FallHeight]))],
            ..Default::default()
        };
        let text = format_full_table(&t);
        assert!(text.contains("Perk LIGHTWEIGHT\n  Specialties = fallheight movefaster\nEnd"), "{text}");
        assert_eq!(parse_full_table(&text).unwrap(), t);
    }
}