//! Black Ops II (T6) string tables (`StringTable` assets, BO2's `.csv` tables such as
//! `mp/statstable.csv` in `patch_mp.ff`), read from a fully loaded zone: a row-major grid of cells.

use crate::t6zone::layouts::{StringTable as T, StringTableCell as C};
use crate::t6zone::Zone;
use crate::FormatError;

/// Every row of the table named `name` (`mp/statstable.csv`), or `None` when the zone lacks it.
pub fn rows(z: &Zone, name: &str) -> Result<Option<Vec<Vec<String>>>, FormatError> {
    let Some(kind) = crate::fastfile::asset_type_id("stringtable") else { return Ok(None) };
    let Some(asset) = z.find(kind, name).or_else(|| z.find(kind, &format!(",{name}"))) else { return Ok(None) };
    let at = asset.addr;
    let cols = z.try_u32(at + T::columnCount)? as usize;
    let rows = z.try_u32(at + T::rowCount)? as usize;
    let values = z.try_u32(at + T::values)?;
    if values == 0 || cols == 0 || cols > 256 || rows > 65536 {
        return Ok(Some(Vec::new()));
    }
    let mut out = Vec::with_capacity(rows);
    for r in 0..rows {
        let mut row = Vec::with_capacity(cols);
        for c in 0..cols {
            let cell = values + C::SIZE * (r * cols + c) as u32;
            let p = z.try_u32(cell + C::string)?;
            row.push(if p == 0 { String::new() } else { z.try_str_ptr(cell + C::string)? });
        }
        out.push(row);
    }
    Ok(Some(out))
}
