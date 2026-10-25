//! A complete Black Ops II (T6, PC) zone loader: walks every asset of a decrypted zone the way the
//! game does and resolves every pointer, including references to data that earlier assets loaded.
//!
//! `generated/` holds one loader per asset type, emitted by OpenAssetTools' ZoneCodeGenerator from
//! its T6 struct definitions and zone-code commands, through a Rust template written for Conquer
//! Ops (a port of OAT's `ZoneLoadTemplate`; OAT is GPL-3.0). This file is the runtime they call: the
//! equivalent of OAT's `ZoneInputStream` and `ContentLoaderT6`.
//!
//! Memory is one simulated 32-bit address space: the eight zone blocks at their declared sizes,
//! then a copy of each asset's root struct (the game copies assets out of the temp block). Pointer
//! fields hold addresses in this space, so a reader follows them with `Zone::u32` and friends.
//!
//! The load checks itself the way OAT does: each read must land exactly at its block's current
//! offset, every reference must point inside its block, and the stream must end where the zone does.

#[allow(clippy::all)]
pub mod generated;
pub mod layouts;

use crate::fastfile;
use crate::FormatError;
use std::fmt;

pub const FOLLOWING: u32 = 0xFFFF_FFFF;
pub const INSERT: u32 = 0xFFFF_FFFE;

const BLOCK_COUNT: usize = 8;
const BLOCK_SHIFT: u32 = 29;
const OFFSET_MASK: u32 = (1 << BLOCK_SHIFT) - 1;
const INSERT_BLOCK: usize = 5; // XFILE_BLOCK_VIRTUAL
const MAX_STRING_COUNT: usize = 1 << 20;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BlockKind {
    Temp,
    Runtime,
    Delay,
    Normal,
}

/// T6 blocks: TEMP, RUNTIME_VIRTUAL, RUNTIME_PHYSICAL, DELAY_VIRTUAL, DELAY_PHYSICAL, VIRTUAL,
/// PHYSICAL, STREAMER_RESERVE.
const BLOCK_KINDS: [BlockKind; BLOCK_COUNT] = [
    BlockKind::Temp,
    BlockKind::Runtime,
    BlockKind::Runtime,
    BlockKind::Delay,
    BlockKind::Delay,
    BlockKind::Normal,
    BlockKind::Normal,
    BlockKind::Normal,
];

/// Asset type numbers (`XAssetType`) to the struct each is loaded as.
const ASSET_STRUCTS: &[(u32, &str)] = &[
    (1, "PhysPreset"),
    (2, "PhysConstraints"),
    (3, "DestructibleDef"),
    (4, "XAnimParts"),
    (5, "XModel"),
    (6, "Material"),
    (7, "MaterialTechniqueSet"),
    (8, "GfxImage"),
    (9, "SndBank"),
    (10, "SndPatch"),
    (11, "clipMap_t"),
    (12, "clipMap_t"),
    (13, "ComWorld"),
    (14, "GameWorldSp"),
    (15, "GameWorldMp"),
    (16, "MapEnts"),
    (17, "GfxWorld"),
    (18, "GfxLightDef"),
    (20, "Font_s"),
    (21, "FontIcon"),
    (22, "MenuList"),
    (23, "menuDef_t"),
    (24, "LocalizeEntry"),
    (25, "WeaponVariantDef"),
    (29, "WeaponAttachment"),
    (30, "WeaponAttachmentUnique"),
    (31, "WeaponCamo"),
    (32, "SndDriverGlobals"),
    (33, "FxEffectDef"),
    (34, "FxImpactTable"),
    (41, "RawFile"),
    (42, "StringTable"),
    (43, "LeaderboardDef"),
    (44, "XGlobals"),
    (45, "ddlRoot_t"),
    (46, "Glasses"),
    (47, "EmblemSet"),
    (48, "ScriptParseTree"),
    (49, "KeyValuePairs"),
    (50, "VehicleDef"),
    (51, "MemoryBlock"),
    (52, "AddonMapEnts"),
    (53, "TracerDef"),
    (54, "SkinnedVertsDef"),
    (55, "Qdb"),
    (56, "Slug"),
    (57, "FootstepTableDef"),
    (58, "FootstepFXTableDef"),
    (59, "ZBarrierDef"),
];

