//! Temporary probe of the install's data shape (removed before the layer lands).

use super::install::Install;

#[test]
#[ignore = "probe: reads ZHBO_BO2_DIR"]
fn probe_unique_shape() {
    let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
    let i = Install::open(std::path::Path::new(&dir)).unwrap();
    let vs = i.variants();
    let atts = i.attachments();
    println!("variants {} attachments {} default={}", vs.len(), atts.len(), atts.contains_key("defaultattachment"));
    let mut bad_kind = 0;
    let mut children = 0;
    let mut max_index = 0;
    for (name, v) in &vs {
        for idx in 0..super::raw::UNIQUES {
            let Some(u) = v.unique(idx) else { continue };
            max_index = max_index.max(idx);
            if idx < 30 && u.kind() != idx {
                bad_kind += 1;
                if bad_kind < 10 { println!("  {name} U[{idx}] kind {} name {}", u.kind(), u.name()); }
            }
            if idx >= 30 { children += 1; }
            if name == "an94_mp" {
                println!("  an94 U[{idx}] {} kind {} mask {:#x} child {} sib {} vm {:?}", u.name(), u.kind(), u.combined_mask(), u.child(), u.sibling(), u.view_model());
            }
        }
    }
    println!("bad kinds {bad_kind}, children {children}, max index {max_index}");
    let patch_weapons = { let k = zhbo_formats::fastfile::asset_type_id("weapon").unwrap(); i.patch.assets.iter().filter(|a| a.kind == k).count() };
    println!("weapon assets in patch_mp: {patch_weapons}");
}

#[test]
#[ignore = "probe: writes the expected weapon rows to ZHBO_PROBE_OUT"]
fn probe_expected_rows() {
    let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
    let t = std::time::Instant::now();
    let rows = super::weapon_rows(std::path::Path::new(&dir)).unwrap();
    let schema = crate::weapons::schema();
    let out = std::env::var("ZHBO_PROBE_OUT").expect("ZHBO_PROBE_OUT");
    std::fs::write(format!("{out}/expected.csv"), crate::csv::write(&schema, &rows)).unwrap();
    println!("{} rows, {} columns, {:?}", rows.len(), schema.columns.len(), t.elapsed());
}

#[test]
#[ignore = "probe: the dual-wield attachment"]
fn probe_dw() {
    let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
    let i = Install::open(std::path::Path::new(&dir)).unwrap();
    let vs = i.variants();
    for n in ["fnp45_mp", "fnp45_dw_mp", "fnp45_lh_mp", "fnp45_dw_lh_mp"] {
        let Some(v) = vs.get(n) else { println!("{n}: none"); continue };
        let d = v.def().unwrap();
        println!("{n}: inv {} dw {} partner {:?} vm {:?}", d.inventory(), d.dual_wield(), d.dual_wield_weapon_name(), d.view_model());
        if let Some(a) = v.attachment(4) { println!("  A[4] {} dualwield {}", a.name(), a.dual_wield()); }
        if let Some(u) = v.unique(4) { println!("  U[4] {} partner {:?} vm {:?} alt {:?}", u.name(), u.dual_wield_weapon_name(), u.view_model(), u.alt_weapon_name()); }
    }
    println!("{:?}", vs.keys().filter(|k| k.contains("fnp45")).collect::<Vec<_>>());
}
