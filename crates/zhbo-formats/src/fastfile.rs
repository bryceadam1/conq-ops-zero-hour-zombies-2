//! Black Ops II (T6, PC) fastfiles: `zone/all/*.ff`, read from the player's own install.
//!
//! A fastfile holds a zone: a stream of game assets (models, animations, materials, ...). The
//! container is
//!
//! * header: magic `TAff0100`, version 147 (u32);
//! * auth header: magic `PHEEBs71`, flags (u32, zero), zone name (32 bytes), RSA signature (256);
//! * XChunks: `u32 size` + `size` bytes, dealt round-robin to 4 streams. Each chunk is
//!   Salsa20-decrypted, then raw-deflated. A zero size ends the file. Chunk size fields never
//!   straddle a 512 KiB boundary of the file (the game's read buffer); the gap before one is padding.
//!
//! Each stream's Salsa20 IV is the first 8 bytes of its current entry in a ring of 200 SHA-1-sized
//! blocks per stream. The ring starts as the zone name repeated (4 bytes per character); after each
//! chunk the SHA-1 of its decrypted bytes is XORed into the stream's next entry.
//!
//! The decompressed zone starts with its size, external size and 8 block sizes, then the
//! `XAssetList` (script strings, dependencies, and `assetCount` (type, pointer) pairs), then the
//! data of every asset in order.
//!
//! Format knowledge from OpenAssetTools (github.com/Laupetin/OpenAssetTools, GPL-3.0).

use crate::FormatError;
use salsa20::cipher::{KeyIvInit, StreamCipher};
use sha1::{Digest, Sha1};

pub const MAGIC: &[u8; 8] = b"TAff0100";
pub const VERSION_PC: u32 = 147;
const AUTH_MAGIC: &[u8; 8] = b"PHEEBs71";
const STREAM_COUNT: usize = 4;
const CHUNK_MAX: usize = 0x8000;
const READ_BUFFER: usize = 0x80000;
const HASH_SIZE: usize = 20;
const HASH_RING: usize = 200;
/// The Salsa20 key for PC zones (public; the same one OpenAssetTools uses).
const KEY_PC: [u8; 32] = [
    0x64, 0x1D, 0x8A, 0x2F, 0xE3, 0x1D, 0x3A, 0xA6, 0x36, 0x22, 0xBB, 0xC9, 0xCE, 0x85, 0x87, 0x22, //
    0x9D, 0x42, 0xB0, 0xF8, 0xED, 0x9B, 0x92, 0x41, 0x30, 0xBF, 0x88, 0xB6, 0x5E, 0xDC, 0x50, 0xBE,
];
/// Pointer value meaning "the data follows in the stream".
pub const PTR_FOLLOWING: u32 = 0xFFFF_FFFF;
/// Pointer value meaning "the data follows, and later pointers alias it" (an insert).
pub const PTR_INSERT: u32 = 0xFFFF_FFFE;
pub const BLOCK_COUNT: usize = 8;

/// T6 asset types, in the game's numbering.
pub const ASSET_TYPE_NAMES: &[&str] = &[
    "xmodelpieces", "physpreset", "physconstraints", "destructibledef", "xanim", "xmodel", "material",
    "techniqueset", "image", "sound", "soundpatch", "clipmap", "clipmap_pvs", "comworld", "gameworld_sp",
    "gameworld_mp", "mapents", "gfxworld", "lightdef", "uimap", "font", "fonticon", "menulist", "menu",
    "localize", "weapon", "weapondef", "weaponvariant", "weaponfull", "attachment", "attachmentunique",
    "weaponcamo", "snddriverglobals", "fx", "impactfx", "aitype", "mptype", "mpbody", "mphead", "character",
    "xmodelalias", "rawfile", "stringtable", "leaderboard", "xglobals", "ddl", "glasses", "emblemset",
    "scriptparsetree", "keyvaluepairs", "vehicledef", "memoryblock", "addonmapents", "tracer", "skinnedverts",
    "qdb", "slug", "footsteptable", "footstepfxtable", "zbarrier",
];
pub const ASSET_XANIM: u32 = 4;
pub const ASSET_XMODEL: u32 = 5;

pub fn asset_type_name(t: u32) -> &'static str {
    ASSET_TYPE_NAMES.get(t as usize).copied().unwrap_or("unknown")
}

