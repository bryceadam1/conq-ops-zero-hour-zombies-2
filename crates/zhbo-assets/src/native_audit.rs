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

use std::collections::BTreeMap;
use zhbo_core::nativebody::{self, Mode};
use zhbo_core::nativevm::{self, Files, Mesh};
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

/// What a texture file the composed mesh names turned out to be, decoded once per run.
#[derive(Clone, Debug, PartialEq)]
enum Texture {
    Fine,
    /// Not in the archive: the renderer binds its 1x1 white stand-in.
    Missing,
    Undecodable(String),
    /// Every pixel opaque white: the renderer's stand-in, baked into a file.
    White,
}

/// Whether `rgba` is a solid, opaque white image (BO2's art never is: a converted colour map that
/// is, is a placeholder that reached the archive).
fn is_white(rgba: &[u8]) -> bool {
    !rgba.is_empty() && rgba.as_chunks::<4>().0.iter().all(|p| p[0] >= 250 && p[1] >= 250 && p[2] >= 250 && p[3] >= 250)
}

/// The texture problems of `mesh` (`what` names the load): a surface the renderer would draw
/// untextured (no colour map), or a texture it names that is missing, does not decode, or is a
/// solid white colour map. Those all draw white in game — on the third-person weapon the overhead
/// view shows, too.
fn texture_problems(mesh: &Mesh, what: &str, read: &dyn Fn(&str) -> Option<Vec<u8>>, seen: &mut BTreeMap<String, Texture>) -> Vec<String> {
    let mut out = Vec::new();
    let mut check = |path: &str, colour: bool, out: &mut Vec<String>| {
        let t = seen
            .entry(path.to_string())
            .or_insert_with(|| match read(path) {
                None => Texture::Missing,
                Some(bytes) => match zhbo_native::tga::decode(&bytes) {
                    Ok(img) if is_white(&img.rgba) => Texture::White,
                    Ok(_) => Texture::Fine,
                    Err(e) => Texture::Undecodable(e.to_string()),
                },
            })
            .clone();
        match t {
            Texture::Fine => {}
            Texture::White if !colour => {}
            Texture::White => out.push(format!("{what}: {path} is solid white")),
            Texture::Missing => out.push(format!("{what}: {path} is not in the archive")),
            Texture::Undecodable(e) => out.push(format!("{what}: {path}: {e}")),
        }
    };
    for s in &mesh.surfaces {
        // Drawn by their own technique from code-made inputs, with no colour file of their own
        // (`Looks::no_file`): the ui3d reticle (the code's UI texture) and the MMS screen.
        let code_texture = s.sonar.is_some() || s.reticle.as_ref().is_some_and(|r| r.kind == zhbo_native::model::ReticleKind::Ui3d);
        match &s.color {
            Some(c) => check(c, true, &mut out),
            None if !code_texture => out.push(format!("{what}: surface {} has no colour map", s.material)),
            None => {}
        }
        for p in [&s.maps, &s.specular, &s.camo].into_iter().flatten() {
            check(p, false, &mut out);
        }
    }
    out.sort();
    out.dedup();
    out
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
    let mut textures = BTreeMap::new();
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
                    for m in std::iter::once(&l.mesh).chain(l.mesh_aimed.as_ref()) {
                        for p in texture_problems(m, &format!("ZV{:02}{}", d.viewmodel, f as char), read, &mut textures) {
                            fail(p);
                        }
                    }
                }
                Err(e) => fail(format!("ZV{:02}{}: {e}", d.viewmodel, f as char)),
            }
        }
        report.third_person += 1;
        // A weapon held on its own body bone (the shield) draws in body mode only; the weapon-only
        // mode leaves it to the W3D weapon.
        let own_bone = read(&zhbo_native::world_weapon_path(d.viewmodel)).and_then(|b| zhbo_native::thirdperson::Weapon::from_bytes(&b).ok()).is_some_and(|w| nativebody::held_on_body_bone(&w).is_some());
        let mode = if own_bone { Mode::Body } else { Mode::Weapon };
        let req = nativebody::Request { vm: d.viewmodel, faction: b'U', mode, state: 0, time: 0.0, rate: 1.0, pos: [0.0; 3], yaw: 0.0, tag: None, stowed: -1, offhands: [-1; 2] };
        match drive(&mut files, read, |files| nativebody::load(files, &req, None)) {
            Ok(l) if l.mesh.surfaces.is_empty() => fail(format!("ZW{:02}: no surface", d.viewmodel)),
            Ok(l) => {
                for p in texture_problems(&l.mesh, &format!("ZW{:02}", d.viewmodel), read, &mut textures) {
                    fail(p);
                }
            }
            Err(e) => fail(format!("ZW{:02}: {e}", d.viewmodel)),
        }
    }
    // Each faction's body, once, with the first converted weapon in hand.
    if let Some(d) = weapons.iter().find(|d| d.viewmodel >= 0) {
        for f in factions() {
            let req = nativebody::Request { vm: d.viewmodel, faction: f, mode: Mode::Body, state: 0, time: 0.0, rate: 1.0, pos: [0.0; 3], yaw: 0.0, tag: None, stowed: -1, offhands: [-1; 2] };
            match drive(&mut files, read, |files| nativebody::load(files, &req, None)) {
                Ok(l) if l.mesh.surfaces.is_empty() => report.problems.push(Problem { weapon: d.id.clone(), what: format!("body {}: no surface", f as char) }),
                Ok(l) => {
                    for what in texture_problems(&l.mesh, &format!("body {}", f as char), read, &mut textures) {
                        report.problems.push(Problem { weapon: d.id.clone(), what });
                    }
                }
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
