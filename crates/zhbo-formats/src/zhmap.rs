//! Zero Hour's chunk files (`DataChunkInput`/`DataChunkOutput`, `Common/System/DataChunk.cpp`), the
//! format of its `.map` files: the `CkMp` table of chunk and key names, then chunks of `id u32,
//! version u16, size i32, data`, nested where a chunk's parser registers sub-parsers
//! (`ObjectsList` holds `Object`s). Read into a tree that keeps every byte it does not interpret,
//! so writing it back reproduces the input, and a converter can add objects to a stock map.
//!
//! A map object (`WorldHeightMap::ParseObjectData`): its position (3 × f32), angle (f32), flags
//! (i32), template name (`readAsciiString`: u16 length, bytes) and, from version 2, its properties
//! (`readDict`: u16 count, each `(nameIndex << 8) | type` then the value by type: bool a byte, int
//! and real 4 bytes, an ASCII string as above, a Unicode string a u16 count of UTF-16 units).

use crate::FormatError;

/// A chunk: its name, version and either its raw data or (for a container) its children.
#[derive(Clone, Debug, PartialEq)]
pub struct Chunk {
    pub name: String,
    pub version: u16,
    pub body: Body,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    Raw(Vec<u8>),
    Children(Vec<Chunk>),
}

/// The chunks that contain chunks (the parsers that register sub-parsers) the converter reads into.
const CONTAINERS: &[&str] = &["ObjectsList"];

/// A whole chunk file: the name table (in id order as read) and the top-level chunks.
#[derive(Clone, Debug, PartialEq)]
pub struct ChunkFile {
    /// `(name, id)` as the table lists them.
    pub names: Vec<(String, u32)>,
    pub chunks: Vec<Chunk>,
}

fn err(m: impl Into<String>) -> FormatError {
    FormatError::Corrupt(m.into())
}

struct Rd<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], FormatError> {
        let end = self.at.checked_add(n).ok_or_else(|| err("chunk file: overflow"))?;
        let s = self.b.get(self.at..end).ok_or_else(|| err("chunk file: past the end"))?;
        self.at = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, FormatError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, FormatError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, FormatError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, FormatError> {
        Ok(f32::from_bits(self.u32()?))
    }
}

impl ChunkFile {
    /// Parse an uncompressed chunk file (a `.map` after `refpack::decompress_file`).
    pub fn parse(bytes: &[u8]) -> Result<ChunkFile, FormatError> {
        let mut r = Rd { b: bytes, at: 0 };
        if r.take(4)? != b"CkMp" {
            return Err(err("chunk file: no CkMp table"));
        }
        let count = r.u32()? as usize;
        if count > 100_000 {
            return Err(err("chunk file: name table too large"));
        }
        let mut names = Vec::with_capacity(count);
        for _ in 0..count {
            let len = r.u8()? as usize;
            let name = String::from_utf8_lossy(r.take(len)?).into_owned();
            let id = r.u32()?;
            names.push((name, id));
        }
        let mut f = ChunkFile { names, chunks: Vec::new() };
        let rest = &bytes[r.at..];
        f.chunks = f.parse_chunks(rest)?;
        Ok(f)
    }

    fn name_of(&self, id: u32) -> Result<String, FormatError> {
        self.names.iter().find(|(_, i)| *i == id).map(|(n, _)| n.clone()).ok_or_else(|| err(format!("chunk file: no name for id {id}")))
    }

    fn parse_chunks(&self, bytes: &[u8]) -> Result<Vec<Chunk>, FormatError> {
        let mut r = Rd { b: bytes, at: 0 };
        let mut out = Vec::new();
        while r.at < bytes.len() {
            let id = r.u32()?;
            let version = r.u16()?;
            let size = r.u32()? as usize;
            let data = r.take(size)?;
            let name = self.name_of(id)?;
            let body = if CONTAINERS.contains(&name.as_str()) { Body::Children(self.parse_chunks(data)?) } else { Body::Raw(data.to_vec()) };
            out.push(Chunk { name, version, body });
        }
        Ok(out)
    }

    /// The id of `name` in the table, adding it if missing.
    pub fn id_of(&mut self, name: &str) -> u32 {
        if let Some((_, id)) = self.names.iter().find(|(n, _)| n == name) {
            return *id;
        }
        let id = self.names.iter().map(|(_, i)| *i).max().unwrap_or(0) + 1;
        self.names.push((name.to_string(), id));
        id
    }

    /// Write the file back (uncompressed).
    pub fn write(&self) -> Vec<u8> {
        let mut out = b"CkMp".to_vec();
        out.extend((self.names.len() as u32).to_le_bytes());
        for (n, id) in &self.names {
            out.push(n.len() as u8);
            out.extend(n.as_bytes());
            out.extend(id.to_le_bytes());
        }
        for c in &self.chunks {
            self.write_chunk(c, &mut out);
        }
        out
    }

