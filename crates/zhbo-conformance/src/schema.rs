//! The shared CSV schema: every fact both sides of the comparison produce from code.
//!
//! A column is a fact that exists in BO2 *and* in the mod. A fact meaningless in one world is not
//! a column (scorestreaks, Zero Hour factions, the V1 plan's deliberate divergences; see
//! [`crate::bo2::rows`]'s exclusion list); a BO2 fact the mod should carry but does not is a column
//! whose mod side is empty, and it fails. There are no informational columns: the differ holds
//! every column to parity.
//!
//! Values are in BO2's units (milliseconds as BO2 truncates them, inches, degrees). The only
//! tolerance is a float column's epsilon, which covers unit round-trip noise (the mod stores
//! seconds and Zero Hour units as `f32`); it is part of the column, not of the differ.

use std::fmt;

/// How a column's cells are compared.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// Exact text (names, enums).
    Text,
    /// An integer, compared exactly.
    Int,
    /// `0` or `1`.
    Bool,
    /// A number, equal within the epsilon.
    Float(f64),
    /// `|`-separated numbers (a vector, a curve), equal element by element within the epsilon.
    Floats(f64),
    /// `|`-separated items whose order matters.
    List,
    /// `|`-separated items, sorted by the row builder; order does not matter.
    Set,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Kind::Text => write!(f, "text"),
            Kind::Int => write!(f, "int"),
            Kind::Bool => write!(f, "bool"),
            Kind::Float(e) => write!(f, "float(eps {e})"),
            Kind::Floats(e) => write!(f, "floats(eps {e})"),
            Kind::List => write!(f, "list"),
            Kind::Set => write!(f, "set"),
        }
    }
}

/// The epsilon of a millisecond column: the mod stores seconds as `f32` (×1000 round trip).
pub const MS_EPS: f64 = 1e-3;
/// The epsilon of an inch column: the mod stores Zero Hour units as `f32` (÷ZH_PER_INCH round trip).
pub const INCH_EPS: f64 = 1e-3;
/// The epsilon of a dimensionless or degree column stored as `f32` on both sides.
pub const F32_EPS: f64 = 1e-5;
/// The epsilon of a rotation quaternion component: degrees through `f32` trigonometry on both sides.
pub const QUAT_EPS: f64 = 1e-4;

#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub name: String,
    /// The cluster the differ groups the column under (`timing`, `parts`, `anims`, …).
    pub area: &'static str,
    pub kind: Kind,
    /// What the fact is and where BO2 decides it (G14/G15 section, file or function).
    pub doc: String,
}

/// One table: its two key columns and its fact columns.
#[derive(Clone, Debug, PartialEq)]
pub struct Schema {
    /// `weapons` or `cac`.
    pub name: &'static str,
    pub key: [&'static str; 2],
    pub columns: Vec<Column>,
}

impl Schema {
    pub fn new(name: &'static str, key: [&'static str; 2]) -> Schema {
        Schema { name, key, columns: Vec::new() }
    }

    /// Add a column; panics on a duplicate name (a schema bug, caught by the unit tests).
    pub fn col(&mut self, name: impl Into<String>, area: &'static str, kind: Kind, doc: impl Into<String>) -> &mut Schema {
        let name = name.into();
        assert!(!self.columns.iter().any(|c| c.name == name), "duplicate column {name}");
        assert!(!self.key.contains(&name.as_str()), "column {name} shadows a key");
        self.columns.push(Column { name, area, kind, doc: doc.into() });
        self
    }

    pub fn index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name == name)
    }

    /// The CSV header: the keys, then the columns.
    pub fn header(&self) -> Vec<String> {
        self.key.iter().map(|k| k.to_string()).chain(self.columns.iter().map(|c| c.name.clone())).collect()
    }
}

/// One row's key: the weapon (or CaC item) and the combination (or sub-key).
pub type Key = (String, String);

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub key: Key,
    /// One cell per schema column, in schema order; `""` is absent.
    pub cells: Vec<String>,
}

/// Builds one row and refuses to finish unless every schema column was set exactly once, so a
/// generator can neither forget a column nor invent one.
pub struct RowBuilder<'a> {
    schema: &'a Schema,
    key: Key,
    cells: Vec<Option<String>>,
    errors: Vec<String>,
}

