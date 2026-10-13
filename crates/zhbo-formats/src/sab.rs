//! Black Ops 2 (T6) PC sound banks: `.sabs` (streamed) and `.sabl` (loaded).
//!
//! Verified against the Steam install (`sound/mpl_common.all.sabs`):
//! ```text
//! 0x00 u32  magic 0x23585532 ("2UX#")
//! 0x04 u32  version (14)
//! 0x08 u32  size of an audio entry (20)
//! 0x0C u32  size of a hash entry (16)
//! 0x10 u32  size of a string entry (64)
//! 0x14 u32  entry count
//! 0x20 u64  file size
//! 0x28 u64  entry table offset
//! 0x30 u64  hash table offset
//! 0x48 [u8;64] bank name, e.g. "mpl_common.all.sabs"
//!
//! entry (20 bytes): u32 id, u32 size, u32 offset, u32 frame_count,
//!                   u8 frame_rate_index, u8 channels, u8 looping, u8 format
//! ```
//! Payloads with `format == 8` are complete FLAC streams (they start with `fLaC`).
//! Entries are keyed by a name hash only; resolving names needs the sound alias tables
//! from the fastfiles (see docs/BO2_ASSETS.md).

use crate::{FormatError, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const MAGIC: u32 = 0x2358_5532;
pub const FORMAT_FLAC: u8 = 8;

/// T6 frame-rate table indexed by `frame_rate_index`.
pub const FRAME_RATES: [u32; 9] = [8000, 12000, 16000, 24000, 32000, 44100, 48000, 96000, 192000];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SabEntry {
    pub id: u32,
    pub size: u32,
    pub offset: u32,
    pub frame_count: u32,
    pub frame_rate_index: u8,
    pub channels: u8,
    pub looping: bool,
    pub format: u8,
}

impl SabEntry {
    pub fn sample_rate(&self) -> Option<u32> {
        FRAME_RATES.get(self.frame_rate_index as usize).copied()
    }

    pub fn duration_secs(&self) -> Option<f32> {
        self.sample_rate().map(|r| self.frame_count as f32 / r as f32)
    }
}

#[derive(Debug)]
pub struct SoundBank {
    source: PathBuf,
    pub name: String,
    pub version: u32,
    entries: Vec<SabEntry>,
}

impl SoundBank {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let source = path.as_ref().to_path_buf();
        let mut file = File::open(&source)?;
        let mut header = [0u8; 0x88];
        file.read_exact(&mut header)
            .map_err(|_| FormatError::Truncated("sab header"))?;
        let u32_at = |o: usize| u32::from_le_bytes(header[o..o + 4].try_into().unwrap());
        let u64_at = |o: usize| u64::from_le_bytes(header[o..o + 8].try_into().unwrap());
        if u32_at(0) != MAGIC {
            return Err(FormatError::BadMagic {
                expected: "2UX#",
                found: header[0..4].try_into().unwrap(),
            });
        }
        let version = u32_at(4);
        let entry_size = u32_at(8) as usize;
        let count = u32_at(0x14) as usize;
        let entry_offset = u64_at(0x28);
        if entry_size < 20 {
            return Err(FormatError::Corrupt(format!("entry size {entry_size} < 20")));
        }
        let name_raw = &header[0x48..0x88];
        let name_len = name_raw.iter().position(|&b| b == 0).unwrap_or(name_raw.len());
        let name = String::from_utf8_lossy(&name_raw[..name_len]).into_owned();

        file.seek(SeekFrom::Start(entry_offset))?;
        let mut table = vec![0u8; entry_size * count];
        file.read_exact(&mut table)
            .map_err(|_| FormatError::Truncated("sab entry table"))?;
        let entries = table
            .chunks_exact(entry_size)
            .map(|e| SabEntry {
                id: u32::from_le_bytes(e[0..4].try_into().unwrap()),
                size: u32::from_le_bytes(e[4..8].try_into().unwrap()),
                offset: u32::from_le_bytes(e[8..12].try_into().unwrap()),
                frame_count: u32::from_le_bytes(e[12..16].try_into().unwrap()),
                frame_rate_index: e[16],
                channels: e[17],
                looping: e[18] != 0,
                format: e[19],
            })
            .collect();
        Ok(Self { source, name, version, entries })
    }

    pub fn entries(&self) -> &[SabEntry] {
        &self.entries
    }

    pub fn find(&self, id: u32) -> Option<&SabEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Raw payload bytes (a FLAC stream when `entry.format == FORMAT_FLAC`).
    pub fn read(&self, entry: &SabEntry) -> Result<Vec<u8>> {
        let mut file = File::open(&self.source)?;
        file.seek(SeekFrom::Start(entry.offset as u64))?;
        let mut buf = vec![0u8; entry.size as usize];
        file.read_exact(&mut buf)
            .map_err(|_| FormatError::Truncated("sab payload"))?;
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_synthetic_bank() {
        let payload = b"fLaC\0\0\0\x22rest-of-stream";
        let entry_offset = 0x800u64 + payload.len() as u64;
        let mut bytes = vec![0u8; 0x800];
        bytes[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        bytes[4..8].copy_from_slice(&14u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&20u32.to_le_bytes());
        bytes[0x14..0x18].copy_from_slice(&1u32.to_le_bytes());
        bytes[0x28..0x30].copy_from_slice(&entry_offset.to_le_bytes());
        bytes[0x48..0x48 + 9].copy_from_slice(b"test.sabs");
        bytes.extend_from_slice(payload);
        let mut entry = Vec::new();
        entry.extend_from_slice(&0xdead_beefu32.to_le_bytes());
        entry.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        entry.extend_from_slice(&0x800u32.to_le_bytes());
        entry.extend_from_slice(&48000u32.to_le_bytes());
        entry.extend_from_slice(&[6, 2, 0, FORMAT_FLAC]);
        bytes.extend_from_slice(&entry);

        let dir = std::env::temp_dir().join(format!("zhbo-sab-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.sabs");
        std::fs::write(&path, bytes).unwrap();

        let bank = SoundBank::open(&path).unwrap();
        assert_eq!(bank.name, "test.sabs");
        assert_eq!(bank.entries().len(), 1);
        let e = bank.find(0xdead_beef).unwrap();
        assert_eq!(e.sample_rate(), Some(48000));
        assert_eq!(e.duration_secs(), Some(1.0));
        assert_eq!(&bank.read(e).unwrap()[0..4], b"fLaC");
        std::fs::remove_dir_all(&dir).ok();
    }
}
