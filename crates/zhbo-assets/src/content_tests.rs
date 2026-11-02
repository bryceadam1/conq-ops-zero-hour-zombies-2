//! The converter's side of the multiplayer content fingerprint (`zhbo_core::content`): two
//! players whose installs differ only in what does not reach the sim — the DLC packs, the
//! language, the art a pack adds — convert weapon tables with the same sim fingerprint, and a
//! table that differs in a sim number does not.

use zhbo_core::content::{vocabulary, weapons};
use zhbo_core::weapons::{parse_full_table, CamoOption, OptionTab, Table, BUILTIN};
use zhbo_formats::weapon::Weapon;
use std::collections::{BTreeMap, BTreeSet};

fn print(t: &Table) -> (u32, u32) {
    // Through the text, as the bridge hands it to the core.
    let back = parse_full_table(&zhbo_core::weapons::format_full_table(t)).unwrap();
    (weapons(&back), vocabulary(&back))
}

/// The DLC-dependent passes over one table: the Create-a-Class lock (`lock_items_without_art`:
/// the item's DLC installed, its art converting) and the DLC camo rows a weapon offers
/// (`camo::offer_what_converted`: the rows this install converted art for).
#[test]
fn dlc_locks_and_dlc_camos_leave_the_sim_fingerprint_alone() {
    let mut base = parse_full_table(BUILTIN).unwrap();
    for d in &mut base.weapons {
        d.cac_slot = "primary".into();
        d.camo_material = "camo_test".into();
        d.camo = vec!["camo_none".into(), "camo_dlc".into()];
    }
    base.camo.push(CamoOption { id: "camo_none".into(), tab: OptionTab::Base, ..Default::default() });
    base.camo.push(CamoOption { id: "camo_dlc".into(), tab: OptionTab::Dlc, ..Default::default() });
    let bo2: Vec<Weapon> = base.weapons.iter().map(|d| Weapon { name: d.id.clone(), ..Default::default() }).collect();
    // Every item carried by a DLC (statstable column 15), as the Peacekeeper is by `dlc1`.
    let statstable: Vec<Vec<String>> = base
        .weapons
        .iter()
        .map(|d| {
            let mut row = vec![String::new(); 16];
            row[4] = d.id.trim_end_matches("_mp").to_string();
            row[15] = "dlc1".into();
            row
        })
        .collect();
    let variant = |dlc: bool, art: bool, camo_art: bool| {
        let mut t = base.clone();
        crate::weapons::lock_items_without_art(&mut t, &bo2, &statstable, |_| art, |_| dlc);
        let with_art: BTreeMap<String, BTreeSet<String>> = if camo_art { [("camo_test".to_string(), ["camo_dlc".to_string()].into())].into() } else { BTreeMap::new() };
        crate::camo::offer_what_converted(&mut t, &with_art);
        t
    };
    let owner = variant(true, true, true);
    let without = variant(false, true, false);
    let no_art = variant(true, false, false);
    assert!(owner.weapons.iter().all(|d| !d.cac_locked) && without.weapons.iter().all(|d| d.cac_locked), "the variants lock differently");
    assert_ne!(owner.weapons[0].camo, without.weapons[0].camo, "the variants offer different camos");
    assert_eq!(print(&owner), print(&without), "a DLC pack does not change the sim");
    assert_eq!(print(&owner), print(&no_art), "art that does not convert does not change the sim");
    // A sim number does (a balance patch).
    let mut patched = owner.clone();
    patched.weapons[0].damage[0] += 1.0;
    assert_ne!(print(&patched).0, print(&owner).0);
}

/// The real install's weapon table, converted the way `generals::files` builds it (the class
/// table, the locks, the converted combinations), over variants of the install laid out as
/// symlink farms: without the DLC zones (`dlc0*`), with a DLC the player does not own present
/// (Revolution's `dlc1_load_mp.ff`, the Peacekeeper's pack), and without the english zones (a
/// localized install: every name and description is gone). The sim fingerprint must not move;
/// the presentation must (the variants did something).
#[test]
#[cfg(unix)]
#[ignore = "reads this machine's Black Ops II install"]
fn install_variants_convert_the_same_sim_content() {
    let bo2_dir = zhbo_requirements::check_requirements().black_ops2.unwrap().path;
    let tmp = std::env::temp_dir().join(format!("zhbo-content-variants-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let farm = |name: &str, keep: &dyn Fn(&str, &str) -> bool, extra: &[(&str, &str)]| {
        let root = tmp.join(name);
        for sub in ["", "zone/all", "zone/english"] {
            std::fs::create_dir_all(root.join(sub)).unwrap();
        }
        for e in std::fs::read_dir(&bo2_dir).unwrap().flatten() {
            if e.file_name() != "zone" {
                std::os::unix::fs::symlink(e.path(), root.join(e.file_name())).unwrap();
            }
        }
        for dir in ["all", "english"] {
            let Ok(list) = std::fs::read_dir(bo2_dir.join("zone").join(dir)) else { continue };
            for e in list.flatten() {
                let f = e.file_name().to_string_lossy().into_owned();
                if keep(dir, &f) {
                    std::os::unix::fs::symlink(e.path(), root.join("zone").join(dir).join(&f)).unwrap();
                }
            }
        }
        for (link, target) in extra {
            std::os::unix::fs::symlink(bo2_dir.join("zone/all").join(target), root.join("zone/all").join(link)).unwrap();
        }
        root
    };
    let convert = |dir: &std::path::Path| {
        let mp = crate::paks::MpPool::load(dir).unwrap();
        let (bo2, atts) = mp.weapons().unwrap();
        let patch_ui = crate::paks::load_zone(dir, "patch_ui_mp").unwrap();
        let (mut table, _) = crate::generals::class_table(dir, &bo2, &atts, &mp.patch, &patch_ui, &mut Vec::new());
        let statstable = zhbo_formats::stringtable::rows(&mp.patch, "mp/statstable.csv").ok().flatten().unwrap_or_default();
        crate::weapons::lock_items_without_art(&mut table, &bo2, &statstable, |_| true, |dlc| zhbo_formats::t6zone::install_path(dir, &format!("{dlc}_load_mp")).is_file());
        let converted = crate::weapons::converted(&mp.patch, &bo2);
        crate::weapons::add_variants(&mut table, &converted, &bo2);
        table
    };
    let real = convert(&bo2_dir);
    let no_dlc = convert(&farm("no-dlc", &|dir, f| !(dir == "all" && f.starts_with("dlc")), &[]));
    let revolution = convert(&farm("revolution", &|_, _| true, &[("dlc1_load_mp.ff", "common_mp.ff")]));
    let localized = convert(&farm("localized", &|dir, _| dir != "english", &[]));
    let _ = std::fs::remove_dir_all(&tmp);

    let pk = |t: &Table| t.weapons.iter().find(|d| d.id == "peacekeeper_mp").map(|d| d.cac_locked);
    assert_ne!(pk(&real), pk(&revolution), "owning Revolution unlocks the Peacekeeper");
    let name = |t: &Table| t.weapons.iter().find(|d| d.id == "an94_mp").map(|d| d.display_name.clone());
    assert_ne!(name(&real), name(&localized), "without the english zones the names change");
    for (what, t) in [("no DLC zones", &no_dlc), ("Revolution present", &revolution), ("no english zones", &localized)] {
        assert_eq!(print(t), print(&real), "{what}: the sim fingerprint moved");
    }
}
