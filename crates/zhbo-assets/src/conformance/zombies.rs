//! The mod's side of the Zombies table (`zhbo_conformance::zombies`): what the core plays, by the
//! core's own code over the install's rule table (installed as the archive installs it), and the
//! wall buys and machines its round engine stands.

use zhbo_conformance::schema::{Row, Schema};
use zhbo_conformance::zombies::{float_row, int_row, text_row, PLAYERS, ROUNDS, TOTAL_ROUNDS};

/// Every actual Zombies row for the install at `bo2_dir`.
pub fn rows(bo2_dir: &std::path::Path, schema: &Schema) -> Result<Vec<Row>, String> {
    zhbo_core::zombies::install_rules(&crate::zombies::Zombies::rule_table(bo2_dir)?);
    let vars = zhbo_core::zombies::Vars::get();
    let mut out = Vec::new();
    for r in ROUNDS {
        out.push(int_row(schema, "round_health", &r.to_string(), vars.health(r) as i64)?);
        out.push(float_row(schema, "spawn_delay", &r.to_string(), vars.spawn_delay_in(r) as f64)?);
    }
    for r in TOTAL_ROUNDS {
        for p in PLAYERS {
            out.push(int_row(schema, "round_total", &format!("{r}:{p}"), vars.zombie_total(r, p) as i64)?);
        }
    }
    for p in PLAYERS {
        out.push(int_row(schema, "points", &format!("kill_{p}p"), vars.score_kill[p as usize - 1] as i64)?);
        out.push(int_row(schema, "points", &format!("score_start_{p}p"), vars.score_start[p as usize - 1] as i64)?);
    }
    for (name, v) in [("damage", vars.score_damage), ("melee_bonus", vars.score_bonus_melee), ("head_bonus", vars.score_bonus_head), ("neck_bonus", vars.score_bonus_neck), ("torso_bonus", vars.score_bonus_torso)] {
        out.push(int_row(schema, "points", name, v as i64)?);
    }
    out.push(int_row(schema, "points", "start", vars.starting_points() as i64)?);
    out.push(float_row(schema, "points", "penalty_downed", vars.penalty_downed as f64)?);
    for b in &zhbo_core::zombies::WallBuys::get().buys {
        out.push(text_row(schema, "wallbuy", &b.weapon, &b.model)?);
    }
    for p in &zhbo_core::zombies::Perks::get().perks {
        out.push(text_row(schema, "perk", &p.specialty, &p.model)?);
    }
    out.push(text_row(schema, "perk", "specialty_weapupgrade", &zhbo_core::zombies::PackAPunch::get().model)?);
    Ok(out)
}