    fn write_chunk(&self, c: &Chunk, out: &mut Vec<u8>) {
        let id = self.names.iter().find(|(n, _)| *n == c.name).map_or(0, |(_, i)| *i);
        let data = match &c.body {
            Body::Raw(d) => d.clone(),
            Body::Children(kids) => {
                let mut d = Vec::new();
                for k in kids {
                    self.write_chunk(k, &mut d);
                }
                d
            }
        };
        out.extend(id.to_le_bytes());
        out.extend(c.version.to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(data);
    }

    /// The top-level chunk named `name`.
    pub fn chunk_mut(&mut self, name: &str) -> Option<&mut Chunk> {
        self.chunks.iter_mut().find(|c| c.name == name)
    }

    pub fn chunk(&self, name: &str) -> Option<&Chunk> {
        self.chunks.iter().find(|c| c.name == name)
    }
}

/// A property value (`Dict::DataType`: 0 bool, 1 int, 2 real, 3 ASCII string, 4 Unicode string).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i32),
    Real(f32),
    Ascii(String),
    Unicode(Vec<u16>),
}

/// A map object, decoded.
#[derive(Clone, Debug, PartialEq)]
pub struct MapObject {
    pub pos: [f32; 3],
    pub angle: f32,
    pub flags: i32,
    pub name: String,
    /// `(key name, value)` in the file's order.
    pub props: Vec<(String, Value)>,
}

impl MapObject {
    /// Decode an `Object` chunk of version `version` against `file`'s name table.
    pub fn read(file: &ChunkFile, version: u16, data: &[u8]) -> Result<MapObject, FormatError> {
        let mut r = Rd { b: data, at: 0 };
        let pos = [r.f32()?, r.f32()?, r.f32()?];
        let angle = r.f32()?;
        let flags = r.u32()? as i32;
        let len = r.u16()? as usize;
        let name = String::from_utf8_lossy(r.take(len)?).into_owned();
        let mut props = Vec::new();
        if version >= 2 {
            let n = r.u16()? as usize;
            for _ in 0..n {
                let kt = r.u32()?;
                let key = file.name_of(kt >> 8)?;
                let v = match kt & 0xff {
                    0 => Value::Bool(r.u8()? != 0),
                    1 => Value::Int(r.u32()? as i32),
                    2 => Value::Real(r.f32()?),
                    3 => {
                        let l = r.u16()? as usize;
                        Value::Ascii(String::from_utf8_lossy(r.take(l)?).into_owned())
                    }
                    4 => {
                        let l = r.u16()? as usize;
                        let b = r.take(2 * l)?;
                        Value::Unicode((0..l).map(|k| u16::from_le_bytes([b[2 * k], b[2 * k + 1]])).collect())
                    }
                    t => return Err(err(format!("map object {name}: property type {t}"))),
                };
                props.push((key, v));
            }
        }
        Ok(MapObject { pos, angle, flags, name, props })
    }

    /// Encode as an `Object` chunk's data (version 2 or later), adding key names to `file`.
    pub fn write(&self, file: &mut ChunkFile) -> Vec<u8> {
        let mut d = Vec::new();
        for c in self.pos {
            d.extend(c.to_le_bytes());
        }
        d.extend(self.angle.to_le_bytes());
        d.extend(self.flags.to_le_bytes());
        d.extend((self.name.len() as u16).to_le_bytes());
        d.extend(self.name.as_bytes());
        d.extend((self.props.len() as u16).to_le_bytes());
        for (k, v) in &self.props {
            let id = file.id_of(k);
            let t = match v {
                Value::Bool(_) => 0,
                Value::Int(_) => 1,
                Value::Real(_) => 2,
                Value::Ascii(_) => 3,
                Value::Unicode(_) => 4,
            };
            d.extend(((id << 8) | t).to_le_bytes());
            match v {
                Value::Bool(b) => d.push(*b as u8),
                Value::Int(i) => d.extend(i.to_le_bytes()),
                Value::Real(x) => d.extend(x.to_le_bytes()),
                Value::Ascii(s) => {
                    d.extend((s.len() as u16).to_le_bytes());
                    d.extend(s.as_bytes());
                }
                Value::Unicode(u) => {
                    d.extend((u.len() as u16).to_le_bytes());
                    for c in u {
                        d.extend(c.to_le_bytes());
                    }
                }
            }
        }
        d
    }

    pub fn prop(&self, key: &str) -> Option<&Value> {
        self.props.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chunk_file_round_trips_and_takes_a_new_object() {
        let mut f = ChunkFile { names: vec![("ObjectsList".into(), 1), ("Object".into(), 2), ("originalOwner".into(), 3), ("HeightMapData".into(), 4)], chunks: Vec::new() };
        let obj = MapObject { pos: [1.0, 2.0, 0.0], angle: 0.5, flags: 0, name: "Tree".into(), props: vec![("originalOwner".into(), Value::Ascii("team".into()))] };
        let data = obj.write(&mut f);
        f.chunks.push(Chunk { name: "HeightMapData".into(), version: 4, body: Body::Raw(vec![9, 9, 9]) });
        f.chunks.push(Chunk { name: "ObjectsList".into(), version: 3, body: Body::Children(vec![Chunk { name: "Object".into(), version: 3, body: Body::Raw(data) }]) });
        let bytes = f.write();
        let back = ChunkFile::parse(&bytes).unwrap();
        assert_eq!(back, f);
        let Body::Children(kids) = &back.chunk("ObjectsList").unwrap().body else { panic!() };
        let Body::Raw(d) = &kids[0].body else { panic!() };
        assert_eq!(MapObject::read(&back, 3, d).unwrap(), obj);
        assert!(ChunkFile::parse(b"nope").is_err());
        assert!(ChunkFile::parse(&bytes[..bytes.len() - 2]).is_err(), "a truncated chunk is refused");
    }
}
