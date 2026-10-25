//! The differ: joins `expected.csv` and `actual.csv` on their two key columns and reports every
//! mismatch, clustered by area and column, then by a signature that groups the same kind of gap
//! across rows (the mod side empty, a constant ratio, the same items missing from a set).
//!
//! Comparison is exact for text, ints, bools, lists and sets; a float column compares within its
//! own epsilon (the schema's, never the differ's). There is no allowlist: every difference is a
//! parity gap.

use crate::csv::Table;
use crate::schema::{Kind, Schema};
use std::collections::{BTreeMap, BTreeSet};

/// One cell that differs.
#[derive(Clone, Debug, PartialEq)]
pub struct Mismatch {
    pub weapon: String,
    pub combo: String,
    pub area: String,
    pub column: String,
    pub expected: String,
    pub actual: String,
    /// What kind of gap this is, shared by every row with the same gap in this column.
    pub signature: String,
}

/// The mismatches of one column, grouped by signature (largest group first).
#[derive(Clone, Debug, PartialEq)]
pub struct Cluster {
    pub area: String,
    pub column: String,
    pub count: usize,
    /// `(signature, rows, example keys)`.
    pub groups: Vec<(String, usize, Vec<String>)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub schema: String,
    pub rows_expected: usize,
    pub rows_actual: usize,
    /// Keys only BO2 has (a combination the mod cannot build) and only the mod has.
    pub only_expected: Vec<String>,
    pub only_actual: Vec<String>,
    pub mismatches: Vec<Mismatch>,
    pub clusters: Vec<Cluster>,
}

impl Report {
    /// Clusters plus the two coverage gaps: the differ passes only when this is zero.
    pub fn gap_count(&self) -> usize {
        self.clusters.len() + usize::from(!self.only_expected.is_empty()) + usize::from(!self.only_actual.is_empty())
    }
}

const EXAMPLES: usize = 4;

fn key_text(weapon: &str, combo: &str) -> String {
    if combo.is_empty() {
        weapon.to_string()
    } else {
        format!("{weapon}+{combo}")
    }
}

fn parse(s: &str) -> Option<f64> {
    s.parse::<f64>().ok()
}

fn floats(s: &str) -> Option<Vec<f64>> {
    if s.is_empty() {
        return Some(Vec::new());
    }
    s.split('|').map(parse).collect()
}

/// Whether two cells of `kind` are equal.
pub fn equal(kind: Kind, e: &str, a: &str) -> bool {
    match kind {
        Kind::Float(eps) => match (parse(e), parse(a)) {
            (Some(x), Some(y)) => (x - y).abs() <= eps,
            _ => e == a,
        },
        Kind::Floats(eps) => match (floats(e), floats(a)) {
            (Some(x), Some(y)) => x.len() == y.len() && x.iter().zip(&y).all(|(p, q)| (p - q).abs() <= eps),
            _ => e == a,
        },
        Kind::Set => {
            let x: BTreeSet<&str> = e.split('|').filter(|s| !s.is_empty()).collect();
            let y: BTreeSet<&str> = a.split('|').filter(|s| !s.is_empty()).collect();
            x == y
        }
        Kind::Text | Kind::Int | Kind::Bool | Kind::List => e == a,
    }
}

/// The gap's signature: rows sharing it are the same defect.
fn signature(kind: Kind, e: &str, a: &str) -> String {
    if a.is_empty() {
        return "mod side empty".to_string();
    }
    if e.is_empty() {
        return format!("BO2 empty, mod `{}`", short(a));
    }
    match kind {
        Kind::Float(_) | Kind::Int => match (parse(e), parse(a)) {
            (Some(x), Some(y)) if x != 0.0 => format!("mod = BO2 × {}", sig4(y / x)),
            (Some(_), Some(y)) => format!("BO2 0, mod {}", sig4(y)),
            _ => format!("`{}` → `{}`", short(e), short(a)),
        },
        Kind::Set | Kind::List => {
            let x: Vec<&str> = e.split('|').collect();
            let y: Vec<&str> = a.split('|').collect();
            let missing: Vec<&str> = x.iter().copied().filter(|v| !y.contains(v)).collect();
            let extra: Vec<&str> = y.iter().copied().filter(|v| !x.contains(v)).collect();
            if missing.is_empty() && extra.is_empty() {
                "same items, different order".to_string()
            } else {
                let mut parts = Vec::new();
                if !missing.is_empty() {
                    parts.push(format!("mod lacks {}", short(&missing.join("|"))));
                }
                if !extra.is_empty() {
                    parts.push(format!("mod adds {}", short(&extra.join("|"))));
                }
                parts.join("; ")
            }
        }
        _ => format!("`{}` → `{}`", short(e), short(a)),
    }
}

fn sig4(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return format!("{v}");
    }
    let digits = (4 - 1 - v.abs().log10().floor() as i32).max(0) as usize;
    format!("{v:.digits$}")
}

