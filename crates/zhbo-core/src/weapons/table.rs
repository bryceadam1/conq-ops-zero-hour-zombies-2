/*
** Conquer Ops - Zero Hour gameplay core: the weapon table: BO2's WeaponDef rows as `key = value` text, and the schema it is generated from.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::def::{BonusCard, DropRules, CamoOption, Class, OptionTab, Effect, FireMode, Inventory, Kind, Kick, PerkRow, Place, ShieldBox, ShieldGeometry, UiText, WeapClass, WeaponDef};
use super::feel::{nums, v3, GunKick, Sway};
use super::handling::{OffhandClass, Penetrate, WeapType};
use super::specialty::Specialties;
use crate::attachments::AttachmentDef;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    pub weapons: Vec<WeaponDef>,
    pub attachments: Vec<AttachmentDef>,
    /// BO2's camo options (`Camo <id>` blocks): the screen's grid cells with their names and
    /// thumbnails (`mp/attachmenttable.csv`'s `camo` rows).
    pub camo: Vec<CamoOption>,
    /// BO2's reticle options (`ReticleOption <id>` blocks): the base `reticle_0..10` rows.
    pub reticles: Vec<CamoOption>,
    /// BO2's reticle colours (`ReticleColorOption <id>` blocks): the base `reticle_color_1..7` rows.
    pub reticle_colors: Vec<CamoOption>,
    /// Third-person meshes of the scorestreak tablet on the soldier model (`TB{k:02}`).
    pub tablet_meshes: u8,
    /// BO2's perk rows (`mp/statstable.csv`'s `specialty` rows, `Perk <ref>` blocks): keyed by the
    /// reference without `PERKS_` (`LIGHTWEIGHT`, `NINJA`), with the localized name and description
    /// and the specialties the perk grants.
    pub perks: Vec<PerkRow>,
    /// BO2's bonus cards (`mp/statstable.csv`'s `bonuscard` rows, `BonusCard <ref>` blocks), in the
    /// table's row order.
    pub bonus_cards: Vec<BonusCard>,
    /// BO2's localized UI strings the core draws (`UiText <key>` blocks, [`UiText`]).
    pub ui_text: Vec<UiText>,
    /// BO2's Create-a-Class weapon groups in the order its weapon-type list adds them
    /// (`WeaponGroups = weapon_smg weapon_assault …`, `cacweaponslot.lua`); empty in a table from
    /// before them.
    pub weapon_groups: Vec<String>,
    /// `CoD.CACUtility` class caps the core reads from data (`ClassCap.maxPerks = 12`,
    /// [`crate::classes::DATA_CAPS`]), in the order written.
    pub class_caps: Vec<(String, i32)>,
    /// The soldier's hit boxes per body state (`HitBox = …` lines, [`crate::hitloc::HitBox`]).
    pub hit_boxes: Vec<crate::hitloc::HitBox>,
    /// BO2's shellshocks the mod plays (`Shock <name>` blocks, [`crate::shellshock::ShockDef`]).
    pub shocks: Vec<crate::shellshock::ShockDef>,
    /// The DLC packs the converting install has (`InstalledDlc = dlc0 dlc1`, each pack's
    /// `zone/all/<dlc>_load_mp.ff`): shown in the lobby, never in the multiplayer fingerprint
    /// ([`crate::content`]: a DLC item is the owner's, the sim is the same without it).
    pub installed_dlc: Vec<String>,
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
        FireMode::Stacked => "stacked".to_string(),
    }
}

pub(crate) fn parse_fire(value: &str) -> Option<FireMode> {
    let mut words = value.split_whitespace();
    Some(match words.next()? {
        "auto" => FireMode::Auto,
        "semi" => FireMode::Semi,
        "minigun" => FireMode::Minigun,
        "stacked" => FireMode::Stacked,
        "burst" => FireMode::Burst(words.next().and_then(|n| n.parse().ok()).unwrap_or(3)),
        _ => return None,
    })
}

/// An optional number: the number, or `-` for none (an attachment field equal to
/// `defaultattachment`'s, which changes nothing).
pub(crate) fn opt_num(x: Option<f32>) -> String {
    x.map_or_else(|| "-".to_string(), |v| v.to_string())
}

/// Parse `n` optional numbers (`-` for none).
pub(crate) fn parse_opt_nums(text: &str, n: usize) -> Result<Vec<Option<f32>>, String> {
    let out: Vec<Option<f32>> = text
        .split_whitespace()
        .map(|w| if w == "-" { Ok(None) } else { w.parse::<f32>().map(Some).map_err(|_| "number or -".to_string()) })
        .collect::<Result<_, _>>()?;
    if out.len() < n {
        return Err("too few values".into());
    }
    Ok(out)
}

/// Write the table in the format [`parse_table`] reads (Zero Hour INI style: `Weapon <id>` and
/// `Attachment <id>` blocks of `key = value` lines, each closed by `End`).
pub fn format_table(defs: &[WeaponDef]) -> String {
    format_full_table(&Table { weapons: defs.to_vec(), ..Default::default() })
}

pub fn format_full_table(t: &Table) -> String {
    let mut s = String::from("; Black Ops II multiplayer weapons, converted from the player's own install by zhbo-assets.\n; Private data: do not redistribute.\n\n");
    s.push_str(&format!("TabletMeshes = {}\n", t.tablet_meshes));
    for b in &t.hit_boxes {
        s.push_str(&format!("HitBox = {}\n", b.format()));
    }
    if !t.weapon_groups.is_empty() {
        s.push_str(&format!("WeaponGroups = {}\n", t.weapon_groups.join(" ")));
    }
    if !t.installed_dlc.is_empty() {
        s.push_str(&format!("InstalledDlc = {}\n", t.installed_dlc.join(" ")));
    }
    for (name, value) in &t.class_caps {
        s.push_str(&format!("ClassCap.{name} = {value}\n"));
    }
    s.push('\n');
    for p in &t.perks {
        write_block(&mut s, "Perk ", &p.stats, PERK_FIELDS, p);
    }
    for b in &t.bonus_cards {
        write_block(&mut s, "BonusCard ", &b.reference, BONUS_CARD_FIELDS, b);
    }
    for u in &t.ui_text {
        write_block(&mut s, "UiText ", &u.key, UI_TEXT_FIELDS, u);
    }
    for k in &t.shocks {
        write_block(&mut s, "Shock ", &k.name, crate::shellshock::SHOCK_FIELDS, k);
    }
    for a in &t.attachments {
        write_block(&mut s, "Attachment ", &a.id, ATTACHMENT_FIELDS, a);
    }
    for c in &t.camo {
        write_block(&mut s, "Camo ", &c.id, CAMO_FIELDS, c);
    }
    for r in &t.reticles {
        write_block(&mut s, "ReticleOption ", &r.id, CAMO_FIELDS, r);
    }
    for c in &t.reticle_colors {
        write_block(&mut s, "ReticleColorOption ", &c.id, CAMO_FIELDS, c);
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
        Camo(Box<CamoOption>),
        Reticle(Box<CamoOption>),
        ReticleColor(Box<CamoOption>),
        Perk(Box<PerkRow>),
        BonusCard(Box<BonusCard>),
        UiText(Box<UiText>),
        Shock(Box<crate::shellshock::ShockDef>),
    }
    let mut table = Table::default();
    let mut cur: Option<Block> = None;
    for (n, raw) in text.lines().enumerate() {
        let line = strip_comment(raw).trim();
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
        if let Some(id) = line.strip_prefix("Camo ") {
            cur = Some(Block::Camo(Box::new(CamoOption { id: id.trim().to_string(), ..Default::default() })));
            continue;
        }
        if let Some(id) = line.strip_prefix("ReticleOption ") {
            cur = Some(Block::Reticle(Box::new(CamoOption { id: id.trim().to_string(), ..Default::default() })));
            continue;
        }
        if let Some(id) = line.strip_prefix("ReticleColorOption ") {
            cur = Some(Block::ReticleColor(Box::new(CamoOption { id: id.trim().to_string(), ..Default::default() })));
            continue;
        }
        if let Some(id) = line.strip_prefix("BonusCard ") {
            cur = Some(Block::BonusCard(Box::new(BonusCard { reference: id.trim().to_string(), ..Default::default() })));
            continue;
        }
        if let Some(key) = line.strip_prefix("UiText ") {
            cur = Some(Block::UiText(Box::new(UiText { key: key.trim().to_string(), ..Default::default() })));
            continue;
        }
        if let Some(name) = line.strip_prefix("Shock ") {
            cur = Some(Block::Shock(Box::new(crate::shellshock::ShockDef { name: name.trim().to_string(), ..Default::default() })));
            continue;
        }
        if let Some(id) = line.strip_prefix("Perk ") {
            cur = Some(Block::Perk(Box::new(PerkRow { stats: id.trim().to_string(), ..Default::default() })));
            continue;
        }
        if line == "End" {
            match cur.take().ok_or_else(|| err("End without a block"))? {
                Block::Weapon(w) => table.weapons.push(*w),
                Block::Attachment(a) => table.attachments.push(*a),
                Block::Camo(c) => table.camo.push(*c),
                Block::Reticle(r) => table.reticles.push(*r),
                Block::ReticleColor(c) => table.reticle_colors.push(*c),
                Block::Perk(p) => table.perks.push(*p),
                Block::BonusCard(b) => table.bonus_cards.push(*b),
                Block::UiText(u) => table.ui_text.push(*u),
                Block::Shock(k) => table.shocks.push(*k),
            }
            continue;
        }
        let (key, value) = line.split_once('=').ok_or_else(|| err("expected key = value"))?;
        let (key, value) = (key.trim(), unquote(value.trim()));
        let nums: Vec<f32> = value.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        let values = Values { text: &value, nums: &nums };
        match cur.as_mut() {
            None => match key {
                "TabletMeshes" => table.tablet_meshes = values.text_num().map_err(|e| err(&e))? as u8,
                "WeaponGroups" => table.weapon_groups = value.split_whitespace().map(str::to_string).collect(),
                "InstalledDlc" => table.installed_dlc = value.split_whitespace().map(str::to_string).collect(),
                k if k.starts_with("ClassCap.") => {
                    let n = values.text_num().map_err(|e| err(&e))? as i32;
                    table.class_caps.push((k["ClassCap.".len()..].to_string(), n));
                }
                "HitBox" => table.hit_boxes.push(crate::hitloc::HitBox::parse(&value).map_err(|e| err(&e))?),
                _ => return Err(err("value outside a block")),
            },
            Some(Block::Attachment(a)) => set_field(ATTACHMENT_FIELDS, a, key, values).map_err(|e| err(&e))?,
            Some(Block::Camo(c)) => set_field(CAMO_FIELDS, c, key, values).map_err(|e| err(&e))?,
            Some(Block::Reticle(r)) => set_field(CAMO_FIELDS, r, key, values).map_err(|e| err(&e))?,
            Some(Block::ReticleColor(c)) => set_field(CAMO_FIELDS, c, key, values).map_err(|e| err(&e))?,
            Some(Block::Perk(p)) => set_field(PERK_FIELDS, p, key, values).map_err(|e| err(&e))?,
            Some(Block::BonusCard(b)) => set_field(BONUS_CARD_FIELDS, b, key, values).map_err(|e| err(&e))?,
            Some(Block::UiText(u)) => set_field(UI_TEXT_FIELDS, u, key, values).map_err(|e| err(&e))?,
            Some(Block::Shock(k)) => set_field(crate::shellshock::SHOCK_FIELDS, k, key, values).map_err(|e| err(&e))?,
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
    /// A string, verbatim (quoted when it has surrounding whitespace or a `;`, [`quote`]).
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

/// Whether a field reaches the deterministic sim (the logic step) or only one machine's
/// presentation. Only [`Role::Sim`] fields are in the multiplayer content fingerprint
/// ([`crate::content`]): two players whose tables differ in a presentation field (a localized
/// name, a sound alias, the art a DLC pack adds) still play together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Read by code a logic step reaches; every peer must hold the same value. The default: a new
    /// field is in the fingerprint until it is shown not to reach the sim.
    Sim,
    /// Read only by the local client, the HUD, the class screens, the renderer or the converter
    /// (`content::tests` scans the sim's sources for the members these set).
    Presentation,
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
    /// Sim or presentation ([`Role`]).
    pub role: Role,
}

macro_rules! field {
    (present $key:literal, $doc:literal, $kind:expr, $get:expr, $set:expr) => {
        Field { key: $key, doc: $doc, kind: $kind, get: $get, set: $set, role: Role::Presentation }
    };
    ($key:literal, $doc:literal, $kind:expr, $get:expr, $set:expr) => {
        Field { key: $key, doc: $doc, kind: $kind, get: $get, set: $set, role: Role::Sim }
    };
}

/// Every `Attachment` block line, in table order.
pub const ATTACHMENT_FIELDS: &[Field<AttachmentDef>] = &[
    field!("Short", "The attachment's two-letter short (`gl`, `reflex`).", FieldKind::Str,
        |a: &AttachmentDef| a.short.clone(),
        |a: &mut AttachmentDef, v: Values| { a.short = v.text.into(); Ok(()) }),
    field!(present "DisplayName", "As BO2's create-a-class shows it.", FieldKind::Str,
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
    field!("MmsMaxDist", "Millimetre scanner's max distance (BO2 inches; the scanner flag is in Traits).", FieldKind::Num,
        |a: &AttachmentDef| a.mms_max_dist.to_string(),
        |a: &mut AttachmentDef, v: Values| { a.mms_max_dist = v.text_num()?; Ok(()) }),
    field!("Fire", "The fire mode it forces (`none` keeps the weapon's).", FieldKind::Enum,
        |a: &AttachmentDef| a.fire.map_or("none".into(), fire_name),
        |a: &mut AttachmentDef, v: Values| { a.fire = parse_fire(v.text); Ok(()) }),
    field!("Clip", "Rounds in the magazine when this optic or mag is on (0 keeps the weapon's).", FieldKind::Num,
        |a: &AttachmentDef| a.clip.to_string(),
        |a: &mut AttachmentDef, v: Values| { a.clip = v.text_num()? as u16; Ok(()) }),
    field!("DamageRangeScale", "Scales the damage curve's ranges.", FieldKind::Num,
        |a: &AttachmentDef| a.damage_range_scale.to_string(),
        |a: &mut AttachmentDef, v: Values| { a.damage_range_scale = v.text_num()?; Ok(()) }),
    field!("Ads", "ADS field of view, in/out scale, and the ADS move scale that replaces the weapon's (0 keeps it).", FieldKind::Nums(4),
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
    field!(present "FullName", "The full name BO2's attachment menu's lower title shows.", FieldKind::Str,
        |a: &AttachmentDef| a.full_name.clone(),
        |a: &mut AttachmentDef, v: Values| { a.full_name = v.text.into(); Ok(()) }),
    field!(present "Description", "The attachment's description line.", FieldKind::Str,
        |a: &AttachmentDef| a.description.clone(),
        |a: &mut AttachmentDef, v: Values| { a.description = v.text.into(); Ok(()) }),
    field!(present "Image", "Its create-a-class icon material (`mp/attachmenttable.csv` column 6).", FieldKind::Str,
        |a: &AttachmentDef| a.image.clone(),
        |a: &mut AttachmentDef, v: Values| { a.image = v.text.into(); Ok(()) }),
    field!("Kind", "BO2's `eAttachment`: the order a combination's attachments are walked in.", FieldKind::Num,
        |a: &AttachmentDef| a.kind.to_string(),
        |a: &mut AttachmentDef, v: Values| { a.kind = v.text_num()? as u8; Ok(()) }),
    field!("QuickReloadScale", "Scales the quick reload, quick empty reload and quick add times.", FieldKind::Nums(3),
        |a: &AttachmentDef| nums(&[a.reload_quick_scale.0, a.reload_quick_scale.1, a.reload_quick_scale.2]),
        |a: &mut AttachmentDef, v: Values| { v.need(3)?; a.reload_quick_scale = (v.num(0), v.num(1), v.num(2)); Ok(()) }),
    field!("IdleAmount", "Aimed idle-sway scale, and the hip idle amount (`-`: unchanged).", FieldKind::Words,
        |a: &AttachmentDef| format!("{} {}", a.ads_idle_scale, opt_num(a.hip_idle)),
        |a: &mut AttachmentDef, v: Values| {
            let o = parse_opt_nums(v.text, 2)?;
            a.ads_idle_scale = o[0].ok_or("idle scale")?;
            a.hip_idle = o[1];
            Ok(())
        }),
    field!("AdsZoom", "Zoom levels 2 and 3, zoom in and out fractions (`-`: unchanged).", FieldKind::Words,
        |a: &AttachmentDef| [a.fov2, a.fov3, a.zoom_in_frac, a.zoom_out_frac].map(opt_num).join(" "),
        |a: &mut AttachmentDef, v: Values| {
            let o = parse_opt_nums(v.text, 4)?;
            (a.fov2, a.fov3, a.zoom_in_frac, a.zoom_out_frac) = (o[0], o[1], o[2], o[3]);
            Ok(())
        }),
    field!("AltScopeAds", "The second optic's ADS in and out times, seconds (`-`: unchanged).", FieldKind::Words,
        |a: &AttachmentDef| [a.alt_scope_in_time, a.alt_scope_out_time].map(opt_num).join(" "),
        |a: &mut AttachmentDef, v: Values| {
            let o = parse_opt_nums(v.text, 2)?;
            (a.alt_scope_in_time, a.alt_scope_out_time) = (o[0], o[1]);
            Ok(())
        }),
    field!("StackFire", "Stack-fire spread and accuracy decay (0: unchanged).", FieldKind::Nums(2),
        |a: &AttachmentDef| nums(&[a.stack_fire.0, a.stack_fire.1]),
        |a: &mut AttachmentDef, v: Values| { v.need(2)?; a.stack_fire = (v.num(0), v.num(1)); Ok(()) }),
    field!("Penetrate", "BO2's penetration type: none, small, medium, large (`none`: unchanged).", FieldKind::Enum,
        |a: &AttachmentDef| a.penetrate.name().into(),
        |a: &mut AttachmentDef, v: Values| { a.penetrate = Penetrate::parse(v.text).ok_or("unknown penetration")?; Ok(()) }),
    field!("Traits", "dual magazine, infrared, millimetre-wave scanner, use as melee, alt mode ADS only, alt switching disabled.", FieldKind::Nums(6),
        |a: &AttachmentDef| [a.dual_mag, a.infrared, a.mms, a.use_as_melee, a.alt_ads_only, a.alt_disable_switching].map(|b| (b as u8).to_string()).join(" "),
        |a: &mut AttachmentDef, v: Values| {
            v.need(6)?;
            (a.dual_mag, a.infrared, a.mms, a.use_as_melee, a.alt_ads_only, a.alt_disable_switching) = (v.flag(0), v.flag(1), v.flag(2), v.flag(3), v.flag(4), v.flag(5));
            Ok(())
        }),
    field!("AdsKickCenterScale", "Scales the aimed view-kick recentre speed (`fAdsViewKickCenterSpeedScale`).", FieldKind::Num,
        |a: &AttachmentDef| a.ads_kick_center_scale.to_string(),
        |a: &mut AttachmentDef, v: Values| { a.ads_kick_center_scale = v.text_num()?; Ok(()) }),
    field!(present "SwayOverride", "Hip sway override: max angle, lerp speed, pitch, yaw, horizontal, vertical scales (`-`: none).", FieldKind::Words,
        |a: &AttachmentDef| opt_sway(a.sway),
        |a: &mut AttachmentDef, v: Values| { a.sway = parse_opt_sway(v)?; Ok(()) }),
    field!(present "AdsSwayOverride", "Aimed sway override, as SwayOverride (`-`: none).", FieldKind::Words,
        |a: &AttachmentDef| opt_sway(a.ads_sway),
        |a: &mut AttachmentDef, v: Values| { a.ads_sway = parse_opt_sway(v)?; Ok(()) }),
    field!(present "ViewPosOverride", "Stand move forward, stand rotation pitch and roll, strafe rotation roll (`-`: unchanged).", FieldKind::Words,
        |a: &AttachmentDef| [a.stand_move_f, a.stand_rot_pitch, a.stand_rot_roll, a.strafe_rot_roll].map(opt_num).join(" "),
        |a: &mut AttachmentDef, v: Values| {
            let o = parse_opt_nums(v.text, 4)?;
            (a.stand_move_f, a.stand_rot_pitch, a.stand_rot_roll, a.strafe_rot_roll) = (o[0], o[1], o[2], o[3]);
            Ok(())
        }),
    field!("Compatible", "The shorts it may be combined with (BO2's `compatibleAttachments`; pairing needs both lists).", FieldKind::Words,
        |a: &AttachmentDef| a.compatible.join(" "),
        |a: &mut AttachmentDef, v: Values| { a.compatible = v.text.split_whitespace().map(str::to_string).collect(); Ok(()) }),
];

/// An optional sway block: its six numbers, or `-` for none.
fn opt_sway(s: Option<Sway>) -> String {
    s.map_or_else(|| "-".to_string(), |s| s.format())
}

fn parse_opt_sway(v: Values) -> Result<Option<Sway>, String> {
    if v.text == "-" {
        return Ok(None);
    }
    v.need(6)?;
    Ok(Some(Sway::from_nums(v.nums)))
}

/// Every `Weapon` block line, in table order.
pub const WEAPON_FIELDS: &[Field<WeaponDef>] = &[
    field!(present "DisplayName", "As BO2's HUD shows it.", FieldKind::Str,
        |d: &WeaponDef| d.display_name.clone(),
        |d: &mut WeaponDef, v: Values| { d.display_name = v.text.into(); Ok(()) }),
    field!("Class", "The weapon class (smg, ar, sniper, launcher, pistol, melee, offhand).", FieldKind::Enum,
        |d: &WeaponDef| d.class.name().into(),
        |d: &mut WeaponDef, v: Values| { d.class = Class::parse(v.text).ok_or("unknown class")?; Ok(()) }),
    field!("WeapClass", "BO2's weapClass_t: rifle, mg, smg, spread, pistol, grenade, rocketlauncher, …, pistol_spread.", FieldKind::Enum,
        |d: &WeaponDef| d.weap_class.name().into(),
        |d: &mut WeaponDef, v: Values| { d.weap_class = WeapClass::parse(v.text).ok_or("unknown weapClass")?; Ok(()) }),
    field!(present "TorsoAnims", "The third-person torso set (zhbo_core::torso): the group of weapons whose every reload, melee, throw, fire and weapon-change torso clip in mp/playeranim.script is the same; 0 none.", FieldKind::Num,
        |d: &WeaponDef| d.torso_anims.to_string(),
        |d: &mut WeaponDef, v: Values| { d.torso_anims = v.text.parse().map_err(|_| "number")?; Ok(()) }),
    field!("BodyAnims", "The third-person body's animation set: the group of weapons BO2's mp/playeranim.script animates alike (by weapClass and playerAnimType); 0 the rifleman's.", FieldKind::Num,
        |d: &WeaponDef| d.body_anims.to_string(),
        |d: &mut WeaponDef, v: Values| { d.body_anims = v.text.parse().map_err(|_| "number")?; Ok(()) }),
    field!("Fire", "auto, semi, minigun, stacked, or `burst n`.", FieldKind::Enum,
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
    field!("Pellets", "BO2's shotCount: pellets per shot for a spread weapClass (0 on projectile weapons).", FieldKind::Num,
        |d: &WeaponDef| d.pellets.to_string(),
        |d: &mut WeaponDef, v: Values| { d.pellets = v.text.parse().map_err(|_| "number")?; Ok(()) }),
    field!("Clip", "Rounds in the magazine.", FieldKind::Num,
        |d: &WeaponDef| d.clip.to_string(),
        |d: &mut WeaponDef, v: Values| { d.clip = v.text.parse().map_err(|_| "number")?; Ok(()) }),
    field!("StartReserve", "Spare rounds at spawn.", FieldKind::Num,
        |d: &WeaponDef| d.start_reserve.to_string(),
        |d: &mut WeaponDef, v: Values| { d.start_reserve = v.text.parse().map_err(|_| "number")?; Ok(()) }),
    field!("MaxReserve", "Spare rounds at most, beside the magazine (BO2's iMaxAmmo).", FieldKind::Num,
        |d: &WeaponDef| d.max_reserve.to_string(),
        |d: &mut WeaponDef, v: Values| { d.max_reserve = v.text.parse().map_err(|_| "number")?; Ok(()) }),
    field!("Reload", "Reload, empty reload, rounds-in time, empty rounds-in time (0: the rounds-in time).", FieldKind::Nums(4),
        |d: &WeaponDef| format!("{} {} {} {}", d.reload_time, d.reload_empty_time, d.reload_add_time, d.reload_empty_add_time),
        |d: &mut WeaponDef, v: Values| {
            v.need(4)?;
            (d.reload_time, d.reload_empty_time, d.reload_add_time, d.reload_empty_add_time) = (v.num(0), v.num(1), v.num(2), v.num(3));
            Ok(())
        }),
    field!("ReloadShells", "Segmented reload (with `SegmentedReload`): start, end, rounds per cycle.", FieldKind::Nums(3),
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
    field!("EmptySwitch", "Raise and drop with an empty clip (seconds).", FieldKind::Nums(2),
        |d: &WeaponDef| nums(&[d.empty_raise_time, d.empty_drop_time]),
        |d: &mut WeaponDef, v: Values| { v.need(2)?; (d.empty_raise_time, d.empty_drop_time) = (v.num(0), v.num(1)); Ok(()) }),
    field!("AltSwitch", "Alt toggle: raise into this mode, drop out of this weapon.", FieldKind::Nums(2),
        |d: &WeaponDef| format!("{} {}", d.alt_raise_time, d.alt_drop_time),
        |d: &mut WeaponDef, v: Values| {
            v.need(2)?;
            (d.alt_raise_time, d.alt_drop_time) = (v.num(0), v.num(1));
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
    field!("DamageRange", "The ranges of the damage curve, BO2's inches (999999 = no falloff).", FieldKind::Nums(6),
        |d: &WeaponDef| d.damage_range.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" "),
        |d: &mut WeaponDef, v: Values| {
            v.need(6)?;
            d.damage_range.copy_from_slice(&v.nums[..6]);
            Ok(())
        }),
    field!("LocationMultipliers", "BO2's `locationDamageMultipliers`: one per hit location (`hitLocation_t`, 21).", FieldKind::Nums(crate::hitloc::COUNT),
        |d: &WeaponDef| d.location_multipliers.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" "),
        |d: &mut WeaponDef, v: Values| {
            v.need(crate::hitloc::COUNT)?;
            d.location_multipliers.copy_from_slice(&v.nums[..crate::hitloc::COUNT]);
            Ok(())
        }),
    field!("AttachmentLocationMultipliers", "Base weapons: the location multipliers an attachment's unique changes (`short=location:mult,...`, `FUN_008ed780`).", FieldKind::Pairs,
        |d: &WeaponDef| d.attachment_location_multipliers.iter().map(|(s, m)| format!("{s}={}", m.iter().map(|(l, x)| format!("{l}:{x}")).collect::<Vec<_>>().join(","))).collect::<Vec<_>>().join(" "),
        |d: &mut WeaponDef, v: Values| {
            d.attachment_location_multipliers = parse_pairs(v.text)
                .into_iter()
                .map(|(short, list)| {
                    let m = list
                        .split(',')
                        .map(|p| p.split_once(':').and_then(|(l, x)| Some((l.parse().ok()?, x.parse().ok()?))).ok_or_else(|| format!("bad location multiplier {p}")))
                        .collect::<Result<Vec<(u8, f32)>, String>>()?;
                    Ok((short, m))
                })
                .collect::<Result<_, String>>()?;
            Ok(())
        }),
    field!("MaxRange", "BO2's maxRange: a turret's AI range (Zero Hour units); 0 on player weapons.", FieldKind::Num,
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
    field!("SpreadRates", "The spread scale's turn add, and the decay's crouched and prone scales (`fHipSpreadTurnAdd`, `fHipSpreadDuckedDecay`, `fHipSpreadProneDecay`).", FieldKind::Nums(3),
        |d: &WeaponDef| format!("{} {} {}", d.spread_turn_add, d.spread_stance_decay.0, d.spread_stance_decay.1),
        |d: &mut WeaponDef, v: Values| {
            v.need(3)?;
            (d.spread_turn_add, d.spread_stance_decay) = (v.num(0), (v.num(1), v.num(2)));
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
    field!("MeleeDelay", "Seconds from the melee press to the strike (BO2's iMeleeDelay).", FieldKind::Num,
        |d: &WeaponDef| d.melee_delay.to_string(),
        |d: &mut WeaponDef, v: Values| { d.melee_delay = v.text_num()?; Ok(()) }),
    field!("MeleeCharge", "The lunge: charge range (Zero Hour units), charge melee time and delay (seconds).", FieldKind::Nums(3),
        |d: &WeaponDef| nums(&[d.melee_charge_range, d.melee_charge_time, d.melee_charge_delay]),
        |d: &mut WeaponDef, v: Values| {
            v.need(3)?;
            (d.melee_charge_range, d.melee_charge_time, d.melee_charge_delay) = (v.num(0), v.num(1), v.num(2));
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
    field!("Offhand", "Offhand states: detonate delay, has detonator, hold use (the pull-back, release delay and detonate time are FireCycle's and DetonateTime).", FieldKind::Nums(3),
        |d: &WeaponDef| format!("{} {} {}", d.detonate_delay, d.has_detonator as u8, d.hold_use as u8),
        |d: &mut WeaponDef, v: Values| {
            v.need(3)?;
            d.detonate_delay = v.num(0);
            (d.has_detonator, d.hold_use) = (v.flag(1), v.flag(2));
            Ok(())
        }),
    field!("HoldCancelable", "1 if the weapon-switch button cancels the held offhand (BO2's offhandHoldIsCancelable).", FieldKind::Num,
        |d: &WeaponDef| (d.hold_cancelable as u8).to_string(),
        |d: &mut WeaponDef, v: Values| { d.hold_cancelable = v.text_num()? != 0.0; Ok(()) }),
    field!("Plantable", "1 if BO2's plant check applies (its plantable flag).", FieldKind::Num,
        |d: &WeaponDef| (d.plantable as u8).to_string(),
        |d: &mut WeaponDef, v: Values| { d.plantable = v.text_num()? != 0.0; Ok(()) }),
    field!("ExplodeOnGround", "1 if a fuse that runs out in the air waits until the grenade is at rest (BO2's bExplodeOnGround).", FieldKind::Num,
        |d: &WeaponDef| (d.explode_on_ground as u8).to_string(),
        |d: &mut WeaponDef, v: Values| { d.explode_on_ground = v.text_num()? != 0.0; Ok(()) }),
    field!("TimedDetonation", "1 if the throw arms its fuse (BO2's timedDetonation); a piece that lands keeps it running.", FieldKind::Num,
        |d: &WeaponDef| (d.timed_detonation as u8).to_string(),
        |d: &mut WeaponDef, v: Values| { d.timed_detonation = v.text_num()? != 0.0; Ok(()) }),
    field!("LaserSensor", "1 for the Sensor Grenade (BO2's isLaserSensor): its blast plays no impact-table effect.", FieldKind::Num,
        |d: &WeaponDef| (d.laser_sensor as u8).to_string(),
        |d: &mut WeaponDef, v: Values| { d.laser_sensor = v.text_num()? != 0.0; Ok(()) }),
    field!("Aims", "1 if the weapon aims down its sights (BO2's aimDownSight).", FieldKind::Num,
        |d: &WeaponDef| (d.aims as u8).to_string(),
        |d: &mut WeaponDef, v: Values| { d.aims = v.text_num()? != 0.0; Ok(()) }),
    field!("Inventory", "BO2's inventory type: primary, offhand, item, altmode, melee, dwlefthand.", FieldKind::Enum,
        |d: &WeaponDef| d.inventory.name().into(),
        |d: &mut WeaponDef, v: Values| { d.inventory = Inventory::parse(v.text).ok_or("unknown inventory type")?; Ok(()) }),
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
    field!("Throw", "A throw's speed along the view's up and along its horizontal forward (BO2's `iProjectileSpeedRelativeUp`, `iProjectileSpeedForward`; t6mp `FUN_0066cf00`), and whether a live one may be thrown back (`bThrowBack`).", FieldKind::Nums(3),
        |d: &WeaponDef| format!("{} {} {}", d.projectile_speed_relative_up, d.projectile_speed_forward, d.throw_back as u8),
        |d: &mut WeaponDef, v: Values| {
            v.need(3)?;
            d.projectile_speed_relative_up = v.num(0);
            d.projectile_speed_forward = v.num(1);
            d.throw_back = v.flag(2);
            Ok(())
        }),
    field!("SpawnedGrenade", "The grenade weapon the projectile becomes where it strikes, stuck to what it hit (BO2's `projExplosion` bolt: `szSpawnedGrenadeWeaponName`); empty for none.", FieldKind::Str,
        |d: &WeaponDef| d.spawns.clone(),
        |d: &mut WeaponDef, v: Values| { d.spawns = v.text.into(); Ok(()) }),
    field!("FxWeapon", "Zero Hour assets made for this weapon by the converter (empty when absent).", FieldKind::Str,
        |d: &WeaponDef| d.fx_weapon.clone(),
        |d: &mut WeaponDef, v: Values| { d.fx_weapon = v.text.into(); Ok(()) }),
    field!("ExplosionFx", "Zero Hour FXList for the def's own blast effect (`projExplosionEffect`, over the impact table's cell); empty when BO2 names none.", FieldKind::Str,
        |d: &WeaponDef| d.explosion_fx.clone(),
        |d: &mut WeaponDef, v: Values| { d.explosion_fx = v.text.into(); Ok(()) }),
    field!("ImpactType", "BO2's `impactType` (+40): the impact-table bucket (0: none).", FieldKind::Num,
        |d: &WeaponDef| d.impact_type.to_string(),
        |d: &mut WeaponDef, v: Values| { d.impact_type = v.text_num()? as i32; Ok(()) }),
    field!(present "ViewFlashFx", "Zero Hour FXList for the first-person muzzle flash (`viewFlashEffect`).", FieldKind::Str,
        |d: &WeaponDef| d.view_flash_fx.clone(),
        |d: &mut WeaponDef, v: Values| { d.view_flash_fx = v.text.into(); Ok(()) }),
    field!(present "ViewShellFx", "Zero Hour FXList for the first-person shell eject (`viewShellEjectEffect`).", FieldKind::Str,
        |d: &WeaponDef| d.view_shell_fx.clone(),
        |d: &mut WeaponDef, v: Values| { d.view_shell_fx = v.text.into(); Ok(()) }),
    field!(present "ViewLastShotShellFx", "Zero Hour FXList for the first-person shell eject on the last round.", FieldKind::Str,
        |d: &WeaponDef| d.view_last_shot_shell_fx.clone(),
        |d: &mut WeaponDef, v: Values| { d.view_last_shot_shell_fx = v.text.into(); Ok(()) }),
    field!("ProjectileObject", "The thrown/projectiled Zero Hour object.", FieldKind::Str,
        |d: &WeaponDef| d.projectile_object.clone(),
        |d: &mut WeaponDef, v: Values| { d.projectile_object = v.text.into(); Ok(()) }),
    field!("ProjectileBounds", "The `projectileModel`'s bounds, inches: mins x y z, maxs x y z (`FUN_007825b0`; the missile's and the placed piece's collision box).", FieldKind::Nums(6),
        |d: &WeaponDef| d.projectile_bounds.map(|v| v.to_string()).join(" "),
        |d: &mut WeaponDef, v: Values| {
            v.need(6)?;
            d.projectile_bounds = [v.num(0), v.num(1), v.num(2), v.num(3), v.num(4), v.num(5)];
            Ok(())
        }),
    field!(present "FireSound", "The shot's BO2 sound event.", FieldKind::Str,
        |d: &WeaponDef| d.fire_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.fire_sound = v.text.into(); Ok(()) }),
    field!(present "EmptyFireSound", "The empty magazine's BO2 sound event.", FieldKind::Str,
        |d: &WeaponDef| d.empty_fire_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.empty_fire_sound = v.text.into(); Ok(()) }),
    field!(present "SilencedFireSound", "The suppressed shot's player BO2 sound event.", FieldKind::Str,
        |d: &WeaponDef| d.silenced_fire_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.silenced_fire_sound = v.text.into(); Ok(()) }),
    field!(present "SilencedFireSoundNpc", "The suppressed shot's third-person BO2 sound event.", FieldKind::Str,
        |d: &WeaponDef| d.silenced_fire_sound_npc.clone(),
        |d: &mut WeaponDef, v: Values| { d.silenced_fire_sound_npc = v.text.into(); Ok(()) }),
    field!("SilencedFxWeapon", "The FX weapon whose INI block carries the suppressed shot.", FieldKind::Str,
        |d: &WeaponDef| d.silenced_fx_weapon.clone(),
        |d: &mut WeaponDef, v: Values| { d.silenced_fx_weapon = v.text.into(); Ok(()) }),
    field!(present "SilencedViewFlashFx", "The shared suppressed first-person FXList.", FieldKind::Str,
        |d: &WeaponDef| d.silenced_view_flash_fx.clone(),
        |d: &mut WeaponDef, v: Values| { d.silenced_view_flash_fx = v.text.into(); Ok(()) }),
    // 5b: the def-level sound events (one per BO2 alias, `ZHBO_BO2_Snd_<alias>`).
    field!(present "RaiseSound", "The raise's BO2 sound event (`raiseSoundPlayer`), played as a raise begins.", FieldKind::Str,
        |d: &WeaponDef| d.raise_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.raise_sound = v.text.into(); Ok(()) }),
    field!(present "FirstRaiseSound", "The first raise's event (`firstRaiseSoundPlayer`).", FieldKind::Str,
        |d: &WeaponDef| d.first_raise_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.first_raise_sound = v.text.into(); Ok(()) }),
    field!(present "AltSwitchSound", "The alt toggle's event (`altSwitchSoundPlayer`), played by the mode coming up (EV_WEAPON_ALT).", FieldKind::Str,
        |d: &WeaponDef| d.alt_switch_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.alt_switch_sound = v.text.into(); Ok(()) }),
    field!(present "PutawaySound", "The putaway's event (`putawaySoundPlayer`), played as the drop begins.", FieldKind::Str,
        |d: &WeaponDef| d.putaway_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.putaway_sound = v.text.into(); Ok(()) }),
    field!(present "AdsZoomSound", "The zoom-in event as aiming starts (`adsZoomSound`; scoped weapons).", FieldKind::Str,
        |d: &WeaponDef| d.ads_zoom_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.ads_zoom_sound = v.text.into(); Ok(()) }),
    field!(present "MeleeSwipeSound", "The melee swing with this weapon in hand (`meleeSwipeSoundPlayer`).", FieldKind::Str,
        |d: &WeaponDef| d.melee_swipe_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.melee_swipe_sound = v.text.into(); Ok(()) }),
    field!(present "MeleeHitSound", "The melee landing on someone (`meleeHitSound`).", FieldKind::Str,
        |d: &WeaponDef| d.melee_hit_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.melee_hit_sound = v.text.into(); Ok(()) }),
    field!(present "FireStartSound", "The fire string's opening (`fireStartSoundPlayer`: a burst's recording).", FieldKind::Str,
        |d: &WeaponDef| d.fire_start_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.fire_start_sound = v.text.into(); Ok(()) }),
    field!(present "FireLastSound", "The last round's shot in place of the shot (`fireLastSoundPlayer`).", FieldKind::Str,
        |d: &WeaponDef| d.fire_last_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.fire_last_sound = v.text.into(); Ok(()) }),
    field!(present "PullbackSound", "The offhand's pull-back (`pullbackSoundPlayer`).", FieldKind::Str,
        |d: &WeaponDef| d.pullback_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.pullback_sound = v.text.into(); Ok(()) }),
    field!(present "NpcSounds", "The third-person def-level sounds (`moment=event`: raise, first_raise, putaway, melee_swipe, melee_hit, pullback, empty_fire; BO2's fields without `Player`), heard at another player.", FieldKind::Pairs,
        |d: &WeaponDef| pairs(&d.npc_sounds),
        |d: &mut WeaponDef, v: Values| { d.npc_sounds = parse_pairs(v.text); Ok(()) }),
    field!(present "SilencedFireStartSound", "The fire string's opening with the silencer (its unique's override, `FUN_008ebe80`).", FieldKind::Str,
        |d: &WeaponDef| d.silenced_fire_start_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.silenced_fire_start_sound = v.text.into(); Ok(()) }),
    field!(present "AttachmentFireSounds", "Base weapons: the shot another attachment's unique overrides (`short=event`, `FUN_008ebe80`).", FieldKind::Pairs,
        |d: &WeaponDef| pairs(&d.attachment_fire_sounds),
        |d: &mut WeaponDef, v: Values| { d.attachment_fire_sounds = parse_pairs(v.text); Ok(()) }),
    field!(present "AttachmentFireStartSounds", "Base weapons: the fire string's opening another attachment overrides (`short=event`).", FieldKind::Pairs,
        |d: &WeaponDef| pairs(&d.attachment_fire_start_sounds),
        |d: &mut WeaponDef, v: Values| { d.attachment_fire_start_sounds = parse_pairs(v.text); Ok(()) }),
    field!("ExplodeSound", "The blast's BO2 sound event.", FieldKind::Str,
        |d: &WeaponDef| d.explode_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.explode_sound = v.text.into(); Ok(()) }),
    field!(present "BounceSounds", "A bounce's sound per surface (`surface=event`; BO2's `bounceSound[surface]`).", FieldKind::Pairs,
        |d: &WeaponDef| pairs(&d.bounce_sounds),
        |d: &mut WeaponDef, v: Values| { d.bounce_sounds = parse_pairs(v.text); Ok(()) }),
    field!("Bounce", "The speed kept bouncing off each surface, along/into (`surface=along/into`; BO2's `parallelBounce`/`perpendicularBounce`).", FieldKind::Pairs,
        |d: &WeaponDef| d.bounce.iter().map(|(s, a, p)| format!("{s}={a}/{p}")).collect::<Vec<_>>().join(" "),
        |d: &mut WeaponDef, v: Values| {
            d.bounce = parse_pairs(v.text)
                .into_iter()
                .filter_map(|(s, ap)| {
                    let (a, p) = ap.split_once('/')?;
                    Some((s, a.parse().ok()?, p.parse().ok()?))
                })
                .collect();
            Ok(())
        }),
    field!("ActivateDist", "Path length before the projectile arms (`iProjectileActivateDist`, ZH units); a hit sooner is a dud.", FieldKind::Num,
        |d: &WeaponDef| d.activate_dist.to_string(),
        |d: &mut WeaponDef, v: Values| { d.activate_dist = v.text_num()?; Ok(()) }),
    field!("DudFx", "Zero Hour FXList for a dud (`projDudEffect`).", FieldKind::Str,
        |d: &WeaponDef| d.dud_fx.clone(),
        |d: &mut WeaponDef, v: Values| { d.dud_fx = v.text.into(); Ok(()) }),
    field!("DudSound", "A dud's own BO2 sound event (`projDudSound`).", FieldKind::Str,
        |d: &WeaponDef| d.dud_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.dud_sound = v.text.into(); Ok(()) }),
    field!(present "Viewmodel", "Viewmodel index in the converted art, or -1.", FieldKind::Num,
        |d: &WeaponDef| d.viewmodel.to_string(),
        |d: &mut WeaponDef, v: Values| { d.viewmodel = v.text.parse().map_err(|_| "number")?; Ok(()) }),
    field!(present "Anims", "Which BO2 viewmodel animation slots the weapon has, as hex bits.", FieldKind::Hex,
        |d: &WeaponDef| format!("{:x}", d.anims),
        |d: &mut WeaponDef, v: Values| { d.anims = u128::from_str_radix(v.text, 16).map_err(|_| "hex")?; Ok(()) }),
    field!(present "ClipBase", "Viewmodel clip base (an alt mode's clips sit after its weapon's).", FieldKind::Num,
        |d: &WeaponDef| d.clip_base.to_string(),
        |d: &mut WeaponDef, v: Values| { d.clip_base = v.text.parse().map_err(|_| "number")?; Ok(()) }),
    field!(present "WorldMeshes", "Third-person meshes on the soldier model (`W{vm}{k}`).", FieldKind::Num,
        |d: &WeaponDef| d.world_meshes.to_string(),
        |d: &mut WeaponDef, v: Values| { d.world_meshes = v.text.parse().map_err(|_| "number")?; Ok(()) }),
    field!("Attachments", "The attachment shorts on this variant, sorted.", FieldKind::Words,
        |d: &WeaponDef| d.attachments.join(" "),
        |d: &mut WeaponDef, v: Values| { d.attachments = v.text.split_whitespace().map(str::to_string).collect(); Ok(()) }),
    field!("Allowed", "Base weapons: each attachment BO2 offers (`short=attachment`).", FieldKind::Pairs,
        |d: &WeaponDef| pairs(&d.allowed),
        |d: &mut WeaponDef, v: Values| { d.allowed = parse_pairs(v.text); Ok(()) }),
    field!(present "Attributes", "BO2's stat bars (mp/attributestable.csv, 0..100): fire rate, damage, range, accuracy.", FieldKind::Nums(4),
        |d: &WeaponDef| nums(&d.attributes),
        |d: &mut WeaponDef, v: Values| { v.need(4)?; for i in 0..4 { d.attributes[i] = v.num(i); } Ok(()) }),
    field!(present "AttachmentAttributes", "Base weapons: each attachment's stat-bar deltas on this weapon (`short=fire,damage,range,accuracy`).", FieldKind::Pairs,
        |d: &WeaponDef| d.attachment_attributes.iter().map(|(s, a)| format!("{s}={}", a.map(|x| x.to_string()).join(","))).collect::<Vec<_>>().join(" "),
        |d: &mut WeaponDef, v: Values| {
            d.attachment_attributes = parse_pairs(v.text)
                .into_iter()
                .map(|(s, a)| {
                    let mut out = [0.0; 4];
                    for (o, x) in out.iter_mut().zip(a.split(',')) {
                        *o = x.parse().map_err(|_| "number")?;
                    }
                    Ok((s, out))
                })
                .collect::<Result<_, &'static str>>()?;
            Ok(())
        }),
    field!("AltModes", "Base weapons: the alt-mode weapon an attachment adds (`short=alt`).", FieldKind::Pairs,
        |d: &WeaponDef| pairs(&d.alt_modes),
        |d: &mut WeaponDef, v: Values| { d.alt_modes = parse_pairs(v.text); Ok(()) }),
    field!("Alt", "The weapon this toggles to (its alt mode, or back from it).", FieldKind::Str,
        |d: &WeaponDef| d.alt.clone(),
        |d: &mut WeaponDef, v: Values| { d.alt = v.text.into(); Ok(()) }),
    field!("CacSlot", "The Create-a-Class slot BO2 offers it in (statstable column 13); empty: not a CaC item. Sim: a received class's repair checks slot fit with it.", FieldKind::Str,
        |d: &WeaponDef| d.cac_slot.clone(),
        |d: &mut WeaponDef, v: Values| { d.cac_slot = v.text.into(); Ok(()) }),
    field!(present "CacGroup", "The Create-a-Class group BO2 lists it under (statstable column 2).", FieldKind::Str,
        |d: &WeaponDef| d.cac_group.clone(),
        |d: &mut WeaponDef, v: Values| { d.cac_group = v.text.into(); Ok(()) }),
    field!(present "CacName", "The Create-a-Class item's localized name (statstable column 3).", FieldKind::Str,
        |d: &WeaponDef| d.cac_name.clone(),
        |d: &mut WeaponDef, v: Values| { d.cac_name = v.text.into(); Ok(()) }),
    field!(present "CacLocked", "1 if Create-a-Class lists the item locked (its art did not convert: a DLC item without the pack).", FieldKind::Num,
        |d: &WeaponDef| (d.cac_locked as u8).to_string(),
        |d: &mut WeaponDef, v: Values| { d.cac_locked = v.text_num()? != 0.0; Ok(()) }),
    field!(present "CacLockHint", "The locked item's hint (`MENU_<statstable column 15>_REQUIRED_HINT`).", FieldKind::Str,
        |d: &WeaponDef| d.cac_lock_hint.clone(),
        |d: &mut WeaponDef, v: Values| { d.cac_lock_hint = v.text.into(); Ok(()) }),
    field!(present "Description", "The Create-a-Class item's localized description (statstable column 7).", FieldKind::Str,
        |d: &WeaponDef| d.description.clone(),
        |d: &mut WeaponDef, v: Values| { d.description = v.text.into(); Ok(()) }),
    field!(present "CacOrder", "The Create-a-Class list order: statstable sort key (column 9), item index (column 0).", FieldKind::Nums(2),
        |d: &WeaponDef| format!("{} {}", d.cac_sort, d.cac_index),
        |d: &mut WeaponDef, v: Values| { v.need(2)?; d.cac_sort = v.nums[0] as i32; d.cac_index = v.nums[1] as i32; Ok(()) }),
    field!(present "DualWieldWeapon", "A dual-wield weapon's left gun (BO2's `szDualWieldWeaponName`).", FieldKind::Str,
        |d: &WeaponDef| d.dual_wield_weapon.clone(),
        |d: &mut WeaponDef, v: Values| { d.dual_wield_weapon = v.text.into(); Ok(()) }),
    field!(present "LeftFireSound", "A dual-wield weapon's left gun's shot BO2 sound event.", FieldKind::Str,
        |d: &WeaponDef| d.left_fire_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.left_fire_sound = v.text.into(); Ok(()) }),
    field!("Flags", "alt keeps its own ammo, silenced, laser, target finder.", FieldKind::Nums(4),
        |d: &WeaponDef| format!("{} {} {} {}", d.alt_own_ammo as u8, d.silenced as u8, d.laser as u8, d.target_finder as u8),
        |d: &mut WeaponDef, v: Values| {
            v.need(4)?;
            d.alt_own_ammo = v.flag(0);
            d.silenced = v.flag(1);
            d.laser = v.flag(2);
            d.target_finder = v.flag(3);
            Ok(())
        }),
    field!("MmsMaxDist", "Millimetre scanner's max distance (BO2 inches; the scanner flag is in Traits).", FieldKind::Num,
        |d: &WeaponDef| d.mms_max_dist.to_string(),
        |d: &mut WeaponDef, v: Values| { d.mms_max_dist = v.text_num()?; Ok(()) }),
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
    field!(present "HipSway", "Hip sway: max angle, lerp speed, pitch, yaw, horizontal and vertical scales.", FieldKind::Nums(6),
        |d: &WeaponDef| d.hip_sway.format(),
        |d: &mut WeaponDef, v: Values| { v.need(6)?; d.hip_sway = Sway::from_nums(v.nums); Ok(()) }),
    field!(present "AdsSway", "Aimed sway, as HipSway (BO2 has no aimed horizontal/vertical scales).", FieldKind::Nums(6),
        |d: &WeaponDef| d.ads_sway.format(),
        |d: &mut WeaponDef, v: Values| { v.need(6)?; d.ads_sway = Sway::from_nums(v.nums); Ok(()) }),
    field!(present "ViewStand", "Viewmodel move offset and rotation standing (`vStandMove`, `vStandRot`).", FieldKind::Nums(6),
        |d: &WeaponDef| { let p = &d.view_pos; nums(&[p.stand_move, p.stand_rot].concat()) },
        |d: &mut WeaponDef, v: Values| { v.need(6)?; let p = &mut d.view_pos; (p.stand_move, p.stand_rot) = (v3(v.nums), v3(&v.nums[3..])); Ok(()) }),
    field!(present "ViewDucked", "Crouched offset, move and rotation (`vDuckedOfs`, `vDuckedMove`, `vDuckedRot`).", FieldKind::Nums(9),
        |d: &WeaponDef| { let p = &d.view_pos; nums(&[p.ducked_ofs, p.ducked_move, p.ducked_rot].concat()) },
        |d: &mut WeaponDef, v: Values| { v.need(9)?; let p = &mut d.view_pos; (p.ducked_ofs, p.ducked_move, p.ducked_rot) = (v3(v.nums), v3(&v.nums[3..]), v3(&v.nums[6..])); Ok(()) }),
    field!(present "ViewProne", "Prone offset, move and rotation (`vProneOfs`, `vProneMove`, `vProneRot`).", FieldKind::Nums(9),
        |d: &WeaponDef| { let p = &d.view_pos; nums(&[p.prone_ofs, p.prone_move, p.prone_rot].concat()) },
        |d: &mut WeaponDef, v: Values| { v.need(9)?; let p = &mut d.view_pos; (p.prone_ofs, p.prone_move, p.prone_rot) = (v3(v.nums), v3(&v.nums[3..]), v3(&v.nums[6..])); Ok(()) }),
    field!(present "ViewStrafe", "Strafe move and rotation (`vStrafeMove`, `vStrafeRot`).", FieldKind::Nums(6),
        |d: &WeaponDef| { let p = &d.view_pos; nums(&[p.strafe_move, p.strafe_rot].concat()) },
        |d: &mut WeaponDef, v: Values| { v.need(6)?; let p = &mut d.view_pos; (p.strafe_move, p.strafe_rot) = (v3(v.nums), v3(&v.nums[3..])); Ok(()) }),
    field!(present "ViewSprint", "Sprint offset, rotation and bob (`vSprintOfs`, `vSprintRot`, `vSprintBob`).", FieldKind::Nums(8),
        |d: &WeaponDef| { let p = &d.view_pos; nums(&[&p.sprint_ofs[..], &p.sprint_rot[..], &p.sprint_bob[..]].concat()) },
        |d: &mut WeaponDef, v: Values| {
            v.need(8)?;
            let p = &mut d.view_pos;
            (p.sprint_ofs, p.sprint_rot, p.sprint_bob) = (v3(v.nums), v3(&v.nums[3..]), [v.num(6), v.num(7)]);
            Ok(())
        }),
    field!(present "ViewRates", "Position and rotation blend rates (standing, prone) and the stand/crouch/prone move min speeds.", FieldKind::Nums(7),
        |d: &WeaponDef| { let p = &d.view_pos; nums(&[p.move_rate, p.prone_move_rate, p.rot_rate, p.prone_rot_rate, p.stand_min_speed, p.ducked_min_speed, p.prone_min_speed]) },
        |d: &mut WeaponDef, v: Values| {
            v.need(7)?;
            let p = &mut d.view_pos;
            (p.move_rate, p.prone_move_rate, p.rot_rate, p.prone_rot_rate) = (v.num(0), v.num(1), v.num(2), v.num(3));
            (p.stand_min_speed, p.ducked_min_speed, p.prone_min_speed) = (v.num(4), v.num(5), v.num(6));
            Ok(())
        }),
    field!(present "ViewAds", "Aimed pitch offset and bob factor (`fAdsAimPitch`, `fAdsBobFactor`).", FieldKind::Nums(2),
        |d: &WeaponDef| nums(&[d.view_pos.ads_aim_pitch, d.view_pos.ads_bob_factor]),
        |d: &mut WeaponDef, v: Values| { v.need(2)?; (d.view_pos.ads_aim_pitch, d.view_pos.ads_bob_factor) = (v.num(0), v.num(1)); Ok(()) }),
    field!(present "SprintCycleScale", "The bob cycle's rate while sprinting (`fSprintCycleScale`, `FUN_008c1d00`).", FieldKind::Nums(1),
        |d: &WeaponDef| nums(&[d.view_pos.sprint_cycle_scale]),
        |d: &mut WeaponDef, v: Values| { v.need(1)?; d.view_pos.sprint_cycle_scale = v.num(0); Ok(()) }),
    field!("Reticle", "Crosshair: centre and side size, min offset, side position, ADS in/out fade fractions, keep when aimed, colour change.", FieldKind::Nums(8),
        |d: &WeaponDef| d.reticle.format(),
        |d: &mut WeaponDef, v: Values| { v.need(8)?; d.reticle.set_nums(v.nums); Ok(()) }),
    field!("ReticleArt", "The crosshair's centre and side materials (`-` for none).", FieldKind::Words,
        |d: &WeaponDef| d.reticle.format_art(),
        |d: &mut WeaponDef, v: Values| { d.reticle.set_art(v.text); Ok(()) }),
    field!(present "CamoList", "The camouflage option ids this weapon offers, `camo_none` first.", FieldKind::Words,
        |d: &WeaponDef| d.camo.join(" "),
        |d: &mut WeaponDef, v: Values| { d.camo = v.text.split_whitespace().map(str::to_string).collect(); Ok(()) }),
    field!(present "CamoMaterial", "The weapon's own camo asset (BO2's `WeaponDef.weaponCamo`: `camo_an94`, the INSAS's `camo_msmc`), empty when none; attachment parts wear their own (the viewmodel manifest's per-part camo).", FieldKind::Str,
        |d: &WeaponDef| d.camo_material.clone(),
        |d: &mut WeaponDef, v: Values| { d.camo_material = v.text.into(); Ok(()) }),
    field!("OverlayMaterial", "The 2D screen-space ADS overlay material (`scope_overlay_<weapon>`).",
        FieldKind::Str,
        |d: &WeaponDef| d.overlay_material.clone(),
        |d: &mut WeaponDef, v: Values| { d.overlay_material = v.text.into(); Ok(()) }),
    field!(present "OverlayMaterialLowRes", "The low-resolution ADS overlay material.", FieldKind::Str,
        |d: &WeaponDef| d.overlay_material_low_res.clone(),
        |d: &mut WeaponDef, v: Values| { d.overlay_material_low_res = v.text.into(); Ok(()) }),
    field!("OverlaySize", "The overlay's width and height in BO2's 640x480 virtual screen (`overlayWidth`, `overlayHeight`), drawn centred at one scale for both axes.", FieldKind::Nums(2),
        |d: &WeaponDef| format!("{} {}", d.overlay_size.0, d.overlay_size.1),
        |d: &mut WeaponDef, v: Values| { v.need(2)?; d.overlay_size = (v.num(0), v.num(1)); Ok(()) }),
    field!("OverlayReticle", "The overlay reticle enum (`weapOverlayReticle_t`).", FieldKind::Num,
        |d: &WeaponDef| d.overlay_reticle.to_string(),
        |d: &mut WeaponDef, v: Values| { d.overlay_reticle = v.text_num()? as u32; Ok(()) }),
    field!("FireCycle", "Last-round fire time, fire delay, rechamber, rechamber bolt, hold fire (seconds); shots before a rechamber; bolt action.", FieldKind::Nums(7),
        |d: &WeaponDef| { let t = &d.timing; format!("{} {} {} {} {} {} {}", t.last_fire_time, t.fire_delay, t.rechamber_time, t.rechamber_bolt_time, t.hold_fire_time, t.shots_before_rechamber, t.bolt_action as u8) },
        |d: &mut WeaponDef, v: Values| {
            v.need(7)?;
            let t = &mut d.timing;
            (t.last_fire_time, t.fire_delay, t.rechamber_time, t.rechamber_bolt_time, t.hold_fire_time) = (v.num(0), v.num(1), v.num(2), v.num(3), v.num(4));
            (t.shots_before_rechamber, t.bolt_action) = (v.num(5) as u16, v.flag(6));
            Ok(())
        }),
    field!("QuickReload", "Quick reload, quick empty reload and quick rounds-in times (seconds).", FieldKind::Nums(3),
        |d: &WeaponDef| nums(&[d.timing.reload_quick_time, d.timing.reload_quick_empty_time, d.timing.reload_quick_add_time]),
        |d: &mut WeaponDef, v: Values| {
            v.need(3)?;
            (d.timing.reload_quick_time, d.timing.reload_quick_empty_time, d.timing.reload_quick_add_time) = (v.num(0), v.num(1), v.num(2));
            Ok(())
        }),
    field!("ReloadStartAdd", "Rounds the segmented reload's start adds (`iReloadStartAdd`).", FieldKind::Num,
        |d: &WeaponDef| d.timing.reload_start_add.to_string(),
        |d: &mut WeaponDef, v: Values| { d.timing.reload_start_add = v.text_num()? as u16; Ok(()) }),
    field!("SpinDownTime", "Minigun spin-down (seconds).", FieldKind::Num,
        |d: &WeaponDef| d.timing.spin_down_time.to_string(),
        |d: &mut WeaponDef, v: Values| { d.timing.spin_down_time = v.text_num()?; Ok(()) }),
    field!("DetonateTime", "The detonator press (seconds).", FieldKind::Num,
        |d: &WeaponDef| d.timing.detonate_time.to_string(),
        |d: &mut WeaponDef, v: Values| { d.timing.detonate_time = v.text_num()?; Ok(()) }),
    field!("SprintInTime", "Lowering into a sprint (seconds).", FieldKind::Num,
        |d: &WeaponDef| d.timing.sprint_in_time.to_string(),
        |d: &mut WeaponDef, v: Values| { d.timing.sprint_in_time = v.text_num()?; Ok(()) }),
    field!("AdsZoom", "Zoom levels 2 and 3 (degrees), zoom in and out fractions of the ADS lerp.", FieldKind::Nums(4),
        |d: &WeaponDef| { let z = &d.zoom; nums(&[z.fov2, z.fov3, z.in_frac, z.out_frac]) },
        |d: &mut WeaponDef, v: Values| { v.need(4)?; let z = &mut d.zoom; (z.fov2, z.fov3, z.in_frac, z.out_frac) = (v.num(0), v.num(1), v.num(2), v.num(3)); Ok(()) }),
    field!("AltScopeAds", "The second optic's ADS in and out times (seconds).", FieldKind::Nums(2),
        |d: &WeaponDef| nums(&[d.zoom.alt_scope_in_time, d.zoom.alt_scope_out_time]),
        |d: &mut WeaponDef, v: Values| { v.need(2)?; (d.zoom.alt_scope_in_time, d.zoom.alt_scope_out_time) = (v.num(0), v.num(1)); Ok(()) }),
    field!("AdsDof", "The aimed depth of field's start and end (inches).", FieldKind::Nums(2),
        |d: &WeaponDef| nums(&[d.zoom.dof.0, d.zoom.dof.1]),
        |d: &mut WeaponDef, v: Values| { v.need(2)?; d.zoom.dof = (v.num(0), v.num(1)); Ok(()) }),
    field!("OverlayAlphaScale", "The scope overlay's opacity (`fOverlayAlphaScale`).", FieldKind::Num,
        |d: &WeaponDef| d.zoom.overlay_alpha_scale.to_string(),
        |d: &mut WeaponDef, v: Values| { d.zoom.overlay_alpha_scale = v.text_num()?; Ok(()) }),
    field!("IdleAmount", "Idle sway amount, aimed and hip.", FieldKind::Nums(2),
        |d: &WeaponDef| nums(&[d.zoom.idle_amount.0, d.zoom.idle_amount.1]),
        |d: &mut WeaponDef, v: Values| { v.need(2)?; d.zoom.idle_amount = (v.num(0), v.num(1)); Ok(()) }),
    field!("IdleSway", "Idle sway clock speed aimed and hip, crouched and prone factors, hold breath to steady.", FieldKind::Nums(5),
        |d: &WeaponDef| { let z = &d.zoom; format!("{} {}", nums(&[z.idle_speed.0, z.idle_speed.1, z.idle_stance.0, z.idle_stance.1]), z.hold_breath as u8) },
        |d: &mut WeaponDef, v: Values| { v.need(5)?; let z = &mut d.zoom; (z.idle_speed, z.idle_stance, z.hold_breath) = ((v.num(0), v.num(1)), (v.num(2), v.num(3)), v.flag(4)); Ok(()) }),
    field!("AntiQuickScope", "Anti-quickscope: enabled, time (s), sway scale, spread multiplier, spread max, sway factor.", FieldKind::Nums(6),
        |d: &WeaponDef| { let a = &d.zoom.anti_quickscope; format!("{} {}", a.enabled as u8, nums(&[a.time, a.scale, a.spread_multiplier, a.spread_max, a.sway_factor])) },
        |d: &mut WeaponDef, v: Values| {
            v.need(6)?;
            d.zoom.anti_quickscope = crate::weapons::AntiQuickScope { enabled: v.flag(0), time: v.num(1), scale: v.num(2), spread_multiplier: v.num(3), spread_max: v.num(4), sway_factor: v.num(5) };
            Ok(())
        }),
    field!("StackFire", "Stack-fire spread and accuracy decay.", FieldKind::Nums(2),
        |d: &WeaponDef| nums(&[d.traits.stack_fire.0, d.traits.stack_fire.1]),
        |d: &mut WeaponDef, v: Values| { v.need(2)?; d.traits.stack_fire = (v.num(0), v.num(1)); Ok(()) }),
    field!("Traits", "clip only, dual magazine, infrared, millimetre-wave scanner, use as melee, rifle bullet, alt mode ADS only, alt switching disabled, ignores attachments.", FieldKind::Nums(9),
        |d: &WeaponDef| {
            let t = &d.traits;
            [t.clip_only, t.dual_mag, t.infrared, t.mms, t.use_as_melee, t.rifle_bullet, t.alt_ads_only, t.alt_disable_switching, t.ignore_attachments].map(|b| (b as u8).to_string()).join(" ")
        },
        |d: &mut WeaponDef, v: Values| {
            v.need(9)?;
            let t = &mut d.traits;
            (t.clip_only, t.dual_mag, t.infrared, t.mms, t.use_as_melee) = (v.flag(0), v.flag(1), v.flag(2), v.flag(3), v.flag(4));
            (t.rifle_bullet, t.alt_ads_only, t.alt_disable_switching, t.ignore_attachments) = (v.flag(5), v.flag(6), v.flag(7), v.flag(8));
            Ok(())
        }),
    field!("MinPlayerDamage", "The least a hit on a player does (`minPlayerDamage`).", FieldKind::Num,
        |d: &WeaponDef| d.traits.min_player_damage.to_string(),
        |d: &mut WeaponDef, v: Values| { d.traits.min_player_damage = v.text_num()?; Ok(()) }),
    field!("Penetrate", "BO2's penetration type: none, small, medium, large.", FieldKind::Enum,
        |d: &WeaponDef| d.traits.penetrate.name().into(),
        |d: &mut WeaponDef, v: Values| { d.traits.penetrate = Penetrate::parse(v.text).ok_or("unknown penetration")?; Ok(()) }),
    field!("OffhandClass", "BO2's offhand class: none, frag_grenade, smoke_grenade, flash_grenade, gear, supplydrop_marker.", FieldKind::Enum,
        |d: &WeaponDef| d.traits.offhand.name().into(),
        |d: &mut WeaponDef, v: Values| { d.traits.offhand = OffhandClass::parse(v.text).ok_or("unknown offhand class")?; Ok(()) }),
    field!("ViewKickCenterStance", "The aimed recentre speed's crouched and prone scales (`fAdsViewKickCenter{Ducked,Prone}Scale`).", FieldKind::Nums(2),
        |d: &WeaponDef| nums(&[d.kick_center_stance.0, d.kick_center_stance.1]),
        |d: &mut WeaponDef, v: Values| { v.need(2)?; d.kick_center_stance = (v.num(0), v.num(1)); Ok(()) }),
    field!("ViewKickCenterScale", "The attachments' product of `fAdsViewKickCenterSpeedScale` on the aimed recentre speed (1 bare; a combination's stacked value).", FieldKind::Num,
        |d: &WeaponDef| d.ads_kick_center_scale.to_string(),
        |d: &mut WeaponDef, v: Values| { d.ads_kick_center_scale = v.text_num()?; Ok(()) }),
    field!("WeapType", "BO2's weapon type: bullet, grenade, projectile, binoculars, gas, bomb, mine, melee, riotshield.", FieldKind::Enum,
        |d: &WeaponDef| d.traits.weap_type.name().into(),
        |d: &mut WeaponDef, v: Values| { d.traits.weap_type = WeapType::parse(v.text).ok_or("unknown weapon type")?; Ok(()) }),
    field!("SegmentedReload", "The reload runs round by round (`bSegmentedReload`; see `ReloadShells`).", FieldKind::Num,
        |d: &WeaponDef| (d.traits.segmented_reload as u8).to_string(),
        |d: &mut WeaponDef, v: Values| { d.traits.segmented_reload = v.text_num()? != 0.0; Ok(()) }),
    field!("UnlimitedAmmo", "The clip never reads as empty for the empty raise and drop (`unlimitedAmmo`).", FieldKind::Num,
        |d: &WeaponDef| (d.traits.unlimited_ammo as u8).to_string(),
        |d: &mut WeaponDef, v: Values| { d.traits.unlimited_ammo = v.text_num()? != 0.0; Ok(()) }),
    field!("GuidedMissileType", "The missile guidance enum (`guidedMissileType_t`; 6 is TV-guided).", FieldKind::Num,
        |d: &WeaponDef| d.guided_missile_type.to_string(),
        |d: &mut WeaponDef, v: Values| { d.guided_missile_type = v.text_num()? as u32; Ok(()) }),
    field!(present "LowAmmoWarning", "The HUD's low-ammo warning (`FUN_0049d3e0`): `lowAmmoWarningThreshold`, `bNoPartialReload`, and whether `ammoCounterClip` counts a clip (1) or not (0).", FieldKind::Nums(3),
        |d: &WeaponDef| format!("{} {} {}", d.low_ammo_threshold, d.no_partial_reload as u8, d.ammo_counter as u8),
        |d: &mut WeaponDef, v: Values| {
            v.need(3)?;
            (d.low_ammo_threshold, d.no_partial_reload, d.ammo_counter) = (v.num(0), v.flag(1), v.flag(2));
            Ok(())
        }),
    field!(present "DpadIcon", "BO2's `dpadIcon`: the action-slot material the alt-fire slot draws (`hud_gl_select`), empty when none.", FieldKind::Str,
        |d: &WeaponDef| d.dpad_icon.clone(),
        |d: &mut WeaponDef, v: Values| { d.dpad_icon = v.text.into(); Ok(()) }),
    // 5b family C: what others see of a shot (the third-person brass and the tracer).
    field!("WorldShellFxWeapon", "The FX weapon playing the third-person shell eject (`worldShellEjectEffect` at `tag_brass`).", FieldKind::Str,
        |d: &WeaponDef| d.world_shell_fx_weapon.clone(),
        |d: &mut WeaponDef, v: Values| { d.world_shell_fx_weapon = v.text.into(); Ok(()) }),
    field!("WorldLastShotShellFxWeapon", "The same on the round that empties the clip (`worldLastShotEjectEffect`).", FieldKind::Str,
        |d: &WeaponDef| d.world_last_shot_shell_fx_weapon.clone(),
        |d: &mut WeaponDef, v: Values| { d.world_last_shot_shell_fx_weapon = v.text.into(); Ok(()) }),
    field!("TracerFxWeapon", "The FX weapon drawing the bullet tracer (`tracerType`, `FUN_008eb420`).", FieldKind::Str,
        |d: &WeaponDef| d.tracer_fx_weapon.clone(),
        |d: &mut WeaponDef, v: Values| { d.tracer_fx_weapon = v.text.into(); Ok(()) }),
    field!(present "EnemyTracerFxWeapon", "The tracer a viewer the shooter is not friendly to sees (`enemyTracerType`, `FUN_004bbf10`).", FieldKind::Str,
        |d: &WeaponDef| d.enemy_tracer_fx_weapon.clone(),
        |d: &mut WeaponDef, v: Values| { d.enemy_tracer_fx_weapon = v.text.into(); Ok(()) }),
    field!("TracerInterval", "A tracer every this many bullets (`TracerDef.drawInterval`; 0: none).", FieldKind::Num,
        |d: &WeaponDef| d.tracer_interval.to_string(),
        |d: &mut WeaponDef, v: Values| { d.tracer_interval = v.text_num()? as u32; Ok(()) }),
    field!("Shield", "An Assault Shield's held, stowed and deployed collision boxes, each rotation (x y z w), position, mins and maxs in inches: the held and stowed on the posed body, the deployed in the planted model's own frame; empty for every other weapon.", FieldKind::Words,
        |d: &WeaponDef| d.shield.map_or(String::new(), |g| nums(&[g.held.to_nums(), g.stowed.to_nums(), g.deployed.to_nums()].concat())),
        |d: &mut WeaponDef, v: Values| {
            d.shield = if v.text.is_empty() {
                None
            } else {
                v.need(3 * ShieldBox::NUMS)?;
                let n = ShieldBox::NUMS;
                Some(ShieldGeometry { held: ShieldBox::from_nums(v.nums), stowed: ShieldBox::from_nums(&v.nums[n..]), deployed: ShieldBox::from_nums(&v.nums[2 * n..]) })
            };
            Ok(())
        }),
    field!(present "FireSoundNpc", "An offhand's third-person throw/plant BO2 sound event (`fireSound`): what other players hear; empty for a gun.", FieldKind::Str,
        |d: &WeaponDef| d.npc_fire_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.npc_fire_sound = v.text.into(); Ok(()) }),
    field!("Drop", "The weapon-drop rules' data: a carried killstreak gun (`bIsCarriedKillstreakWeapon`: never dropped, not counted among the primaries), spared by the dropped-weapon cleanup (`avoidDropCleanup`), not picked up prone (`blocksProne`).", FieldKind::Nums(3),
        |d: &WeaponDef| [d.drop.carried_killstreak, d.drop.avoid_drop_cleanup, d.drop.blocks_prone].map(|b| (b as u8).to_string()).join(" "),
        |d: &mut WeaponDef, v: Values| {
            v.need(3)?;
            d.drop = DropRules { carried_killstreak: v.flag(0), avoid_drop_cleanup: v.flag(1), blocks_prone: v.flag(2) };
            Ok(())
        }),
    field!("DropObject", "The Zero Hour object a dropped one lies on the ground as (`ZHBO_Drop_<base>`, its BO2 world model).", FieldKind::Str,
        |d: &WeaponDef| d.drop_object.clone(),
        |d: &mut WeaponDef, v: Values| { d.drop_object = v.text.into(); Ok(()) }),
    field!(present "HudIconRatio", "The HUD icon's shape (`hudIconRatio`: 0 1:1, 1 2:1, 2 4:1), as the pickup hint draws it.", FieldKind::Num,
        |d: &WeaponDef| d.hud_icon_ratio.to_string(),
        |d: &mut WeaponDef, v: Values| { d.hud_icon_ratio = v.text_num()? as u8; Ok(()) }),
    field!(present "PickupSound", "Picking the weapon up (`pickupSoundPlayer`, EV_ITEM_PICKUP).", FieldKind::Str,
        |d: &WeaponDef| d.pickup_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.pickup_sound = v.text.into(); Ok(()) }),
    field!(present "AmmoPickupSound", "Taking its ammo by walking over one (`ammoPickupSoundPlayer`, EV_AMMO_PICKUP).", FieldKind::Str,
        |d: &WeaponDef| d.ammo_pickup_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.ammo_pickup_sound = v.text.into(); Ok(()) }),
    field!(present "PickupSoundNpc", "What others hear of the pickup (`pickupSound`).", FieldKind::Str,
        |d: &WeaponDef| d.npc_pickup_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.npc_pickup_sound = v.text.into(); Ok(()) }),
    field!(present "AmmoPickupSoundNpc", "What others hear of the ammo pickup (`ammoPickupSound`).", FieldKind::Str,
        |d: &WeaponDef| d.npc_ammo_pickup_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.npc_ammo_pickup_sound = v.text.into(); Ok(()) }),
    field!(present "FireStartSoundNpc", "A gun's third-person BO2 sound event as its fire string opens (`fireStartSound`): what other players hear beside the shot; empty when BO2 names none.", FieldKind::Str,
        |d: &WeaponDef| d.npc_fire_start_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.npc_fire_start_sound = v.text.into(); Ok(()) }),
    field!(present "FireLastSoundNpc", "A gun's third-person BO2 sound event for the round that empties the magazine (`fireLastSound`): what other players hear instead of the shot; empty when BO2 names none.", FieldKind::Str,
        |d: &WeaponDef| d.npc_fire_last_sound.clone(),
        |d: &mut WeaponDef, v: Values| { d.npc_fire_last_sound = v.text.into(); Ok(()) }),
    field!("ScreenFx", "A smoke grenade's screen: the Zero Hour FXList of the cloud BO2's script plays where it goes off (`smoke_center_mp`'s effect); empty for everything else.", FieldKind::Str,
        |d: &WeaponDef| d.screen_fx.clone(),
        |d: &mut WeaponDef, v: Values| { d.screen_fx = v.text.into(); Ok(()) }),
    field!(present "HudIcon", "BO2's `hudIcon`: the weapon's HUD icon material (the class buttons, the editor's rows, the offhand counters draw it), empty when none.", FieldKind::Str,
        |d: &WeaponDef| d.hud_icon.clone(),
        |d: &mut WeaponDef, v: Values| { d.hud_icon = v.text.into(); Ok(()) }),
    field!(present "KillIcon", "BO2's `killIcon`: the obituary's icon material, empty when none (BO2 then draws `hud_obit_death_suicide`).", FieldKind::Str,
        |d: &WeaponDef| d.kill_icon.material.clone(),
        |d: &mut WeaponDef, v: Values| { d.kill_icon.material = v.text.into(); Ok(()) }),
    field!(present "KillIconShape", "The obituary icon's `killIconRatio` (0 1:1, 1 2:1, 2 4:1) and `flipKillIcon` (1: mirrored).", FieldKind::Nums(2),
        |d: &WeaponDef| format!("{} {}", d.kill_icon.ratio, d.kill_icon.flip as u8),
        |d: &mut WeaponDef, v: Values| { v.need(2)?; d.kill_icon.ratio = v.num(0) as u8; d.kill_icon.flip = v.num(1) != 0.0; Ok(()) }),
    field!(present "Indicator", "The grenade indicator: `projExplosion`, `bShowIndicator` (0/1) and `iIndicatorRadius` (inches).", FieldKind::Nums(3),
        |d: &WeaponDef| format!("{} {} {}", d.indicator.proj_explosion, d.indicator.show as u8, d.indicator.radius),
        |d: &mut WeaponDef, v: Values| { v.need(3)?; d.indicator.proj_explosion = v.num(0) as u8; d.indicator.show = v.num(1) != 0.0; d.indicator.radius = v.num(2); Ok(()) }),
    field!(present "IndicatorIcon", "BO2's `indicatorIcon`: the grenade indicator's icon material, empty for the default `hud_grenadeicon`.", FieldKind::Str,
        |d: &WeaponDef| d.indicator.icon.clone(),
        |d: &mut WeaponDef, v: Values| { d.indicator.icon = v.text.into(); Ok(()) }),
    field!(present "WorldWeapon", "The third-person weapon's own index (`W{index}{k}`, `ZW<index>.zbw`) when it is not the viewmodel's: an alt mode BO2 draws with its own worldModel; -1 for the viewmodel's.", FieldKind::Num,
        |d: &WeaponDef| d.world_weapon.to_string(),
        |d: &mut WeaponDef, v: Values| { d.world_weapon = v.text.parse().map_err(|_| "number")?; Ok(()) }),
];

/// The `Camo <id>` block: one BO2 camouflage option from `mp/attachmenttable.csv`'s `camo` rows.
pub const CAMO_FIELDS: &[Field<CamoOption>] = &[
    field!(present "Name", "The localized name the grid shows (`MPUI_*`).", FieldKind::Str,
        |c: &CamoOption| c.name.clone(),
        |c: &mut CamoOption, v: Values| { c.name = v.text.into(); Ok(()) }),
    field!(present "Image", "The camo thumbnail image/material the grid cell draws.", FieldKind::Str,
        |c: &CamoOption| c.image.clone(),
        |c: &mut CamoOption, v: Values| { c.image = v.text.into(); Ok(()) }),
    field!(present "Sort", "BO2's sort key (column 5 of the `camo` row).", FieldKind::Num,
        |c: &CamoOption| c.sort.to_string(),
        |c: &mut CamoOption, v: Values| { c.sort = v.text.parse().map_err(|_| "number")?; Ok(()) }),
    field!(present "Tab", "The personalize tab it sits on: base, dlc or dlc2 (`AddCamos`'s rule).", FieldKind::Enum,
        |c: &CamoOption| c.tab.name().into(),
        |c: &mut CamoOption, v: Values| { c.tab = OptionTab::parse(v.text).ok_or("unknown tab")?; Ok(()) }),
    field!(present "Mtx", "1 for a store option (column 16 names an `mtx_weapon*` item).", FieldKind::Num,
        |c: &CamoOption| (c.mtx as u8).to_string(),
        |c: &mut CamoOption, v: Values| { c.mtx = v.text.trim() == "1"; Ok(()) }),
];

/// Every `Perk` block line: one `specialty` row of `mp/statstable.csv` ([`PerkRow`]).
pub const PERK_FIELDS: &[Field<PerkRow>] = &[
    field!(present "Name", "The localized name (column 3, `PERKS_*`).", FieldKind::Str,
        |p: &PerkRow| p.name.clone(),
        |p: &mut PerkRow, v: Values| { p.name = v.text.into(); Ok(()) }),
    field!(present "Description", "The localized description (column 7, `PERKS_*_DESC`).", FieldKind::Str,
        |p: &PerkRow| p.description.clone(),
        |p: &mut PerkRow, v: Values| { p.description = v.text.into(); Ok(()) }),
    field!("Specialties", "The specialties it grants (column 4), by name.", FieldKind::Words,
        |p: &PerkRow| p.specialties.format(),
        |p: &mut PerkRow, v: Values| { p.specialties = Specialties::parse(v.text)?; Ok(()) }),
];

/// Every `BonusCard` block line: one `bonuscard` row of `mp/statstable.csv` ([`BonusCard`]).
pub const BONUS_CARD_FIELDS: &[Field<BonusCard>] = &[
    field!(present "Name", "The localized name (column 3, `BONUSCARD_*`).", FieldKind::Str,
        |b: &BonusCard| b.name.clone(),
        |b: &mut BonusCard, v: Values| { b.name = v.text.into(); Ok(()) }),
    field!(present "Description", "The localized description (column 7, `MENU_BONUSCARD_*_DESC`).", FieldKind::Str,
        |b: &BonusCard| b.description.clone(),
        |b: &mut BonusCard, v: Values| { b.description = v.text.into(); Ok(()) }),
    field!(present "Image", "The card image (column 6, `menu_mp_bonuscard_*`).", FieldKind::Str,
        |b: &BonusCard| b.image.clone(),
        |b: &mut BonusCard, v: Values| { b.image = v.text.into(); Ok(()) }),
    field!("Slot", "The loadout slot (column 13, `bonuscard1..7`; `Engine.GetLoadoutSlotForItem`).", FieldKind::Str,
        |b: &BonusCard| b.slot.clone(),
        |b: &mut BonusCard, v: Values| { b.slot = v.text.into(); Ok(()) }),
    field!("Cost", "The allocation cost (column 12).", FieldKind::Num,
        |b: &BonusCard| b.cost.to_string(),
        |b: &mut BonusCard, v: Values| { b.cost = v.text_num()? as i32; Ok(()) }),
    field!("Sort", "The sort key the grid orders by (column 9, `Engine.GetItemSortKey`).", FieldKind::Num,
        |b: &BonusCard| b.sort.to_string(),
        |b: &mut BonusCard, v: Values| { b.sort = v.text_num()? as i32; Ok(()) }),
    field!("Index", "The item index (column 0), the grid's tie-break.", FieldKind::Num,
        |b: &BonusCard| b.index.to_string(),
        |b: &mut BonusCard, v: Values| { b.index = v.text_num()? as i32; Ok(()) }),
    field!("UnlockLevel", "The level that unlocks it (column 10).", FieldKind::Num,
        |b: &BonusCard| b.unlock_level.to_string(),
        |b: &mut BonusCard, v: Values| { b.unlock_level = v.text_num()? as i32; Ok(()) }),
];

/// Every `UiText` block line: one localized UI string ([`UiText`]).
pub const UI_TEXT_FIELDS: &[Field<UiText>] = &[
    field!(present "Text", "The english text of the block's localize key.", FieldKind::Str,
        |u: &UiText| u.text.clone(),
        |u: &mut UiText, v: Values| { u.text = v.text.into(); Ok(()) }),
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

/// A value as the table writes it: verbatim, or in double quotes (with `\"` and `\\` escaped)
/// when the plain form would not read back exactly — surrounding whitespace (the reader trims
/// it; BO2's localized descriptions can end in spaces), a `;` (the comment marker) or a leading
/// quote. [`unquote`] is its inverse.
pub fn quote(value: &str) -> std::borrow::Cow<'_, str> {
    if value.trim() != value || value.contains(';') || value.starts_with('"') {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")).into()
    } else {
        value.into()
    }
}

/// A read value: a [`quote`]d one unescaped, any other as it is.
pub fn unquote(value: &str) -> std::borrow::Cow<'_, str> {
    let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).filter(|_| value.len() >= 2) else { return value.into() };
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        out.push(if c == '\\' { chars.next().unwrap_or('\\') } else { c });
    }
    out.into()
}

/// A line without its `;` comment; a `;` inside a quoted value is the value's.
fn strip_comment(raw: &str) -> &str {
    let (mut quoted, mut escaped) = (false, false);
    for (i, c) in raw.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            ';' if !quoted => return &raw[..i],
            _ => {}
        }
    }
    raw
}

/// Append one block's lines.
pub fn write_block<T>(out: &mut String, header: &str, id: &str, fields: &[Field<T>], t: &T) {
    out.push_str(header);
    out.push_str(id);
    out.push('\n');
    for f in fields {
        out.push_str(&format!("  {} = {}\n", f.key, quote(&(f.get)(t))));
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
        check(CAMO_FIELDS);
        assert_eq!(WEAPON_FIELDS.len(), 180);
        assert_eq!(ATTACHMENT_FIELDS.len(), 31);
        assert_eq!(CAMO_FIELDS.len(), 5);
    }

    /// A value the plain form would change — BO2's descriptions that end in spaces, a `;` (the
    /// comment marker), quotes and backslashes — is written quoted and reads back exactly; plain
    /// values stay unquoted, and a commented line still loses its comment.
    #[test]
    fn a_string_value_round_trips_exactly() {
        let texts = ["Aim faster after sprinting.  Mantle and climb faster. ", "  lead", "a; b", "\"q\" and \\ back\\", "plain text", ""];
        for t in texts {
            let p = PerkRow { stats: "X".into(), name: "N".into(), description: t.into(), ..Default::default() };
            let tab = Table { perks: vec![p], ..Default::default() };
            let text = format_full_table(&tab);
            let back = parse_full_table(&text).expect("re-parses");
            assert_eq!(back.perks[0].description, t, "{text}");
        }
        assert_eq!(quote("plain text"), "plain text");
        assert_eq!(quote("ends "), "\"ends \"");
        let commented = parse_full_table("Perk X ; a perk\n  Name = N ; the name\n  Description = \"x; y \" ; trailing\nEnd\n").unwrap();
        assert_eq!((commented.perks[0].name.as_str(), commented.perks[0].description.as_str()), ("N", "x; y "));
    }

    /// BO2's display names keep their exact text (the Tac-45 Dual Wield's ends in a space).
    #[test]
    fn a_display_name_with_a_trailing_space_round_trips() {
        let d = WeaponDef { id: "fnp45_dw_mp".into(), display_name: "Tac-45 Dual Wield ".into(), ..Default::default() };
        let back = parse_full_table(&format_full_table(&Table { weapons: vec![d], ..Default::default() })).expect("re-parses");
        assert_eq!(back.weapons[0].display_name, "Tac-45 Dual Wield ");
    }

    #[test]
    fn the_hud_hint_text_round_trips_as_ui_text() {
        let t = Table { ui_text: vec![UiText { key: "WEAPON_CANT_PLANT_EQUIPMENT".into(), text: "Cannot Plant Here".into() }], ..Default::default() };
        let back = parse_full_table(&format_full_table(&t)).expect("the table re-parses");
        assert_eq!(back.ui_text, t.ui_text);
        assert_eq!(crate::weapons::Catalog::from_table(back).ui_text("WEAPON_CANT_PLANT_EQUIPMENT"), Some("Cannot Plant Here"));
    }

    #[test]
    fn an_attachments_compatibility_list_round_trips() {
        let a = AttachmentDef { id: "reflex".into(), short: "reflex".into(), compatible: vec!["dualclip".into(), "gl".into()], ..Default::default() };
        let t = Table { attachments: vec![a.clone()], ..Default::default() };
        let back = parse_full_table(&format_full_table(&t)).expect("the table re-parses");
        assert_eq!(back.attachments[0].compatible, a.compatible);
    }

    #[test]
    fn the_silencer_fields_round_trip() {
        let mut d = WeaponDef { id: "an94_mp".into(), ..Default::default() };
        d.empty_fire_sound = "ZHBO_BO2_DryFireGeneric".into();
        d.silenced_fire_sound = "ZHBO_BO2_Fire_an94_mp_silenced".into();
        d.silenced_fire_sound_npc = "ZHBO_BO2_FireNpc_an94_mp_silenced".into();
        d.silenced_fx_weapon = "ZHBO_FX_an94_mp_silenced".into();
        d.silenced_view_flash_fx = "ZHBO_BO2FX_fx_muz_silencer_md".into();
        let text = format_table(std::slice::from_ref(&d));
        let back = parse_table(&text).expect("the table re-parses");
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].empty_fire_sound, d.empty_fire_sound);
        assert_eq!(back[0].silenced_fire_sound, d.silenced_fire_sound);
        assert_eq!(back[0].silenced_fire_sound_npc, d.silenced_fire_sound_npc);
        assert_eq!(back[0].silenced_fx_weapon, d.silenced_fx_weapon);
        assert_eq!(back[0].silenced_view_flash_fx, d.silenced_view_flash_fx);
    }

    #[test]
    fn the_def_sound_fields_round_trip() {
        let mut d = WeaponDef { id: "sig556_mp".into(), ..Default::default() };
        let ev = |a: &str| format!("ZHBO_BO2_Snd_{a}");
        d.raise_sound = ev("fly_generic_raise_plr");
        d.first_raise_sound = ev("fly_generic_first_raise_plr");
        d.alt_switch_sound = ev("wpn_weap_raise_plr");
        d.putaway_sound = ev("fly_generic_down_plr");
        d.ads_zoom_sound = ev("fly_scope_zoom");
        d.melee_swipe_sound = ev("wpn_melee_whoosh_plr");
        d.melee_hit_sound = ev("wpn_melee_knife_hit_body");
        d.fire_start_sound = ev("wpn_sig556_fire_plr_burst");
        d.fire_last_sound = ev("wpn_strela_fire_plr");
        d.pullback_sound = ev("wpn_semtex_charge");
        d.silenced_fire_start_sound = ev("null_sound");
        let back = parse_table(&format_table(std::slice::from_ref(&d))).expect("the table re-parses");
        assert_eq!(back, vec![d]);
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
    fn viewmodel_effect_names_round_trip() {
        let d = WeaponDef {
            id: "an94_mp".into(),
            view_flash_fx: "ZHBO_BO2FX_fx_muzzle".into(),
            view_shell_fx: "ZHBO_BO2FX_fx_rifle".into(),
            view_last_shot_shell_fx: "ZHBO_BO2FX_fx_rifle_last".into(),
            ..Default::default()
        };
        let text = format_table(&[d]);
        assert!(text.contains("ViewFlashFx = ZHBO_BO2FX_fx_muzzle"), "{text}");
        assert!(text.contains("ViewShellFx = ZHBO_BO2FX_fx_rifle"), "{text}");
        assert!(text.contains("ViewLastShotShellFx = ZHBO_BO2FX_fx_rifle_last"), "{text}");
        let back = parse_table(&text).unwrap();
        assert_eq!(back[0].view_flash_fx, "ZHBO_BO2FX_fx_muzzle");
        assert_eq!(back[0].view_shell_fx, "ZHBO_BO2FX_fx_rifle");
        assert_eq!(back[0].view_last_shot_shell_fx, "ZHBO_BO2FX_fx_rifle_last");
    }

    #[test]
    fn the_impact_bucket_round_trips() {
        // BO2 assigns every gun a bucket; the table carries it so the sim can name the row.
        let d = WeaponDef { id: "an94_mp".into(), impact_type: 2, ..Default::default() };
        let text = format_table(&[d]);
        assert!(text.contains("ImpactType = 2"), "{text}");
        assert_eq!(parse_table(&text).unwrap()[0].impact_type, 2);
    }

    #[test]
    fn specialties_and_perk_rows_round_trip() {
        use super::super::specialty::Specialty;
        let d = WeaponDef { id: "mp7_mp+fastmag".into(), specialties: Specialties::of(&[Specialty::FastReload]), ..Default::default() };
        let a = AttachmentDef { id: "rf".into(), specialties: Specialties::of(&[Specialty::Rof]), ..Default::default() };
        let t = Table {
            weapons: vec![d],
            attachments: vec![a],
            perks: vec![PerkRow {
                stats: "LIGHTWEIGHT".into(),
                name: "Lightweight".into(),
                description: "Move faster.  Take no damage when falling.".into(),
                specialties: Specialties::of(&[Specialty::MoveFaster, Specialty::FallHeight]),
            }],
            ..Default::default()
        };
        let text = format_full_table(&t);
        assert!(
            text.contains("Perk LIGHTWEIGHT\n  Name = Lightweight\n  Description = Move faster.  Take no damage when falling.\n  Specialties = fallheight movefaster\nEnd"),
            "{text}"
        );
        assert_eq!(parse_full_table(&text).unwrap(), t);
    }

    #[test]
    fn weapon_groups_round_trip() {
        let t = Table { weapon_groups: vec!["weapon_smg".into(), "weapon_assault".into()], class_caps: vec![("maxPerks".into(), 12)], ..Default::default() };
        let text = format_full_table(&t);
        assert!(text.contains("WeaponGroups = weapon_smg weapon_assault\nClassCap.maxPerks = 12\n"), "{text}");
        assert_eq!(parse_full_table(&text).unwrap(), t);
    }

    #[test]
    fn ui_text_round_trips() {
        let t = Table { ui_text: vec![UiText { key: "MPUI_SECOND_SPECIAL_GRENADE_CAPS".into(), text: "SECOND TACTICAL".into() }], ..Default::default() };
        let text = format_full_table(&t);
        assert!(text.contains("UiText MPUI_SECOND_SPECIAL_GRENADE_CAPS\n  Text = SECOND TACTICAL\nEnd"), "{text}");
        assert_eq!(parse_full_table(&text).unwrap(), t);
    }

    #[test]
    fn bonus_cards_round_trip() {
        let b = BonusCard {
            reference: "bonuscard_overkill".into(),
            name: "Overkill".into(),
            description: "Take a primary weapon as your second weapon.".into(),
            image: "menu_mp_bonuscard_overkill".into(),
            slot: "bonuscard3".into(),
            cost: 1,
            sort: 4,
            index: 180,
            unlock_level: 15,
        };
        let t = Table { bonus_cards: vec![b], ..Default::default() };
        let text = format_full_table(&t);
        assert!(text.contains("BonusCard bonuscard_overkill\n  Name = Overkill\n"), "{text}");
        assert!(text.contains("  Slot = bonuscard3\n  Cost = 1\n  Sort = 4\n  Index = 180\n  UnlockLevel = 15\nEnd"), "{text}");
        assert_eq!(parse_full_table(&text).unwrap(), t);
    }

    #[test]
    fn camo_options_and_the_weapon_camo_set_round_trip() {
        let d = WeaponDef {
            id: "an94_mp".into(),
            camo: vec!["camo_none".into(), "camo_multicam".into()],
            camo_material: "camo_an94".into(),
            ..Default::default()
        };
        let c = CamoOption { id: "camo_multicam".into(), name: "Multicam".into(), image: "menu_camo_multicam_pattern".into(), sort: 1, ..Default::default() };
        let t = Table { weapons: vec![d], camo: vec![c], ..Default::default() };
        let text = format_full_table(&t);
        assert!(text.contains("Camo camo_multicam\n  Name = Multicam\n  Image = menu_camo_multicam_pattern\n  Sort = 1\n  Tab = base\n  Mtx = 0\nEnd"), "{text}");
        assert!(text.contains("CamoList = camo_none camo_multicam"), "{text}");
        assert!(text.contains("CamoMaterial = camo_an94"), "{text}");
        assert_eq!(parse_full_table(&text).unwrap(), t);
    }

    #[test]
    fn reticle_options_and_colours_round_trip() {
        let r = CamoOption { id: "reticle_0".into(), name: "Dot".into(), ..Default::default() };
        let c = CamoOption { id: "reticle_color_1".into(), name: "Red".into(), ..Default::default() };
        let t = Table { reticles: vec![r], reticle_colors: vec![c], ..Default::default() };
        let text = format_full_table(&t);
        assert!(text.contains("ReticleOption reticle_0\n  Name = Dot\n  Image = \n  Sort = 0\n  Tab = base\n  Mtx = 0\nEnd"), "{text}");
        assert!(text.contains("ReticleColorOption reticle_color_1\n  Name = Red\n  Image = \n  Sort = 0\n  Tab = base\n  Mtx = 0\nEnd"), "{text}");
        assert_eq!(parse_full_table(&text).unwrap(), t);
    }

    #[test]
    fn overlay_material_round_trips() {
        let d = WeaponDef {
            id: "as50_mp".into(),
            overlay_material: "scope_overlay_xpr50".into(),
            overlay_material_low_res: "scope_overlay_xpr50_low_res".into(),
            overlay_reticle: 1,
            overlay_size: (480.0, 240.0),
            zoom: crate::weapons::Zoom { in_frac: 0.016, out_frac: 0.05, overlay_alpha_scale: 1.0, ..Default::default() },
            guided_missile_type: 6,
            ..Default::default()
        };
        let text = format_table(std::slice::from_ref(&d));
        assert!(text.contains("OverlayAlphaScale = 1"), "{text}");
        assert!(text.contains("OverlayMaterial = scope_overlay_xpr50"), "{text}");
        assert!(text.contains("OverlayMaterialLowRes = scope_overlay_xpr50_low_res"), "{text}");
        assert!(text.contains("OverlayReticle = 1"), "{text}");
        assert!(text.contains("OverlaySize = 480 240"), "{text}");
        assert!(text.contains("GuidedMissileType = 6"), "{text}");
        let back = parse_table(&text).unwrap();
        assert_eq!(back[0], d);
    }

    #[test]
    fn the_handling_fields_round_trip() {
        use super::super::handling::{CycleTimes, OffhandClass, Penetrate, Traits, Zoom};
        let d = WeaponDef {
            id: "dsr50_mp".into(),
            timing: CycleTimes { last_fire_time: 0.528, rechamber_time: 1.0, rechamber_bolt_time: 0.4, shots_before_rechamber: 4, bolt_action: true, reload_quick_time: 1.012, reload_start_add: 2, spin_down_time: 1.0, sprint_in_time: 0.25, ..Default::default() },
            zoom: Zoom { fov2: 12.5, fov3: 30.0, in_frac: 0.016, out_frac: 0.05, alt_scope_in_time: 0.15, dof: (2.2, 6.4), overlay_alpha_scale: 1.0, idle_amount: (60.0, 20.0), ..Default::default() },
            traits: Traits { dual_mag: true, rifle_bullet: true, ignore_attachments: true, stack_fire: (1.5, 1.0), min_player_damage: 5.0, penetrate: Penetrate::Large, offhand: OffhandClass::Gear, ..Default::default() },
            ..Default::default()
        };
        let text = format_table(std::slice::from_ref(&d));
        assert!(text.contains("FireCycle = 0.528 0 1 0.4 0 4 1"), "{text}");
        assert!(text.contains("Penetrate = large"), "{text}");
        assert!(text.contains("OffhandClass = gear"), "{text}");
        assert_eq!(parse_table(&text).unwrap()[0], d);
    }

    #[test]
    fn optional_attachment_fields_round_trip_as_dashes() {
        let a = AttachmentDef { id: "acog".into(), short: "acog".into(), kind: 1, fov2: Some(35.0), zoom_out_frac: Some(0.4), hip_idle: None, ..Default::default() };
        let t = Table { attachments: vec![a], ..Default::default() };
        let text = format_full_table(&t);
        assert!(text.contains("AdsZoom = 35 - - 0.4"), "{text}");
        assert!(text.contains("IdleAmount = 1 -"), "{text}");
        assert!(text.contains("SwayOverride = -"), "{text}");
        assert_eq!(parse_full_table(&text).unwrap(), t);
    }

    #[test]
    fn the_gun_feel_attachment_fields_round_trip() {
        let sway = Sway { max_angle: 1.5, lerp_speed: 5.0, pitch_scale: 0.5, yaw_scale: -0.5, horiz_scale: 0.075, vert_scale: 0.075 };
        let a = AttachmentDef {
            id: "steadyaim".into(),
            short: "steadyaim".into(),
            ads_kick_center_scale: 0.975,
            sway: Some(sway),
            stand_rot_pitch: Some(-0.1),
            stand_rot_roll: Some(0.0),
            strafe_rot_roll: Some(1.0),
            ..Default::default()
        };
        let t = Table { attachments: vec![a], ..Default::default() };
        let text = format_full_table(&t);
        assert!(text.contains("SwayOverride = 1.5 5 0.5 -0.5 0.075 0.075"), "{text}");
        assert!(text.contains("AdsSwayOverride = -"), "{text}");
        assert!(text.contains("ViewPosOverride = - -0.1 0 1"), "{text}");
        assert_eq!(parse_full_table(&text).unwrap(), t);
    }
    #[test]
    fn dpad_icon_round_trips() {
        let d = WeaponDef { id: "gl_an94_mp".into(), dpad_icon: "hud_gl_select".into(), ..Default::default() };
        let text = format_table(std::slice::from_ref(&d));
        assert!(text.contains("DpadIcon = hud_gl_select"), "{text}");
        assert_eq!(parse_table(&text).unwrap()[0], d);
    }
}
