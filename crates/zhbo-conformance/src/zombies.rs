//! The Zombies table: TranZit classic's facts the player's install holds, by BO2's own rules, each
//! row keyed by its kind and item and holding one value (an integer, a number or a text).
//!
//! * **What stands on the map** (TranZit's map entities, `zm_transit.ff`'s `MapEnts`):
//!   - `wallbuy/<weapon>`: the wall buys `init_spawnable_weapon_upgrade` keeps for
//!     `zclassic_transit` (`_zm_weapons.gsc:839-881`: the targetnames, a `script_noteworthy` empty or
//!     naming it), each the model its target struct shows (`:914-918`);
//!   - `perk/<specialty>`: the perk machines `perk_machine_spawn_init` keeps for
//!     `zclassic_perks_transit` (their `script_string`), each its model; Pack-a-Punch is
//!     `specialty_weapupgrade`.
//! * **The rules** (`mp/zombiemode.csv`, `common_zm`, read by `set_zombie_var`'s rule with
//!   difficulty 1, column 2; its defaults from `_zm.gsc` `init_levelvars`, `:850-945`):
//!   - `round_health/<round>`: `ai_calculate_health` (`_zm.gsc:3566-3584`);
//!   - `round_total/<round>:<players>`: `round_spawning`'s total with `default_max_zombie_func`
//!     (`:2872-2949`);
//!   - `spawn_delay/<round>`: `zombie_spawn_delay`, decayed each round by `round_think` (`:3496-3501`);
//!   - `points/<name>`: the kill points by players, the hit, the bonuses, the starting points and the
//!     share lost going down.
//!
//! The expected side here reads the install only; the mod's side is `zhbo-assets::conformance`.
//! What BO2 keeps only in compiled script (prices, timings) is not in any asset the oracle reads.

use crate::schema::{Kind, Row, RowBuilder, Schema};
use zhbo_formats::mapents::{get, Entity};

/// The rounds and player counts both sides enumerate.
pub const ROUNDS: std::ops::RangeInclusive<u32> = 1..=60;
pub const TOTAL_ROUNDS: std::ops::RangeInclusive<u32> = 1..=30;
pub const PLAYERS: std::ops::RangeInclusive<u32> = 1..=4;

pub fn schema() -> Schema {
    let mut s = Schema::new("zombies", ["kind", "item"]);
    s.col("int", "value", Kind::Int, "An integer fact (health, a count, points).");
    s.col("float", "value", Kind::Float(1e-4), "A number (seconds, a share).");
    s.col("text", "value", Kind::Text, "A name (a model).").asset();
    s
}

/// One row of each value type.
pub fn int_row(schema: &Schema, kind: &str, item: &str, v: i64) -> Result<Row, String> {
    let mut b = RowBuilder::new(schema, kind, item);
    b.int("int", v).absent("float").absent("text");
    b.finish()
}

pub fn float_row(schema: &Schema, kind: &str, item: &str, v: f64) -> Result<Row, String> {
    let mut b = RowBuilder::new(schema, kind, item);
    b.absent("int").float("float", v).absent("text");
    b.finish()
}

pub fn text_row(schema: &Schema, kind: &str, item: &str, v: &str) -> Result<Row, String> {
    let mut b = RowBuilder::new(schema, kind, item);
    b.absent("int").absent("float").text("text", v);
    b.finish()
}

/// `set_zombie_var( var, default, is_float, column )` over the rule table's rows.
struct Vars<'a> {
    rows: &'a [Vec<String>],
}

impl Vars<'_> {
    fn cell(&self, name: &str, column: usize) -> Option<&str> {
        self.rows.iter().find(|r| r.first().is_some_and(|c| c == name)).and_then(|r| r.get(column)).map(|s| s.trim()).filter(|s| !s.is_empty())
    }
    fn int(&self, name: &str, default: i64, column: usize) -> i64 {
        self.cell(name, column).and_then(|c| c.parse::<f64>().ok()).map_or(default, |v| v as i64)
    }
    fn float(&self, name: &str, default: f32, column: usize) -> f32 {
        self.cell(name, column).and_then(|c| c.parse::<f32>().ok()).unwrap_or(default)
    }
}

/// `init_levelvars`' column: `int( difficulty ) + 1`, difficulty 1.
const COLUMN: usize = 2;