/// A decrypted, decompressed zone.
pub struct Zone {
    /// Name from the auth header, e.g. `common_mp`.
    pub name: String,
    /// The zone data: everything after the container, starting with the sizes.
    pub data: Vec<u8>,
}

fn u32_at(d: &[u8], at: usize, what: &'static str) -> Result<u32, FormatError> {
    d.get(at..at + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .ok_or(FormatError::Truncated(what))
}

/// Decrypt and decompress a fastfile.
pub fn read(file: &[u8]) -> Result<Zone, FormatError> {
    if file.len() < 312 || &file[0..8] != MAGIC {
        let found = file.get(0..4).map(|b| b.try_into().unwrap()).unwrap_or([0; 4]);
        return Err(FormatError::BadMagic { expected: "TAff0100", found });
    }
    let version = u32_at(file, 8, "version")?;
    if version != VERSION_PC {
        return Err(FormatError::Corrupt(format!("fastfile version {version}, expected {VERSION_PC} (PC)")));
    }
    if &file[12..20] != AUTH_MAGIC {
        return Err(FormatError::Corrupt("missing PHEEBs71 auth header".into()));
    }
    let name_bytes = &file[24..56];
    let name_len = name_bytes.iter().position(|&b| b == 0).unwrap_or(32);
    let name = String::from_utf8_lossy(&name_bytes[..name_len]).into_owned();
    if name.is_empty() {
        return Err(FormatError::Corrupt("empty zone name".into()));
    }
    let mut pos = 312; // after the 256-byte signature

    // IV ring: the zone name (at most 31 characters) repeated, each character filling 4 bytes.
    let ring_len = HASH_RING * STREAM_COUNT * HASH_SIZE;
    let key_name = &name.as_bytes()[..name.len().min(31)];
    let mut ring = vec![0u8; ring_len];
    for (i, word) in ring.chunks_exact_mut(4).enumerate() {
        word.fill(key_name[i % key_name.len()]);
    }
    let mut ring_index = [0usize; STREAM_COUNT];
    let hash_block = |index: usize, stream: usize| (index * STREAM_COUNT + stream) * HASH_SIZE;

    let mut data = Vec::new();
    let mut buffer_offset = pos % READ_BUFFER;
    let mut stream = 0usize;
    let mut chunk = vec![0u8; CHUNK_MAX];
    loop {
        if buffer_offset + 4 > READ_BUFFER {
            pos += READ_BUFFER - buffer_offset;
            buffer_offset = 0;
        }
        let Some(size_bytes) = file.get(pos..pos + 4) else { break };
        let size = u32::from_le_bytes(size_bytes.try_into().unwrap()) as usize;
        pos += 4;
        buffer_offset = (buffer_offset + 4) % READ_BUFFER;
        if size == 0 {
            break;
        }
        if size > CHUNK_MAX {
            return Err(FormatError::Corrupt(format!("chunk of {size} bytes at {:#x}", pos - 4)));
        }
        let src = file.get(pos..pos + size).ok_or(FormatError::Truncated("chunk"))?;
        pos += size;
        buffer_offset = (buffer_offset + size) % READ_BUFFER;

        let block = hash_block(ring_index[stream], stream);
        let iv: [u8; 8] = ring[block..block + 8].try_into().unwrap();
        chunk[..size].copy_from_slice(src);
        salsa20::Salsa20::new(&KEY_PC.into(), &iv.into()).apply_keystream(&mut chunk[..size]);
        let digest = Sha1::digest(&chunk[..size]);
        ring_index[stream] = (ring_index[stream] + 1) % HASH_RING;
        let next = hash_block(ring_index[stream], stream);
        for (r, h) in ring[next..next + HASH_SIZE].iter_mut().zip(digest.iter()) {
            *r ^= h;
        }

        let out = miniz_oxide::inflate::decompress_to_vec(&chunk[..size])
            .map_err(|e| FormatError::Corrupt(format!("inflate failed in stream {stream}: {e:?}")))?;
        data.extend_from_slice(&out);
        stream = (stream + 1) % STREAM_COUNT;
    }
    Ok(Zone { name, data })
}

/// The zone's header and asset list (no asset data).
pub struct ZoneIndex {
    pub size: u32,
    pub external_size: u32,
    pub block_sizes: [u32; BLOCK_COUNT],
    pub script_strings: Vec<String>,
    pub depends: Vec<String>,
    /// `(type, pointer)` per asset; the pointer is `PTR_FOLLOWING` for assets whose data follows.
    pub assets: Vec<(u32, u32)>,
    /// Offset in `Zone::data` where the first asset's data starts.
    pub assets_data_offset: usize,
}

/// Reads NUL-terminated strings from a `const char*` array that follows in the stream.
fn read_string_array(d: &[u8], pos: &mut usize, count: usize) -> Result<Vec<String>, FormatError> {
    let mut ptrs = Vec::with_capacity(count);
    for _ in 0..count {
        ptrs.push(u32_at(d, *pos, "string pointer")?);
        *pos += 4;
    }
    let mut out = Vec::with_capacity(count);
    for p in ptrs {
        if p == PTR_FOLLOWING {
            let rest = d.get(*pos..).ok_or(FormatError::Truncated("string"))?;
            let len = rest.iter().position(|&b| b == 0).ok_or(FormatError::Truncated("string"))?;
            out.push(String::from_utf8_lossy(&rest[..len]).into_owned());
            *pos += len + 1;
        } else {
            out.push(String::new()); // null, or a reference to an earlier string
        }
    }
    Ok(out)
}

impl Zone {
    pub fn index(&self) -> Result<ZoneIndex, FormatError> {
        let d = &self.data;
        let size = u32_at(d, 0, "zone size")?;
        let external_size = u32_at(d, 4, "zone size")?;
        let mut block_sizes = [0u32; BLOCK_COUNT];
        for (i, b) in block_sizes.iter_mut().enumerate() {
            *b = u32_at(d, 8 + 4 * i, "block sizes")?;
        }
        let mut pos = 8 + 4 * BLOCK_COUNT;
        let string_count = u32_at(d, pos, "asset list")? as usize;
        let strings_ptr = u32_at(d, pos + 4, "asset list")?;
        let depend_count = u32_at(d, pos + 8, "asset list")? as usize;
        let depends_ptr = u32_at(d, pos + 12, "asset list")?;
        let asset_count = u32_at(d, pos + 16, "asset list")? as usize;
        let assets_ptr = u32_at(d, pos + 20, "asset list")?;
        pos += 24;
        if string_count > 1 << 20 || depend_count > 1 << 20 || asset_count > 1 << 20 {
            return Err(FormatError::Corrupt("implausible asset list counts".into()));
        }
        let script_strings =
            if strings_ptr != 0 { read_string_array(d, &mut pos, string_count)? } else { Vec::new() };
        let depends = if depends_ptr != 0 { read_string_array(d, &mut pos, depend_count)? } else { Vec::new() };
        let mut assets = Vec::with_capacity(asset_count);
        if assets_ptr != 0 {
            for _ in 0..asset_count {
                assets.push((u32_at(d, pos, "asset")?, u32_at(d, pos + 4, "asset")?));
                pos += 8;
            }
        }
        Ok(ZoneIndex { size, external_size, block_sizes, script_strings, depends, assets, assets_data_offset: pos })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_other_files() {
        assert!(read(b"not a fastfile").is_err());
        let mut f = vec![0u8; 400];
        f[0..8].copy_from_slice(b"TAff0100");
        f[8..12].copy_from_slice(&146u32.to_le_bytes());
        assert!(read(&f).is_err(), "console version must be refused");
    }

    /// Needs the player's BO2 install: `ZHBO_BO2_DIR=... cargo test -p zhbo-formats -- --ignored`.
    #[test]
    #[ignore]
    fn reads_a_real_zone() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR");
        let file = std::fs::read(std::path::Path::new(&dir).join("zone/all/patch.ff")).unwrap();
        let zone = read(&file).unwrap();
        assert_eq!(zone.name, "patch");
        let index = zone.index().unwrap();
        // The declared size counts what follows the sizes and the block-size table.
        assert_eq!(index.size as usize, zone.data.len() - 8 - 4 * BLOCK_COUNT, "declared size matches what was decompressed");
        assert!(!index.assets.is_empty());
        assert!(index.assets.iter().all(|&(t, _)| (t as usize) < ASSET_TYPE_NAMES.len()));
    }
}