fn short(s: &str) -> String {
    const MAX: usize = 120;
    if s.chars().count() <= MAX {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(MAX).collect::<String>())
    }
}

/// Diff two tables written under `schema`.
pub fn diff(schema: &Schema, expected: &Table, actual: &Table) -> Result<Report, String> {
    let header = schema.header();
    for (name, t) in [("expected", expected), ("actual", actual)] {
        if t.header != header {
            return Err(format!("{name}.csv's header is not the {} schema's", schema.name));
        }
    }
    let index = |t: &Table| -> Result<BTreeMap<(String, String), usize>, String> {
        let mut m = BTreeMap::new();
        for (i, r) in t.rows.iter().enumerate() {
            if m.insert((r[0].clone(), r[1].clone()), i).is_some() {
                return Err(format!("duplicate key {} {}", r[0], r[1]));
            }
        }
        Ok(m)
    };
    let (ei, ai) = (index(expected)?, index(actual)?);
    let mut report = Report { schema: schema.name.to_string(), rows_expected: expected.rows.len(), rows_actual: actual.rows.len(), ..Default::default() };
    report.only_expected = ei.keys().filter(|k| !ai.contains_key(*k)).map(|(w, c)| key_text(w, c)).collect();
    report.only_actual = ai.keys().filter(|k| !ei.contains_key(*k)).map(|(w, c)| key_text(w, c)).collect();
    for (k, &i) in &ei {
        let Some(&j) = ai.get(k) else { continue };
        let (er, ar) = (&expected.rows[i], &actual.rows[j]);
        for (c, col) in schema.columns.iter().enumerate() {
            let (e, a) = (&er[c + 2], &ar[c + 2]);
            if !equal(col.kind, e, a) {
                report.mismatches.push(Mismatch {
                    weapon: k.0.clone(),
                    combo: k.1.clone(),
                    area: col.area.to_string(),
                    column: col.name.clone(),
                    expected: e.clone(),
                    actual: a.clone(),
                    signature: signature(col.kind, e, a),
                });
            }
        }
    }
    // Cluster: per column (schema order), groups per signature, largest first.
    for col in &schema.columns {
        let rows: Vec<&Mismatch> = report.mismatches.iter().filter(|m| m.column == col.name).collect();
        if rows.is_empty() {
            continue;
        }
        let mut groups: BTreeMap<&str, (usize, Vec<String>)> = BTreeMap::new();
        for m in &rows {
            let g = groups.entry(m.signature.as_str()).or_default();
            g.0 += 1;
            if g.1.len() < EXAMPLES {
                g.1.push(key_text(&m.weapon, &m.combo));
            }
        }
        let mut groups: Vec<(String, usize, Vec<String>)> = groups.into_iter().map(|(s, (n, ex))| (s.to_string(), n, ex)).collect();
        groups.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        report.clusters.push(Cluster { area: col.area.to_string(), column: col.name.clone(), count: rows.len(), groups });
    }
    Ok(report)
}

/// The long-form mismatch list (`weapon,combo,area,column,expected,actual,signature`), one line per
/// differing cell, for SQL/CSV tools.
pub fn mismatches_csv(report: &Report) -> String {
    let mut s = Schema::new("diff", ["weapon", "combo"]);
    for c in ["area", "column", "expected", "actual", "signature"] {
        s.col(c, "diff", Kind::Text, "");
    }
    let rows: Vec<crate::schema::Row> = report
        .mismatches
        .iter()
        .map(|m| crate::schema::Row {
            key: (m.weapon.clone(), m.combo.clone()),
            cells: vec![m.area.clone(), m.column.clone(), m.expected.clone(), m.actual.clone(), m.signature.clone()],
        })
        .collect();
    crate::csv::write(&s, &rows)
}

