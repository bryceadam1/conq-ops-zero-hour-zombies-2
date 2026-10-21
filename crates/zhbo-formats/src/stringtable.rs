//! Black Ops II (T6) string tables (`StringTable` assets, the `.csv` files the UI and scripts read,
//! such as `mp/statstable.csv` in `patch_mp.ff`): a grid of strings, row by row.

use crate::t6zone::layouts::{StringTable as T, StringTableCell as C};
use crate::t6zone::Zone;
use crate::FormatError;

/// One table: its asset name and its rows (every row has the table's column count).
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub name: String,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    /// The rows whose column `col` equals `value`.
    pub fn rows_where<'a>(&'a self, col: usize, value: &'a str) -> impl Iterator<Item = &'a [String]> + 'a {
        self.rows.iter().filter(move |r| r.get(col).is_some_and(|c| c == value)).map(Vec::as_slice)
    }
}

/// The string table named `name` in a loaded zone (`mp/statstable.csv`).
pub fn find(z: &Zone, name: &str) -> Result<Option<Table>, FormatError> {
    let Some(kind) = crate::fastfile::asset_type_id("stringtable") else { return Ok(None) };
    let Some(asset) = z.find(kind, name).or_else(|| z.find(kind, &format!(",{name}"))) else { return Ok(None) };
    read(z, asset.addr).map(Some)
}

/// The `StringTable` at `t`.
pub fn read(z: &Zone, t: u32) -> Result<Table, FormatError> {
    let cols = z.try_u32(t + T::columnCount)? as usize;
    let rows = z.try_u32(t + T::rowCount)? as usize;
    if cols > 4096 || rows > 1 << 20 {
        return Err(FormatError::Corrupt(format!("implausible string table {cols}x{rows}")));
    }
    let values = z.try_u32(t + T::values)?;
    let mut out = Vec::with_capacity(rows);
    for r in 0..rows {
        let mut row = Vec::with_capacity(cols);
        for c in 0..cols {
            let cell = values + C::SIZE * (r * cols + c) as u32;
            row.push(if values == 0 { String::new() } else { z.try_str_ptr(cell + C::string)? });
        }
        out.push(row);
    }
    Ok(Table { name: z.try_str_ptr(t + T::name)?.trim_start_matches(',').to_string(), rows: out })
}

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

#[cfg(test)]
mod tests {
    /// The real `mp/statstable.csv` parses and names every weapon's image. Needs the BO2 install:
    /// `ZHBO_BO2_DIR=... cargo test -p zhbo-formats --release -- --ignored`.
    #[test]
    #[ignore]
    fn reads_the_real_statstable() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR");
        let file = std::fs::read(std::path::Path::new(&dir).join("zone/all/patch_mp.ff")).unwrap();
        let zone = crate::fastfile::read(&file).unwrap();
        let loaded = crate::t6zone::load(&zone).unwrap();
        let table = super::find(&loaded, "mp/statstable.csv").unwrap().expect("statstable.csv");
        // Column 4 is the item's reference name, column 6 its image (`mp7` -> `menu_mp_weapons_mp7`).
        let mp7 = table.rows_where(4, "mp7").next().expect("the MP7 row");
        assert_eq!(mp7.get(6).map(String::as_str), Some("menu_mp_weapons_mp7"));
        assert!(table.rows.len() > 100, "every item row: {}", table.rows.len());
    }
}
