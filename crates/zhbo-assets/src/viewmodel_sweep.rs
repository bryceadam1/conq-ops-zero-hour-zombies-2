/*
** Conquer Ops - converter: the art a headless viewmodel sweep needs.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! One weapon-with-attachment combination's native blobs, converted from the player's own install
//! by the same code `cargo xtask build` runs.
//!
//! `zhbo-core`'s `render_viewmodels` example (headless; `cargo run -p zhbo-core --example
//! render_viewmodels --features renderer -- <outdir>`) draws every player weapon and every
//! attachment it offers through the game's own native viewmodel path, so a human can review the
//! whole catalog without launching the game. The converted archive holds the base weapons and the
//! loadout's own combination (the converter converts what a class can carry,
//! [`crate::weapons::converted`]), so the example converts the other combinations on demand through
//! [`Source`] — the converter's own [`crate::viewmodels::weapon_viewmodels`], never a second
//! conversion path. The blobs live in memory for the length of one render.
//!
//! Development tool only: it reads the player's install, writes nothing to it, and never launches
//! or connects to Black Ops II.

use crate::generals::{load_zone, FACTIONS};
use crate::paks::Paks;
use crate::viewmodels::weapon_viewmodels;
use std::collections::BTreeMap;
use std::path::Path;
use zhbo_formats::t6zone::Zone;

/// The player's install, opened once: the zones and image packs the viewmodel conversion reads.
pub struct Source {
    common: Zone,
    patch: Zone,
    zones: Vec<Zone>,
    paks: Paks,
    bo2: Vec<zhbo_formats::weapon::Weapon>,
    atts: Vec<zhbo_formats::weapon::Attachment>,
    defs: Vec<zhbo_core::weapons::WeaponDef>,
}

impl Source {
    /// Open the game's zones (`common_mp`, the patch, the three factions' art) and build the weapon
    /// table the conversion places attachment clips from.
    pub fn open(bo2_dir: &Path) -> Result<Source, String> {
        let common = load_zone(bo2_dir, "common_mp")?;
        let patch = load_zone(bo2_dir, crate::fx::ZONE)?;
        let zones: Vec<Zone> = FACTIONS.iter().map(|f| load_zone(bo2_dir, f.zone)).collect::<Result<_, _>>()?;
        let paks = Paks::open(bo2_dir)?;
        let bo2 = zhbo_formats::weapon::all(&common).map_err(|e| e.to_string())?;
        let atts = zhbo_formats::weapon::attachments(&common).map_err(|e| e.to_string())?;
        let names = crate::weapons::names(bo2_dir);
        let mut table = crate::weapons::table(&bo2, &atts, &names);
        crate::weapons::add_variants(&mut table, &crate::weapons::converted(&bo2));
        Ok(Source { common, patch, zones, paks, bo2, atts, defs: table.weapons })
    }

    /// The native blobs for converted viewmodel `id` — `an94_mp` or `an94_mp+reflex` — keyed by
    /// archive path: the manifests (`Art\Native\ZV00<faction>.zbv`, the id at index 0 of this
    /// one-entry conversion), the parts' models, the clips the combination plays and the textures
    /// they name. The W3D output (the same conversion also builds it in game) is dropped: the sweep
    /// draws the native path only.
    pub fn composition(&self, id: &str) -> Result<BTreeMap<String, Vec<u8>>, String> {
        let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        let mut defs = self.defs.clone();
        weapon_viewmodels(&self.patch, &self.common, &self.zones, &self.paks, &mut out, &self.bo2, &self.atts, &mut defs, std::slice::from_ref(&id.to_string()))?;
        out.retain(|path, _| path.starts_with(zhbo_native::DIR) || path.starts_with("Art\\Textures\\"));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "needs the BO2 install: ZHBO_BO2_DIR"]
    fn converts_a_solo_combination() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");
        let base = src.composition("an94_mp").expect("the bare weapon");
        let combo = src.composition("an94_mp+reflex").expect("the weapon with its optic");
        let parts = |blobs: &BTreeMap<String, Vec<u8>>| {
            zhbo_native::viewmodel::Viewmodel::from_bytes(blobs.get(&zhbo_native::viewmodel_path(0, 'U')).expect("a manifest")).expect("the manifest parses").parts.len()
        };
        assert_eq!(parts(&base), 4, "the arms, the gun and the magazine twice");
        assert_eq!(parts(&combo), 5, "the optic rides the gun's view-model tag");
        assert!(combo.len() > base.len());
    }

    #[test]
    #[ignore = "probe: the flagged defects"]
    fn probe_defects() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");
        let summary = |id: &str| {
            let mut out = BTreeMap::new();
            let mut defs = src.defs.clone();
            match crate::viewmodels::weapon_viewmodels(&src.patch, &src.common, &src.zones, &src.paks, &mut out, &src.bo2, &src.atts, &mut defs, &[id.to_string()]) {
                Ok((s, _)) => eprintln!("SUMMARY {id}: {s}"),
                Err(e) => eprintln!("SUMMARY {id}: ERR {e}"),
            }
        };
        let tga_dir = std::env::var_os("ZHBO_SWEEP_TGA").map(std::path::PathBuf::from);
        let show = |id: &str| {
            summary(id);
            let blobs = src.composition(id).expect(id);
            if let Some(root) = &tga_dir {
                let dir = root.join(id);
                std::fs::create_dir_all(&dir).ok();
                for (path, bytes) in &blobs {
                    if let Some(name) = path.rsplit('\\').next().filter(|n| n.ends_with(".tga")) {
                        std::fs::write(dir.join(name), bytes).ok();
                    }
                }
            }
            let vm = zhbo_native::viewmodel::Viewmodel::from_bytes(blobs.get(&zhbo_native::viewmodel_path(0, 'U')).expect("manifest")).expect("parse");
            eprintln!("=== {id} ===");
            eprintln!("  hide: {:?}", vm.hide);
            for (i, p) in vm.parts.iter().enumerate() {
                let m = zhbo_native::model::Model::from_bytes(blobs.get(&p.model).unwrap()).unwrap();
                eprintln!("  part {i}: {} tag={:?} offset={:?} rigid={} mins={:?} maxs={:?} root={:?}", p.model.rsplit('\\').next().unwrap(), p.tag, p.offset.1, p.rigid, m.mins, m.maxs, m.bones.first().map(|b| (&b.name, b.position)));
                for (j, mat) in m.materials.iter().enumerate() {
                    eprintln!("    mat {j}: {:?} blend={:?} cull={:?} emissive={} color={:?} maps={:?}", mat.name, mat.blend, mat.cull, mat.emissive, mat.color, mat.maps);
                }
                for (j, surf) in m.lod0().iter().enumerate() {
                    let blend = m.materials.get(surf.material as usize).map(|x| x.blend);
                    eprintln!("    surf {j}: material={} blend={:?}", surf.material, blend);
                }
            }
            eprintln!("  markers: {:?}", vm.markers.iter().map(|m| (&m.name, &m.parent, m.local.1)).collect::<Vec<_>>());
            eprintln!("  clips: {:?}", vm.clips);
        };
        for id in ["hatchet_mp", "trophy_system_mp", "flash_grenade_mp", "proximity_grenade_mp", "emp_grenade_mp", "frag_grenade_mp", "sensor_grenade_mp", "an94_mp", "mp7_mp", "as50_mp", "dsr50_mp"] {
            show(id);
        }
        for name in ["hatchet_mp", "trophy_system_mp", "flash_grenade_mp", "proximity_grenade_mp", "emp_grenade_mp", "frag_grenade_mp", "sensor_grenade_mp", "mp7_mp", "as50_mp", "dsr50_mp"] {
            if let Some(w) = src.bo2.iter().find(|w| w.name == name) {
                eprintln!("WEAPON {name}: view_model={:?} hide={:?} anims={:?} ads_aim_pitch={} dual={} attach_view_models={:?}", w.view_model, w.hide_tags, w.anims, w.view_pos.ads_aim_pitch, w.dual_wield, w.attach_view_models);
            }
        }
        for (base, short) in [("mp7_mp", "reflex"), ("as50_mp", "ir"), ("as50_mp", "silencer"), ("as50_mp", "swayreduc"), ("fnp45_mp", "dualclip"), ("fiveseven_mp", "dualclip"), ("kard_mp", "dualclip"), ("judge_mp", "dualclip"), ("beretta93r_mp", "dualclip"), ("kard_mp", "steadyaim"), ("ksg_mp", "steadyaim")] {
            let id = format!("{base}+{short}");
            eprintln!("COMBO {id}");
            if let Some(w) = src.bo2.iter().find(|w| w.name == base) {
                let variant = w.attachment_variants.iter().find(|(_, v)| crate::weapons::short_name(v) == short);
                eprintln!("  variant: {variant:?}");
                if let Some((kind, aid)) = variant {
                    if let Some(u) = w.attachment_uniques.iter().find(|u| u.kind == *kind && u.combined == 0) {
                        eprintln!("  unique: name={:?} kind={} view_model={:?} tag={:?} offset={:?} rot={:?} hide={:?} disable_base_clip={} dual={:?}", u.name, u.kind, u.view_model, u.view_model_tag, u.view_offset, u.view_rotation, u.hide_tags, u.disable_base_clip, u.dual_wield_weapon);
                    }
                    if let Some(a) = src.atts.iter().find(|a| &a.name == aid) {
                        eprintln!("  Attachment: name={:?} point={} dual_wield={} dual_mag={} laser={}", a.name, a.point, a.dual_wield, a.dual_mag, a.laser_sight);
                    }
                }
            }
            show(&id);
        }
    }
}
