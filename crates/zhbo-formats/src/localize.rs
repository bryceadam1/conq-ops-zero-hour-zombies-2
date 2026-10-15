//! Black Ops II (T6) localized strings, read from a fully loaded language zone such as
//! `zone/english/en_common_mp.ff`: `LocalizeEntry` assets map a key (`WEAPON_AN94`) to its text.

use crate::t6zone::layouts::LocalizeEntry as LE;
use crate::t6zone::Zone;
use std::collections::HashMap;

/// Every key and its text in a loaded zone.
pub fn entries(z: &Zone) -> HashMap<String, String> {
    let Some(kind) = crate::fastfile::asset_type_id("localize") else { return HashMap::new() };
    z.assets
        .iter()
        .filter(|a| a.kind == kind)
        .map(|a| (z.str_ptr(a.addr + LE::name), z.str_ptr(a.addr + LE::value)))
        .filter(|(k, _)| !k.is_empty())
        .collect()
}
