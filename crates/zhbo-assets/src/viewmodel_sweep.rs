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
        crate::weapons::add_variants(&mut table, &crate::weapons::converted(&patch, &bo2), &bo2);
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
        weapon_viewmodels(&self.patch, &self.common, &self.zones, &self.paks, &mut out, &self.bo2, &self.atts, &mut defs, std::slice::from_ref(&id.to_string()), 0)?;
        out.retain(|path, _| path.starts_with(zhbo_native::DIR) || path.starts_with("Art\\Textures\\"));
        Ok(out)
    }

    /// How many base parts (`attachViewModel` entries: the magazine plus the base scope/optic art)
    /// the converter loads for `id`, by the same classifier the conversion runs. The sweep's note
    /// names the count, so a part that stops loading is attributable to the row. `None` when the
    /// install's weapon is unknown.
    pub fn base_part_count(&self, id: &str) -> Option<usize> {
        let (base, shorts) = zhbo_core::attachments::split_id(id);
        let w = self.bo2.iter().find(|w| w.name == base)?;
        let view = w.view_model.as_deref()?;
        let c = crate::viewmodels::combo(w, &self.atts, &shorts);
        Some(crate::viewmodels::base_parts(&self.patch, &self.common, w, &c, view).len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One viewmodel's composed skeleton from its converted blobs — the pose every native path
    /// starts from.
    fn skeleton(vm: &zhbo_native::viewmodel::Viewmodel, blobs: &BTreeMap<String, Vec<u8>>) -> zhbo_native::pose::Skeleton {
        let models: Vec<zhbo_native::model::Model> = vm.parts.iter().map(|p| zhbo_native::model::Model::from_bytes(blobs.get(&p.model).unwrap_or_else(|| panic!("{}", p.model))).expect("a model")).collect();
        let refs: Vec<zhbo_native::pose::PartRef> = vm.parts.iter().zip(&models).map(|(p, m)| zhbo_native::pose::PartRef { model: m, tag: p.tag.as_deref(), offset: p.offset, rigid: p.rigid }).collect();
        let (skel, problems) = zhbo_native::pose::Skeleton::compose(&refs, &vm.markers, &vm.hide);
        assert!(problems.is_empty(), "{problems:?}");
        skel
    }

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
        assert_eq!(parts(&base), 3, "the arms, the gun and the magazine (the spare on tag_clip1 is the gun's own mesh)");
        assert_eq!(parts(&combo), 4, "the optic rides the gun's view-model tag");
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

    /// The AN-94 authors its magazine rolled −15° (`attachViewModelRotations`): the manifest's
    /// magazine carries that rotation, and it is the only magazine part — the reload's spare on
    /// `tag_clip1` is skinned into the gun model, so no copy hangs there.
    #[test]
    #[ignore = "needs the BO2 install: ZHBO_BO2_DIR"]
    fn the_an94_magazine_keeps_its_authored_roll() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");
        let w = src.bo2.iter().find(|w| w.name == "an94_mp").expect("an94_mp");
        let i = w.attach_view_models.iter().position(|(m, _)| m.contains("attach_mag")).expect("the AN-94's authored magazine");
        let rotation = w.attach_view_rotations[i];
        assert!(rotation.iter().any(|c| *c != 0.0), "the authored rotation is not zero: {rotation:?}");
        let blobs = src.composition("an94_mp").expect("the base AN-94");
        let vm = zhbo_native::viewmodel::Viewmodel::from_bytes(blobs.get(&zhbo_native::viewmodel_path(0, 'U')).expect("a manifest")).expect("the manifest parses");
        let mags: Vec<_> = vm.parts.iter().filter(|p| p.model.contains(&w.attach_view_models[i].0)).collect();
        assert_eq!(mags.len(), 1, "one magazine part: {:?}", vm.parts);
        assert_eq!(mags[0].offset, (crate::xf::from_angles(rotation), w.attach_view_offsets[i]), "the authored rotation and offset");
        assert!(vm.parts.iter().all(|p| p.tag.as_deref() != Some("tag_clip1")), "no copy on tag_clip1");
    }

    /// The snipers' base scope is a second `attachViewModel` part: the conversion loads it, hangs
    /// it at the gun root (`j_gun`, the authored tag is empty) and keeps its authored offset. The
    /// four weapons are the complete set with non-magazine base art (the M32 is the fifth).
    #[test]
    #[ignore = "needs the BO2 install: ZHBO_BO2_DIR"]
    fn the_sniper_scope_is_a_base_part() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");
        for (id, offset) in [
            ("as50_mp", [4.456, 0.0, 4.253]),
            ("ballista_mp", [5.312, 0.0, 4.737]),
            ("dsr50_mp", [-5.701, 0.0, 4.843]),
            ("svu_mp", [4.253, -0.033, 6.839]),
        ] {
            let blobs = src.composition(id).expect(id);
            let vm = zhbo_native::viewmodel::Viewmodel::from_bytes(blobs.get(&zhbo_native::viewmodel_path(0, 'U')).expect("a manifest")).expect("the manifest parses");
            assert_eq!(vm.parts.len(), 4, "{id}: the arms, the gun, the magazine and the scope");
            let scope = vm.parts.iter().find(|p| p.model.contains("_scope_view")).unwrap_or_else(|| panic!("{id}: the base scope part"));
            assert_eq!(scope.tag.as_deref(), Some("j_gun"), "{id}: the empty authored tag is the gun root");
            assert_eq!(scope.offset.1, offset, "{id}: the scope keeps its authored offset");
        }
    }

    /// The `_scope_view` snipers read the part's `tag_scope` bone; the M32's non-`_scope_view`
    /// sight keeps the computed window. The marker's `local` is in the gun root's frame, so both
    /// assertions compose the target bone's bind through the gun root's inverse.
    #[test]
    #[ignore = "needs the BO2 install: ZHBO_BO2_DIR"]
    fn the_scope_view_marker_reads_tag_scope_and_the_m32_keeps_the_window() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");
        let load = |id: &str| {
            let blobs = src.composition(id).expect(id);
            let vm = zhbo_native::viewmodel::Viewmodel::from_bytes(blobs.get(&zhbo_native::viewmodel_path(0, 'U')).expect("a manifest")).expect("the manifest parses");
            (vm, blobs)
        };
        for id in ["dsr50_mp", "ballista_mp"] {
            let (vm, blobs) = load(id);
            let marker = vm.markers.iter().find(|m| m.name == zhbo_native::AIM_TAG).expect("an aim marker");
            let skel = skeleton(&vm, &blobs);
            let gun_root = skel.parts.get(1).expect("the gun part").first;
            let scope = skel.bone("tag_scope").expect("the scope's tag_scope bone");
            let expected = zhbo_native::math::compose(zhbo_native::math::inverse(skel.bones[gun_root].bind), (zhbo_native::math::QUAT_IDENTITY, skel.bones[scope].bind.1));
            assert!((marker.local.1[0] - expected.1[0]).abs() < 1e-4, "{id}: x");
            assert!((marker.local.1[1] - expected.1[1]).abs() < 1e-4, "{id}: y");
            assert!((marker.local.1[2] - expected.1[2]).abs() < 1e-4, "{id}: z");
        }
        // The M32's sight model is not `_scope_view`: its marker stays on the window point.
        let (vm, blobs) = load("m32_mp");
        let marker = vm.markers.iter().find(|m| m.name == zhbo_native::AIM_TAG).expect("an aim marker");
        let skel = skeleton(&vm, &blobs);
        let gun_root = skel.parts.get(1).expect("the gun part").first;
        let sight = vm.parts.iter().position(|p| p.model.contains("attach_optic_m32")).expect("the M32's sight part");
        let sight_bone = skel.parts.get(sight).expect("the sight's part bones").first;
        let model = zhbo_native::model::Model::from_bytes(blobs.get(&vm.parts[sight].model).expect("the sight's model")).expect("the model");
        let mid = model.mins[2] + (model.maxs[2] - model.mins[2]) * crate::generals::OPTIC_WINDOW;
        let window = zhbo_native::math::apply(skel.bones[sight_bone].bind, [0.0, 0.0, mid]);
        let expected = zhbo_native::math::compose(zhbo_native::math::inverse(skel.bones[gun_root].bind), (zhbo_native::math::QUAT_IDENTITY, window));
        assert!((marker.local.1[0] - expected.1[0]).abs() < 1e-4, "m32: x");
        assert!((marker.local.1[1] - expected.1[1]).abs() < 1e-4, "m32: y");
        assert!((marker.local.1[2] - expected.1[2]).abs() < 1e-4, "m32: z");
    }

    /// The IR combination moves the base scope instead of replacing it: `viewModelOffsetBaseAttachment`
    /// is in the same `j_gun` frame as `attachViewModelOffset` (the AS50 keeps the authored Y/Z of
    /// `[9.889, 0, 4.253]`), so it REPLACES the part's offset; the optic's mount then loads at its own
    /// `viewModelAddOffsets` (`[1, 0, 4.711]`, under the optic's `[1.045, 0, 5.568]`) and before it.
    #[test]
    #[ignore = "needs the BO2 install: ZHBO_BO2_DIR"]
    fn the_ir_combo_moves_the_base_scope_and_adds_the_mount() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");
        let manifest = |id: &str| {
            let blobs = src.composition(id).expect(id);
            zhbo_native::viewmodel::Viewmodel::from_bytes(blobs.get(&zhbo_native::viewmodel_path(0, 'U')).expect("a manifest")).expect("the manifest parses")
        };
        let vm = manifest("as50_mp+ir");
        let names: Vec<&str> = vm.parts.iter().map(|p| p.model.as_str()).collect();
        let at = |needle: &str| vm.parts.iter().position(|p| p.model.contains(needle)).unwrap_or_else(|| panic!("{needle} in {names:?}"));
        assert_eq!(names.len(), 6, "the arms, the gun, the magazine, the moved scope, the mount and the optic: {names:?}");
        assert_eq!(vm.parts[at("xpr50_scope_view")].offset.1, [9.889, 0.0, 4.253], "the base scope is moved, not removed: {names:?}");
        let near = |part: usize, want: [f32; 3]| (0..3).all(|k| (vm.parts[part].offset.1[k] - want[k]).abs() < 1e-4);
        let (mount, optic) = (at("dualband_mount_view"), at("dualband_view"));
        assert!(near(mount, [1.0, 0.0, 4.711]), "the mount rides viewModelAddOffsets: {:?}", vm.parts[mount].offset.1);
        assert!(near(optic, [1.045, 0.0, 5.568]), "the optic keeps viewModelOffsets: {:?}", vm.parts[optic].offset.1);
        assert!(mount < optic, "the mount draws before the optic ({mount} < {optic}): {names:?}");
        let scope = |vm: &zhbo_native::viewmodel::Viewmodel| vm.parts.iter().find(|p| p.model.contains("xpr50_scope_view")).expect("the base scope").offset.1;
        assert_eq!(scope(&manifest("as50_mp+vzoom")), [4.456, 0.0, 4.253], "no override flag, no move");
        let combined = manifest("as50_mp+swayreduc+ir");
        assert_eq!(scope(&combined), [9.889, 0.0, 4.253], "the combined IR unique moves the base scope too");
        // The exact combined unique authors no mount of its own (`viewModelAdditional` is 0), but
        // the composition is per single attachment: `combo()` keeps the single `au_<w>_ir` entry
        // for the `ir` short regardless of the exact combined unique, so its mount loads here too —
        // the same single-entry rule that already drew the dual-band optic on combined combos
        // before this change (a pre-existing over-draw, neither introduced nor fixed here).
        assert!(combined.parts.iter().any(|p| p.model.contains("dualband_mount_view")), "the mount follows the single IR entry, like the optic: {:?}", combined.parts.iter().map(|p| &p.model).collect::<Vec<_>>());
        assert!(combined.parts.iter().any(|p| p.model.contains("t6_attach_optic_dualband_view")), "the dual-band optic is the pre-existing single-entry over-draw: {:?}", combined.parts.iter().map(|p| &p.model).collect::<Vec<_>>());
    }

    /// An attachment with `disableBaseWeaponAttachment` replaces the base art: the ACOG swaps its
    /// scope for the base one, and the Ballista's `is` (an iron sight with no view model at all)
    /// simply takes it off. Neither stack the base scope under the replacement.
    #[test]
    #[ignore = "needs the BO2 install: ZHBO_BO2_DIR"]
    fn the_acog_replaces_the_base_scope() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");
        let parts = |blobs: &BTreeMap<String, Vec<u8>>| {
            zhbo_native::viewmodel::Viewmodel::from_bytes(blobs.get(&zhbo_native::viewmodel_path(0, 'U')).expect("a manifest")).expect("the manifest parses").parts
        };
        let as50 = parts(&src.composition("as50_mp+acog").expect("the AS50 with its ACOG"));
        assert_eq!(as50.len(), 4, "the ACOG replaces the scope, so the count is the base's 4");
        assert!(!as50.iter().any(|p| p.model.contains("xpr50_scope_view")), "the base scope is suppressed: {as50:?}");
        assert!(as50.iter().any(|p| p.model.contains("acog")), "the ACOG is mounted");
        let ballista = parts(&src.composition("ballista_mp+is").expect("the Ballista's iron sight"));
        assert_eq!(ballista.len(), 3, "suppression with no replacement geometry: the scope comes off");
        assert!(!ballista.iter().any(|p| p.model.contains("ballista_scope_view")), "the base scope is suppressed: {ballista:?}");
    }

    /// The M32 lists its sight twice (`_view` for the hip, `_ads` for the aim): the same lens and
    /// reticle at the same offset. Both load; the native manifest's ADS handoff swaps them.
    #[test]
    #[ignore = "needs the BO2 install: ZHBO_BO2_DIR"]
    fn the_m32_sight_loads_both() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");
        let blobs = src.composition("m32_mp").expect("the base M32");
        let vm = zhbo_native::viewmodel::Viewmodel::from_bytes(blobs.get(&zhbo_native::viewmodel_path(0, 'U')).expect("a manifest")).expect("the manifest parses");
        assert_eq!(vm.parts.len(), 4, "the arms, the gun and both sights");
        let sights = vm.parts.iter().filter(|p| p.model.contains("optic_m32")).count();
        assert_eq!(sights, 2, "both the hip `_view` and the ADS `_ads` pair: {vm:?}");
        assert!(vm.parts.iter().any(|p| p.model.contains("_ads") && p.ads_only), "the ADS variant is marked ads_only: {vm:?}");
    }

    /// The five pistol dualclips set neither replacement flag: their fast-mag sleeve is additive
    /// over the baked magazine, and the base art does not change. Suppression here would hide the
    /// attachment (the offset frame is a separate cycle).
    #[test]
    #[ignore = "needs the BO2 install: ZHBO_BO2_DIR"]
    fn the_pistol_dualclip_suppresses_nothing() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");
        let w = src.bo2.iter().find(|w| w.name == "fnp45_mp").expect("fnp45_mp");
        let c = crate::viewmodels::combo(w, &src.atts, &["dualclip"]);
        assert!(!c.parts.is_empty(), "the dualclip resolves to a unique");
        assert!(!c.no_base_attachment, "disableBaseWeaponAttachment is 0: the fast mag stacks");
        assert!(!c.no_clip, "disableBaseWeaponClip is 0: there is no separate base clip part");
        assert_eq!(
            src.base_part_count("fnp45_mp+dualclip"),
            src.base_part_count("fnp45_mp"),
            "the dualclip adds no base part and suppresses none (its own sleeve is an attachment part)"
        );
    }

    /// The dual-wield composite loads both guns: the base pistol's composite (the arms and the
    /// base gun) and the DW composite (the arms, the base gun and the `_lh` gun). The base's
    /// counts match RESEARCH.md's table (79 bones / 8058 vertices / 7 surfaces); the DW composite
    /// adds the `_lh` gun's ~6 bones / ~2205 vertices / 5 surfaces, giving ~10k vertices and 12
    /// surfaces. Each of the five DW pistols composes both guns.
    #[test]
    #[ignore = "needs the BO2 install: ZHBO_BO2_DIR"]
    fn the_dual_wield_composite_loads_both_guns() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");
        let counts = |id: &str| {
            let blobs = src.composition(id).expect(id);
            let manifest = zhbo_native::viewmodel::Viewmodel::from_bytes(blobs.get(&zhbo_native::viewmodel_path(0, 'U')).expect("a manifest")).expect("the manifest parses");
            let models: Vec<zhbo_native::model::Model> =
                manifest.parts.iter().map(|p| zhbo_native::model::Model::from_bytes(blobs.get(&p.model).expect(&p.model)).expect("a model")).collect();
            let refs: Vec<zhbo_native::pose::PartRef> = manifest.parts.iter().zip(&models).map(|(p, m)| zhbo_native::pose::PartRef { model: m, tag: p.tag.as_deref(), offset: p.offset, rigid: p.rigid }).collect();
            let (skel, _) = zhbo_native::pose::Skeleton::compose(&refs, &manifest.markers, &manifest.hide);
            let bones = skel.bones.len();
            let verts: usize = models.iter().flat_map(|m| m.lods.iter().flat_map(|l| (l.first_surface..l.first_surface + l.surface_count).filter_map(|s| m.surfaces.get(s as usize)).map(|s| s.vertices.len()))).sum();
            let surfaces: usize = models.iter().map(|m| m.surfaces.len()).sum();
            (bones, verts, surfaces, manifest.parts.len())
        };
        // The base's composite: the arms and the base gun. Its counts match RESEARCH.md's table.
        let (bb, bv, bs, _) = counts("fnp45_mp");
        assert_eq!((bb, bv, bs), (79, 8058, 7), "the base pistol's composite");
        // The DW composite: the base's parts plus the `_lh` gun's, a separate rig on the left palm.
        let (db, dv, ds, dp) = counts("fnp45_dw_mp");
        assert!(db >= 81, "the DW composite adds the left gun's bones: {db} vs {bb}");
        assert!(dv >= 10_000, "the DW composite adds the left gun's vertices: {dv} vs {bv}");
        // The raw `_lh` model carries 12 surfaces; the native path drops the reticle and clan-tag
        // surfaces the W3D pass leaves out too (`looks.dropped`), leaving 10.
        assert!(ds >= 10, "the DW composite adds the left gun's surfaces to the base's 7: {ds}");
        assert!(dp > 2, "the DW composite adds a part: {dp}");
        // Each of the five DW pistols composes both guns, whatever its per-weapon surface counts.
        for (base, dw) in [
            ("fnp45_mp", "fnp45_dw_mp"),
            ("fiveseven_mp", "fiveseven_dw_mp"),
            ("kard_mp", "kard_dw_mp"),
            ("judge_mp", "judge_dw_mp"),
            ("beretta93r_mp", "beretta93r_dw_mp"),
        ] {
            let (_, bv, bs, bp) = counts(base);
            let (db, dv, ds, dp) = counts(dw);
            assert!(db > bb, "{dw} adds the left gun's bones");
            assert!(dv > bv, "{dw} adds the left gun's vertices: {dv} vs {bv}");
            assert!(ds > bs, "{dw} adds the left gun's surfaces: {ds} vs {bs}");
            assert!(dp > bp, "{dw} adds a part: {dp} vs {bp}");
        }
    }

    /// The off-hand equipment rides the knife attach bone (the left palm), not the primary gun's
    /// `tag_weapon` anchor that the equipment clips park or hide. The inventory-2 claymore and PDA
    /// are in the same set; the AN-94 stays on the gun anchor as the control.
    #[test]
    #[ignore = "needs the BO2 install: ZHBO_BO2_DIR"]
    fn the_offhand_items_ride_the_knife_attach() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");
        let gun_tag = |id: &str| {
            let w = src.bo2.iter().find(|w| w.name == id).expect(id);
            let view = w.view_model.as_deref().expect("a view model");
            let blobs = src.composition(id).expect(id);
            let vm = zhbo_native::viewmodel::Viewmodel::from_bytes(blobs.get(&zhbo_native::viewmodel_path(0, 'U')).expect("a manifest")).expect("the manifest parses");
            vm.parts.iter().find(|p| p.model.contains(view)).and_then(|p| p.tag.clone())
        };
        for id in [
            "hatchet_mp", "frag_grenade_mp", "sticky_grenade_mp", "emp_grenade_mp",
            "flash_grenade_mp", "concussion_grenade_mp", "sensor_grenade_mp", "willy_pete_mp",
            "proximity_grenade_mp", "trophy_system_mp", "tactical_insertion_mp",
            "bouncingbetty_mp", "satchel_charge_mp", "claymore_mp", "pda_hack_mp",
        ] {
            assert_eq!(gun_tag(id).as_deref(), Some("tag_knife_attach"), "{id} should hang from the left palm");
        }
        assert_eq!(gun_tag("an94_mp").as_deref(), Some("tag_weapon"), "the gun control stays on the gun anchor");
    }

    /// The equipment's held/idle clips author the item at the waist, so its palm sits outside the
    /// game's hip view volume: at stand-still the hip placement is the identity eye frame, and the
    /// projection puts the palm's NDC y from just past the bottom edge to several frame-heights
    /// below it (the hatchet is behind the eye). This is the pose, not a harness bug — a hand-tuned
    /// hip offset would move the harness away from the game. The old ">200 px at hip" criterion
    /// asked the pose to do something it does not do; this asserts the mount in model space instead.
    /// One row per item is printed: id, slot, clip name, fwd/left/up and ndc_y. Cycle:
    /// `~/.opencode/plan/equipment-hip-pose/` (§3.1).
    #[test]
    #[ignore = "needs the BO2 install: ZHBO_BO2_DIR"]
    fn the_equipment_palm_is_outside_the_hip_frame() {
        use zhbo_core::viewmodel::slot;

        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");
        // The harness's projection, computed the same way (`render_viewmodels.rs`: t = 1/tan(hfov/2),
        // proj[5] = t * aspect). NDC y = proj[5] * up / forward; ±1 is the top/bottom edge of the
        // picture. Model space is the eye frame in inches: X forward, Y left, Z up.
        let aspect = 16.0 / 9.0;
        let t = 1.0 / (zhbo_core::client::horizontal_fov(zhbo_core::tuning::Tuning::default().cg_fov, aspect) / 2.0).tan();
        let proj5 = t * aspect;

        for id in [
            "frag_grenade_mp", "sticky_grenade_mp", "emp_grenade_mp", "concussion_grenade_mp",
            "willy_pete_mp", "flash_grenade_mp", "sensor_grenade_mp", "hatchet_mp",
            "trophy_system_mp", "tactical_insertion_mp", "bouncingbetty_mp",
            "satchel_charge_mp", "proximity_grenade_mp",
        ] {
            let blobs = src.composition(id).expect(id);
            let vm = zhbo_native::viewmodel::Viewmodel::from_bytes(blobs.get(&zhbo_native::viewmodel_path(0, 'U')).expect("a manifest")).expect("the manifest parses");
            // The mount: the item's own part hangs from the left palm's tag.
            let w = src.bo2.iter().find(|w| w.name == id).expect(id);
            let view = w.view_model.as_deref().expect("a view model");
            let item = vm.parts.iter().find(|p| p.model.contains(view)).expect("the item part");
            assert_eq!(item.tag.as_deref(), Some("tag_knife_attach"), "{id}: the item hangs from the left palm");
            // The slot the game plays while an item is held; fall back to the idle only to report a
            // stale manifest (the native path does the same, `nativevm/mod.rs:304-310`).
            let (slot_name, clip_path) = match vm.clip(slot::HOLD_FIRE as u16) {
                Some(path) => ("HOLD_FIRE", path),
                None => ("IDLE", vm.clip(slot::IDLE as u16).expect("an idle clip")),
            };
            let anim = zhbo_native::anim::Anim::from_bytes(blobs.get(clip_path).expect(clip_path)).expect(clip_path);
            let models: Vec<zhbo_native::model::Model> = vm.parts.iter().map(|p| zhbo_native::model::Model::from_bytes(blobs.get(&p.model).expect(&p.model)).expect("a model")).collect();
            let refs: Vec<zhbo_native::pose::PartRef> = vm.parts.iter().zip(&models).map(|(p, m)| zhbo_native::pose::PartRef { model: m, tag: p.tag.as_deref(), offset: p.offset, rigid: p.rigid }).collect();
            let (skel, problems) = zhbo_native::pose::Skeleton::compose(&refs, &vm.markers, &vm.hide);
            assert!(problems.is_empty(), "{id}: {problems:?}");
            let (mut local, mut ms) = (Vec::new(), Vec::new());
            skel.sample(&anim, 0.0, &mut local);
            skel.model_space(&local, &mut ms);
            let palm_i = skel.bone("tag_knife_attach").unwrap_or_else(|| panic!("{id}: no tag_knife_attach bone"));
            let palm = skel.bone_point(&ms, palm_i);
            let gun_i = skel.bone("j_gun").unwrap_or_else(|| panic!("{id}: no j_gun bone"));
            let gun = skel.bone_point(&ms, gun_i);
            let wrist_i = skel.bone("j_wrist_le").unwrap_or_else(|| panic!("{id}: no j_wrist_le bone"));
            let wrist = skel.bone_point(&ms, wrist_i);
            // The part root coincides with the palm: the item's mount offset is zero.
            assert!((0..3).all(|k| (gun[k] - palm[k]).abs() <= 0.01), "{id}: j_gun {gun:?} should sit on the palm {palm:?}");
            // It is the left hand: the parked gun anchor (`tag_weapon`) sits ~80" away on equipment.
            let gap = ((0..3).map(|k| (palm[k] - wrist[k]).powi(2)).sum::<f32>()).sqrt();
            assert!(gap <= 8.0, "{id}: the palm {palm:?} should be within 8\" of j_wrist_le {wrist:?} (distance {gap:.2}\")");
            // Outside the hip view volume: the palm's NDC y is below the bottom edge, or its forward
            // is non-positive (the hatchet, behind the eye, has no meaningful NDC).
            let ndc_y = proj5 * palm[2] / palm[0];
            assert!(palm[0] <= 0.0 || ndc_y < -1.0, "{id}: the palm {palm:?} is in the hip frame (fwd {:.2}, ndc_y {ndc_y:.2})", palm[0]);
            eprintln!("{id}: {slot_name} {} fwd={:.2} left={:.2} up={:.2} ndc_y={ndc_y:.2}{}", anim.name, palm[0], palm[1], palm[2], if palm[0] <= 0.0 { " (behind the eye)" } else { "" });
        }
    }

    /// BO2 hides equipment sub-bones by parking them hundreds of inches away: the trophy's deploy
    /// flies its panels off (`sndnt#fly_falling_panels` at 0.463) and the M84's `tag_clip` leaves
    /// the hand. The tracks are authored data, not a conversion artefact — do not filter large
    /// translations, and do not read a track as absolute: playback composes
    /// `posed local = the model's stored local + the track` (`zhbo_native::pose`), the rule the MP
    /// body rig proves where the two readings differ (`pb_death_faceplant`'s `j_spinelower` is a
    /// faceplant as an offset and a collapse as an absolute). The game plays `HOLD_FIRE` while an
    /// off-hand item is held and `FIRE` while it is thrown, never `IDLE`
    /// (`zhbo_core::viewmodel`, `OffhandUse`), so the sweep's `IDLE` half is not the clip the game
    /// shows. Cycle: `~/.opencode/plan/equipment-subbone-tracks/`.
    #[test]
    #[ignore = "needs the BO2 install: ZHBO_BO2_DIR"]
    fn equipment_subbone_tracks_are_bo2s_authored_parking() {
        use zhbo_core::viewmodel::slot;
        use zhbo_native::anim::Anim;

        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");

        // One weapon's converted manifest and blobs, through the sweep's own conversion.
        let load = |id: &str| -> (zhbo_native::viewmodel::Viewmodel, BTreeMap<String, Vec<u8>>) {
            let blobs = src.composition(id).expect(id);
            let vm = zhbo_native::viewmodel::Viewmodel::from_bytes(blobs.get(&zhbo_native::viewmodel_path(0, 'U')).expect("a manifest")).expect("the manifest parses");
            (vm, blobs)
        };
        let clip = |vm: &zhbo_native::viewmodel::Viewmodel, blobs: &BTreeMap<String, Vec<u8>>, slot: i32| -> Anim {
            let blob = vm.clip(slot as u16).unwrap_or_else(|| panic!("no clip in slot {slot:#x}"));
            Anim::from_bytes(blobs.get(blob).unwrap_or_else(|| panic!("{blob} is not in the conversion"))).expect(blob)
        };
        let track = |a: &Anim, bone: &str| -> Vec<(u16, [f32; 3])> {
            a.bones.iter().find(|b| b.name == bone).and_then(|b| b.translation.clone()).unwrap_or_else(|| panic!("{}: no {bone} translation track", a.name))
        };
        let near = |a: [f32; 3], b: [f32; 3], what: &str| assert!((0..3).all(|k| (a[k] - b[k]).abs() <= 0.01), "{what}: {a:?}, BO2 authored {b:?}");
        let bounds = |t: &[(u16, [f32; 3])]| {
            t.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(mut lo, mut hi), (_, p)| {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
                (lo, hi)
            })
        };
        let anim_path = zhbo_native::anim_path;

        // (a) The trophy's doors. `_idle` (the clip the harness renders) holds the deployed park
        // constantly and `_fire` starts on the same park; `_hold_fire` — the clip the game plays
        // while the trophy is held — flies the panels off through BO2's open numbers to that park.
        let (trophy, tb) = load("trophy_system_mp");
        assert_eq!(trophy.clip(slot::IDLE as u16), Some(anim_path("viewmodel_trophy_system_idle").as_str()), "the sweep's slot");
        assert_eq!(trophy.clip(slot::HOLD_FIRE as u16), Some(anim_path("viewmodel_trophy_system_hold_fire").as_str()), "the game's held clip");
        assert_eq!(trophy.clip(slot::FIRE as u16), Some(anim_path("viewmodel_trophy_system_fire").as_str()), "the game's throw clip");
        let idle = clip(&trophy, &tb, slot::IDLE);
        let fire = clip(&trophy, &tb, slot::FIRE);
        for (bone, park) in [("tag_left_door", [25.11, -554.70, -547.91]), ("tag_right_door", [22.66, -530.85, -547.27])] {
            for a in [&idle, &fire] {
                let t = track(a, bone);
                assert_eq!(t.len(), 1, "{}: {bone} is parked by one constant key", a.name);
                near(t[0].1, park, &format!("{} {bone}", a.name));
            }
        }
        let hold = clip(&trophy, &tb, slot::HOLD_FIRE);
        for (bone, (lo, hi)) in [
            ("tag_left_door", ([-58.38, -554.75, -547.99], [25.14, 0.0, 0.0])),
            ("tag_right_door", ([-15.59, -530.90, -547.35], [22.69, 28.71, 0.0])),
        ] {
            let t = track(&hold, bone);
            assert_eq!(t.len(), 24, "{}: {bone}'s authored deploy keys", hold.name);
            near(t[0].1, [0.0; 3], &format!("{} {bone} first key: the bind pose, panels on", hold.name));
            let (blo, bhi) = bounds(&t);
            near(blo, lo, &format!("{} {bone} low", hold.name));
            near(bhi, hi, &format!("{} {bone} high", hold.name));
        }
        assert!(
            hold.notes.iter().any(|n| n.name == "sndnt#fly_falling_panels" && (n.time - 0.463).abs() < 5e-4),
            "{}: the deploy's own notify says what the park is: {:?}",
            hold.name,
            hold.notes
        );

        // (b) The M84: idle parked, `_pullpin` (held) comes in near the palm then leaves, `_throw`
        // (fire) carries it to ±200″ — it leaves the hand.
        let (flash, fb) = load("flash_grenade_mp");
        assert_eq!(flash.clip(slot::HOLD_FIRE as u16), Some(anim_path("viewmodel_m84_pullpin").as_str()), "the game's held clip");
        assert_eq!(flash.clip(slot::FIRE as u16), Some(anim_path("viewmodel_m84_throw").as_str()), "the game's throw clip");
        let m84_idle = clip(&flash, &fb, slot::IDLE);
        let t = track(&m84_idle, "tag_clip");
        assert_eq!(t.len(), 1, "{}: the idle parks `tag_clip`", m84_idle.name);
        near(t[0].1, [-18.14, -24.69, 15.12], "m84 idle tag_clip");
        let pull = clip(&flash, &fb, slot::HOLD_FIRE);
        let t = track(&pull, "tag_clip");
        assert_eq!(t.len(), 27, "{}: the authored pull-pin keys", pull.name);
        near(t[0].1, [-4.54, 4.50, -1.86], "m84 pullpin tag_clip first key: near the palm");
        let (lo, hi) = bounds(&t);
        near(lo, [-18.14, -24.69, -1.86], "m84 pullpin tag_clip low");
        near(hi, [2.49, 4.50, 15.12], "m84 pullpin tag_clip high");
        let throw = clip(&flash, &fb, slot::FIRE);
        let t = track(&throw, "tag_clip");
        assert_eq!(t.len(), 22, "{}: the authored throw keys", throw.name);
        near(t[0].1, [-12.78, -12.08, 9.66], "m84 throw tag_clip first key");
        let (lo, hi) = bounds(&t);
        near(lo, [-198.27, -171.94, -203.84], "m84 throw tag_clip low: the ±200″ park");
        near(hi, [100.47, 73.31, 9.66], "m84 throw tag_clip high");

        // (c) The slot assertions above are the point: what the game plays (`HOLD_FIRE`, `FIRE`)
        // carries the tracks. Equipment also has no ADS pass to pose — the M84 carries no ADS clip
        // at all and the trophy's are the empty torso-only ones — so the sweep's aimed half
        // (`nativevm::place::aligned`) stages it with no equipment clip.
        for ads in [zhbo_formats::weapon::anim::ADS_UP as u16, zhbo_formats::weapon::anim::ADS_DOWN as u16] {
            assert_eq!(flash.clip(ads), None, "the M84 carries no ADS clip");
            let ads_path = trophy.clip(ads).expect("the trophy's authored ADS clip");
            let a = Anim::from_bytes(tb.get(ads_path).expect(ads_path)).expect(ads_path);
            assert_eq!(a.bones.len(), 1, "{}: the ADS clip keys only the torso", a.name);
            assert_eq!(a.bones[0].name, "tag_torso", "{}", a.name);
        }

        // (d) Playback composes `posed local = the model's stored local + the track`. The trophy's
        // doors are where the two readings differ (their stored local is real, unlike the
        // viewhands' zeros), so this is the body-rig proof's shape on converted equipment: at the
        // deploy's end the door sits its stored local away from the authored park, and reading the
        // track as absolute would move it (RESEARCH §2: `j_handle`/`j_sensor` collapse that way).
        let posed = |skel: &zhbo_native::pose::Skeleton, anim: &Anim, bone: &str| {
            let i = skel.bone(bone).unwrap_or_else(|| panic!("{}: no {bone}", anim.name));
            let mut local = Vec::new();
            skel.sample(anim, anim.num_frames as f32, &mut local);
            let end = anim.bones.iter().find(|b| b.name == bone).and_then(|b| b.translation_at(anim.num_frames as f32)).unwrap_or_else(|| panic!("{}: no {bone} track", anim.name));
            (local[i].1, skel.bones[i].bind_local.1, end)
        };
        let ts = skeleton(&trophy, &tb);
        for (bone, stored, park) in [
            ("tag_left_door", [-0.03, -4.74, 5.27], [25.11, -554.70, -547.91]),
            ("tag_right_door", [-0.03, 4.75, 5.27], [22.66, -530.85, -547.27]),
        ] {
            let (at_end, bind, end) = posed(&ts, &hold, bone);
            near(bind, stored, &format!("{bone}: the model's stored local"));
            near(end, park, &format!("{bone}: the deploy ends on the idle's park"));
            near(at_end, [stored[0] + end[0], stored[1] + end[1], stored[2] + end[2]], &format!("{bone}: posed = stored + track at the deploy's end"));
            assert!((0..3).any(|k| (at_end[k] - end[k]).abs() > 1.0), "{bone}: reading the track as absolute would differ ({at_end:?} vs {end:?})");
        }
        // The M84's `tag_clip` is its part's root with an identity attach offset, so here — as for
        // the viewhands — the two readings coincide; the composed value is still stored + track.
        let fs = skeleton(&flash, &fb);
        let (at_end, bind, end) = posed(&fs, &throw, "tag_clip");
        near(bind, [0.0; 3], "m84 tag_clip: the part root's attach offset");
        near(at_end, [bind[0] + end[0], bind[1] + end[1], bind[2] + end[2]], "m84 tag_clip: posed = stored + track at the throw's end");
    }

    /// BO2's aimed pose is animation data: `ads_up`'s last key is the whole hip→aimed rig move
    /// (translation on all three axes plus the rig root's rotation), so the authored tag lands
    /// where BO2 authored it. The acceptance is therefore "the authored tag lands at the measured
    /// value", not "the marker is on the axis": the KSG's 2.198″, the launchers' 1.7–2.9″ and the
    /// AS50's 1.302″ are BO2's own residuals — forcing them onto the axis would re-introduce the
    /// bug this cycle removes. Values from `ads-alignment` §2.1/§3 and `ads-transition-clips`
    /// §1.3, in composed space, inches, with no `gunpos` and no aim pitch (the probe's reading).
    #[test]
    #[ignore = "needs the BO2 install: ZHBO_BO2_DIR"]
    fn the_ads_clip_reproduces_bo2s_authored_pose() {
        use zhbo_core::viewmodel::slot;
        use zhbo_native::anim::Anim;

        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");
        // (id, the bone the family's authored line lands on, expected (y, z) after `ads_up`'s last
        // key). Scoped snipers read `tag_scope`; the no-tag families read the converter's marker.
        // The an94/mp7 rows are the research's *reflex-mounted* compositions: their measured
        // `tag_sights` values reproduce exactly (`an94_mp` bare reads (−0.003, +0.491), `mp7_mp`
        // bare (−0.002, +0.050), because with the optic off, `tag_sights` is the gun's own iron
        // tag; the optic replaces it). Asserting the bare ids against these numbers would pin two
        // different measurements to one row.
        let table: &[(&str, &str, [f32; 2])] = &[
            ("an94_mp+reflex", "tag_sights", [0.034, 0.017]),
            ("mp7_mp+reflex", "tag_sights", [0.033, -0.561]),
            ("mk48_mp", "tag_sights", [-0.002, -0.025]),
            ("ksg_mp", "tag_sights", [0.005, -2.198]),
            ("dsr50_mp", "tag_scope", [0.009, 0.011]),
            ("ballista_mp", "tag_scope", [0.027, -0.117]),
            ("as50_mp", "tag_scope", [0.004, -1.302]),
            ("fnp45_mp", zhbo_native::AIM_TAG, [-0.038, -0.035]),
            ("smaw_mp", zhbo_native::AIM_TAG, [0.011, 0.419]),
            ("fhj18_mp", zhbo_native::AIM_TAG, [-2.917, 0.488]),
            ("m32_mp", zhbo_native::AIM_TAG, [0.002, 0.107]),
        ];
        // The idle at frame 0, then the ADS clip over it at `frame` — the game's own order
        // (`Skeleton::sample`, then `sample_over`) — read in composed model space.
        let posed = |skel: &zhbo_native::pose::Skeleton, idle: &Anim, layer: Option<(&Anim, f32)>, tag: &str, id: &str| -> ([f32; 3], [f32; 3]) {
            let (mut local, mut ms) = (Vec::new(), Vec::new());
            skel.sample(idle, 0.0, &mut local);
            if let Some((a, frame)) = layer {
                skel.sample_over(a, frame, &mut local);
            }
            skel.model_space(&local, &mut ms);
            let i = skel.bone(tag).unwrap_or_else(|| panic!("{id}: no {tag} bone"));
            let gun = skel.bone("j_gun").unwrap_or_else(|| panic!("{id}: no j_gun bone"));
            // The barrel's direction is the bone's own +X (a vector: the bone's rotation only).
            (skel.bone_point(&ms, i), zhbo_native::math::qrot(ms[gun].0, [1.0, 0.0, 0.0]))
        };
        let angle = |d: [f32; 3]| {
            let n = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            (d[0] / n).clamp(-1.0, 1.0).acos().to_degrees()
        };
        let load = |id: &str| -> (zhbo_native::viewmodel::Viewmodel, BTreeMap<String, Vec<u8>>) {
            let blobs = src.composition(id).expect(id);
            let vm = zhbo_native::viewmodel::Viewmodel::from_bytes(blobs.get(&zhbo_native::viewmodel_path(0, 'U')).expect("a manifest")).expect("the manifest parses");
            (vm, blobs)
        };
        let anim = |id: &str, blobs: &BTreeMap<String, Vec<u8>>, path: &str| Anim::from_bytes(blobs.get(path).unwrap_or_else(|| panic!("{id}: {path} is not in the conversion"))).expect(path);

        for &(id, tag, want) in table {
            let (vm, blobs) = load(id);
            // Every gun def carries both transitions; a missing one is a conversion defect, and
            // failing here is louder than silently sliding.
            let up_path = vm.clip(slot::ADS_UP as u16).unwrap_or_else(|| panic!("{id}: no 0x55 ads_up clip"));
            let down_path = vm.clip(slot::ADS_DOWN as u16).unwrap_or_else(|| panic!("{id}: no 0x56 ads_down clip"));
            let idle_path = vm.clip(slot::IDLE as u16).unwrap_or_else(|| panic!("{id}: no idle clip"));
            let (up, down, idle) = (anim(id, &blobs, up_path), anim(id, &blobs, down_path), anim(id, &blobs, idle_path));
            let skel = skeleton(&vm, &blobs);
            let (aimed, barrel) = posed(&skel, &idle, Some((&up, up.num_frames as f32)), tag, id);
            assert!(
                (aimed[1] - want[0]).abs() <= 0.05 && (aimed[2] - want[1]).abs() <= 0.05,
                "{id}: {tag} lands at ({:.3}, {:.3}) in, BO2 authored ({:.3}, {:.3})",
                aimed[1],
                aimed[2],
                want[0],
                want[1]
            );
            // The twins: `ads_down`'s first key is `ads_up`'s last, so the layer's switch-and-step
            // reversal is continuous without a cross-fade.
            let (start, _) = posed(&skel, &idle, Some((&down, 0.0)), tag, id);
            assert!((0..3).all(|k| (start[k] - aimed[k]).abs() <= 0.02), "{id}: ads_down frame 0 {start:?} is not ads_up frame {} {aimed:?}", up.num_frames);
            if matches!(id, "smaw_mp" | "fhj18_mp") {
                // The launchers are the only real rotations in the set (SMAW +16.52°/−10.60°,
                // FHJ-18 +14.59°/−19.43° at the hip): the barrel ends on the view axis only if the
                // rotation track — not just the translation — is applied.
                let (_, hip_barrel) = posed(&skel, &idle, None, tag, id);
                assert!(angle(hip_barrel) > 10.0, "{id}: the hip barrel is {:.2}° off — the overlay must be what straightens it", angle(hip_barrel));
                // SMAW 19.86° → 0.38° (the research's own −0.38°/−0.01°); the FHJ-18 25.34° → 2.32°,
                // its tube keeping a small authored cant (the research measured the SMAW's end
                // angles only and called the FHJ-18 "likewise straightened"). Both are the authored
                // data's business, never forced onto the axis.
                let limit = if id == "smaw_mp" { 0.5 } else { 3.0 };
                assert!(angle(barrel) <= limit, "{id}: the barrel is {:.2}° off the view axis after the overlay (hip {:.2}°)", angle(barrel), angle(hip_barrel));
            }
        }

        // The dual-wield shape: the pair's `tag_torso` track is constant at the bind local, so the
        // layer's root *move* is zero — aiming does not shift the guns (BO2 authors the pair on the
        // same spot; the pair stays symmetric). The authored value is the rig root's own local
        // ([-4.64, 0, −1.38] on this viewhands rig), not the origin: what must be zero is the move.
        let (vm, blobs) = load("fnp45_dw_mp");
        let up_path = vm.clip(slot::ADS_UP as u16).expect("fnp45_dw_mp: a 0x55 ads_up clip");
        assert!(vm.clip(slot::ADS_DOWN as u16).is_some(), "fnp45_dw_mp: a 0x56 ads_down clip");
        let up = anim("fnp45_dw_mp", &blobs, up_path);
        let track = up.bones.iter().find(|b| b.name == "tag_torso").unwrap_or_else(|| panic!("fnp45_dw_mp: the layer keys tag_torso"));
        let first = track.translation_at(0.0).expect("a tag_torso translation track");
        for f in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let at = track.translation_at(f * up.num_frames as f32).expect("a tag_torso translation track");
            assert!((0..3).all(|k| (at[k] - first[k]).abs() <= 0.05), "fnp45_dw_mp: at {f} the layer is {at:?}, not the constant {first:?}");
        }
        let skel = skeleton(&vm, &blobs);
        let root = skel.bone("tag_torso").map(|i| skel.bones[i].bind_local.1).expect("the rig root");
        assert!((0..3).all(|k| (first[k] - root[k]).abs() <= 0.05), "fnp45_dw_mp: the layer's root move is {first:?}, not the bind local {root:?} — the pair would move");
    }

    /// The W3D ADS layer's engine composition: the action clip's pose everywhere except
    /// `tag_torso`, where the ADS clip's pose takes over — the native `sample_over` rule, which
    /// the C++ `HAnimComboClass` pivot maps mirror. This checks the layer's authored pose against
    /// the same table as `the_ads_clip_reproduces_bo2s_authored_pose`; the in-game `--w3d-ads`
    /// leg exercises the actual engine combo.
    #[test]
    #[ignore = "needs the BO2 install: ZHBO_BO2_DIR"]
    fn the_w3d_ads_layer_composes_over_the_action_clip() {
        use zhbo_core::viewmodel::slot;
        use zhbo_native::anim::Anim;

        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");
        let table: &[(&str, &str, [f32; 2])] = &[
            ("an94_mp+reflex", "tag_sights", [0.034, 0.017]),
            ("mp7_mp+reflex", "tag_sights", [0.033, -0.561]),
            ("mk48_mp", "tag_sights", [-0.002, -0.025]),
            ("ksg_mp", "tag_sights", [0.005, -2.198]),
            ("dsr50_mp", "tag_scope", [0.009, 0.011]),
            ("ballista_mp", "tag_scope", [0.027, -0.117]),
            ("as50_mp", "tag_scope", [0.004, -1.302]),
            ("fnp45_mp", zhbo_native::AIM_TAG, [-0.038, -0.035]),
            ("smaw_mp", zhbo_native::AIM_TAG, [0.011, 0.419]),
            ("fhj18_mp", zhbo_native::AIM_TAG, [-2.917, 0.488]),
            ("m32_mp", zhbo_native::AIM_TAG, [0.002, 0.107]),
        ];
        // The engine's composition: the action clip at frame 0, then the ADS clip's tag_torso over
        // it. `sample_over` is the native twin; the C++ combo's pivot maps produce the same result.
        let posed = |skel: &zhbo_native::pose::Skeleton, idle: &Anim, layer: (&Anim, f32), tag: &str, id: &str| -> [f32; 3] {
            let (mut local, mut ms) = (Vec::new(), Vec::new());
            skel.sample(idle, 0.0, &mut local);
            skel.sample_over(layer.0, layer.1, &mut local);
            skel.model_space(&local, &mut ms);
            let i = skel.bone(tag).unwrap_or_else(|| panic!("{id}: no {tag} bone"));
            skel.bone_point(&ms, i)
        };
        let load = |id: &str| -> (zhbo_native::viewmodel::Viewmodel, BTreeMap<String, Vec<u8>>) {
            let blobs = src.composition(id).expect(id);
            let vm = zhbo_native::viewmodel::Viewmodel::from_bytes(blobs.get(&zhbo_native::viewmodel_path(0, 'U')).expect("a manifest")).expect("the manifest parses");
            (vm, blobs)
        };
        let anim = |id: &str, blobs: &BTreeMap<String, Vec<u8>>, path: &str| Anim::from_bytes(blobs.get(path).unwrap_or_else(|| panic!("{id}: {path} is not in the conversion"))).expect(path);

        for &(id, tag, want) in table {
            let (vm, blobs) = load(id);
            let up_path = vm.clip(slot::ADS_UP as u16).unwrap_or_else(|| panic!("{id}: no 0x55 ads_up clip"));
            let idle_path = vm.clip(slot::IDLE as u16).unwrap_or_else(|| panic!("{id}: no idle clip"));
            let (up, idle) = (anim(id, &blobs, up_path), anim(id, &blobs, idle_path));
            let skel = skeleton(&vm, &blobs);
            let aimed = posed(&skel, &idle, (&up, up.num_frames as f32), tag, id);
            assert!(
                (aimed[1] - want[0]).abs() <= 0.05 && (aimed[2] - want[1]).abs() <= 0.05,
                "{id}: the W3D layer's {tag} lands at ({:.3}, {:.3}) in, BO2 authored ({:.3}, {:.3})",
                aimed[1],
                aimed[2],
                want[0],
                want[1]
            );
        }
    }

    #[test]
    #[ignore = "probe: the flagged defects"]
    fn probe_defects() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");
        // STEP 0 raw-tag probe: the parser only reads `attachViewModelTag` when `attachViewModel`
        // is non-empty; read the raw array directly for the thirteen off-hand items and the gun
        // control (`an94_mp`) to see whether BO2 authored the knife tag on the equipment.
        let kind = zhbo_formats::fastfile::asset_type_id("weapon").expect("weapon assets");
        for name in ["hatchet_mp", "frag_grenade_mp", "sticky_grenade_mp", "emp_grenade_mp", "flash_grenade_mp", "concussion_grenade_mp", "sensor_grenade_mp", "willy_pete_mp", "proximity_grenade_mp", "trophy_system_mp", "tactical_insertion_mp", "bouncingbetty_mp", "satchel_charge_mp", "an94_mp"] {
            let raw = src.common.find(kind, name).map_or_else(|| "MISSING".to_string(), |a| {
                let tags = src.common.try_u32(a.addr + zhbo_formats::t6zone::layouts::WeaponVariantDef::attachViewModelTag).unwrap_or(0);
                if tags == 0 { "EMPTY (null)".to_string() } else { (0..8).map(|i| src.common.str_ptr(tags + 4 * i)).collect::<Vec<_>>().join(", ") }
            });
            eprintln!("RAW attachViewModelTag {name}: [{raw}]");
        }
        let summary = |id: &str| {
            let mut out = BTreeMap::new();
            let mut defs = src.defs.clone();
            match crate::viewmodels::weapon_viewmodels(&src.patch, &src.common, &src.zones, &src.paks, &mut out, &src.bo2, &src.atts, &mut defs, &[id.to_string()], 0) {
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

    /// c1 diagnostic dump (phase 3a): for each of the ten invisible attachments, print the mount
    /// tag's bind transform, the gun's root bind transform, the authored view offset, and the
    /// attachment's composed position under Convention A (weapon-origin frame) and Convention B
    /// (mount-tag frame), compared against the base gun's bounding box and the view cone. No
    /// behavior change: this only prints. The output is the per-item convention verdict table the
    /// plan's §3.1 asks for.
    #[test]
    #[ignore = "diagnostic: needs the BO2 install: ZHBO_BO2_DIR"]
    fn dump_attachment_conventions() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR");
        let src = Source::open(Path::new(&dir)).expect("open the install");
        let aspect = 16.0 / 9.0;
        let t = 1.0 / (zhbo_core::client::horizontal_fov(zhbo_core::tuning::Tuning::default().cg_fov, aspect) / 2.0).tan();
        let proj5 = t * aspect;
        let items = [
            ("as50_mp", "ir"),
            ("as50_mp", "silencer"),
            ("as50_mp", "swayreduc"),
            ("fnp45_mp", "dualclip"),
            ("fiveseven_mp", "dualclip"),
            ("kard_mp", "dualclip"),
            ("judge_mp", "dualclip"),
            ("beretta93r_mp", "dualclip"),
            ("kard_mp", "steadyaim"),
            ("ksg_mp", "steadyaim"),
        ];
        for (weapon, att) in items {
            let blobs = src.composition(weapon).expect(weapon);
            let vm = zhbo_native::viewmodel::Viewmodel::from_bytes(blobs.get(&zhbo_native::viewmodel_path(0, 'U')).expect("a manifest")).expect("the manifest parses");
            let models: Vec<zhbo_native::model::Model> = vm.parts.iter().map(|p| zhbo_native::model::Model::from_bytes(blobs.get(&p.model).expect(&p.model)).expect("a model")).collect();
            let refs: Vec<zhbo_native::pose::PartRef> = vm.parts.iter().zip(&models).map(|(p, m)| zhbo_native::pose::PartRef { model: m, tag: p.tag.as_deref(), offset: p.offset, rigid: p.rigid }).collect();
            let (skel, _) = zhbo_native::pose::Skeleton::compose(&refs, &vm.markers, &vm.hide);
            let w = src.bo2.iter().find(|w| w.name == weapon).expect(weapon);
            let view_name = w.view_model.as_deref().expect("a view model");
            let gun_part = vm.parts.iter().position(|p| p.model.contains(view_name)).expect("the gun part");
            let gun_model = &models[gun_part];
            let gun_bind = skel.bones[skel.parts[gun_part].first].bind;
            let tag_weapon_bind = skel.bones[skel.bone("tag_weapon").expect("tag_weapon")].bind;
            let u = w.attachment_variants.iter().find(|(_, id)| crate::weapons::short_name(id) == att).and_then(|(kind, _)| w.attachment_uniques.iter().find(|u| u.kind == *kind && u.combined == 0)).expect(att);
            let offset = (crate::xf::from_angles(u.view_rotation), u.view_offset);
            let mount_tag = u.view_model_tag.as_deref().unwrap_or("j_gun");
            let tag_bind = skel.bone(mount_tag).map(|i| skel.bones[i].bind).unwrap_or(zhbo_native::math::IDENTITY);
            let pos_a = zhbo_native::math::compose(gun_bind, offset).1;
            let pos_b = zhbo_native::math::compose(tag_bind, offset).1;
            let eye_a = zhbo_native::math::apply(tag_weapon_bind, pos_a);
            let eye_b = zhbo_native::math::apply(tag_weapon_bind, pos_b);
            let in_bbox_a = (0..3).all(|k| pos_a[k] >= gun_model.mins[k] && pos_a[k] <= gun_model.maxs[k]);
            let in_bbox_b = (0..3).all(|k| pos_b[k] >= gun_model.mins[k] && pos_b[k] <= gun_model.maxs[k]);
            let ndc_a = proj5 * eye_a[2] / eye_a[0];
            let ndc_b = proj5 * eye_b[2] / eye_b[0];
            let in_cone_a = eye_a[0] > 0.0 && ndc_a.abs() <= 1.0;
            let in_cone_b = eye_b[0] > 0.0 && ndc_b.abs() <= 1.0;
            eprintln!("{weapon}+{att}: tag={mount_tag} offset={:?}", u.view_offset);
            eprintln!("  tag_bind={:?} gun_bind={:?}", tag_bind.1, gun_bind.1);
            eprintln!("  A pos={:?} in_bbox={in_bbox_a} eye={:?} ndc={ndc_a:.2} in_cone={in_cone_a}", pos_a, eye_a);
            eprintln!("  B pos={:?} in_bbox={in_bbox_b} eye={:?} ndc={ndc_b:.2} in_cone={in_cone_b}", pos_b, eye_b);
        }
    }
}