impl<'a> RowBuilder<'a> {
    pub fn new(schema: &'a Schema, weapon: impl Into<String>, combo: impl Into<String>) -> RowBuilder<'a> {
        RowBuilder { schema, key: (weapon.into(), combo.into()), cells: vec![None; schema.columns.len()], errors: Vec::new() }
    }

    fn put(&mut self, name: &str, want: fn(Kind) -> bool, text: String) -> &mut Self {
        match self.schema.index(name) {
            None => self.errors.push(format!("unknown column {name}")),
            Some(i) if !want(self.schema.columns[i].kind) => {
                self.errors.push(format!("column {name} is {}, set with the wrong kind", self.schema.columns[i].kind))
            }
            Some(i) if self.cells[i].is_some() => self.errors.push(format!("column {name} set twice")),
            Some(i) => self.cells[i] = Some(text),
        }
        self
    }

    /// The fact is absent on this side (any kind).
    pub fn absent(&mut self, name: &str) -> &mut Self {
        self.put(name, |_| true, String::new())
    }

    pub fn text(&mut self, name: &str, v: impl AsRef<str>) -> &mut Self {
        self.put(name, |k| k == Kind::Text, v.as_ref().to_string())
    }

    pub fn opt_text(&mut self, name: &str, v: Option<impl AsRef<str>>) -> &mut Self {
        let s = v.map(|v| v.as_ref().to_string()).unwrap_or_default();
        self.put(name, |k| k == Kind::Text, s)
    }

    pub fn int(&mut self, name: &str, v: i64) -> &mut Self {
        self.put(name, |k| k == Kind::Int, v.to_string())
    }

    pub fn bool(&mut self, name: &str, v: bool) -> &mut Self {
        self.put(name, |k| k == Kind::Bool, if v { "1" } else { "0" }.to_string())
    }

    pub fn float(&mut self, name: &str, v: f64) -> &mut Self {
        self.put(name, |k| matches!(k, Kind::Float(_)), num(v))
    }

    pub fn floats(&mut self, name: &str, v: &[f64]) -> &mut Self {
        let s = v.iter().map(|x| num(*x)).collect::<Vec<_>>().join("|");
        self.put(name, |k| matches!(k, Kind::Floats(_)), s)
    }

    pub fn list<S: AsRef<str>>(&mut self, name: &str, v: &[S]) -> &mut Self {
        let s = v.iter().map(|x| x.as_ref()).collect::<Vec<_>>().join("|");
        self.put(name, |k| k == Kind::List, s)
    }

    pub fn set<S: AsRef<str>>(&mut self, name: &str, v: &[S]) -> &mut Self {
        let mut items: Vec<&str> = v.iter().map(|x| x.as_ref()).collect();
        items.sort_unstable();
        items.dedup();
        self.put(name, |k| k == Kind::Set, items.join("|"))
    }

    /// As [`RowBuilder::finish`], but a column never set is empty: for a table whose kinds use
    /// different columns (the CaC table). A column one side forgets still diffs against the other.
    pub fn finish_sparse(mut self) -> Result<Row, String> {
        for c in self.cells.iter_mut().filter(|c| c.is_none()) {
            *c = Some(String::new());
        }
        self.finish()
    }

    pub fn finish(self) -> Result<Row, String> {
        let mut errors = self.errors;
        let missing: Vec<&str> = self.cells.iter().zip(&self.schema.columns).filter(|(c, _)| c.is_none()).map(|(_, col)| col.name.as_str()).collect();
        if !missing.is_empty() {
            errors.push(format!("columns never set: {}", missing.join(", ")));
        }
        if errors.is_empty() {
            Ok(Row { key: self.key, cells: self.cells.into_iter().map(Option::unwrap_or_default).collect() })
        } else {
            Err(format!("{} {}: {}", self.key.0, self.key.1, errors.join("; ")))
        }
    }
}

/// A number as the CSV writes it: the shortest text that reads back to the same `f64`, with
/// `-0` folded to `0` and integers without a fraction.
pub fn num(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    format!("{v}")
}

/// An `f32` widened to the `f64` its shortest decimal text names (so `0.1f32` is `0.1`, not
/// `0.10000000149`): both sides pass their `f32` values through this before [`RowBuilder::float`].
pub fn f32_value(v: f32) -> f64 {
    format!("{v}").parse().unwrap_or(v as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> Schema {
        let mut s = Schema::new("weapons", ["weapon", "combo"]);
        s.col("clip", "ammo", Kind::Int, "").col("tags", "hide", Kind::Set, "").col("fov", "ads", Kind::Float(F32_EPS), "");
        s
    }

    #[test]
    fn a_row_needs_every_column_exactly_once() {
        let s = schema();
        let mut b = RowBuilder::new(&s, "an94_mp", "reflex");
        b.int("clip", 30).set("tags", &["b", "a", "a"]);
        let err = b.finish().unwrap_err();
        assert!(err.contains("columns never set: fov"), "{err}");

        let mut b = RowBuilder::new(&s, "an94_mp", "");
        b.int("clip", 30).int("clip", 31).set("tags", &["x"]).absent("fov").text("bogus", "1");
        let err = b.finish().unwrap_err();
        assert!(err.contains("set twice") && err.contains("unknown column bogus"), "{err}");
    }

    #[test]
    fn kinds_are_enforced_and_sets_are_sorted() {
        let s = schema();
        let mut b = RowBuilder::new(&s, "an94_mp", "");
        b.text("clip", "30").set("tags", &["tag_b", "tag_a", "tag_b"]).float("fov", f32_value(0.1));
        assert!(b.finish().unwrap_err().contains("wrong kind"));
        let mut b = RowBuilder::new(&s, "an94_mp", "");
        b.int("clip", 30).set("tags", &["tag_b", "tag_a", "tag_b"]).float("fov", f32_value(0.1));
        let row = b.finish().unwrap();
        assert_eq!(row.cells, vec!["30", "tag_a|tag_b", "0.1"]);
    }

    #[test]
    #[should_panic(expected = "duplicate column")]
    fn duplicate_columns_are_a_schema_bug() {
        let mut s = schema();
        s.col("clip", "ammo", Kind::Int, "");
    }
}
