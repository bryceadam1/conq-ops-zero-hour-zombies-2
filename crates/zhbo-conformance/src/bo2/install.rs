//! The player's BO2 install as the oracle reads it: the multiplayer zones, loaded once.

use super::raw::{Attachment, Variant};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use zhbo_formats::t6zone::{self, Zone};

fn load(dir: &Path, name: &str) -> Result<Zone, String> {
    let path = dir.join("zone").join("all").join(format!("{name}.ff"));
    let file = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let ff = zhbo_formats::fastfile::read(&file).map_err(|e| format!("{name}: {e}"))?;
    t6zone::load(&ff).map_err(|e| format!("{name}: {e}"))
}

/// The zones the expected side reads: `common_mp` (the weapons, attachments and models),
/// `patch_mp` (the stringtables and the patched models) and `patch_ui_mp` (the rest of the UI Lua).
pub struct Install {
    pub common: Zone,
    pub patch: Zone,
    pub patch_ui: Zone,
    /// The english localized strings of the multiplayer zones, later zones over earlier ones.
    pub strings: HashMap<String, String>,
}

/// The english string zones a multiplayer session loads, in load order.
const STRING_ZONES: &[&str] = &["code_pre_gfx_mp", "code_post_gfx_mp", "ui_mp", "common_mp", "patch_mp", "patch_ui_mp"];

fn strings(dir: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for zone in STRING_ZONES {
        let path = dir.join("zone").join("english").join(format!("en_{zone}.ff"));
        let Ok(file) = std::fs::read(&path) else { continue };
        let Ok(ff) = zhbo_formats::fastfile::read(&file) else { continue };
        let Ok(z) = t6zone::load(&ff) else { continue };
        out.extend(zhbo_formats::localize::entries(&z).unwrap_or_default());
    }
    out
}

impl Install {
    pub fn open(dir: &Path) -> Result<Install, String> {
        Ok(Install { common: load(dir, "common_mp")?, patch: load(dir, "patch_mp")?, patch_ui: load(dir, "patch_ui_mp")?, strings: strings(dir) })
    }

    /// Every `WeaponVariantDef` by name. A later zone's asset of the same name replaces an earlier
    /// one, as BO2's asset pool does when `patch_mp` loads after `common_mp`.
    pub fn variants(&self) -> BTreeMap<String, Variant<'_>> {
        let mut out = BTreeMap::new();
        let Some(kind) = zhbo_formats::fastfile::asset_type_id("weapon") else { return out };
        for z in [&self.common, &self.patch] {
            for a in z.assets.iter().filter(|a| a.kind == kind) {
                let v = Variant { z, at: a.addr };
                out.insert(v.name(), v);
            }
        }
        out
    }

    /// Every `WeaponAttachment` by name (the same override order).
    pub fn attachments(&self) -> BTreeMap<String, Attachment<'_>> {
        let mut out = BTreeMap::new();
        let Some(kind) = zhbo_formats::fastfile::asset_type_id("attachment") else { return out };
        for z in [&self.common, &self.patch] {
            for a in z.assets.iter().filter(|a| a.kind == kind) {
                let at = Attachment { z, at: a.addr };
                out.insert(at.name(), at);
            }
        }
        out
    }

    /// A localize key's text; the engine shows the key itself when it has no entry
    /// ("Could not translate display name", `FUN_0052d930`).
    pub fn localize(&self, key: &str) -> String {
        self.strings.get(key).cloned().unwrap_or_else(|| key.to_string())
    }

    /// An XModel as the asset pool holds it: `patch_mp`'s asset replaces `common_mp`'s.
    pub fn model(&self, name: &str) -> Option<zhbo_formats::xmodel::Model> {
        zhbo_formats::xmodel::find(&self.patch, name).or_else(|_| zhbo_formats::xmodel::find(&self.common, name)).ok()
    }

    /// A stringtable's rows (`mp/statstable.csv`), from the patch zone first.
    pub fn table(&self, name: &str) -> Result<Vec<Vec<String>>, String> {
        for z in [&self.patch, &self.common] {
            if let Some(rows) = zhbo_formats::stringtable::rows(z, name).map_err(|e| e.to_string())? {
                return Ok(rows);
            }
        }
        Err(format!("{name}: not in patch_mp or common_mp"))
    }
}
