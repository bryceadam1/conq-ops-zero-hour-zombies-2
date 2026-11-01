//! The native-art audit: every converted weapon in the table — base weapons, every attachment
//! combination create-a-class can build, alt modes — loaded the way the game loads it, and checked
//! to draw.
//!
//! First person: [`zhbo_core::nativevm::load`] for each viewmodel and faction (`ZV<vm><F>.zbv`, the
//! name the bridge asks for), fed over the same file protocol the bridge serves; the composed hip
//! mesh and the ADS part list's mesh must each carry at least one surface. BO2 hides the whole
//! viewmodel under a scope overlay at run time, never in the art, so every weapon must draw at both
//! ends here. Third person: [`zhbo_core::nativebody::load`] in weapon mode for each world manifest
//! (`ZW<vm>.zbw`), which must compose at least one surface, and each faction's body once.
//!
//! A weapon whose art the converter skipped (its `viewmodel` is −1) has nothing to check. Read-only
//! over a file lookup, so the same audit runs on a converted archive (`zhbo-assets audit-native`)
//! and on the converter's output in memory (the real-install test).

use zhbo_core::nativebody::{self, Mode};
use zhbo_core::nativevm::{self, Files};
use zhbo_core::weapons::WeaponDef;

/// The faction letters the converter writes arms for (`generals::FACTIONS`).
fn factions() -> impl Iterator<Item = u8> {
    crate::generals::FACTIONS.iter().map(|f| f.letter as u8)
}

/// How many file rounds a load may take (the bridge's protocol: each round answers every request).
const ROUNDS: usize = 64;

/// One weapon's failure.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub weapon: String,
    pub what: String,
}

/// What the audit saw.
#[derive(Debug, Default)]
pub struct Report {
    /// First-person loads checked (viewmodel × faction) and third-person world manifests checked.
    pub first_person: usize,
    pub third_person: usize,
    pub problems: Vec<Problem>,
}

/// Run `step` over `files` until it finishes, answering its requests from `read`.
fn drive<T>(files: &mut Files, read: &dyn Fn(&str) -> Option<Vec<u8>>, mut step: impl FnMut(&mut Files) -> Result<Option<T>, String>) -> Result<T, String> {
    for _ in 0..ROUNDS {
        if let Some(done) = step(files)? {
            return Ok(done);
        }
        let wanted: Vec<String> = files.wanted().map(str::to_string).collect();
        if wanted.is_empty() {
            return Err("waiting with nothing requested".into());
        }
        for p in wanted {
            let bytes = read(&p);
            files.provide(&p, bytes.as_deref());
        }
    }
    Err(format!("not loaded after {ROUNDS} rounds"))
}

