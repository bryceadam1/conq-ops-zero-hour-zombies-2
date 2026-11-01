//! The optics' reticle options: BO2's `mp/weaponoptions.csv` `reticle` rows (in `patch_mp`) as the
//! lens materials the core swaps in for the class's reticle choice
//! (`zhbo_native::reticles::Reticles`).
//!
//! BO2's rule (`t6mp` `FUN_00529360`): each row is `index, reticle, optic, material, ,
//! material-for-the-RMR`; the engine fills one table per optic column, column 3 for every optic
//! and column 5 for the reflex's second (the RMR's `t6_attach_optic_rmr_view`). Row 0 names the
//! material the optic's model carries; row `n` option `reticle_n`'s. The materials load in full from
//! `code_post_gfx_mp` (the CaC's art zone), then `patch_mp` (the store rows 11..37, whose materials
//! exist nowhere else), then `common_mp`.

use crate::paks::Paks;
use crate::textures::{self, Looks};
use std::collections::BTreeMap;
use zhbo_formats::t6zone::Zone;
use zhbo_formats::xmodel;
use zhbo_native::reticles::Reticles;

/// The material columns of a `reticle` row (3: every optic; 5: the reflex's RMR column).
const COLUMNS: [usize; 2] = [3, 5];

/// The optic columns in table order: `(optic, column)` to each row's material name by index.
pub(crate) fn columns(rows: &[Vec<String>]) -> Vec<Vec<String>> {
    let mut sets: Vec<((String, usize), BTreeMap<usize, String>)> = Vec::new();
    for r in rows.iter().filter(|r| r.get(1).is_some_and(|c| c.trim() == "reticle")) {
        let (Some(index), Some(optic)) = (r.first().and_then(|i| i.trim().parse::<usize>().ok()), r.get(2)) else { continue };
        for col in COLUMNS {
            let Some(material) = r.get(col).map(|m| m.trim()).filter(|m| !m.is_empty()) else { continue };
            let key = (optic.trim().to_string(), col);
            let at = match sets.iter().position(|(k, _)| *k == key) {
                Some(at) => at,
                None => {
                    sets.push((key, BTreeMap::new()));
                    sets.len() - 1
                }
            };
            sets[at].1.insert(index, material.to_string());
        }
    }
    // Rows by index from 0; a gap (none in the stock table) repeats row 0, BO2's empty slot.
    sets.into_iter()
        .filter_map(|(_, rows)| {
            let stock = rows.get(&0)?.clone();
            let last = *rows.keys().last()?;
            Some((0..=last).map(|i| rows.get(&i).cloned().unwrap_or_else(|| stock.clone())).collect())
        })
        .collect()
}

/// The reticle options as lens materials, their art written into `files`. A set whose stock
/// material does not convert is left out (its lens draws its own stock); a row that does not
/// convert keeps the stock material.
pub(crate) fn reticles(patch: &Zone, zones: &[&Zone], paks: &Paks, files: &mut BTreeMap<String, Vec<u8>>) -> Reticles {
    let Ok(Some(rows)) = zhbo_formats::stringtable::rows(patch, "mp/weaponoptions.csv") else { return Reticles::default() };
    let mut looks = Looks::default();
    let mut convert = |name: &str| -> Option<zhbo_native::model::Material> {
        let m = zones.iter().find_map(|z| xmodel::find_asset_material(z, name).or_else(|| xmodel::find_linked_material(z, name)).filter(|m| !m.constants.is_empty()))?;
        let texture = textures::lens_reticle(&m, paks, files, &mut looks)?;
        Some(crate::native::material_record(Some(&m), Some(&texture), &looks))
    };
    let sets = columns(&rows)
        .into_iter()
        .filter_map(|names| {
            let stock = convert(&names[0])?;
            Some(names.iter().enumerate().map(|(i, n)| if i == 0 { stock.clone() } else { convert(n).unwrap_or_else(|| stock.clone()) }).collect())
        })
        .collect();
    Reticles { sets }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(cells: &[&str]) -> Vec<String> {
        cells.iter().map(|c| c.to_string()).collect()
    }

    /// `weaponoptions.csv`'s shape: the reflex's two columns (its own and the RMR's) are two sets;
    /// row 0 is each set's stock material; non-reticle rows are ignored.
    #[test]
    fn the_table_gives_one_set_per_optic_column_in_row_order() {
        let rows = vec![
            row(&["a0", "b1", "c2", "d3", "e4", "f5"]),
            row(&["0", "reticle", "reflex", "mc/r", "", "mc/rmr"]),
            row(&["1", "reticle", "reflex", "mc/r_01", "", "mc/rmr_01"]),
            row(&["0", "reticle", "acog", "mc/a", "", ""]),
            row(&["2", "reticle", "acog", "mc/a_02", "", ""]),
            row(&["0", "camo", "x", "mc/c", "", ""]),
        ];
        let sets = columns(&rows);
        assert_eq!(sets, vec![vec!["mc/r".to_string(), "mc/r_01".into()], vec!["mc/rmr".into(), "mc/rmr_01".into()], vec!["mc/a".into(), "mc/a".into(), "mc/a_02".into()]]);
    }

    /// Needs the player's BO2 install: `ZHBO_BO2_DIR=... cargo test -p zhbo-assets --release -- --ignored`.
    #[test]
    #[ignore]
    fn the_installs_reticle_options_convert() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let patch = crate::generals::load_zone(&dir, crate::fx::ZONE).unwrap();
        let cpg = crate::generals::load_zone(&dir, textures::RETICLE_ZONE).unwrap();
        let common = crate::generals::load_zone(&dir, "common_mp").unwrap();
        let mut paks = Paks::open(&dir).unwrap();
        paks.learn(&cpg);
        let mut files = BTreeMap::new();
        let r = reticles(&patch, &[&cpg, &patch, &common], &paks, &mut files);
        let reflex = r.option("mc/mtl_t6_attach_optic_reflex_reticle", 0).expect("the reflex's stock lens");
        assert!(reflex.reticle.is_some(), "a projected reticle");
        let third = r.option("mc/mtl_t6_attach_optic_reflex_reticle", 3).expect("reticle option 3");
        assert_eq!(third.name, "mc/mtl_t6_attach_optic_reflex_reticle_03");
        assert!(third.color.as_ref().is_some_and(|c| files.contains_key(c)), "its art converted");
        for optic in ["acog", "holo"] {
            let stock = format!("mc/mtl_t6_attach_optic_{optic}_reticle");
            assert!(r.option(&stock, 10).is_some_and(|m| m.reticle.is_some()), "{optic}: option 10");
        }
        // The store rows convert from `patch_mp`, each its own material (row 23 is `_24`, whose
        // colour is the `_25` image; row 24 is `_38`).
        let reflex = |n: usize| r.option("mc/mtl_t6_attach_optic_reflex_reticle", n).map(|m| m.name.clone());
        assert_eq!(reflex(24).as_deref(), Some("mc/mtl_t6_attach_optic_reflex_reticle_38"));
        assert_eq!(reflex(37).as_deref(), Some("mc/mtl_t6_attach_optic_reflex_reticle_40"));
    }
}
