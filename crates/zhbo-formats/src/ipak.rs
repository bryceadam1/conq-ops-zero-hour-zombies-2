//! Black Ops II `.ipak` image packs (`zone\all\*.ipak`).
//!
//! Verified against the Steam install:
//! ```text
//! 0x00 "KAPI", u32 version (0x50000), u32 file size, u32 section count
//! 0x10 sections { u32 type (1 = index, 2 = data), u32 offset, u32 size, u32 item count }
//! index entry (16 bytes): u32 data hash, u32 name hash, u32 offset (into data section), u32 size
//! data: blocks of a 0x80-byte header { u32 count<<24 | offset, u32 command[31] = size | flag<<24 }
//!       followed by the command payloads; flag 1 = LZO1X, 0 = stored.
//! ```
//! Every entry decompresses to a complete `IWi` image (see [`crate::iwi`]). Packs are compressed, not
//! encrypted.
//!
//! An entry is keyed by the pair a `GfxImage` carries ([`ImageKey`], OpenAssetTools'
//! `IPakIndexEntryKey`): the data hash is `GfxImage::streamedParts[0].hash` (+40) and the name hash is
//! `GfxImage::hash` (+76), [`r_hash_string`] of the bare image name (verified on 17,152 zone-source
//! image names: every one that hits an index entry hits its second field, none its first). A PC
//! `GfxImage` has a single streamed part, so one image is exactly one entry. The data hash alone is
//! not unique: base.ipak has 157 values shared by different images (different names, often
//! different sizes), and an image's revisions in different packs share the name hash with
//! different data hashes (`scope_overlay_xpr50_1024`: base.ipak `03e75800`, patch_mp.ipak
//! `1c99c1b0`) — only the pair names one image.

