//! The class's camo choice applied to a composed viewmodel's surfaces: name-matched material
//! replacements for style camos and the chosen pattern layer for pattern camos. The recipe is the
//! converted `camo_<weapon>` blob; the rule is research §4.1, stated once here.

use zhbo_native::camo::{Camo, Set};
use zhbo_native::model::CamoLayer;

use super::{Files, Mesh};

/// Read a weapon's camo blob, or `None` when the archive lacks it (the weapon then draws stock).
pub(crate) fn read(files: &mut Files, asset: &str) -> Result<Option<Camo>, String> {
    let path = zhbo_native::camo_path(asset);
    if files.is_missing(&path) {
        return Ok(None);
    }
    let Some(bytes) = files.get(&path) else { return Ok(None) };
    let camo = Camo::from_bytes(&bytes).map_err(|e| format!("{path}: {e}"))?;
    files.forget(&path);
    Ok(Some(camo))
}

/// A stable draw-cache key for a choice (`0` = no camo: `none`, `camo_none` or empty). Cache
/// identity only; it never reaches a blob.
pub(crate) fn key_of(asset: &str, option: &str) -> u64 {
    if asset.is_empty() || option.is_empty() || option == "none" || option == "camo_none" {
        return 0;
    }
    let mut h: u64 = 0xcbf29ce484222325;
    for b in asset.as_bytes().iter().chain([b'|'].iter()).chain(option.as_bytes()) {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h | 1
}

/// Apply `option`'s recipe to every matching surface of `mesh`. A style camo replaces the named
/// surfaces' materials (and clears their camo layer); a pattern camo sets the layer image the
/// surface's own default kind takes. `camo_none`, an unmapped id and a blob without the option
/// change nothing.
pub(crate) fn apply(mesh: &mut Mesh, camo: &Camo, option: &str) {
    let Some(recipe) = camo.options.iter().find(|o| o.id == option) else { return };
    if let Some(style) = recipe.style.and_then(|i| camo.styles.get(i as usize)) {
        for entry in &style.entries {
            for pair in &entry.pairs {
                let Some(material) = camo.materials.iter().find(|m| m.name == pair.camo) else { continue };
                for s in mesh.surfaces.iter_mut().filter(|s| s.material == pair.base) {
                    s.color = material.color.clone();
                    s.maps = material.maps.clone();
                    s.blend = material.blend;
                    s.emissive = material.emissive;
                    s.sort_key = material.sort_key;
                    s.cull = material.cull;
                    // A style material carries no camo layer: its own art is the whole look.
                    s.camo_layer = CamoLayer::None;
                    s.camo = None;
                }
            }
        }
    }
    if let Some(set) = recipe.pattern.and_then(|i| camo.sets.get(i as usize)) {
        paint(mesh, set);
    }
}

fn paint(mesh: &mut Mesh, set: &Set) {
    for s in &mut mesh.surfaces {
        s.camo = match s.camo_layer {
            CamoLayer::Pattern => set.pattern.clone(),
            CamoLayer::Solid => set.solid.clone(),
            CamoLayer::Neutral | CamoLayer::None => None,
        };
        if s.camo.is_some() {
            s.camo_uv = [set.offset[0], set.offset[1], set.scale, set.gain];
        }
    }
}

/// The local player's chosen camo and the viewmodel it belongs to (PLAN D4).
#[derive(Clone, Debug, PartialEq)]
pub struct LocalCamo {
    pub vm: i32,
    pub asset: String,
    pub option: String,
}
