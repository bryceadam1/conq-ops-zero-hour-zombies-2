//! Black Ops II `.ipak` image packs (`zone\all\*.ipak`).
//!
//! Verified against the Steam install:
//! ```text
//! 0x00 "KAPI", u32 version (0x50000), u32 file size, u32 section count
//! 0x10 sections { u32 type (1 = index, 2 = data), u32 offset, u32 size, u32 item count }
//! index entry (16 bytes): u32 name hash, u32 data hash, u32 offset (into data section), u32 size
//! data: blocks of a 0x80-byte header { u32 count<<24 | offset, u32 command[31] = size | flag<<24 }
//!       followed by the command payloads; flag 1 = LZO1X, 0 = stored.
//! ```
//! Every entry decompresses to a complete `IWi` image (see [`crate::iwi`]). Packs are compressed, not
//! encrypted. Names are hashes; the name strings live in the (encrypted) fastfiles and are not used.

use crate::{lzo, FormatError, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const MAGIC: &[u8; 4] = b"KAPI";
const BLOCK_HEADER: u64 = 0x80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IpakEntry {
    pub name_hash: u32,
    pub data_hash: u32,
    pub offset: u32,
    pub size: u32,
}

#[derive(Debug)]
pub struct Ipak {
    source: PathBuf,
    data_offset: u64,
    entries: Vec<IpakEntry>,
}

impl Ipak {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let source = path.as_ref().to_path_buf();
        let mut f = File::open(&source)?;
        let mut head = [0u8; 16];
        f.read_exact(&mut head).map_err(|_| FormatError::Truncated("ipak header"))?;
        if &head[0..4] != MAGIC {
            return Err(FormatError::BadMagic { expected: "KAPI", found: head[0..4].try_into().unwrap() });
        }
        let count = u32::from_le_bytes(head[12..16].try_into().unwrap()) as usize;
        let mut secs = vec![0u8; 16 * count];
        f.read_exact(&mut secs).map_err(|_| FormatError::Truncated("ipak sections"))?;
        let section = |kind: u32| {
            secs.chunks_exact(16).map(|s| {
                let v = |i: usize| u32::from_le_bytes(s[i * 4..i * 4 + 4].try_into().unwrap());
                (v(0), v(1), v(2), v(3))
            })
            .find(|s| s.0 == kind)
        };
        let (Some(index), Some(data)) = (section(1), section(2)) else {
            // Some small packs (e.g. empty DLC stubs) have no index.
            return Ok(Self { source, data_offset: 0, entries: Vec::new() });
        };
        f.seek(SeekFrom::Start(index.1 as u64))?;
        let mut raw = vec![0u8; index.3 as usize * 16];
        f.read_exact(&mut raw).map_err(|_| FormatError::Truncated("ipak index"))?;
        let entries = raw
            .chunks_exact(16)
            .map(|e| {
                let v = |i: usize| u32::from_le_bytes(e[i * 4..i * 4 + 4].try_into().unwrap());
                IpakEntry { name_hash: v(0), data_hash: v(1), offset: v(2), size: v(3) }
            })
            .collect();
        Ok(Self { source, data_offset: data.1 as u64, entries })
    }

    pub fn entries(&self) -> &[IpakEntry] {
        &self.entries
    }

    /// Decompress an entry. `limit` stops early once that many bytes are produced (e.g. to read
    /// just an image header).
    pub fn read(&self, entry: &IpakEntry, limit: Option<usize>) -> Result<Vec<u8>> {
        let mut f = File::open(&self.source)?;
        let mut pos = self.data_offset + entry.offset as u64;
        let end = pos + entry.size as u64;
        let mut out = Vec::new();
        while pos < end {
            f.seek(SeekFrom::Start(pos))?;
            let mut head = [0u8; BLOCK_HEADER as usize];
            f.read_exact(&mut head).map_err(|_| FormatError::Truncated("ipak block header"))?;
            let count = (u32::from_le_bytes(head[0..4].try_into().unwrap()) >> 24) as usize;
            if count == 0 || count > 31 {
                break;
            }
            let mut payload_len = 0u64;
            for i in 0..count {
                let cmd = u32::from_le_bytes(head[4 + i * 4..8 + i * 4].try_into().unwrap());
                let size = (cmd & 0x00ff_ffff) as usize;
                let flag = cmd >> 24;
                let mut chunk = vec![0u8; size];
                f.read_exact(&mut chunk).map_err(|_| FormatError::Truncated("ipak block data"))?;
                payload_len += size as u64;
                match flag {
                    1 => {
                        let (data, _) = lzo::decompress(&chunk, 0x8000)?;
                        out.extend_from_slice(&data);
                    }
                    0 => out.extend_from_slice(&chunk),
                    _ => {} // padding / skip commands
                }
                if limit.is_some_and(|l| out.len() >= l) {
                    return Ok(out);
                }
            }
            // Blocks are 128-byte aligned.
            pos += (BLOCK_HEADER + payload_len).div_ceil(BLOCK_HEADER) * BLOCK_HEADER;
        }
        Ok(out)
    }
}