#[derive(Debug)]
pub enum LoadError {
    UnknownAsset(String),
    UnsupportedAssetType(u32),
    StreamEnd { wanted: usize, at: usize },
    Misplaced { block: usize, dst: u32, expected: u32 },
    BlockOverflow { block: usize, offset: u32, size: u32 },
    BadOffset(u32),
    NoBlock,
    Corrupt(String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::UnknownAsset(n) => write!(f, "no loader for asset struct {n}"),
            LoadError::UnsupportedAssetType(t) => write!(f, "unsupported asset type {t}"),
            LoadError::StreamEnd { wanted, at } => write!(f, "zone data ended: wanted {wanted} bytes at {at:#x}"),
            LoadError::Misplaced { block, dst, expected } => {
                write!(f, "load into block {block} at {dst:#x}, but the block is at {expected:#x}")
            }
            LoadError::BlockOverflow { block, offset, size } => write!(f, "block {block} overflow: offset {offset:#x} of {size:#x}"),
            LoadError::BadOffset(p) => write!(f, "zone pointer {p:#x} is outside its block"),
            LoadError::NoBlock => write!(f, "no block pushed"),
            LoadError::Corrupt(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for LoadError {}

#[derive(Clone, Debug)]
pub struct Asset {
    /// The asset type number (`fastfile::ASSET_TYPE_NAMES`).
    pub kind: u32,
    /// The struct it is loaded as, e.g. `XModel`.
    pub structure: &'static str,
    /// Address of the asset's root struct.
    pub addr: u32,
}

/// The loader's state: stream position, block offsets and the address space being filled.
pub struct Stream<'a> {
    data: &'a [u8],
    pos: usize,
    mem: Vec<u8>,
    base: [u32; BLOCK_COUNT],
    size: [u32; BLOCK_COUNT],
    off: [u32; BLOCK_COUNT],
    stack: Vec<usize>,
    temp_offsets: Vec<u32>,
    linked: Vec<(&'static str, u32)>,
}

impl<'a> Stream<'a> {
    fn top(&self) -> Result<usize, LoadError> {
        self.stack.last().copied().ok_or(LoadError::NoBlock)
    }

    pub fn push_block(&mut self, block: usize) {
        self.stack.push(block);
        if BLOCK_KINDS[block] == BlockKind::Temp {
            self.temp_offsets.push(self.off[block]);
        }
    }

    pub fn pop_block(&mut self) {
        if let Some(block) = self.stack.pop() {
            // The temp block's contents are only valid while it is in use.
            if BLOCK_KINDS[block] == BlockKind::Temp {
                self.off[block] = self.temp_offsets.pop().unwrap_or(0);
            }
        }
    }

    /// Align the current block and return the address its next data goes to.
    pub fn alloc(&mut self, align: u32) -> u32 {
        let Ok(b) = self.top() else { return 0 };
        if align > 1 {
            self.off[b] = self.off[b].div_ceil(align) * align;
        }
        self.base[b] + self.off[b]
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], LoadError> {
        let s = self.data.get(self.pos..self.pos + n).ok_or(LoadError::StreamEnd { wanted: n, at: self.pos })?;
        self.pos += n;
        Ok(s)
    }

    fn advance(&mut self, b: usize, n: u32) -> Result<(), LoadError> {
        self.off[b] += n;
        if self.off[b] > self.size[b] {
            return Err(LoadError::BlockOverflow { block: b, offset: self.off[b], size: self.size[b] });
        }
        Ok(())
    }

    /// Load `size` bytes into the current block at `dst`, which must be the block's current end.
    pub fn load(&mut self, dst: u32, size: usize) -> Result<(), LoadError> {
        let b = self.top()?;
        let expected = self.base[b] + self.off[b];
        if dst != expected {
            return Err(LoadError::Misplaced { block: b, dst, expected });
        }
        let end = dst as usize + size;
        if end > self.mem.len() {
            return Err(LoadError::BlockOverflow { block: b, offset: self.off[b] + size as u32, size: self.size[b] });
        }
        match BLOCK_KINDS[b] {
            BlockKind::Temp | BlockKind::Normal => {
                let src = self.take(size)?;
                self.mem[dst as usize..end].copy_from_slice(src);
            }
            BlockKind::Runtime => self.mem[dst as usize..end].fill(0),
            BlockKind::Delay => return Err(LoadError::Corrupt(format!("data in delay block {b}"))),
        }
        self.advance(b, size as u32)
    }

    pub fn ptr(&self, addr: u32) -> u32 {
        self.mem.get(addr as usize..addr as usize + 4).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn set_ptr(&mut self, addr: u32, value: u32) {
        if let Some(b) = self.mem.get_mut(addr as usize..addr as usize + 4) {
            b.copy_from_slice(&value.to_le_bytes());
        }
    }

    /// An integer field of `size` bytes.
    pub fn rd(&self, addr: u32, size: u32, signed: bool) -> i64 {
        let a = addr as usize;
        let Some(b) = self.mem.get(a..a + size as usize) else { return 0 };
        match (size, signed) {
            (1, false) => b[0] as i64,
            (1, true) => b[0] as i8 as i64,
            (2, false) => u16::from_le_bytes([b[0], b[1]]) as i64,
            (2, true) => i16::from_le_bytes([b[0], b[1]]) as i64,
            (4, false) => u32::from_le_bytes(b.try_into().unwrap()) as i64,
            (4, true) => i32::from_le_bytes(b.try_into().unwrap()) as i64,
            (8, _) => i64::from_le_bytes(b.try_into().unwrap()),
            _ => 0,
        }
    }

    /// A count from an evaluation, as the C++ code's size_t conversion would see it.
    pub fn count(&self, value: i64) -> usize {
        usize::try_from(value).unwrap_or(usize::MAX / 2)
    }

    pub fn arith_div(&self, a: i64, b: i64) -> i64 {
        if b == 0 { 0 } else { a / b }
    }

    /// A zone offset (`(block << 29 | offset) + 1`) as an address.
    pub fn convert_ptr(&self, zone_ptr: u32) -> Result<u32, LoadError> {
        let o = zone_ptr.wrapping_sub(1);
        let block = (o >> BLOCK_SHIFT) as usize;
        let offset = o & OFFSET_MASK;
        if block >= BLOCK_COUNT || offset >= self.size[block] {
            return Err(LoadError::BadOffset(zone_ptr));
        }
        Ok(self.base[block] + offset)
    }

    /// A zone offset to an inserted pointer slot: the address stored in the slot.
    pub fn convert_alias(&self, zone_ptr: u32) -> Result<u32, LoadError> {
        let slot = self.convert_ptr(zone_ptr)?;
        Ok(self.ptr(slot))
    }

    /// Reserve a pointer slot in the insert block for an asset loaded with an INSERT pointer.
    pub fn insert_pointer(&mut self) -> u32 {
        let b = INSERT_BLOCK;
        self.off[b] = self.off[b].div_ceil(4) * 4;
        let slot = self.base[b] + self.off[b];
        self.off[b] += 4;
        slot
    }

    /// `const char*` at `field`: a NUL-terminated string follows, or a reference.
    pub fn load_xstring(&mut self, field: u32) -> Result<(), LoadError> {
        let v = self.ptr(field);
        if v == 0 {
            return Ok(());
        }
        if v == FOLLOWING {
            let p = self.alloc(1);
            let b = self.top()?;
            let rest = &self.data[self.pos.min(self.data.len())..];
            let len = rest.iter().position(|&c| c == 0).ok_or(LoadError::StreamEnd { wanted: 1, at: self.data.len() })? + 1;
            let end = p as usize + len;
            if end > self.mem.len() {
                return Err(LoadError::BlockOverflow { block: b, offset: self.off[b] + len as u32, size: self.size[b] });
            }
            let src = self.take(len)?;
            self.mem[p as usize..end].copy_from_slice(src);
            self.advance(b, len as u32)?;
            self.set_ptr(field, p);
        } else {
            let p = self.convert_ptr(v)?;
            self.set_ptr(field, p);
        }
        Ok(())
    }

    pub fn load_xstring_array(&mut self, array: u32, at_stream_start: bool, count: usize) -> Result<(), LoadError> {
        if at_stream_start {
            self.load(array, 4 * count)?;
        }
        for i in 0..count {
            self.load_xstring(array + 4 * i as u32)?;
        }
        Ok(())
    }

    /// Copy a loaded asset's root struct out of the temp block, as the game does.
    pub fn link_asset(&mut self, structure: &'static str, ptr_var: u32, size: u32) {
        let src = self.ptr(ptr_var) as usize;
        let dst = self.mem.len().div_ceil(16) * 16;
        self.mem.resize(dst + size as usize, 0);
        self.mem.copy_within(src..src + size as usize, dst);
        self.set_ptr(ptr_var, dst as u32);
        self.linked.push((structure, dst as u32));
    }
}

/// A fully loaded zone: its memory and its assets.
pub struct Zone {
    pub name: String,
    mem: Vec<u8>,
    pub script_strings: Vec<String>,
    pub assets: Vec<Asset>,
    /// Bytes of zone data left over after the last asset (0 for a clean load).
    pub trailing_bytes: usize,
}

/// A zone assembled from raw memory: reader tests (here and in the conformance oracle) build one
/// without a fastfile stream.
pub fn synthetic(name: &str, mem: Vec<u8>, script_strings: Vec<String>, assets: Vec<Asset>) -> Zone {
    Zone { name: name.to_string(), mem, script_strings, assets, trailing_bytes: 0 }
}

#[cfg(test)]
pub(crate) fn synthetic_zone(name: &str, mem: Vec<u8>, script_strings: Vec<String>, assets: Vec<Asset>) -> Zone {
    synthetic(name, mem, script_strings, assets)
}

/// Load every asset of a decrypted zone. On failure the error says which asset broke.
pub fn load(zone: &fastfile::Zone) -> Result<Zone, LoadError> {
    let d = &zone.data;
    let rd = |at: usize| d.get(at..at + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).ok_or(LoadError::StreamEnd { wanted: 4, at });
    let mut size = [0u32; BLOCK_COUNT];
    for (i, s) in size.iter_mut().enumerate() {
        *s = rd(8 + 4 * i)?;
    }
    let mut base = [0u32; BLOCK_COUNT];
    let mut next: u64 = 16; // address 0 is null
    for i in 0..BLOCK_COUNT {
        base[i] = next as u32;
        next += (size[i] as u64).div_ceil(16) * 16;
    }
    if next > u32::MAX as u64 / 2 {
        return Err(LoadError::Corrupt(format!("zone needs {next} bytes of memory")));
    }
    let mut s = Stream {
        data: d,
        pos: 8 + 4 * BLOCK_COUNT,
        mem: vec![0u8; next as usize],
        base,
        size,
        off: [0; BLOCK_COUNT],
        stack: Vec::new(),
        temp_offsets: Vec::new(),
        linked: Vec::new(),
    };

    // XAssetList is read raw (it is not part of any block).
    let list = s.take(24)?;
    let list: Vec<u32> = (0..6).map(|i| u32::from_le_bytes(list[4 * i..4 * i + 4].try_into().unwrap())).collect();
    let (string_count, strings_ptr, depend_count, depends_ptr, asset_count, assets_ptr) =
        (list[0] as usize, list[1], list[2] as usize, list[3], list[4] as usize, list[5]);
    if string_count > MAX_STRING_COUNT || depend_count > MAX_STRING_COUNT || asset_count > MAX_STRING_COUNT {
        return Err(LoadError::Corrupt("implausible asset list".into()));
    }

    s.push_block(INSERT_BLOCK);
    let mut script_strings = Vec::new();
    if strings_ptr != 0 {
        let arr = s.alloc(4);
        s.load_xstring_array(arr, true, string_count)?;
        for i in 0..string_count {
            script_strings.push(cstr_at(&s.mem, s.ptr(arr + 4 * i as u32)));
        }
    }
    if depends_ptr != 0 {
        let arr = s.alloc(4);
        s.load_xstring_array(arr, true, depend_count)?;
    }
    let mut assets = Vec::new();
    if assets_ptr != 0 {
        let arr = s.alloc(4);
        s.load(arr, 8 * asset_count)?;
        for i in 0..asset_count {
            let entry = arr + 8 * i as u32;
            let kind = s.ptr(entry);
            let structure = ASSET_STRUCTS
                .iter()
                .find(|(t, _)| *t == kind)
                .map(|(_, n)| *n)
                .ok_or(LoadError::UnsupportedAssetType(kind))?;
            generated::load_asset(&mut s, structure, entry + 4).map_err(|e| {
                LoadError::Corrupt(format!("asset {i} of {asset_count} ({}): {e}", fastfile::asset_type_name(kind)))
            })?;
            assets.push(Asset { kind, structure, addr: s.ptr(entry + 4) });
        }
    }
    s.pop_block();

    Ok(Zone { name: zone.name.clone(), trailing_bytes: d.len() - s.pos, mem: s.mem, script_strings, assets })
}

fn cstr_at(mem: &[u8], addr: u32) -> String {
    let Some(rest) = mem.get(addr as usize..) else { return String::new() };
    let len = rest.iter().position(|&b| b == 0).unwrap_or(0);
    String::from_utf8_lossy(&rest[..len]).into_owned()
}

/// Offset of the name pointer in each asset struct that has one.
fn name_offset(structure: &str) -> Option<u32> {
    Some(match structure {
        "GfxImage" => 72,
        "Material" | "XModel" | "XAnimParts" | "MaterialTechniqueSet" | "PhysPreset" | "PhysConstraints"
        | "DestructibleDef" | "RawFile" | "StringTable" | "FxEffectDef" | "FxImpactTable" | "LocalizeEntry"
        | "WeaponCamo" | "TracerDef" | "Font_s" | "FontIcon" | "MenuList" | "menuDef_t" | "KeyValuePairs"
        | "MemoryBlock" | "ScriptParseTree" | "SkinnedVertsDef" | "Qdb" | "Slug" | "FootstepTableDef"
        | "FootstepFXTableDef" | "ZBarrierDef" | "XGlobals" | "ddlRoot_t" | "Glasses" | "EmblemSet"
        | "LeaderboardDef" | "VehicleDef" | "GfxLightDef" | "WeaponAttachment" | "WeaponAttachmentUnique"
        | "WeaponVariantDef" | "SndBank" | "SndPatch" | "clipMap_t" | "ComWorld" | "GameWorldSp" | "GameWorldMp"
        | "MapEnts" | "GfxWorld" | "AddonMapEnts" | "SndDriverGlobals" => 0,
        _ => return None,
    })
}

impl Zone {
    pub fn u8(&self, addr: u32) -> u8 {
        self.mem.get(addr as usize).copied().unwrap_or(0)
    }
    pub fn u16(&self, addr: u32) -> u16 {
        self.mem.get(addr as usize..addr as usize + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
    }
    pub fn i16(&self, addr: u32) -> i16 {
        self.u16(addr) as i16
    }
    pub fn u32(&self, addr: u32) -> u32 {
        self.mem.get(addr as usize..addr as usize + 4).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()))
    }
    pub fn f32(&self, addr: u32) -> f32 {
        f32::from_bits(self.u32(addr))
    }
    pub fn bytes(&self, addr: u32, len: usize) -> &[u8] {
        self.mem.get(addr as usize..addr as usize + len).unwrap_or(&[])
    }

    /// Fallible reads for asset boundaries (top-level struct fields): unlike the lenient ones
    /// above, an address outside the zone memory is an error, so a corrupt pointer cannot silently
    /// read as "field is 0". Inner fields keep the lenient accessors.
    pub fn try_u8(&self, addr: u32) -> Result<u8, FormatError> {
        self.mem.get(addr as usize).copied().ok_or_else(|| bad_addr(addr, 1))
    }
    pub fn try_u16(&self, addr: u32) -> Result<u16, FormatError> {
        let b = self.mem.get(addr as usize..addr as usize + 2).ok_or_else(|| bad_addr(addr, 2))?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    pub fn try_u32(&self, addr: u32) -> Result<u32, FormatError> {
        let b = self.mem.get(addr as usize..addr as usize + 4).ok_or_else(|| bad_addr(addr, 4))?;
        Ok(u32::from_le_bytes(b.try_into().unwrap()))
    }
    pub fn try_f32(&self, addr: u32) -> Result<f32, FormatError> {
        Ok(f32::from_bits(self.try_u32(addr)?))
    }
    pub fn try_bytes(&self, addr: u32, len: usize) -> Result<&[u8], FormatError> {
        self.mem.get(addr as usize..addr as usize + len).ok_or_else(|| bad_addr(addr, len as u32))
    }

    /// The string a `const char*` field points to, falling back to "".
    pub fn try_str_ptr(&self, field: u32) -> Result<String, FormatError> {
        match self.try_u32(field)? {
            0 => Ok(String::new()),
            p => Ok(cstr_at(&self.mem, p)),
        }
    }

    /// The string a `const char*` field points to.
    pub fn str_ptr(&self, field: u32) -> String {
        match self.u32(field) {
            0 => String::new(),
            p => cstr_at(&self.mem, p),
        }
    }
    pub fn script_string(&self, index: u16) -> &str {
        self.script_strings.get(index as usize).map_or("", String::as_str)
    }

    /// An asset's name, for the asset types that have one.
    pub fn asset_name(&self, asset: &Asset) -> Option<String> {
        name_offset(asset.structure).map(|o| self.str_ptr(asset.addr + o))
    }

    /// Find an asset by type number and name (a leading ',' marks a reference and is ignored).
    pub fn find(&self, kind: u32, name: &str) -> Option<&Asset> {
        self.assets
            .iter()
            .filter(|a| a.kind == kind)
            .find(|a| self.asset_name(a).is_some_and(|n| n.trim_start_matches(',') == name))
    }

    /// Every address holding `name` as a NUL-terminated string (an inline struct's name).
    pub fn strings(&self, name: &str) -> Vec<u32> {
        let needle: Vec<u8> = name.bytes().chain([0]).collect();
        let mut out = Vec::new();
        let mut at = 0;
        while let Some(i) = self.mem[at..].windows(needle.len()).position(|w| w == needle.as_slice()) {
            let addr = at + i;
            // A whole string, not the tail of a longer one.
            if addr == 0 || self.mem[addr - 1] == 0 || self.mem[addr - 1] == b',' {
                out.push(addr as u32);
            }
            at = addr + 1;
        }
        out
    }

    /// Every 4-byte-aligned field whose value is `target` (the structs pointing at it).
    pub fn pointers_to(&self, target: u32) -> Vec<u32> {
        let want = target.to_le_bytes();
        self.mem.chunks_exact(4).enumerate().filter(|(_, c)| *c == want).map(|(i, _)| 4 * i as u32).collect()
    }
}

fn bad_addr(addr: u32, len: u32) -> FormatError {
    FormatError::Corrupt(format!("address {addr:#x} (+{len}) is outside the zone memory"))
}

#[cfg(test)]
mod tests {
    /// Every zone in the player's install loads cleanly. Needs the BO2 install:
    /// `ZHBO_BO2_DIR=... cargo test -p zhbo-formats --release -- --ignored`.
    #[test]
    #[ignore]
    fn loads_common_zones() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR");
        for name in ["patch", "faction_fbi_mp", "common_mp"] {
            let file = std::fs::read(std::path::Path::new(&dir).join(format!("zone/all/{name}.ff"))).unwrap();
            let zone = crate::fastfile::read(&file).unwrap();
            let loaded = super::load(&zone).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(loaded.trailing_bytes < 64, "{name}: {} bytes left over", loaded.trailing_bytes);
            assert!(!loaded.assets.is_empty());
        }
    }
}