/// `ai_calculate_health( round )`.
fn health(v: &Vars, round: u32) -> i64 {
    let start = v.int("zombie_health_start", 150, COLUMN);
    let increase = v.int("zombie_health_increase", 100, COLUMN);
    let multiplier = v.float("zombie_health_increase_multiplier", 0.1, COLUMN);
    let mut h = start;
    for i in 2..=round {
        if i >= 10 {
            let old = h;
            h += (h as f32 * multiplier) as i64;
            if h < old {
                return old;
            }
        } else {
            h = (h as f32 + increase as f32) as i64;
        }
    }
    h
}

/// `round_spawning`'s total for `players`, then `default_max_zombie_func`.
fn total(v: &Vars, round: u32, players: u32) -> i64 {
    let max_ai = v.int("zombie_max_ai", 24, COLUMN) as f32;
    let per_player = v.int("zombie_ai_per_player", 6, COLUMN) as f32;
    let mut multiplier = round as f32 / 5.0;
    if multiplier < 1.0 {
        multiplier = 1.0;
    }
    if round >= 10 {
        multiplier *= round as f32 * 0.15;
    }
    let max = if players == 1 { max_ai as i64 + (0.5 * per_player * multiplier) as i64 } else { max_ai as i64 + ((players - 1) as f32 * per_player * multiplier) as i64 };
    let share = match round {
        r if r < 2 => 0.25,
        r if r < 3 => 0.3,
        r if r < 4 => 0.5,
        r if r < 5 => 0.7,
        r if r < 6 => 0.9,
        _ => return max,
    };
    (max as f32 * share) as i64
}

/// `zombie_spawn_delay` in `round`: its start, then each round before it `round_think`'s decay.
fn spawn_delay(v: &Vars, round: u32) -> f32 {
    let mut timer = v.float("zombie_spawn_delay", 2.0, COLUMN);
    for _ in 1..round {
        if timer > 0.08 {
            timer *= 0.95;
        } else if timer < 0.08 {
            timer = 0.08;
        }
    }
    timer
}

/// The wall buys and perk machines TranZit classic stands, from its map entities.
fn map_rows(schema: &Schema, ents: &[Entity]) -> Result<Vec<Row>, String> {
    let model_of = |target: Option<&str>| target.and_then(|t| ents.iter().find(|e| get(e, "targetname") == Some(t))).and_then(|e| get(e, "model")).unwrap_or("");
    let names = |s: Option<&str>, sep: char| s.unwrap_or("").split(sep).map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect::<Vec<_>>();
    let mut rows = Vec::new();
    const WALL_TARGETS: [&str; 6] = ["weapon_upgrade", "bowie_upgrade", "sickle_upgrade", "tazer_upgrade", "buildable_wallbuy", "claymore_purchase"];
    for e in ents.iter().filter(|e| get(e, "targetname").is_some_and(|t| WALL_TARGETS.contains(&t))) {
        let Some(weapon) = get(e, "zombie_weapon_upgrade") else { continue };
        let modes = names(get(e, "script_noteworthy"), ',');
        if !modes.is_empty() && !modes.iter().any(|m| m == "zclassic_transit") {
            continue;
        }
        rows.push(text_row(schema, "wallbuy", weapon, model_of(get(e, "target")))?);
    }
    for e in ents.iter().filter(|e| get(e, "targetname") == Some("zm_perk_machine")) {
        if !names(get(e, "script_string"), ' ').iter().any(|m| m == "zclassic_perks_transit") {
            continue;
        }
        let Some(perk) = get(e, "script_noteworthy") else { continue };
        rows.push(text_row(schema, "perk", perk, get(e, "model").unwrap_or(""))?);
    }
    Ok(rows)
}

