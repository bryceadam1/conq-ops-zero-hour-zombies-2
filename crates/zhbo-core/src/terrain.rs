/*
** Conquer Ops - Zero Hour gameplay core: the ground's surface class, read from the map file.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! The ground's surface under a point, from the map's own tile data.
//!
//! The surface feeds the sim: a grenade's bounce takes the surface's parallel and perpendicular
//! scales ([`crate::projectiles`]), and the impact and blast cells it picks ride the events. So the
//! answer must be the same on every machine of a lockstep game, whatever its graphics settings,
//! and on a peer that renders nothing. Zero Hour's own query (`TerrainVisual::getTerrainTile`) lives
//! in the client: it reads the terrain visual's copy of the height map, which exists only when the
//! client subsystem loaded it. This module reads the same data from the map file itself, the way
//! the logic loads its terrain (`W3DTerrainLogic::loadMap`): the bridge hands the core the map's
//! bytes and Zero Hour's terrain types once per map ([`crate::ffi::zhbo_terrain_load`]), and the
//! sim's ground query ([`crate::host::LogicHost::surface_at`]) reads the grid built here.
//!
//! The format is Zero Hour's chunk file (`DataChunkInput`, `Common/System/DataChunk.cpp`): the
//! `CkMp` table of chunk names, then chunks of `id u32, version u16, size i32, data`. Two are read:
//! * `HeightMapData` (`WorldHeightMap::ParseHeightMapData`): the cell grid's width, height and
//!   border (version 3 on), its boundaries (version 4 on) and the height samples (unused here);
//! * `BlendTileData` (`WorldHeightMap::ParseBlendTileData`): one tile index per cell, the blend
//!   layers and cliff data (skipped, sized by the chunk's version), then the texture classes, each
//!   a run of tiles (`firstTile`, `numTiles`) named for a terrain type of `Terrain.ini`.
//!
//! A cell's surface is `WorldHeightMap::getTerrainNameAt`'s answer: the cell under the point
//! (`floor(x / MAP_XY_FACTOR)` plus the border, clamped to the grid), its tile index over four (four
//! grid cells share a tile), the first texture class whose run holds it, and that name's terrain
//! type (`TerrainTypeCollection::findTerrain`, an exact match) classified by the bridge. A cell
//! whose name matches no terrain type is the default surface, as the engine's null tile is.

use crate::impacts::Surface;
use std::sync::RwLock;

/// World units per height-map cell (`MAP_XY_FACTOR`, `Common/MapObject.h`).
pub const MAP_XY_FACTOR: f32 = 10.0;

/// The map's surfaces, one per height-map cell (border included), row-major.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceGrid {
    pub width: i32,
    pub height: i32,
    pub border: i32,
    pub cells: Vec<Surface>,
}

impl SurfaceGrid {
    /// The surface under world point (`x`, `y`): `WorldHeightMap::getTerrainNameAt`'s cell.
    pub fn at(&self, x: f32, y: f32) -> Surface {
        if self.width <= 0 || self.height <= 0 {
            return Surface::Default;
        }
        let cell = |v: f32, n: i32| ((v / MAP_XY_FACTOR).floor() as i32).saturating_add(self.border).clamp(0, n - 1);
        let (xi, yi) = (cell(x, self.width), cell(y, self.height));
        self.cells.get((yi * self.width + xi) as usize).copied().unwrap_or(Surface::Default)
    }
}

/// A little-endian reader over a chunk's bytes; `None` past the end.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(n)?;
        let s = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    fn i32(&mut self) -> Option<i32> {
        self.take(4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u32(&mut self) -> Option<u32> {
        self.i32().map(|v| v as u32)
    }
    fn count(&mut self, each: usize) -> Option<usize> {
        let n = usize::try_from(self.i32()?).ok()?;
        // A count the remaining bytes cannot hold is corrupt.
        (n.checked_mul(each)? <= self.bytes.len() - self.at).then_some(n)
    }
}

/// The two chunks' data this module reads.
struct HeightMap {
    width: i32,
    height: i32,
    border: i32,
}

fn parse_height_map(data: &[u8], version: u16) -> Option<HeightMap> {
    let mut r = Reader { bytes: data, at: 0 };
    let (width, height) = (r.i32()?, r.i32()?);
    let border = if version >= 3 { r.i32()? } else { 0 };
    if version >= 4 {
        let n = r.count(8)?;
        r.take(n * 8)?;
    }
    let size = r.i32()?;
    (width > 0 && height > 0 && width.checked_mul(height) == Some(size)).then_some(HeightMap { width, height, border })
}

/// One texture class: a run of tiles named for a terrain type.
struct TexClass {
    first: i32,
    count: i32,
    name: String,
}

fn parse_blend_tiles(data: &[u8], version: u16, map: &HeightMap) -> Option<(Vec<i16>, Vec<TexClass>)> {
    let mut r = Reader { bytes: data, at: 0 };
    let cells = usize::try_from(map.width * map.height).ok()?;
    if usize::try_from(r.i32()?).ok()? != cells {
        return None;
    }
    let tiles: Vec<i16> = r.take(cells * 2)?.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b)).collect();
    // The blend layer, the extra blend layer (version 6), the cliff info (version 5).
    let layers = 1 + usize::from(version >= 6) + usize::from(version >= 5);
    r.take(cells * 2 * layers)?;
    if version >= 7 {
        // The cliff-state bits: version 7 wrote rows `(width + 1) / 8` bytes wide, later versions
        // `(width + 7) / 8` (`ParseBlendTileData`).
        let row = if version == 7 { (map.width + 1) / 8 } else { (map.width + 7) / 8 };
        r.take(usize::try_from(row * map.height).ok()?)?;
    }
    let _bitmap_tiles = r.i32()?;
    let _blended_tiles = r.i32()?;
    if version >= 5 {
        let _cliff_info = r.i32()?;
    }
    let n = r.count(18)?;
    let mut classes = Vec::with_capacity(n);
    for _ in 0..n {
        let (first, count, _width, _legacy) = (r.i32()?, r.i32()?, r.i32()?, r.i32()?);
        let len = r.u16()? as usize;
        let name = String::from_utf8_lossy(r.take(len)?).into_owned();
        classes.push(TexClass { first, count, name });
    }
    Some((tiles, classes))
}

/// Build the surface grid of a map file's (decompressed) bytes. `terrain` is Zero Hour's terrain
/// types in collection order, each name with its surface (the bridge classifies `Terrain.ini`'s
/// classes); a texture class whose name is none of them is [`Surface::Default`].
pub fn parse_map(bytes: &[u8], terrain: &[(String, Surface)]) -> Result<SurfaceGrid, String> {
    let mut r = Reader { bytes, at: 0 };
    if r.take(4) != Some(b"CkMp".as_slice()) {
        return Err("not a chunk file (no CkMp table)".into());
    }
    let n = r.count(5).ok_or("a corrupt chunk table")?;
    let mut names: Vec<(u32, String)> = Vec::with_capacity(n);
    for _ in 0..n {
        let len = r.u8().ok_or("a corrupt chunk table")? as usize;
        let name = String::from_utf8_lossy(r.take(len).ok_or("a corrupt chunk table")?).into_owned();
        names.push((r.u32().ok_or("a corrupt chunk table")?, name));
    }
    let name_of = |id: u32| names.iter().find(|(i, _)| *i == id).map(|(_, n)| n.as_str());
    let (mut map, mut blend) = (None, None);
    // The top-level chunks, in file order (the engine parses them in that order too: the blend
    // data is sized by the height map read before it).
    while r.at < bytes.len() {
        let (Some(id), Some(version), Some(size)) = (r.u32(), r.u16(), r.i32()) else { break };
        let data = usize::try_from(size).ok().and_then(|s| r.take(s)).ok_or("a chunk runs past the end of the file")?;
        match name_of(id) {
            Some("HeightMapData") => map = parse_height_map(data, version),
            Some("BlendTileData") => {
                let m = map.as_ref().ok_or("BlendTileData before HeightMapData")?;
                blend = parse_blend_tiles(data, version, m);
            }
            _ => {}
        }
    }
    let map = map.ok_or("no readable HeightMapData")?;
    let (tiles, classes) = blend.ok_or("no readable BlendTileData")?;
    let class_surface: Vec<Surface> = classes
        .iter()
        .map(|c| terrain.iter().find(|(name, _)| *name == c.name).map_or(Surface::Default, |(_, s)| *s))
        .collect();
    let cells = tiles
        .iter()
        .map(|&t| {
            // Four grid cells share a tile (`tileNdx >> 2`).
            let tile = i32::from(t) >> 2;
            classes.iter().position(|c| tile >= c.first && tile < c.first.saturating_add(c.count)).map_or(Surface::Default, |i| class_surface[i])
        })
        .collect();
    Ok(SurfaceGrid { width: map.width, height: map.height, border: map.border, cells })
}

/// The loaded map's grid (one per map; the bridge loads it at the loading screen, and again on the
/// first ground query of a map it has not loaded).
static GRID: RwLock<Option<SurfaceGrid>> = RwLock::new(None);

/// Install a map's grid (replacing the previous map's), or clear it with `None`.
pub fn install(grid: Option<SurfaceGrid>) {
    *GRID.write().unwrap_or_else(|e| e.into_inner()) = grid;
}

/// The loaded map's surface under (`x`, `y`); [`Surface::Default`] with no map loaded.
pub fn surface_at(x: f32, y: f32) -> Surface {
    GRID.read().unwrap_or_else(|e| e.into_inner()).as_ref().map_or(Surface::Default, |g| g.at(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chunk file the way `DataChunkOutput` writes one: the name table, then the chunks.
    pub(crate) fn chunk_file(chunks: &[(&str, u16, Vec<u8>)]) -> Vec<u8> {
        let mut out = b"CkMp".to_vec();
        out.extend_from_slice(&(chunks.len() as i32).to_le_bytes());
        for (i, (name, _, _)) in chunks.iter().enumerate() {
            out.push(name.len() as u8);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&(i as u32 + 1).to_le_bytes());
        }
        for (i, (_, version, data)) in chunks.iter().enumerate() {
            out.extend_from_slice(&(i as u32 + 1).to_le_bytes());
            out.extend_from_slice(&version.to_le_bytes());
            out.extend_from_slice(&(data.len() as i32).to_le_bytes());
            out.extend_from_slice(data);
        }
        out
    }

    fn i32s(v: &[i32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    /// A `width` x `height` map with `border`, every cell's tile index from `tile`, and texture
    /// classes `(first, count, name)`, in the current chunk versions (height map 4, blend 8).
    pub(crate) fn map(width: i32, height: i32, border: i32, tile: impl Fn(i32, i32) -> i16, classes: &[(i32, i32, &str)]) -> Vec<u8> {
        let cells = (width * height) as usize;
        let mut hm = i32s(&[width, height, border, 1, width - 2 * border, height - 2 * border, width * height]);
        hm.extend(std::iter::repeat_n(7u8, cells));
        let mut bt = i32s(&[width * height]);
        for y in 0..height {
            for x in 0..width {
                bt.extend_from_slice(&tile(x, y).to_le_bytes());
            }
        }
        // blend, extra blend, cliff info.
        bt.extend(std::iter::repeat_n(0u8, cells * 2 * 3));
        bt.extend(std::iter::repeat_n(0u8, ((width + 7) / 8 * height) as usize));
        bt.extend(i32s(&[64, 1, 1, classes.len() as i32]));
        for (first, count, name) in classes {
            bt.extend(i32s(&[*first, *count, 4, 0]));
            bt.extend_from_slice(&(name.len() as u16).to_le_bytes());
            bt.extend_from_slice(name.as_bytes());
        }
        // What follows the classes (edge classes, blend tiles) is not read.
        bt.extend(i32s(&[0, 0]));
        chunk_file(&[("WorldInfo", 1, vec![0; 6]), ("HeightMapData", 4, hm), ("BlendTileData", 8, bt), ("ObjectsList", 3, vec![1, 2, 3])])
    }

    fn terrain() -> Vec<(String, Surface)> {
        [("TEDesert1", Surface::Sand), ("AsphaltType1", Surface::Asphalt), ("GrassLight", Surface::Grass)].iter().map(|(n, s)| (n.to_string(), *s)).collect()
    }

    /// `getTerrainNameAt`: the cell under the point plus the border, its tile over four, the first
    /// class holding it, that class's terrain type; unknown names and tiles of no class are the
    /// default surface.
    #[test]
    fn a_cell_reads_its_tiles_texture_class() {
        // Columns 0-3 sand (tiles 0..), 4-7 asphalt, 8-11 a class Terrain.ini lacks, the rest tiles
        // of no class.
        let tile = |x: i32, _y: i32| ((x / 4) * 16 + (x % 4)) as i16;
        let bytes = map(20, 10, 2, tile, &[(0, 4, "TEDesert1"), (4, 4, "AsphaltType1"), (8, 4, "NotInTerrainIni")]);
        let g = parse_map(&bytes, &terrain()).unwrap();
        assert_eq!((g.width, g.height, g.border), (20, 10, 2));
        // World x 0 is grid column 2 (the border): tile 2 >> 2 = 0, sand.
        assert_eq!(g.at(0.0, 0.0), Surface::Sand);
        assert_eq!(g.at(19.9, 5.0), Surface::Sand, "column 3");
        assert_eq!(g.at(20.0, 5.0), Surface::Asphalt, "column 4: tile 16 >> 2 = 4");
        assert_eq!(g.at(60.0, 5.0), Surface::Default, "column 8: a name Terrain.ini lacks");
        assert_eq!(g.at(100.0, 5.0), Surface::Default, "column 12: a tile of no class");
        // Off the map clamps to the edge cells, as the engine's lookup does.
        assert_eq!(g.at(-500.0, -500.0), Surface::Sand);
        assert_eq!(g.at(1e9, 1e9), Surface::Default);
        assert_eq!(g.at(f32::NAN, 0.0), Surface::Sand, "a NaN reads cell 0, never panics");
    }

    /// The older chunk versions the shipped maps can carry: height map 3 (no boundaries) and blend
    /// 5-7 (fewer layers, version 7's short cliff rows).
    #[test]
    fn older_chunk_versions_parse() {
        for blend_version in [5u16, 6, 7] {
            let (w, h) = (9, 4);
            let cells = (w * h) as usize;
            let mut hm = i32s(&[w, h, 0, w * h]);
            hm.extend(std::iter::repeat_n(0u8, cells));
            let mut bt = i32s(&[w * h]);
            bt.extend((0..cells).flat_map(|_| 4i16.to_le_bytes()));
            let layers = 1 + usize::from(blend_version >= 6) + 1;
            bt.extend(std::iter::repeat_n(0u8, cells * 2 * layers));
            if blend_version >= 7 {
                bt.extend(std::iter::repeat_n(0u8, ((w + 1) / 8 * h) as usize));
            }
            bt.extend(i32s(&[64, 1, 1, 1, 0, 4, 4, 0]));
            bt.extend_from_slice(&10u16.to_le_bytes());
            bt.extend_from_slice(b"GrassLight");
            let bytes = chunk_file(&[("HeightMapData", 3, hm), ("BlendTileData", blend_version, bt)]);
            // Every tile is 4: 4 >> 2 = 1, inside class 0's run (0..4).
            let g = parse_map(&bytes, &terrain()).unwrap_or_else(|e| panic!("blend {blend_version}: {e}"));
            assert_eq!(g.border, 0);
            assert!(g.cells.iter().all(|s| *s == Surface::Grass), "blend {blend_version}");
        }
    }

    /// The ground a peer's sim reads depends on logic data only: the map bytes and terrain types
    /// the bridge hands `zhbo_terrain_load` (the same file and `Terrain.ini` on every machine) and
    /// the logic's water table (the `surface_at` callback). A machine that renders, one with other
    /// graphics settings and a `-headless` one run the same bridge code and no client state, so
    /// they read the same surface at every point; the old query read the client's terrain visual.
    #[test]
    fn every_machine_reads_the_same_ground_from_the_map_file() {
        use crate::host::{CLogicHost, LogicApi, LogicHost};
        let tile = |x: i32, y: i32| (((x + y) % 3) * 16) as i16;
        let bytes = map(30, 30, 3, tile, &[(0, 4, "TEDesert1"), (4, 4, "AsphaltType1"), (8, 4, "GrassLight")]);
        let terrain = terrain();
        let names: Vec<std::ffi::CString> = terrain.iter().map(|(n, _)| std::ffi::CString::new(n.as_str()).unwrap()).collect();
        let ptrs: Vec<*const std::ffi::c_char> = names.iter().map(|n| n.as_ptr()).collect();
        let surfaces: Vec<i32> = terrain.iter().map(|(_, s)| *s as i32).collect();
        let cells = unsafe { crate::ffi::zhbo_terrain_load(bytes.as_ptr(), bytes.len() as i32, ptrs.as_ptr(), surfaces.as_ptr(), ptrs.len() as i32) };
        assert_eq!(cells, 900);
        // The logic's water table: a lake west of x = 0.
        extern "C" fn lake(x: f32, _y: f32) -> i32 {
            if x < 0.0 { Surface::Water as i32 } else { Surface::Default as i32 }
        }
        // SAFETY: every field of the table is an `Option` of a function pointer; all-zero is all `None`.
        let mut api: LogicApi = unsafe { std::mem::zeroed() };
        api.surface_at = Some(lake);
        let host = CLogicHost(api);
        let expected = parse_map(&bytes, &terrain).unwrap();
        for i in -40..300 {
            let (x, y) = (i as f32 * 0.93, (299 - i) as f32 * 0.71);
            let want = if x < 0.0 { Surface::Water } else { expected.at(x, y) };
            assert_eq!(host.surface_at(x, y), want, "({x}, {y})");
            assert_eq!(crate::ffi::zhbo_terrain_surface_at(x, y), expected.at(x, y) as i32);
        }
        assert!([Surface::Sand, Surface::Asphalt, Surface::Grass].iter().all(|s| expected.cells.contains(s)));
        // A file that does not parse clears the grid: every machine then reads the default.
        let cells = unsafe { crate::ffi::zhbo_terrain_load(b"junk".as_ptr(), 4, ptrs.as_ptr(), surfaces.as_ptr(), ptrs.len() as i32) };
        assert_eq!(cells, -1);
        assert_eq!(host.surface_at(50.0, 50.0), Surface::Default);
        install(None);
    }

    /// Every map the player's Zero Hour ships reads: each `.map` of every archive (decompressed as
    /// the engine's `CachedFileInputStream` does) parses, and every cell's texture class names a
    /// terrain type of the install's `Terrain.ini` — the grid answers what the terrain visual's
    /// lookup would, for the real data, with no terrain visual in sight. `--nocapture` prints the
    /// per-map counts.
    #[test]
    #[ignore = "reads this machine's Zero Hour install (ZHBO_ZH_DIR)"]
    fn every_shipped_map_reads_its_ground_surfaces() {
        use zhbo_formats::big::BigArchive;
        let zh = zhbo_requirements::check_requirements().zero_hour.expect("a Zero Hour install").path;
        let archives: Vec<BigArchive> = std::fs::read_dir(&zh)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("big")))
            .filter_map(|p| BigArchive::open(&p).ok())
            .collect();
        // Terrain.ini's entries (`Terrain <name>` blocks, the Default file and the overrides); the
        // test only needs to know a name is a terrain type, so each reads as dirt.
        let mut terrain: Vec<(String, Surface)> = Vec::new();
        for a in &archives {
            for e in a.entries().iter().filter(|e| e.path.to_ascii_lowercase().ends_with("terrain.ini")) {
                let text = String::from_utf8_lossy(&a.read(e).unwrap()).into_owned();
                for line in text.lines() {
                    let mut words = line.split(';').next().unwrap_or_default().split_whitespace();
                    if words.next().is_some_and(|w| w.eq_ignore_ascii_case("Terrain")) {
                        if let Some(name) = words.next() {
                            terrain.push((name.to_string(), Surface::Dirt));
                        }
                    }
                }
            }
        }
        assert!(terrain.len() > 50, "Terrain.ini's types: {}", terrain.len());
        let mut maps = 0;
        for a in &archives {
            for e in a.entries().iter().filter(|e| e.path.to_ascii_lowercase().ends_with(".map")) {
                let bytes = zhbo_formats::refpack::decompress_file(&a.read(e).unwrap()).unwrap_or_else(|err| panic!("{}: {err}", e.path));
                let g = parse_map(&bytes, &terrain).unwrap_or_else(|err| panic!("{}: {err}", e.path));
                let unresolved = g.cells.iter().filter(|s| **s == Surface::Default).count();
                println!("{}: {} x {} (border {}), {unresolved} cells of no terrain type", e.path, g.width, g.height, g.border);
                assert_eq!(unresolved, 0, "{}: every cell's tile names a terrain type", e.path);
                maps += 1;
            }
        }
        assert!(maps > 50, "the install's maps: {maps}");
    }

    /// A corrupt or truncated file is an error, never a panic or a wrong grid.
    #[test]
    fn a_corrupt_map_is_refused() {
        let good = map(8, 8, 1, |_, _| 0, &[(0, 4, "GrassLight")]);
        assert!(parse_map(&good, &terrain()).is_ok());
        assert!(parse_map(b"EAR\0junk", &terrain()).is_err(), "a compressed file must be decompressed first");
        for cut in [3, 8, 40, good.len() / 2, good.len() - 30] {
            assert!(parse_map(&good[..cut], &terrain()).is_err(), "cut at {cut}");
        }
        // Noise after the header never panics.
        let mut s = 0x1234_5678_9abc_def0u64;
        for _ in 0..500 {
            let mut bytes = good.clone();
            for _ in 0..8 {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                let i = (s as usize) % bytes.len();
                bytes[i] = (s >> 32) as u8;
            }
            let _ = parse_map(&bytes, &terrain());
        }
    }
}