use crate::{lzo, FormatError, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const MAGIC: &[u8; 4] = b"KAPI";
const BLOCK_HEADER: u64 = 0x80;

/// What an image's pack entry is looked up by: `GfxImage::streamedParts[0].hash` (the data hash, the
/// index entry's first field) and `GfxImage::hash` (the name hash, its second).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageKey {
    pub data_hash: u32,
    pub name_hash: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IpakEntry {
    pub data_hash: u32,
    pub name_hash: u32,
    pub offset: u32,
    pub size: u32,
}

impl IpakEntry {
    pub fn key(&self) -> ImageKey {
        ImageKey { data_hash: self.data_hash, name_hash: self.name_hash }
    }
}

/// BO2's `R_HashString`: the name hash of an image (`GfxImage::hash`, an ipak entry's second field),
/// djb2-xor over the bare name with ASCII letters folded to lower case (`c | 0x20`), seed 0.
pub const fn r_hash_string(name: &str) -> u32 {
    let b = name.as_bytes();
    let mut h = 0u32;
    let mut i = 0;
    while i < b.len() {
        h = h.wrapping_mul(33) ^ (b[i] | 0x20) as u32;
        i += 1;
    }
    h
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
        let file_len = f.metadata()?.len();
        let mut head = [0u8; 16];
        f.read_exact(&mut head).map_err(|_| FormatError::Truncated("ipak header"))?;
        if &head[0..4] != MAGIC {
            return Err(FormatError::BadMagic { expected: "KAPI", found: head[0..4].try_into().unwrap() });
        }
        let count = u32::from_le_bytes(head[12..16].try_into().unwrap()) as usize;
        // The section count is file-controlled; keep its table within the file.
        if count.checked_mul(16).map_or(true, |need| need as u64 > file_len.saturating_sub(16)) {
            return Err(FormatError::Corrupt(format!("ipak section count {count} does not fit in the file")));
        }
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
        let index_len = (index.3 as u64).checked_mul(16).ok_or_else(|| FormatError::Corrupt("ipak index too long".into()))?;
        if index.1 as u64 > file_len || index_len > file_len - index.1 as u64 {
            return Err(FormatError::Corrupt(format!(
                "ipak index ({}, {} bytes) is past the end of the {}-byte file",
                index.1, index_len, file_len
            )));
        }
        f.seek(SeekFrom::Start(index.1 as u64))?;
        let mut raw = vec![0u8; index_len as usize];
        f.read_exact(&mut raw).map_err(|_| FormatError::Truncated("ipak index"))?;
        let entries = raw
            .chunks_exact(16)
            .map(|e| {
                let v = |i: usize| u32::from_le_bytes(e[i * 4..i * 4 + 4].try_into().unwrap());
                IpakEntry { data_hash: v(0), name_hash: v(1), offset: v(2), size: v(3) }
            })
            .collect::<Vec<_>>();
        let mut entries = entries;
        entries.sort_by_key(IpakEntry::key);
        Ok(Self { source, data_offset: data.1 as u64, entries })
    }

    /// The index, sorted by [`ImageKey`].
    pub fn entries(&self) -> &[IpakEntry] {
        &self.entries
    }

    /// The entry of the image with this key.
    pub fn entry(&self, key: ImageKey) -> Option<&IpakEntry> {
        self.entries.binary_search_by_key(&key, IpakEntry::key).ok().map(|i| &self.entries[i])
    }

    /// Decompress an entry. `limit` stops early once that many bytes are produced (e.g. to read
    /// just an image header).
    pub fn read(&self, entry: &IpakEntry, limit: Option<usize>) -> Result<Vec<u8>> {
        let mut f = File::open(&self.source)?;
        let mut pos = self.data_offset + entry.offset as u64;
        let end = pos + entry.size as u64;
        let mut out = Vec::new();
        while pos < end {
            if end - pos < BLOCK_HEADER {
                return Err(FormatError::Corrupt(format!(
                    "ipak entry {:#x}: {} bytes left, less than a block header",
                    entry.data_hash,
                    end - pos
                )));
            }
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
                // A command must stay inside this entry: a corrupt size would otherwise read into
                // the next entry's data (and its 24-bit size is also the allocation cap here).
                let remaining = end - pos - BLOCK_HEADER - payload_len;
                if size as u64 > remaining {
                    return Err(FormatError::Corrupt(format!(
                        "ipak entry {:#x}: command {i} reads {} bytes, {} past the entry",
                        entry.data_hash,
                        size,
                        size as u64 - remaining
                    )));
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r_hash_string_matches_the_packs_name_field() {
        // (image name, the second field of its ipak entries) from the install (RESEARCH item 17).
        for (name, hash) in [
            ("scope_overlay_xpr50_1024", 0x9c47_55f7),
            ("scope_overlay_ballista_1024", 0xd33f_a264),
            ("scope_overlay_dsr50_1024", 0x7f2a_d828),
            ("scope_overlay_svu_1024", 0xc856_97b8),
            ("scope_overlay_envg_1024", 0x453d_04f2),
            ("scope_overlay_specter_1024", 0x13fa_896e),
            ("scope_overlay_vzoom_1024", 0x613f_e1e9),
        ] {
            assert_eq!(r_hash_string(name), hash, "{name}");
        }
        // Case-folded, like the engine's lookup.
        assert_eq!(r_hash_string("Scope_Overlay_XPR50_1024"), 0x9c47_55f7);
        assert_eq!(r_hash_string(""), 0);
    }

    /// A pack with `entries` (data hash, name hash, stored payload), one stored block each.
    fn synthetic(entries: &[(u32, u32, &[u8])]) -> Vec<u8> {
        let mut data = Vec::new();
        let mut index = Vec::new();
        for (dh, nh, payload) in entries {
            let start = data.len() as u32;
            let mut block = vec![0u8; BLOCK_HEADER as usize];
            block[0..4].copy_from_slice(&(1u32 << 24).to_le_bytes());
            block[4..8].copy_from_slice(&(payload.len() as u32).to_le_bytes()); // flag 0 = stored
            block.extend_from_slice(payload);
            block.resize(block.len().div_ceil(BLOCK_HEADER as usize) * BLOCK_HEADER as usize, 0);
            data.extend_from_slice(&block);
            for v in [*dh, *nh, start, block.len() as u32] {
                index.extend_from_slice(&v.to_le_bytes());
            }
        }
        let index_at = 16 + 32;
        let data_at = index_at + index.len() as u32;
        let mut f = Vec::new();
        f.extend_from_slice(MAGIC);
        for v in [0x50000, 0, 2] {
            f.extend_from_slice(&u32::to_le_bytes(v));
        }
        for v in [1, index_at, index.len() as u32, entries.len() as u32, 2, data_at, data.len() as u32, entries.len() as u32] {
            f.extend_from_slice(&u32::to_le_bytes(v));
        }
        f.extend_from_slice(&index);
        f.extend_from_slice(&data);
        let len = f.len() as u32;
        f[8..12].copy_from_slice(&len.to_le_bytes());
        f
    }

    #[test]
    fn entries_are_data_hash_then_name_hash_and_looked_up_by_the_pair() {
        // Two different images sharing a data hash (as 157 do in base.ipak), and a revision of
        // the first under another data hash.
        let a = r_hash_string("image_a");
        let b = r_hash_string("image_b");
        let bytes = synthetic(&[(0x1111, b, b"bbbb"), (0x1111, a, b"aaaa"), (0x2222, a, b"a2a2")]);
        let path = std::env::temp_dir().join(format!("zhbo-ipak-test-{}.ipak", std::process::id()));
        std::fs::write(&path, &bytes).unwrap();
        let pak = Ipak::open(&path).unwrap();
        let first = pak.entries().iter().find(|e| e.offset == 0).unwrap();
        assert_eq!((first.data_hash, first.name_hash), (0x1111, b), "the first field is the data hash");
        let read = |dh, nh| pak.entry(ImageKey { data_hash: dh, name_hash: nh }).map(|e| pak.read(e, None).unwrap());
        assert_eq!(read(0x1111, a).as_deref(), Some(&b"aaaa"[..]));
        assert_eq!(read(0x1111, b).as_deref(), Some(&b"bbbb"[..]));
        assert_eq!(read(0x2222, a).as_deref(), Some(&b"a2a2"[..]));
        assert_eq!(read(0x2222, b), None);
        assert!(pak.entries().windows(2).all(|w| w[0].key() < w[1].key()));
        std::fs::remove_file(&path).ok();
    }
}