/// Audit every converted weapon of `weapons` against the files `read` returns.
pub fn audit(weapons: &[WeaponDef], read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Report {
    let mut report = Report::default();
    // One cache for the whole run, as the game keeps one: a load forgets the blobs it decoded, and
    // the next round asks again.
    let mut files = Files::default();
    let mut seen_vm = std::collections::BTreeSet::new();
    for d in weapons.iter().filter(|d| d.viewmodel >= 0) {
        // Alt modes and aliases share their parent's viewmodel: one check each.
        if !seen_vm.insert(d.viewmodel) {
            continue;
        }
        if seen_vm.len() % 1000 == 0 {
            eprintln!("native audit: {} viewmodels", seen_vm.len());
        }
        let mut fail = |what: String| report.problems.push(Problem { weapon: d.id.clone(), what });
        for f in factions() {
            report.first_person += 1;
            match drive(&mut files, read, |files| nativevm::load(files, d.viewmodel, f, None, None, 0)) {
                Ok(l) => {
                    if l.mesh.surfaces.is_empty() {
                        fail(format!("ZV{:02}{}: no surface at hip", d.viewmodel, f as char));
                    }
                    if l.mesh_aimed.as_ref().unwrap_or(&l.mesh).surfaces.is_empty() {
                        fail(format!("ZV{:02}{}: no surface at full ADS", d.viewmodel, f as char));
                    }
                }
                Err(e) => fail(format!("ZV{:02}{}: {e}", d.viewmodel, f as char)),
            }
        }
        report.third_person += 1;
        let req = nativebody::Request { vm: d.viewmodel, faction: b'U', mode: Mode::Weapon, state: 0, time: 0.0, rate: 1.0, pos: [0.0; 3], yaw: 0.0, tag: None, stowed: -1 };
        match drive(&mut files, read, |files| nativebody::load(files, &req, None)) {
            Ok(l) if l.mesh.surfaces.is_empty() => fail(format!("ZW{:02}: no surface", d.viewmodel)),
            Ok(_) => {}
            Err(e) => fail(format!("ZW{:02}: {e}", d.viewmodel)),
        }
    }
    // Each faction's body, once, with the first converted weapon in hand.
    if let Some(d) = weapons.iter().find(|d| d.viewmodel >= 0) {
        for f in factions() {
            let req = nativebody::Request { vm: d.viewmodel, faction: f, mode: Mode::Body, state: 0, time: 0.0, rate: 1.0, pos: [0.0; 3], yaw: 0.0, tag: None, stowed: -1 };
            match drive(&mut files, read, |files| nativebody::load(files, &req, None)) {
                Ok(l) if l.mesh.surfaces.is_empty() => report.problems.push(Problem { weapon: d.id.clone(), what: format!("body {}: no surface", f as char) }),
                Ok(_) => {}
                Err(e) => report.problems.push(Problem { weapon: d.id.clone(), what: format!("body {}: {e}", f as char) }),
            }
        }
    }
    report
}

/// Audit a converted archive (`ConquerOps_BO2.big`): its own weapon table and blobs.
pub fn audit_archive(path: &std::path::Path) -> Result<Report, String> {
    let archive = zhbo_formats::big::BigArchive::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let table_path = zhbo_core::names::WEAPON_TABLE_PATH;
    let entry = archive.find(table_path).ok_or_else(|| format!("the archive has no {table_path}"))?;
    let text = String::from_utf8(archive.read(entry).map_err(|e| format!("{table_path}: {e}"))?).map_err(|e| format!("{table_path}: {e}"))?;
    let table = zhbo_core::weapons::parse_full_table(&text)?;
    // `BigArchive::find` is a linear scan; the audit asks for tens of thousands of paths.
    let index: std::collections::HashMap<String, usize> = archive.entries().iter().enumerate().map(|(i, e)| (zhbo_formats::big::normalize(&e.path), i)).collect();
    let read = |p: &str| index.get(&zhbo_formats::big::normalize(p)).and_then(|&i| archive.read(&archive.entries()[i]).ok());
    Ok(audit(&table.weapons, &read))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Every converted weapon — the base set and every combination create-a-class offers — draws in
    /// first person at hip and at full ADS for every faction, and in third person.
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn every_converted_weapon_draws() {
        let bo2 = std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR");
        let (files, _) = crate::generals::files(Path::new(&bo2)).unwrap();
        let index: std::collections::HashMap<String, usize> = files.iter().enumerate().map(|(i, (p, _))| (zhbo_formats::big::normalize(p), i)).collect();
        let read = |p: &str| index.get(&zhbo_formats::big::normalize(p)).map(|&i| files[i].1.clone());
        let text = String::from_utf8(read(crate::weapons::TABLE_PATH).expect("the weapon table")).unwrap();
        let table = zhbo_core::weapons::parse_full_table(&text).unwrap();
        let report = audit(&table.weapons, &read);
        println!("{} first-person loads, {} world manifests", report.first_person, report.third_person);
        assert!(report.first_person > 1000, "the combinations are checked too: {}", report.first_person);
        let lines: Vec<String> = report.problems.iter().map(|p| format!("{}: {}", p.weapon, p.what)).collect();
        assert!(lines.is_empty(), "{} weapons do not draw:\n{}", lines.len(), lines.join("\n"));
    }
}
