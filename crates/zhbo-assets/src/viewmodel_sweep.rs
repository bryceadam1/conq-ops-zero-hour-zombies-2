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
    #[ignore = "probe: where the equipment sits at idle"]
    fn probe_equipment_idle_pose() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");
        for id in ["hatchet_mp", "trophy_system_mp", "flash_grenade_mp", "proximity_grenade_mp", "emp_grenade_mp", "frag_grenade_mp", "sensor_grenade_mp", "an94_mp"] {
            let blobs = src.composition(id).expect(id);
            let vm = zhbo_native::viewmodel::Viewmodel::from_bytes(blobs.get(&zhbo_native::viewmodel_path(0, 'U')).expect("manifest")).expect("parse");
            let models: Vec<zhbo_native::model::Model> = vm.parts.iter().map(|p| zhbo_native::model::Model::from_bytes(blobs.get(&p.model).unwrap()).unwrap()).collect();
            let refs: Vec<zhbo_native::pose::PartRef> = vm.parts.iter().zip(&models).map(|(p, m)| zhbo_native::pose::PartRef { model: m, tag: p.tag.as_deref(), offset: p.offset, rigid: p.rigid }).collect();
            let (skel, problems) = zhbo_native::pose::Skeleton::compose(&refs, &vm.markers, &vm.hide);
            let Some(path) = vm.clip(zhbo_core::viewmodel::slot::IDLE as u16) else { eprintln!("{id}: no idle"); continue };
            let anim = zhbo_native::anim::Anim::from_bytes(blobs.get(path).expect(path)).expect("anim");
            let (mut local, mut ms) = (Vec::new(), Vec::new());
            skel.sample(&anim, 0.0, &mut local);
            skel.model_space(&local, &mut ms);
            eprintln!("{id}: problems={problems:?} parts={:?}", vm.parts.iter().map(|p| (&p.model, &p.tag, p.offset.1)).collect::<Vec<_>>());
            for name in ["tag_view", "tag_weapon", "tag_weapon1", "j_wrist_ri", "j_wrist_le", "j_gun"] {
                if let Some(i) = skel.bone(name) {
                    let track = anim.bones.iter().find(|t| t.name == name).and_then(|t| t.translation_at(0.0));
                    eprintln!(
                        "  {name}: bind={:?} bind_local={:?} anim_base={:?} track={:?} local={:?} posed={:?}",
                        skel.bones[i].bind.1, skel.bones[i].bind_local.1, skel.bones[i].anim_base, track, local[i].1, skel.bone_point(&ms, i)
                    );
                }
            }
        }
    }

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

    /// The base magazine is the weapon's `attachViewModel` part, placed at the authored
    /// `attachViewModelOffset` from its root; the name search is only a fallback.
    #[test]
    #[ignore = "needs the BO2 install: ZHBO_BO2_DIR"]
    fn the_mp7_magazine_rides_its_authored_offset() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");
        let w = src.bo2.iter().find(|w| w.name == "mp7_mp").expect("mp7_mp");
        let ((model, _), offset) = w
            .attach_view_models
            .iter()
            .zip(&w.attach_view_offsets)
            .find(|((m, _), _)| m.contains("attach_mag"))
            .expect("the MP7's authored magazine");
        assert!(offset.iter().any(|c| *c != 0.0), "the authored offset is not zero: {offset:?}");
        let blobs = src.composition("mp7_mp").expect("the base MP7");
        let vm = zhbo_native::viewmodel::Viewmodel::from_bytes(blobs.get(&zhbo_native::viewmodel_path(0, 'U')).expect("a manifest")).expect("the manifest parses");
        let mag = vm.parts.iter().find(|p| p.model.contains(model)).expect("the magazine part");
        assert_eq!(mag.tag.as_deref(), Some("j_gun"), "the magazine hangs from the gun root");
        assert_eq!(mag.offset.1, *offset, "the magazine keeps its authored offset");
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
            if let Some(d) = src.defs.iter().find(|d| d.id == name) {
                eprintln!("DEF {name}: view_pos={:?}", d.view_pos);
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

    #[test]
    #[ignore = "probe: the MP7's skin weights, native smooth vs the W3D path's rigid"]
    fn probe_mp7_weights() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");
        for name in ["t6_wpn_smg_mp7_view", "t6_attach_mag_mp7_view"] {
            if let Ok(raw) = crate::viewmodels::find_model(&src.patch, &src.common, name) {
                eprintln!("RAW {name}:");
                for (i, b) in raw.bones.iter().enumerate() {
                    eprintln!("  raw {i}: {} parent={:?} pos={:?}", b.name, b.parent, b.position);
                }
            }
        }
        let blobs = src.composition("mp7_mp+reflex").expect("compose");
        for anim_name in ["viewmodel_mp7_idle", "viewmodel_mp7_reload"] {
            if let Some(bytes) = blobs.get(&zhbo_native::anim_path(anim_name)) {
                if let Ok(anim) = zhbo_native::anim::Anim::from_bytes(bytes) {
                    for b in &anim.bones {
                        if b.name.contains("clip") || b.name.contains("reload") || b.name == "tag_weapon" {
                            eprintln!("ANIM {anim_name} {} rot={:?} trans={:?}", b.name, b.rotation, b.translation);
                        }
                    }
                }
            }
        }
        let vm = zhbo_native::viewmodel::Viewmodel::from_bytes(blobs.get(&zhbo_native::viewmodel_path(0, 'U')).expect("manifest")).expect("parse");
        if let Some(w) = src.bo2.iter().find(|w| w.name == "mp7_mp") {
            eprintln!("BASE ATTACH {:?} OFFSETS {:?}", w.attach_view_models, w.attach_view_offsets);
        }
        let models: Vec<zhbo_native::model::Model> = vm.parts.iter().map(|p| zhbo_native::model::Model::from_bytes(blobs.get(&p.model).unwrap()).unwrap()).collect();
        let refs: Vec<zhbo_native::pose::PartRef> = vm.parts.iter().zip(&models).map(|(p, m)| zhbo_native::pose::PartRef { model: m, tag: p.tag.as_deref(), offset: p.offset, rigid: p.rigid }).collect();
        let (skel, problems) = zhbo_native::pose::Skeleton::compose(&refs, &vm.markers, &vm.hide);
        eprintln!("COMPOSE problems={problems:?}");
        if let Some(i) = skel.bone("tag_clip") {
            let anim = zhbo_native::anim::Anim::from_bytes(blobs.get(&zhbo_native::anim_path("viewmodel_mp7_idle")).unwrap()).unwrap();
            let (mut local, mut ms) = (Vec::new(), Vec::new());
            skel.sample(&anim, 0.0, &mut local);
            skel.model_space(&local, &mut ms);
            eprintln!("TAG_CLIP posed={:?} local={:?} bind={:?}", skel.bone_point(&ms, i), local[i].1, skel.bones[i].bind.1);
        }
        for (i, p) in vm.parts.iter().enumerate() {
            let m = zhbo_native::model::Model::from_bytes(blobs.get(&p.model).unwrap()).expect("model");
            eprintln!("PART {i} {} tag={:?} offset={:?}", p.model.rsplit('\\').next().unwrap(), p.tag, p.offset.1);
            for (j, b) in m.bones.iter().enumerate() {
                eprintln!("  bone {j}: {} parent={:?}", b.name, b.parent);
            }
            if p.model.contains("mp7_view") {
                let mut low = ([f32::MAX; 3], [f32::MIN; 3]);
                for s in m.lod0() {
                    for v in &s.vertices {
                        if v.position[2] < -1.5 && v.position[0] < 4.0 {
                            for k in 0..3 {
                                low.0[k] = low.0[k].min(v.position[k]);
                                low.1[k] = low.1[k].max(v.position[k]);
                            }
                        }
                    }
                }
                eprintln!("LOW-REAR grip bounds: mins={:?} maxs={:?}", low.0, low.1);
            }
            for (k, s) in m.lod0().iter().enumerate() {
                let mut one = 0;
                let mut multi = 0;
                let mut by_bones = BTreeMap::<String, usize>::new();
                let mut bounds = BTreeMap::<String, ([f32; 3], [f32; 3])>::new();
                for v in &s.vertices {
                    let n = v.weights.iter().filter(|w| **w > 0.001).count();
                    if n <= 1 {
                        one += 1
                    } else {
                        multi += 1
                    }
                    let names: Vec<&str> = v.bones.iter().zip(v.weights.iter()).filter(|(_, w)| **w > 0.001).map(|(b, _)| m.bones[*b as usize].name.as_str()).collect();
                    *by_bones.entry(names.join("+")).or_default() += 1;
                    for (b, _) in v.bones.iter().zip(v.weights.iter()).filter(|(_, w)| **w > 0.001) {
                        let e = bounds.entry(m.bones[*b as usize].name.clone()).or_insert(([f32::MAX; 3], [f32::MIN; 3]));
                        for k in 0..3 {
                            e.0[k] = e.0[k].min(v.position[k]);
                            e.1[k] = e.1[k].max(v.position[k]);
                        }
                    }
                }
                eprintln!("  surface {k}: {one} single-bone, {multi} multi-bone");
                for (names, n) in by_bones.iter().take(16) {
                    eprintln!("    {n:5} {names}");
                }
                for (name, (mn, mx)) in &bounds {
                    eprintln!("    bounds {name}: mins={mn:?} maxs={mx:?}");
                }
            }
        }
    }
}