/// The clustered report as Markdown: totals, coverage, then each area and column's gap groups.
pub fn markdown(report: &Report) -> String {
    let mut s = format!("# Conformance report: `{}`\n\n", report.schema);
    s.push_str(&format!(
        "{} expected rows, {} actual rows, {} differing cells in {} column clusters; {} rows only BO2 has, {} only the mod has.\n\n",
        report.rows_expected,
        report.rows_actual,
        report.mismatches.len(),
        report.clusters.len(),
        report.only_expected.len(),
        report.only_actual.len()
    ));
    if report.gap_count() == 0 {
        s.push_str("**No gaps: the mod matches BO2 on every column of every row.**\n");
        return s;
    }
    let mut areas: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for c in &report.clusters {
        let a = areas.entry(c.area.as_str()).or_default();
        a.0 += 1;
        a.1 += c.count;
    }
    s.push_str("## By area\n\n| area | columns | cells |\n|---|---|---|\n");
    let mut by_area: Vec<_> = areas.into_iter().collect();
    by_area.sort_by_key(|a| std::cmp::Reverse(a.1 .1));
    for (a, (cols, cells)) in &by_area {
        s.push_str(&format!("| {a} | {cols} | {cells} |\n"));
    }
    for (name, keys) in [("Rows only BO2 has", &report.only_expected), ("Rows only the mod has", &report.only_actual)] {
        if keys.is_empty() {
            continue;
        }
        s.push_str(&format!("\n## {name} ({})\n\n", keys.len()));
        for k in keys.iter().take(40) {
            s.push_str(&format!("- `{k}`\n"));
        }
        if keys.len() > 40 {
            s.push_str(&format!("- … {} more (see the CSVs)\n", keys.len() - 40));
        }
    }
    for (area, _) in &by_area {
        s.push_str(&format!("\n## {area}\n"));
        for c in report.clusters.iter().filter(|c| c.area == *area) {
            s.push_str(&format!("\n### `{}` — {} rows, {} kinds of gap\n\n", c.column, c.count, c.groups.len()));
            for (sig, n, ex) in c.groups.iter().take(12) {
                s.push_str(&format!("- {n} × {sig} (e.g. {})\n", ex.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(", ")));
            }
            if c.groups.len() > 12 {
                s.push_str(&format!("- … {} more kinds (see diff.csv)\n", c.groups.len() - 12));
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csv::{read, write};
    use crate::schema::{RowBuilder, F32_EPS};

    fn schema() -> Schema {
        let mut s = Schema::new("weapons", ["weapon", "combo"]);
        s.col("fire_ms", "timing", Kind::Float(1e-3), "")
            .col("hide", "hide", Kind::Set, "")
            .col("parts", "parts", Kind::List, "")
            .col("fov", "ads", Kind::Floats(F32_EPS), "")
            .col("sound", "sounds", Kind::Text, "");
        s
    }

    fn row(s: &Schema, w: &str, c: &str, fire: f64, hide: &[&str], parts: &[&str], sound: &str) -> crate::schema::Row {
        let mut b = RowBuilder::new(s, w, c);
        b.float("fire_ms", fire).set("hide", hide).list("parts", parts).floats("fov", &[1.0, 2.0]);
        if sound.is_empty() {
            b.absent("sound");
        } else {
            b.text("sound", sound);
        }
        b.finish().unwrap()
    }

    #[test]
    fn epsilon_sets_lists_and_coverage() {
        let s = schema();
        let e = read(&write(&s, &[
            row(&s, "a_mp", "", 100.0, &["t1", "t2"], &["m", "n"], "fire_a"),
            row(&s, "a_mp", "reflex", 90.0, &["t1"], &["m"], "fire_a"),
            row(&s, "b_mp", "", 50.0, &[], &[], "fire_b"),
        ]))
        .unwrap();
        let a = read(&write(&s, &[
            // Within epsilon; the set in another order; the list reordered is a gap.
            row(&s, "a_mp", "", 100.0004, &["t2", "t1"], &["n", "m"], "fire_a"),
            // 0.5 ms off; the mod hides one tag more; the sound is missing.
            row(&s, "a_mp", "reflex", 90.5, &["t1", "t9"], &["m"], ""),
            row(&s, "c_mp", "", 50.0, &[], &[], "fire_c"),
        ]))
        .unwrap();
        let r = diff(&s, &e, &a).unwrap();
        assert_eq!(r.only_expected, vec!["b_mp"]);
        assert_eq!(r.only_actual, vec!["c_mp"]);
        let cols: Vec<&str> = r.mismatches.iter().map(|m| m.column.as_str()).collect();
        assert_eq!(cols, vec!["parts", "fire_ms", "hide", "sound"]);
        let sig = |c: &str| r.mismatches.iter().find(|m| m.column == c).unwrap().signature.clone();
        assert_eq!(sig("parts"), "same items, different order");
        assert_eq!(sig("hide"), "mod adds t9");
        assert_eq!(sig("sound"), "mod side empty");
        assert!(sig("fire_ms").starts_with("mod = BO2 × 1.00"), "{}", sig("fire_ms"));
        assert_eq!(r.gap_count(), 4 + 2);
        assert!(markdown(&r).contains("### `hide` — 1 rows"));
    }

    #[test]
    fn identical_tables_have_no_gaps() {
        let s = schema();
        let t = read(&write(&s, &[row(&s, "a_mp", "", 100.0, &["t1"], &["m"], "x")])).unwrap();
        let r = diff(&s, &t, &t).unwrap();
        assert_eq!(r.gap_count(), 0);
        assert!(markdown(&r).contains("No gaps"));
    }

    #[test]
    fn a_foreign_header_is_refused() {
        let s = schema();
        let t = read("weapon,combo,other\nx,,1\n").unwrap();
        assert!(diff(&s, &t, &t).is_err());
    }
}
