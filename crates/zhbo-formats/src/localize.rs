//! Black Ops II (T6) localized strings, read from a fully loaded language zone such as
//! `zone/english/en_common_mp.ff`: `LocalizeEntry` assets map a key (`WEAPON_AN94`) to its text.

use crate::t6zone::layouts::LocalizeEntry as LE;
use crate::t6zone::Zone;
use std::collections::HashMap;

/// Every key and its text in a loaded zone.
pub fn entries(z: &Zone) -> Result<HashMap<String, String>, crate::FormatError> {
    let Some(kind) = crate::fastfile::asset_type_id("localize") else { return Ok(HashMap::new()) };
    Ok(z.assets
        .iter()
        .filter(|a| a.kind == kind)
        .map(|a| -> Result<(String, String), crate::FormatError> {
            Ok((z.try_str_ptr(a.addr + LE::name)?, z.try_str_ptr(a.addr + LE::value)?))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|(k, _)| !k.is_empty())
        .collect())
}

/// The localized strings of `zones` in the player's install, in its own language
/// ([`crate::t6zone::language`]: `zone/english/en_<zone>.ff`, `zone/french/fr_<zone>.ff`, ...),
/// later zones over earlier ones. A zone that is missing or unreadable is skipped.
pub fn install_strings(bo2_dir: &std::path::Path, zones: &[&str]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for zone in zones {
        let Ok(z) = crate::t6zone::load_file(&crate::t6zone::language_path(bo2_dir, zone)) else { continue };
        out.extend(entries(&z).unwrap_or_default());
    }
    out
}
