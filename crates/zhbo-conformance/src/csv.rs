//! One CSV writer and reader for both sides (RFC 4180 quoting), so `expected.csv` and `actual.csv`
//! are byte-comparable in shape and any SQL/CSV tool can load them.

use crate::schema::{Row, Schema};

/// A parsed table: the header row and the data rows, cells as text.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

fn quote(cell: &str) -> String {
    if cell.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_string()
    }
}

fn line(cells: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    cells.into_iter().map(|c| quote(c.as_ref())).collect::<Vec<_>>().join(",")
}

/// The CSV text of `rows` under `schema`'s header, rows sorted by key.
pub fn write(schema: &Schema, rows: &[Row]) -> String {
    let mut sorted: Vec<&Row> = rows.iter().collect();
    sorted.sort_by(|a, b| a.key.cmp(&b.key));
    let mut out = line(schema.header());
    out.push('\n');
    for r in sorted {
        out.push_str(&line([r.key.0.as_str(), r.key.1.as_str()].into_iter().chain(r.cells.iter().map(String::as_str))));
        out.push('\n');
    }
    out
}

/// The schema itself as a CSV (`column,area,kind,doc`), written next to the tables so a query can
/// join on areas and read each column's tolerance and citation.
pub fn write_schema(schema: &Schema) -> String {
    let mut out = line(["column", "area", "kind", "doc"]);
    out.push('\n');
    for c in &schema.columns {
        out.push_str(&line([c.name.as_str(), c.area, &c.kind.to_string(), c.doc.as_str()]));
        out.push('\n');
    }
    out
}

/// Parse CSV text (the format [`write`] produces; quoted cells may hold commas, quotes, newlines).
pub fn read(text: &str) -> Result<Table, String> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    cell.push('"');
                }
                '"' => quoted = false,
                _ => cell.push(c),
            }
            continue;
        }
        match c {
            '"' if cell.is_empty() => quoted = true,
            ',' => record.push(std::mem::take(&mut cell)),
            '\r' => {}
            '\n' => {
                record.push(std::mem::take(&mut cell));
                records.push(std::mem::take(&mut record));
            }
            _ => cell.push(c),
        }
    }
    if quoted {
        return Err("unterminated quoted cell".to_string());
    }
    if !cell.is_empty() || !record.is_empty() {
        record.push(cell);
        records.push(record);
    }
    let mut it = records.into_iter();
    let header = it.next().ok_or("empty CSV")?;
    let rows: Vec<Vec<String>> = it.collect();
    if let Some((i, r)) = rows.iter().enumerate().find(|(_, r)| r.len() != header.len()) {
        return Err(format!("row {} has {} cells, the header {}", i + 2, r.len(), header.len()));
    }
    Ok(Table { header, rows })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Kind, RowBuilder};

    #[test]
    fn tables_round_trip_with_awkward_cells() {
        let mut s = Schema::new("weapons", ["weapon", "combo"]);
        s.col("name", "names", Kind::Text, "").col("parts", "parts", Kind::List, "");
        let mut b = RowBuilder::new(&s, "an94_mp", "gl+reflex");
        b.text("name", "AN-94, \"the\" rifle\nline two").list("parts", &["a", "b"]);
        let mut c = RowBuilder::new(&s, "an94_mp", "");
        c.absent("name").list("parts", &["m"]);
        let text = write(&s, &[b.finish().unwrap(), c.finish().unwrap()]);
        let t = read(&text).unwrap();
        assert_eq!(t.header, vec!["weapon", "combo", "name", "parts"]);
        // Sorted by key: the bare weapon first.
        assert_eq!(t.rows[0], vec!["an94_mp", "", "", "m"]);
        assert_eq!(t.rows[1], vec!["an94_mp", "gl+reflex", "AN-94, \"the\" rifle\nline two", "a|b"]);
    }

    #[test]
    fn ragged_rows_are_refused() {
        assert!(read("a,b\n1\n").unwrap_err().contains("row 2"));
        assert!(read("a\n\"open\n").is_err());
    }
}
