/*
** Conquer Ops - the graphics detail data: the mod's GameLOD overrides (data/Data/INI/GameLOD*).
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/
//! Zero Hour's graphics detail is two INI files: `GameLODPresets.ini` (which level the detector
//! recommends for the machine) and `GameLOD.ini` (what each static level sets, and the dynamic
//! levels the render frame rate picks). The mod ships overrides of both in `ConquerOps.big`
//! (`Data\INI\GameLODPresets\ZHBO_GameLODPresets.ini`, `Data\INI\GameLOD\ZHBO_GameLOD.ini`), loaded
//! after the stock files; a block there changes only the fields it names. This module reads that
//! dialect so the tests can hold the data to its rules: hardware the 2002 presets cannot classify
//! is recommended High (host patch 0025), no frame rate thins an effect that carries gameplay, and
//! the two dynamic fields that reach the logic (`DebrisSkipMask`, `SlowDeathScale`) stay no-ops,
//! since the dynamic level follows each machine's own frame rate and a lockstep sim must not.

/// The mod's override of `GameLODPresets.ini`.
pub const PRESETS_INI: &str = include_str!("../../../data/Data/INI/GameLODPresets/ZHBO_GameLODPresets.ini");
/// The mod's override of `GameLOD.ini`.
pub const LEVELS_INI: &str = include_str!("../../../data/Data/INI/GameLOD/ZHBO_GameLOD.ini");

/// Zero Hour's `ParticlePriorityType`, lowest first (`ParticlePriorityNames` without `NONE`).
pub const PRIORITIES: [&str; 13] = [
    "WEAPON_EXPLOSION",
    "SCORCHMARK",
    "DUST_TRAIL",
    "BUILDUP",
    "DEBRIS_TRAIL",
    "UNIT_DAMAGE_FX",
    "DEATH_EXPLOSION",
    "SEMI_CONSTANT",
    "CONSTANT",
    "WEAPON_TRAIL",
    "AREA_EFFECT",
    "CRITICAL",
    "ALWAYS_RENDER",
];

/// A priority's rank in [`PRIORITIES`] (`None` for a name Zero Hour does not know).
pub fn rank(priority: &str) -> Option<usize> {
    PRIORITIES.iter().position(|p| p.eq_ignore_ascii_case(priority))
}

/// The static levels a recommendation may name (`StaticGameLODNames` without `Custom`).
pub const LEVELS: [&str; 4] = ["Low", "Medium", "High", "VeryHigh"];

/// One `Kind = Name ... End` block: its fields in file order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Block {
    pub kind: String,
    pub name: String,
    pub fields: Vec<(String, String)>,
}

impl Block {
    /// The last value of `key` (case-insensitive), as the engine's parse leaves it.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields.iter().rev().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }
}

/// A GameLOD-dialect file: its blocks and its one-line statements (`ReallyLowMHz = 600`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LodIni {
    pub blocks: Vec<Block>,
    pub statements: Vec<(String, String)>,
}

/// The blocks that run to an `End` (`StaticGameLOD`, `DynamicGameLOD`); every other line with a
/// value is a one-line statement (`BenchProfile`, `LODPreset`, `ReallyLowMHz`,
/// `UnclassifiedHardwareLOD`).
const BLOCK_KINDS: [&str; 2] = ["StaticGameLOD", "DynamicGameLOD"];

/// A value without its `;` comment or the stray `;` the stock file ends some values with.
fn clean(v: &str) -> String {
    v.split(';').next().unwrap_or("").trim().to_string()
}

/// Read the GameLOD dialect: `Kind = Name` opens a block, `End` (also `End;...`) closes it, a
/// field is `Key = Value`, `;` starts a comment.
pub fn parse(text: &str) -> Result<LodIni, String> {
    let mut ini = LodIni::default();
    let mut open: Option<Block> = None;
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.eq_ignore_ascii_case("End") {
            ini.blocks.push(open.take().ok_or_else(|| format!("line {}: End outside a block", n + 1))?);
            continue;
        }
        let (key, value) = line.split_once('=').ok_or_else(|| format!("line {}: no '=' in {line:?}", n + 1))?;
        let (key, value) = (key.trim(), clean(value));
        match &mut open {
            Some(b) => b.fields.push((key.to_string(), value)),
            None if BLOCK_KINDS.iter().any(|k| k.eq_ignore_ascii_case(key)) => {
                open = Some(Block { kind: key.to_string(), name: value, fields: Vec::new() })
            }
            None => ini.statements.push((key.to_string(), value)),
        }
    }
    if let Some(b) = open {
        return Err(format!("{} {} has no End", b.kind, b.name));
    }
    Ok(ini)
}

impl LodIni {
    /// The block `kind name` (case-insensitive), if the file has one.
    pub fn block(&self, kind: &str, name: &str) -> Option<&Block> {
        self.blocks.iter().find(|b| b.kind.eq_ignore_ascii_case(kind) && b.name.eq_ignore_ascii_case(name))
    }

    /// The last value of the statement `key`.
    pub fn statement(&self, key: &str) -> Option<&str> {
        self.statements.iter().rev().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }

    /// `self` with `over` loaded after it, as the engine loads a directory's files after the stock
    /// file (`INI_LOAD_OVERWRITE`): a block of the same kind and name takes the fields it names and
    /// keeps the rest; a new block or statement is added.
    pub fn overlaid(&self, over: &LodIni) -> LodIni {
        let mut out = self.clone();
        for b in &over.blocks {
            match out.blocks.iter_mut().find(|o| o.kind.eq_ignore_ascii_case(&b.kind) && o.name.eq_ignore_ascii_case(&b.name)) {
                Some(o) => o.fields.extend(b.fields.iter().cloned()),
                None => out.blocks.push(b.clone()),
            }
        }
        out.statements.extend(over.statements.iter().cloned());
        out
    }
}

/// The rules every effective set of dynamic levels must keep, as messages for each break.
/// * `DebrisSkipMask` 0 and `SlowDeathScale` 1 at every level: both are read by the logic
///   (`ObjectCreationList`'s debris, `SlowDeathBehavior`), and the dynamic level follows the
///   machine's own frame rate, so anything else would desync a lockstep game.
/// * Every priority in `kept` is drawn at every level (at or above `MinParticlePriority`) and never
///   skipped (at or above `MinParticleSkipPriority`, or a zero `ParticleSkipMask`).
pub fn dynamic_rule_breaks(levels: &LodIni, kept: &[&str]) -> Vec<String> {
    let mut breaks = Vec::new();
    for b in levels.blocks.iter().filter(|b| b.kind.eq_ignore_ascii_case("DynamicGameLOD")) {
        let int = |k: &str| b.get(k).map(|v| v.parse::<i64>().map_err(|e| format!("{}: {k} {v:?}: {e}", b.name)));
        match int("DebrisSkipMask") {
            Some(Ok(0)) | None => {}
            Some(v) => breaks.push(format!("{}: DebrisSkipMask {v:?} skips logic debris by frame rate", b.name)),
        }
        match b.get("SlowDeathScale").map(|v| v.parse::<f64>()) {
            Some(Ok(1.0)) | None => {}
            Some(v) => breaks.push(format!("{}: SlowDeathScale {v:?} scales logic deaths by frame rate", b.name)),
        }
        let skip_mask = match int("ParticleSkipMask") {
            Some(Ok(m)) => m,
            None => 0,
            Some(Err(e)) => {
                breaks.push(e);
                0
            }
        };
        for p in kept {
            let r = rank(p).unwrap_or_else(|| panic!("{p}: not a Zero Hour particle priority"));
            let at = |k: &str| b.get(k).map(|v| rank(v).ok_or_else(|| format!("{}: {k} {v:?} is no priority", b.name)));
            match at("MinParticlePriority") {
                Some(Ok(min)) if r < min => breaks.push(format!("{}: {p} is not drawn (MinParticlePriority {})", b.name, PRIORITIES[min])),
                Some(Err(e)) => breaks.push(e),
                _ => {}
            }
            match at("MinParticleSkipPriority") {
                Some(Ok(min)) if skip_mask != 0 && r < min => {
                    breaks.push(format!("{}: {p} is thinned (ParticleSkipMask {skip_mask} below {})", b.name, PRIORITIES[min]))
                }
                Some(Err(e)) => breaks.push(e),
                _ => {}
            }
        }
    }
    breaks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::Role;

    /// The converted effects whose particles carry gameplay: they must look the same at every frame
    /// rate (the smoke screen blocks sight in the sim whatever the frame rate).
    fn gameplay_priorities() -> Vec<&'static str> {
        vec![Role::Area.priority()]
    }

    /// Zero Hour's stock dynamic levels, as `INIZH.big`'s `GameLOD.ini` ships them (the fields the
    /// rules read; the real-install test below reads the file itself).
    const STOCK_DYNAMIC: &str = "\
DynamicGameLOD = VeryHigh
  MinimumFPS=25
  ParticleSkipMask=0;
  DebrisSkipMask=0;
  SlowDeathScale=1.0
  MinParticlePriority=WEAPON_EXPLOSION
  MinParticleSkipPriority=CRITICAL
End
DynamicGameLOD = High
  MinimumFPS=20
  ParticleSkipMask=0;
  DebrisSkipMask=0;
  SlowDeathScale=1.0
  MinParticlePriority=UNIT_DAMAGE_FX
  MinParticleSkipPriority=CRITICAL
End
DynamicGameLOD = Medium
  MinimumFPS=10
  ParticleSkipMask=1; ;only generate particles when lower counter bits equal mask
  DebrisSkipMask=0;
  SlowDeathScale=1.0
  MinParticlePriority=WEAPON_TRAIL
  MinParticleSkipPriority=CRITICAL
End
DynamicGameLOD = Low
  MinimumFPS=0
  ParticleSkipMask=3;
  DebrisSkipMask=0;
  SlowDeathScale=1.0
  MinParticlePriority=AREA_EFFECT ;priority at below which we don't draw particles
  MinParticleSkipPriority=CRITICAL
End";

    #[test]
    fn the_mod_recommends_high_for_hardware_the_2002_presets_cannot_classify() {
        let presets = parse(PRESETS_INI).unwrap();
        assert_eq!(presets.statement("UnclassifiedHardwareLOD"), Some("High"));
        for (key, value) in &presets.statements {
            assert_eq!(key, "UnclassifiedHardwareLOD", "the override adds no 2002 hardware presets");
            assert!(LEVELS.contains(&value.as_str()), "{value}: a level the patch's parse accepts (not Custom)");
        }
        assert!(presets.blocks.is_empty());
    }

    #[test]
    fn the_parse_reads_the_stock_dialect() {
        let stock = parse(STOCK_DYNAMIC).unwrap();
        assert_eq!(stock.blocks.len(), 4);
        let low = stock.block("DynamicGameLOD", "Low").unwrap();
        assert_eq!(low.get("ParticleSkipMask"), Some("3"), "the trailing ';' is not the value's");
        assert_eq!(low.get("MinParticlePriority"), Some("AREA_EFFECT"));
        assert!(parse("DynamicGameLOD = Low\n  ParticleSkipMask = 1\n").is_err(), "no End");
    }

    #[test]
    fn the_stock_levels_thin_the_smoke_screen_below_20_fps() {
        // Why the override exists: stock Zero Hour skips every particle under CRITICAL by the skip
        // mask, the screen's AREA_EFFECT cloud among them.
        let breaks = dynamic_rule_breaks(&parse(STOCK_DYNAMIC).unwrap(), &gameplay_priorities());
        assert_eq!(breaks.len(), 2, "{breaks:#?}");
        assert!(breaks.iter().all(|b| b.contains("AREA_EFFECT is thinned")), "{breaks:#?}");
    }

    #[test]
    fn no_frame_rate_thins_a_gameplay_effect_or_touches_the_logic() {
        let levels = parse(LEVELS_INI).unwrap();
        for b in &levels.blocks {
            assert_eq!(b.kind, "DynamicGameLOD", "the override changes only the dynamic levels");
            assert!(["VeryHigh", "High", "Medium", "Low"].contains(&b.name.as_str()), "{}: a level the engine has", b.name);
        }
        let effective = parse(STOCK_DYNAMIC).unwrap().overlaid(&levels);
        let breaks = dynamic_rule_breaks(&effective, &gameplay_priorities());
        assert!(breaks.is_empty(), "{breaks:#?}");
    }

    #[test]
    fn the_rules_catch_a_logic_field_that_varies() {
        let bad = parse("DynamicGameLOD = Low\n  DebrisSkipMask=1\n  SlowDeathScale=0.5\nEnd\n").unwrap();
        assert_eq!(dynamic_rule_breaks(&bad, &[]).len(), 2);
    }

    /// The same rules over the player's own `GameLOD.ini` (Zero Hour's `INIZH.big`) with the mod's
    /// override on top, as the engine loads them: `ZHBO_ZH_DIR=... cargo test -p zhbo-assets
    /// the_installs_dynamic_levels -- --ignored`. Also: the High static level the mod recommends
    /// draws at full texture resolution with room for many grenades.
    #[test]
    #[ignore = "reads this machine's Zero Hour install (ZHBO_ZH_DIR)"]
    fn the_installs_dynamic_levels_keep_the_rules() {
        let zh = zhbo_requirements::check_requirements().zero_hour.expect("a Zero Hour install").path;
        let big = zhbo_formats::big::BigArchive::open(zh.join("INIZH.big")).unwrap();
        let entry = big.find("Data\\INI\\GameLOD.ini").expect("INIZH.big has GameLOD.ini");
        let stock = parse(&String::from_utf8_lossy(&big.read(entry).unwrap())).unwrap();
        assert_eq!(stock.blocks.iter().filter(|b| b.kind == "DynamicGameLOD").count(), 4);
        let effective = stock.overlaid(&parse(LEVELS_INI).unwrap());
        let breaks = dynamic_rule_breaks(&effective, &gameplay_priorities());
        assert!(breaks.is_empty(), "{breaks:#?}");
        let level = parse(PRESETS_INI).unwrap().statement("UnclassifiedHardwareLOD").unwrap().to_string();
        let high = effective.block("StaticGameLOD", &level).expect("the recommended level is defined");
        assert_eq!(high.get("TextureReductionFactor"), Some("0"), "full-resolution textures");
        let cap: u32 = high.get("MaxParticleCount").unwrap().parse().unwrap();
        assert!(cap >= 3000, "{level}: particle cap {cap}");
    }
}
