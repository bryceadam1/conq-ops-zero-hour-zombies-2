//! SAGE engine `.big` archives (Generals / Zero Hour).
//!
//! Layout (all offsets absolute):
//! ```text
//! 0x00  [u8;4]  "BIGF" (or "BIG4")
//! 0x04  u32 LE  archive size
//! 0x08  u32 BE  file count
//! 0x0C  u32 BE  offset of first file's data (header size)
//! 0x10  entries: { u32 BE offset, u32 BE size, NUL-terminated path }
//! ```
//! Paths use backslashes and are matched case-insensitively by the engine.

use crate::{FormatError, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BigEntry {
    pub path: String,
    pub offset: u32,
    pub size: u32,
}

#[derive(Debug)]
pub struct BigArchive {
    source: PathBuf,
    entries: Vec<BigEntry>,
}

impl BigArchive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let source = path.as_ref().to_path_buf();
        let mut file = File::open(&source)?;
        let mut header = [0u8; 16];
        file.read_exact(&mut header)
            .map_err(|_| FormatError::Truncated("BIG header"))?;
        let magic: [u8; 4] = header[0..4].try_into().unwrap();
        if &magic != b"BIGF" && &magic != b"BIG4" {
            return Err(FormatError::BadMagic { expected: "BIGF/BIG4", found: magic });
        }
        let count = u32::from_be_bytes(header[8..12].try_into().unwrap()) as usize;
        let data_start = u32::from_be_bytes(header[12..16].try_into().unwrap()) as usize;
        if data_start < 16 {
            return Err(FormatError::Corrupt(format!("header size {data_start} < 16")));
        }
        let mut table = vec![0u8; data_start - 16];
        file.read_exact(&mut table)
            .map_err(|_| FormatError::Truncated("BIG file table"))?;
        let entries = parse_table(&table, count)?;
        Ok(Self { source, entries })
    }

    pub fn entries(&self) -> &[BigEntry] {
        &self.entries
    }

    pub fn find(&self, path: &str) -> Option<&BigEntry> {
        let wanted = normalize(path);
        self.entries.iter().find(|e| normalize(&e.path) == wanted)
    }

    pub fn read(&self, entry: &BigEntry) -> Result<Vec<u8>> {
        let mut file = File::open(&self.source)?;
        file.seek(SeekFrom::Start(entry.offset as u64))?;
        let mut buf = vec![0u8; entry.size as usize];
        file.read_exact(&mut buf)
            .map_err(|_| FormatError::Truncated("BIG member data"))?;
        Ok(buf)
    }
}

fn parse_table(table: &[u8], count: usize) -> Result<Vec<BigEntry>> {
    let mut entries = Vec::with_capacity(count);
    let mut pos = 0usize;
    for _ in 0..count {
        if pos + 8 > table.len() {
            return Err(FormatError::Truncated("BIG entry"));
        }
        let offset = u32::from_be_bytes(table[pos..pos + 4].try_into().unwrap());
        let size = u32::from_be_bytes(table[pos + 4..pos + 8].try_into().unwrap());
        pos += 8;
        let name_len = table[pos..]
            .iter()
            .position(|&b| b == 0)
            .ok_or(FormatError::Truncated("BIG entry name"))?;
        let path = String::from_utf8_lossy(&table[pos..pos + name_len]).into_owned();
        pos += name_len + 1;
        entries.push(BigEntry { path, offset, size });
    }
    Ok(entries)
}

/// Engine-style path key: lower case, backslashes.
pub fn normalize(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase()
}

/// Build a BIGF archive from (archive path, contents) pairs. Paths are stored with backslashes.
pub fn write_archive(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let names: Vec<String> = files.iter().map(|(n, _)| n.replace('/', "\\")).collect();
    let table_len: usize = names.iter().map(|n| 8 + n.len() + 1).sum();
    let data_start = 16 + table_len;
    let total = data_start + files.iter().map(|(_, d)| d.len()).sum::<usize>();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"BIGF");
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(files.len() as u32).to_be_bytes());
    out.extend_from_slice(&(data_start as u32).to_be_bytes());
    let mut offset = data_start;
    for (name, (_, data)) in names.iter().zip(files) {
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(name.as_bytes());
        out.push(0);
        offset += data.len();
    }
    for (_, data) in files {
        out.extend_from_slice(data);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(files: &[(&str, &[u8])]) -> Vec<u8> {
        let owned: Vec<(String, Vec<u8>)> = files.iter().map(|(n, d)| (n.to_string(), d.to_vec())).collect();
        write_archive(&owned)
    }

    #[test]
    fn round_trips_a_synthetic_archive() {
        let bytes = build(&[("Data\\INI\\A.ini", b"Object A\nEnd\n"), ("data\\b.txt", b"hi")]);
        let dir = std::env::temp_dir().join(format!("zhbo-big-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.big");
        std::fs::write(&path, bytes).unwrap();

        let big = BigArchive::open(&path).unwrap();
        assert_eq!(big.entries().len(), 2);
        let a = big.find("data/ini/a.INI").expect("case-insensitive lookup");
        assert_eq!(big.read(a).unwrap(), b"Object A\nEnd\n");
        let b = big.find("Data\\B.txt").unwrap();
        assert_eq!(big.read(b).unwrap(), b"hi");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_bad_magic() {
        let dir = std::env::temp_dir().join(format!("zhbo-big-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.big");
        std::fs::write(&path, b"NOPE0000000000000000").unwrap();
        assert!(matches!(BigArchive::open(&path), Err(FormatError::BadMagic { .. })));
        std::fs::remove_dir_all(&dir).ok();
    }
}
