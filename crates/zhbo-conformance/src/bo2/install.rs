//! The player's BO2 install as the oracle reads it: the multiplayer zones, loaded once.

use super::raw::{Attachment, Variant};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use zhbo_formats::t6zone::{self, Zone};

fn load(dir: &Path, name: &str) -> Result<Zone, String> {
    t6zone::load_file(&t6zone::install_path(dir, name))
}

/// The zones the expected side reads: `common_mp` (the weapons, attachments and models),
/// `common_patch_mp` (the balance patches of 79 weapons and 49 attachments, the Peacekeeper),
/// `patch_mp` (the stringtables and the patched models) and `patch_ui_mp` (the rest of the UI Lua).
pub struct Install {
    pub common: Zone,
    pub common_patch: Zone,
    pub patch: Zone,
    pub patch_ui: Zone,
    /// The english localized strings of the multiplayer zones, later zones over earlier ones.
    pub strings: HashMap<String, String>,
}

/// The english string zones a multiplayer session loads, in load order.
const STRING_ZONES: &[&str] = &["code_pre_gfx_mp", "code_post_gfx_mp", "ui_mp", "common_mp", "patch_mp", "patch_ui_mp"];

fn strings(dir: &Path) -> HashMap<String, String> {
    zhbo_formats::localize::install_strings(dir, STRING_ZONES)
}

impl Install {
    pub fn open(dir: &Path) -> Result<Install, String> {
        Ok(Install {
            common: load(dir, "common_mp")?,
            common_patch: load(dir, "common_patch_mp")?,
            patch: load(dir, "patch_mp")?,
            patch_ui: load(dir, "patch_ui_mp")?,
            strings: strings(dir),
        })
    }

    /// The weapon zones in rising asset-pool priority: `common_mp` (zone flag 0x80), its patch zone
    /// `common_patch_mp` (0x100, which `DB_LoadXAssets`, `FUN_0059a550`, loads with it as
    /// `"%s_patch%s"`), then `patch_mp` (flag 2). `DB_LinkXAssetEntry` (`FUN_00597000`) keeps the
    /// asset of the zone whose flag ranks higher in `FUN_00596e60` (0x36 < 0x37 < 0x41), so a later
    /// zone here replaces an earlier one; a `,name` reference stub never replaces a real asset.
    fn pool(&self) -> [&Zone; 3] {
        [&self.common, &self.common_patch, &self.patch]
    }

    /// Every `WeaponVariantDef` by name, as the asset pool holds it (`pool`).
    pub fn variants(&self) -> BTreeMap<String, Variant<'_>> {
        let mut out = BTreeMap::new();
        let Some(kind) = zhbo_formats::fastfile::asset_type_id("weapon") else { return out };
        for z in self.pool() {
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
        for z in self.pool() {
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

    /// An XModel as the asset pool holds it (`pool`, highest priority first); a model a zone
    /// carries without geometry (a stub the next zone's real asset replaces) is skipped.
    pub fn model(&self, name: &str) -> Option<zhbo_formats::xmodel::Model> {
        let found: Vec<_> = self.pool().into_iter().rev().filter_map(|z| zhbo_formats::xmodel::find(z, name).ok()).collect();
        let real = found.iter().position(|m| !m.lods.is_empty()).unwrap_or(0);
        found.into_iter().nth(real)
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
