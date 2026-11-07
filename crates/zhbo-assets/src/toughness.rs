/*
** Conquer Ops - converter: the General's health, as tough as Zero Hour's heroes.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! The General's health against Zero Hour's own weapons (the engine's damage path: its units, its
//! structures, its superweapons) comes from Zero Hour's heroes, read from the player's own install:
//! Colonel Burton, Black Lotus and Jarmen Kell (`zhbo_core::names::TOUGHNESS_HEROES`). Each hero's
//! `ActiveBody` `MaxHealth` and default `ArmorSet` armour are read from `INIZH.big`'s object INIs;
//! the General wears the same armour (`HumanArmor`, in `ZZZ_ZHBO_General.ini`), so a Zero Hour
//! weapon kills him in as many hits as a hero when his `MaxHealth` is theirs. The converter writes
//! their mean health times the tuning pack's `GeneralHeroToughness` (1: exactly a hero's) into each
//! General object's `MaxHealth` and `InitialHealth`.
//!
//! BO2's weapons do not see this number: a General takes a BO2 hit as its share of BO2's
//! `playerMaxHealth` of whatever health he has (`zhbo_core::damage::to_zh`), so BO2's
//! shots-to-kill hold at any toughness.

use std::collections::BTreeMap;
use std::path::Path;
use zhbo_formats::big::BigArchive;

/// One hero as the install defines him.
#[derive(Clone, Debug, PartialEq)]
pub struct Hero {
    pub object: String,
    pub max_health: f32,
    /// His default `ArmorSet`'s (`Conditions = None`) armour.
    pub armor: String,
}

/// What the General's health is derived from, and the result.
#[derive(Clone, Debug, PartialEq)]
pub struct Toughness {
    pub heroes: Vec<Hero>,
    /// The General's own default armour (the hand-written INI's).
    pub armor: String,
    /// The tuning pack's `GeneralHeroToughness`.
    pub scale: f32,
    /// The General's `MaxHealth`: the heroes' mean health times `scale`.
    pub health: f32,
}

impl Toughness {
    /// The converter's summary line.
    pub fn summary(&self) -> String {
        let heroes: Vec<String> = self.heroes.iter().map(|h| format!("{} {}", h.object, h.max_health)).collect();
        format!("General health {} ({} x {}, {})", self.health, heroes.join(" / "), self.scale, self.armor)
    }
}

/// A line without its comment, trimmed.
fn clean(line: &str) -> &str {
    line.split(';').next().unwrap_or("").trim()
}

/// `name`'s top-level block in `text` (`Object <name>` to the `End` at the start of a line).
pub fn object_block<'a>(text: &'a str, kind: &str, name: &str) -> Option<Vec<&'a str>> {
    let mut lines = text.lines();
    lines.by_ref().find(|l| {
        let mut w = clean(l).split_whitespace();
        w.next() == Some(kind) && w.next() == Some(name) && w.next().is_none()
    })?;
    let mut block = Vec::new();
    for l in lines {
        if !l.starts_with([' ', '\t']) && clean(l) == "End" {
            return Some(block);
        }
        block.push(l);
    }
    None
}

/// `key = value`'s value on a cleaned line, if the line sets `key`.
fn value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let (k, v) = clean(line).split_once('=')?;
    (k.trim() == key).then(|| v.trim())
}

/// An object's `ActiveBody` `MaxHealth`.
pub fn max_health(block: &[&str]) -> Option<f32> {
    let body = block.iter().position(|l| value(l, "Body").is_some_and(|v| v.starts_with("ActiveBody")))?;
    block[body + 1..].iter().find_map(|l| value(l, "MaxHealth")).and_then(|v| v.parse().ok())
}

/// An object's default armour: the `Armor` of its `ArmorSet` with `Conditions = None`.
pub fn default_armor(block: &[&str]) -> Option<String> {
    let mut i = 0;
    while i < block.len() {
        if clean(block[i]) == "ArmorSet" {
            let (mut none, mut armor) = (false, None);
            for l in &block[i + 1..] {
                if clean(l) == "End" {
                    break;
                }
                none |= value(l, "Conditions").is_some_and(|v| v.eq_ignore_ascii_case("None"));
                armor = armor.or(value(l, "Armor").map(str::to_string));
            }
            if none {
                return armor;
            }
        }
        i += 1;
    }
    None
}

/// Zero Hour's armour `name` from `Armor.ini`: each damage type's share (1 = 100 %), with
/// `DEFAULT` as every type's until a later line names one (`ArmorTemplate::parseArmorCoefficients`:
/// `DEFAULT` sets them all, in file order).
#[derive(Clone, Debug, PartialEq)]
pub struct Armor {
    pub default: f32,
    pub types: BTreeMap<String, f32>,
}

impl Armor {
    pub fn parse(armor_ini: &str, name: &str) -> Option<Armor> {
        let block = object_block(armor_ini, "Armor", name)?;
        let mut a = Armor { default: 1.0, types: BTreeMap::new() };
        for l in block {
            let Some(v) = value(l, "Armor") else { continue };
            let mut w = v.split_whitespace();
            let (Some(kind), Some(pct)) = (w.next(), w.next()) else { continue };
            let Ok(pct) = pct.trim_end_matches('%').parse::<f32>() else { continue };
            let kind = kind.to_ascii_uppercase();
            if kind == "DEFAULT" {
                a.types.clear();
                a.default = pct / 100.0;
            } else {
                a.types.insert(kind, pct / 100.0);
            }
        }
        Some(a)
    }

    /// The share of `damage_type` this armour lets through (Zero Hour reads type names without
    /// regard to case: `Weapon.ini` writes `Gattling`).
    pub fn share(&self, damage_type: &str) -> f32 {
        self.types.get(&damage_type.to_ascii_uppercase()).copied().unwrap_or(self.default)
    }
}

/// The General's health from the install's object INIs (`objects`: every `Data\INI\Object` text)
/// and the hand-written General INI: each hero of `heroes` defined once, every one in the
/// General's own armour (a hero in other armour would need a per-damage-type weighting this does
/// not guess at), their mean `MaxHealth` times `scale`.
pub fn derive(objects: &[String], general_ini: &str, general: &str, heroes: &[&str], scale: f32) -> Result<Toughness, String> {
    let general_block = object_block(general_ini, "Object", general).ok_or_else(|| format!("{general} is not in the General INI"))?;
    let armor = default_armor(&general_block).ok_or_else(|| format!("{general} has no default ArmorSet"))?;
    let mut found = Vec::new();
    for &object in heroes {
        let blocks: Vec<Vec<&str>> = objects.iter().filter_map(|t| object_block(t, "Object", object)).collect();
        let [block] = blocks.as_slice() else {
            return Err(format!("{object} is defined {} times in the install's object INIs", blocks.len()));
        };
        let max_health = max_health(block).ok_or_else(|| format!("{object} has no ActiveBody MaxHealth"))?;
        let hero_armor = default_armor(block).ok_or_else(|| format!("{object} has no default ArmorSet"))?;
        if hero_armor != armor {
            return Err(format!("{object} wears {hero_armor}, the General {armor}: their health alone does not compare"));
        }
        found.push(Hero { object: object.to_string(), max_health, armor: hero_armor });
    }
    if found.is_empty() {
        return Err("no heroes to compare the General with".into());
    }
    let mean = found.iter().map(|h| h.max_health).sum::<f32>() / found.len() as f32;
    Ok(Toughness { heroes: found, armor, scale, health: mean * scale })
}

/// Every General object of `ini` with `health` as its `MaxHealth` and `InitialHealth`: the lines
/// of its `ActiveBody` set in place (each object must have both; the block runs to the `End` at
/// the start of a line, as [`object_block`] reads it).
pub fn apply(ini: &str, objects: &[&str], health: f32) -> Result<String, String> {
    let v = format!("{health:.1}");
    let mut out = String::with_capacity(ini.len());
    let (mut inside, mut set) = (false, 0usize);
    let mut done = Vec::new();
    for line in ini.split_inclusive('\n') {
        let c = clean(line);
        if !line.starts_with([' ', '\t']) {
            let mut w = c.split_whitespace();
            if w.next() == Some("Object") {
                inside = w.next().is_some_and(|n| objects.contains(&n));
                set = 0;
            } else if c == "End" && inside {
                if set != 2 {
                    return Err("a General object without MaxHealth and InitialHealth".into());
                }
                done.push(());
                inside = false;
            }
        }
        if inside && (value(line, "MaxHealth").is_some() || value(line, "InitialHealth").is_some()) {
            let indent = &line[..line.len() - line.trim_start().len()];
            let key = c.split('=').next().unwrap_or("").trim();
            let eol = if line.ends_with("\r\n") { "\r\n" } else if line.ends_with('\n') { "\n" } else { "" };
            out.push_str(&format!("{indent}{key:<19} = {v}{eol}"));
            set += 1;
            continue;
        }
        out.push_str(line);
    }
    if done.len() != objects.len() {
        return Err(format!("{} of {} General objects found", done.len(), objects.len()));
    }
    Ok(out)
}

/// The install's `INIZH.big` texts under `prefix` (lower case, `\` separators) ending in `.ini`.
pub fn install_inis(zh_dir: &Path, prefix: &str) -> Result<Vec<(String, String)>, String> {
    let big = BigArchive::open(zh_dir.join("INIZH.big")).map_err(|e| format!("INIZH.big: {e}"))?;
    let mut out = Vec::new();
    for e in big.entries() {
        let lower = e.path.to_ascii_lowercase();
        if lower.starts_with(prefix) && lower.ends_with(".ini") {
            let bytes = big.read(e).map_err(|err| format!("{}: {err}", e.path))?;
            out.push((e.path.clone(), String::from_utf8_lossy(&bytes).into_owned()));
        }
    }
    Ok(out)
}

/// [`derive`] over the player's install, for the General INI `general_ini`.
pub fn from_install(zh_dir: &Path, general_ini: &str) -> Result<Toughness, String> {
    let objects: Vec<String> = install_inis(zh_dir, "data\\ini\\object\\")?.into_iter().map(|(_, t)| t).collect();
    let general = crate::generals::FACTIONS[0].object;
    derive(&objects, general_ini, general, &zhbo_core::names::TOUGHNESS_HEROES, zhbo_core::tuning::Tuning::default().general_hero_toughness)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEROES: &str = "Object HeroA\n  ArmorSet\n    Conditions      = None\n    Armor           = HumanArmor\n  End\n  ArmorSet\n    Conditions      = PLAYER_UPGRADE\n    Armor           = ChemSuitHumanArmor\n  End\n  Body = ActiveBody ModuleTag_02\n    MaxHealth       = 200.0 ; a hero\n    InitialHealth   = 200.0\n  End\nEnd\r\n\nObject HeroB\n  ArmorSet\n    Conditions = None\n    Armor = HumanArmor\n  End\n  Body = ActiveBody ModuleTag_02\n    MaxHealth = 260.0\n  End\nEnd\n\nObject Tank\n  ArmorSet\n    Conditions = None\n    Armor = TankArmor\n  End\n  Body = ActiveBody ModuleTag_02\n    MaxHealth = 480.0\n  End\nEnd\n";

    #[test]
    fn the_general_is_the_heroes_mean_times_the_knob() {
        let general = crate::generals::base_ini();
        let g = crate::generals::FACTIONS[0].object;
        let t = derive(&[HEROES.to_string()], general, g, &["HeroA", "HeroB"], 1.0).unwrap();
        assert_eq!(t.armor, "HumanArmor");
        assert_eq!(t.heroes.iter().map(|h| h.max_health).collect::<Vec<_>>(), [200.0, 260.0], "the default ArmorSet's, past the comment");
        assert_eq!(t.health, 230.0);
        assert_eq!(derive(&[HEROES.to_string()], general, g, &["HeroA", "HeroB"], 1.5).unwrap().health, 345.0);
        // A hero in other armour, or missing, or twice, is refused rather than guessed at.
        assert!(derive(&[HEROES.to_string()], general, g, &["HeroA", "Tank"], 1.0).unwrap_err().contains("TankArmor"));
        assert!(derive(&[HEROES.to_string()], general, g, &["Nobody"], 1.0).is_err());
        assert!(derive(&[HEROES.to_string(), HEROES.to_string()], general, g, &["HeroA"], 1.0).is_err());
    }

    #[test]
    fn every_general_object_gets_the_health() {
        let objects: Vec<&str> = crate::generals::FACTIONS.iter().map(|f| f.object).collect();
        let out = apply(crate::generals::base_ini(), &objects, 230.0).unwrap();
        for o in &objects {
            let block = object_block(&out, "Object", o).unwrap();
            assert_eq!(max_health(&block), Some(230.0), "{o}");
            assert!(block.iter().any(|l| value(l, "InitialHealth") == Some("230.0")), "{o}");
        }
        assert_eq!(out.matches("MaxHealth").count(), crate::generals::base_ini().matches("MaxHealth").count(), "set, not added");
    }

    #[test]
    fn an_armour_reads_its_default_then_its_types() {
        let ini = "Armor NoArmor\n  Armor = DEFAULT 100%\n  Armor = HAZARD_CLEANUP 0%\nEnd\nArmor HumanArmor\n  Armor = CRUSH 200%\n  Armor = SNIPER 200% ; ouch\nEnd\n";
        let human = Armor::parse(ini, "HumanArmor").unwrap();
        assert_eq!((human.share("SNIPER"), human.share("SMALL_ARMS"), human.share("Crush")), (2.0, 1.0, 2.0));
        assert_eq!(Armor::parse(ini, "NoArmor").unwrap().share("HAZARD_CLEANUP"), 0.0);
    }
}
