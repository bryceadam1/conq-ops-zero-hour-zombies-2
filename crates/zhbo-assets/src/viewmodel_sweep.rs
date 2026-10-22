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
}
