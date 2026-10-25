//! The BO2 parity conformance run: `expected.csv` from the player's BO2 install by BO2's own rules
//! (`zhbo_conformance::bo2`), `actual.csv` from what the mod ships ([`actual`]), and the clustered
//! diff between them.
//!
//! The CSVs are large generated artifacts: [`run`] refuses to write them inside the repository.
//! Development tool only: it reads the player's install and never launches or connects to BO2.

pub mod actual;

use std::path::{Path, PathBuf};
use zhbo_conformance::{csv, diff, schema::Schema};

/// One table's outcome.
pub struct Table {
    pub name: &'static str,
    pub report: diff::Report,
}

/// The repository root this crate was built from (two levels above its manifest).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Whether `dir` is the repository or inside it.
pub fn inside_repo(dir: &Path) -> bool {
    let (Ok(root), Ok(dir)) = (repo_root().canonicalize(), absolute(dir)) else { return false };
    dir.starts_with(root)
}

/// `dir` made absolute and canonical as far as it exists (the rest appended), so a directory not
/// yet created is still placed correctly.
fn absolute(dir: &Path) -> std::io::Result<PathBuf> {
    let dir = if dir.is_absolute() { dir.to_path_buf() } else { std::env::current_dir()?.join(dir) };
    let mut existing = dir.clone();
    let mut rest = Vec::new();
    while !existing.exists() {
        match (existing.file_name().map(|f| f.to_os_string()), existing.parent().map(Path::to_path_buf)) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                existing = parent;
            }
            _ => break,
        }
    }
    let mut out = existing.canonicalize()?;
    for r in rest.into_iter().rev() {
        out.push(r);
    }
    Ok(out)
}

fn write(dir: &Path, name: &str, text: &str) -> Result<(), String> {
    std::fs::write(dir.join(name), text).map_err(|e| format!("{}: {e}", dir.join(name).display()))
}

/// Diff one table and write its files: `<name>-expected.csv`, `<name>-actual.csv`,
/// `<name>-diff.csv`, `<name>-schema.csv`, `<name>-report.md`.
fn table(dir: &Path, name: &'static str, schema: &Schema, expected: &[zhbo_conformance::schema::Row], actual: &[zhbo_conformance::schema::Row]) -> Result<Table, String> {
    let (e, a) = (csv::write(schema, expected), csv::write(schema, actual));
    write(dir, &format!("{name}-expected.csv"), &e)?;
    write(dir, &format!("{name}-actual.csv"), &a)?;
    write(dir, &format!("{name}-schema.csv"), &csv::write_schema(schema))?;
    let report = diff::diff(schema, &csv::read(&e)?, &csv::read(&a)?)?;
    write(dir, &format!("{name}-diff.csv"), &diff::mismatches_csv(&report))?;
    write(dir, &format!("{name}-report.md"), &diff::markdown(&report))?;
    Ok(Table { name, report })
}

/// Run the whole comparison into `out_dir` (created; must be outside the repository).
pub fn run(bo2_dir: &Path, out_dir: &Path) -> Result<Vec<Table>, String> {
    if inside_repo(out_dir) {
        return Err(format!("{} is inside the repository; the conformance CSVs are generated artifacts — choose a directory outside it", out_dir.display()));
    }
    std::fs::create_dir_all(out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let weapons = zhbo_conformance::weapons::schema();
    let expected = zhbo_conformance::bo2::weapon_rows(bo2_dir)?;
    let shipped = actual::Shipped::convert(bo2_dir)?;
    let actual = actual::rows(&shipped, &weapons)?;
    Ok(vec![table(out_dir, "weapons", &weapons, &expected, &actual)?])
}

/// One line per table, for the CLI and the test's failure message.
pub fn summary(tables: &[Table], out_dir: &Path) -> String {
    let mut s = String::new();
    for t in tables {
        let r = &t.report;
        s.push_str(&format!(
            "{}: {} expected rows, {} actual, {} differing cells in {} column clusters, {} rows only BO2 has, {} only the mod has — {}\n",
            t.name,
            r.rows_expected,
            r.rows_actual,
            r.mismatches.len(),
            r.clusters.len(),
            r.only_expected.len(),
            r.only_actual.len(),
            out_dir.join(format!("{}-report.md", t.name)).display()
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_output_must_live_outside_the_repository() {
        assert!(inside_repo(&repo_root()));
        assert!(inside_repo(&repo_root().join("target").join("conformance-not-yet-created")));
        assert!(!inside_repo(&std::env::temp_dir().join("zhbo-conformance-test")));
    }

    /// The conformance gate: every column of every row matches BO2, and both sides have the same
    /// rows. It fails while any diff cluster exists — the initial backlog is the layer's proof.
    /// The CSVs and the report go to `ZHBO_CONFORMANCE_OUT` (default: the system temp dir).
    #[test]
    #[ignore = "reads this machine's Black Ops II install (ZHBO_BO2_DIR); converts it in memory (~80 s release)"]
    fn conformance() {
        let bo2 = std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR to the Black Ops II install");
        let out = std::env::var("ZHBO_CONFORMANCE_OUT").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir().join("zhbo-conformance"));
        let tables = run(Path::new(&bo2), &out).expect("the conformance run completes");
        let gaps: usize = tables.iter().map(|t| t.report.gap_count()).sum();
        assert_eq!(gaps, 0, "BO2 parity gaps:\n{}", summary(&tables, &out));
    }
}