/// The rule rows from the rule table's `rows`.
pub fn rule_rows(schema: &Schema, rows: &[Vec<String>]) -> Result<Vec<Row>, String> {
    let v = Vars { rows };
    let mut out = Vec::new();
    for r in ROUNDS {
        out.push(int_row(schema, "round_health", &r.to_string(), health(&v, r))?);
        out.push(float_row(schema, "spawn_delay", &r.to_string(), spawn_delay(&v, r) as f64)?);
    }
    for r in TOTAL_ROUNDS {
        for p in PLAYERS {
            out.push(int_row(schema, "round_total", &format!("{r}:{p}"), total(&v, r, p))?);
        }
    }
    for p in PLAYERS {
        out.push(int_row(schema, "points", &format!("kill_{p}p"), v.int(&format!("zombie_score_kill_{p}player"), 50, 1))?);
        out.push(int_row(schema, "points", &format!("score_start_{p}p"), v.int(&format!("zombie_score_start_{p}p"), 500, COLUMN))?);
    }
    for (name, default) in [("damage", ("zombie_score_damage_normal", 10)), ("melee_bonus", ("zombie_score_bonus_melee", 80)), ("head_bonus", ("zombie_score_bonus_head", 50)), ("neck_bonus", ("zombie_score_bonus_neck", 20)), ("torso_bonus", ("zombie_score_bonus_torso", 10))] {
        out.push(int_row(schema, "points", name, v.int(default.0, default.1, 1))?);
    }
    // `level.player_starting_points = round_number * 500` (`_zm.gsc:839`), round 1.
    out.push(int_row(schema, "points", "start", 500)?);
    out.push(float_row(schema, "points", "penalty_downed", v.float("penalty_downed", 0.05, COLUMN) as f64)?);
    Ok(out)
}

/// Every expected Zombies row for the install at `dir`.
pub fn expected(dir: &std::path::Path) -> Result<Vec<Row>, String> {
    let schema = schema();
    let common = zhbo_formats::t6zone::load_file(&zhbo_formats::t6zone::install_path(dir, "common_zm"))?;
    let table = zhbo_formats::stringtable::rows(&common, "mp/zombiemode.csv").map_err(|e| e.to_string())?.ok_or("common_zm has no mp/zombiemode.csv")?;
    let mut rows = rule_rows(&schema, &table)?;
    let map = zhbo_formats::t6zone::load_file(&zhbo_formats::t6zone::install_path(dir, "zm_transit"))?;
    let ents = zhbo_formats::mapents::entities(&map).map_err(|e| e.to_string())?.ok_or("zm_transit has no map entities")?;
    rows.extend(map_rows(&schema, &ents)?);
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(rows: &[(&str, &str)]) -> Vec<Vec<String>> {
        rows.iter().map(|(n, v)| vec![n.to_string(), v.to_string(), v.to_string()]).collect()
    }

    #[test]
    fn bo2s_rounds_by_its_defaults() {
        let t = table(&[]);
        let v = Vars { rows: &t };
        assert_eq!((health(&v, 1), health(&v, 2), health(&v, 9), health(&v, 10)), (150, 250, 950, 1045));
        assert_eq!((total(&v, 1, 1), total(&v, 1, 2), total(&v, 6, 4)), (6, 7, 24 + (3.0f32 * 6.0 * 1.2) as i64));
        assert!((spawn_delay(&v, 2) - 1.9).abs() < 1e-5);
        let late = spawn_delay(&v, 200);
        assert!((late - 0.08).abs() < 1e-6, "the floor");
    }

    #[test]
    fn a_table_value_overrides_the_default() {
        let t = table(&[("zombie_health_start", "200")]);
        assert_eq!(health(&Vars { rows: &t }, 1), 200);
    }

    #[test]
    fn the_map_keeps_classics_wall_buys_and_perks() {
        let ents = zhbo_formats::mapents::parse(
            r#"{ "targetname" "weapon_upgrade" "zombie_weapon_upgrade" "m14_zm" "target" "a" }
               { "targetname" "a" "model" "t6_wpn_ar_m14_world" }
               { "targetname" "weapon_upgrade" "zombie_weapon_upgrade" "mp5k_zm" "script_noteworthy" "zgrief_town" "target" "b" }
               { "targetname" "zm_perk_machine" "script_noteworthy" "specialty_armorvest" "model" "zombie_vending_jugg" "script_string" "zclassic_perks_transit zgrief_perks_town" }
               { "targetname" "zm_perk_machine" "script_noteworthy" "specialty_rof" "model" "zombie_vending_doubletap2" "script_string" "zgrief_perks_farm" }"#,
        );
        let rows = map_rows(&schema(), &ents).unwrap();
        let keys: Vec<(String, String)> = rows.iter().map(|r| r.key.clone()).collect();
        assert_eq!(keys, vec![("wallbuy".into(), "m14_zm".into()), ("perk".into(), "specialty_armorvest".into())]);
    }
}
