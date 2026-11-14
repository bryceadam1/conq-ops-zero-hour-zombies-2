//! A Zombies map for Conquer Ops, built from a stock Zero Hour map of the player's own install
//! (`MapsZH.big`; nothing is shipped): the same terrain, props and waypoints, with zombie spawn
//! points (`zhbo_core::names::ZOMBIE_SPAWNER`, BO2's `zombie_spawn_locations`) added around each
//! player's start. Their presence is what makes it a Zombies map: the engine stands a survivor
//! for each player instead of his buildings (host patch 0035) and the core runs the round engine
//! (`zhbo_core::sim::zombies`). The spawn points join the map's neutral team (`team`), whose side
//! the engine makes the players' enemy, so the zombies they spawn are.
//!
//! The archive carries the map (`zhbo_core::names::ZOMBIE_MAPS`); the bridge copies it into the
//! user-data folder's `Maps`, where Zero Hour lists user maps.

use zhbo_formats::zhmap::{Body, Chunk, ChunkFile, MapObject, Value};

/// The stock map the Zombies map is made from, in `MapsZH.big`.
pub const BASE: &str = "Maps\\Tournament Desert\\Tournament Desert";
/// Spawn points per player start, and how far from it (Zero Hour units).
pub const SPAWNERS_PER_START: usize = 4;
pub const SPAWN_RING: f32 = 400.0;
/// `MAP_XY_FACTOR` (`Common/MapObject.h`): world units per height-map cell.
const MAP_XY_FACTOR: f32 = 10.0;

/// The map's playable extent in world units: the height map's cells inside the border
/// (`HeightMapData`: width, height and, from version 3, the border).
fn extent(file: &ChunkFile) -> Option<(f32, f32)> {
    let c = file.chunk("HeightMapData")?;
    let Body::Raw(d) = &c.body else { return None };
    let i = |k: usize| d.get(4 * k..4 * k + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap()));
    let (w, h) = (i(0)?, i(1)?);
    let border = if c.version >= 3 { i(2)? } else { 0 };
    Some((((w - 2 * border).max(0)) as f32 * MAP_XY_FACTOR, ((h - 2 * border).max(0)) as f32 * MAP_XY_FACTOR))
}

/// The map's objects, decoded.
fn objects(file: &ChunkFile) -> Result<Vec<MapObject>, String> {
    let Some(Body::Children(kids)) = file.chunk("ObjectsList").map(|c| &c.body) else { return Err("the map has no ObjectsList".into()) };
    kids.iter()
        .filter(|c| c.name == "Object")
        .map(|c| match &c.body {
            Body::Raw(d) => MapObject::read(file, c.version, d).map_err(|e| e.to_string()),
            Body::Children(_) => Err("a nested Object".into()),
        })
        .collect()
}

/// The spawn points for a map whose player starts are `starts`, inside `extent`: a ring around
/// each start, a quarter turn apart from the diagonals; a point off the map is left out.
pub fn spawn_points(starts: &[[f32; 2]], extent: (f32, f32)) -> Vec<[f32; 2]> {
    let mut out = Vec::new();
    for s in starts {
        for k in 0..SPAWNERS_PER_START {
            let a = std::f32::consts::FRAC_PI_4 + k as f32 * std::f32::consts::TAU / SPAWNERS_PER_START as f32;
            let p = [s[0] + SPAWN_RING * a.cos(), s[1] + SPAWN_RING * a.sin()];
            if p[0] > 0.0 && p[1] > 0.0 && p[0] < extent.0 && p[1] < extent.1 {
                out.push(p);
            }
        }
    }
    out
}

/// The Zombies map from the stock map's bytes (`.map`, RefPack-compressed or not): the files for
/// `ZOMBIE_MAPS` (the map; the preview is copied as it is by [`files`]) and a summary.
pub fn from_map(map: &[u8]) -> Result<(Vec<u8>, String), String> {
    let raw = zhbo_formats::refpack::decompress_file(map).map_err(|e| e.to_string())?;
    let mut file = ChunkFile::parse(&raw).map_err(|e| e.to_string())?;
    let objs = objects(&file)?;
    let mut starts: Vec<(String, [f32; 2])> = objs
        .iter()
        .filter_map(|o| match o.prop("waypointName") {
            Some(Value::Ascii(n)) if n.starts_with("Player_") && n.ends_with("_Start") => Some((n.clone(), [o.pos[0], o.pos[1]])),
            _ => None,
        })
        .collect();
    starts.sort_by(|a, b| a.0.cmp(&b.0));
    if starts.is_empty() {
        return Err("the map has no player starts".into());
    }
    let extent = extent(&file).ok_or("the map has no HeightMapData")?;
    let points = spawn_points(&starts.iter().map(|s| s.1).collect::<Vec<_>>(), extent);
    let version = file.chunk("ObjectsList").and_then(|c| match &c.body {
        Body::Children(k) => k.iter().find(|k| k.name == "Object").map(|k| k.version),
        _ => None,
    });
    let version = version.unwrap_or(3);
    let mut new_chunks = Vec::new();
    for (k, p) in points.iter().enumerate() {
        let o = MapObject {
            pos: [p[0], p[1], 0.0],
            angle: 0.0,
            flags: 0,
            name: zhbo_core::names::ZOMBIE_SPAWNER.to_string(),
            props: vec![
                ("originalOwner".into(), Value::Ascii("team".into())),
                ("uniqueID".into(), Value::Ascii(format!("{} {}", zhbo_core::names::ZOMBIE_SPAWNER, k + 1))),
                ("objectInitialHealth".into(), Value::Int(100)),
                ("objectEnabled".into(), Value::Bool(true)),
                ("objectIndestructible".into(), Value::Bool(true)),
                ("objectSelectable".into(), Value::Bool(false)),
            ],
        };
        new_chunks.push(Chunk { name: "Object".into(), version, body: Body::Raw(o.write(&mut file)) });
    }
    if let Some(Chunk { body: Body::Children(kids), .. }) = file.chunk_mut("ObjectsList") {
        kids.extend(new_chunks);
    }
    let summary = format!("zombies map: {} spawn points around {} player starts of {BASE}", points.len(), starts.len());
    Ok((file.write(), summary))
}

/// Archive files: each path and its bytes.
pub type Files = Vec<(String, Vec<u8>)>;

/// The Zombies map's files from the player's Zero Hour install (`zh_dir`), as the archive
/// carries them (`ZOMBIE_MAPS` order: the map, then its preview), and a summary.
pub fn files(zh_dir: &std::path::Path) -> Result<(Files, String), String> {
    let big = zhbo_formats::big::BigArchive::open(zh_dir.join("MapsZH.big")).map_err(|e| e.to_string())?;
    let read = |ext: &str| -> Result<Vec<u8>, String> {
        let e = big.find(&format!("{BASE}.{ext}")).ok_or(format!("MapsZH.big has no {BASE}.{ext}"))?;
        big.read(e).map_err(|e| e.to_string())
    };
    let (map, summary) = from_map(&read("map")?)?;
    let [m, t] = zhbo_core::names::ZOMBIE_MAPS;
    let mut out = vec![(m.0.to_string(), map)];
    if let Ok(tga) = read("tga") {
        out.push((t.0.to_string(), tga));
    }
    Ok((out, summary))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spawn_points_ring_each_start_inside_the_map() {
        let pts = spawn_points(&[[1000.0, 600.0], [1000.0, 2100.0]], (2000.0, 2700.0));
        assert_eq!(pts.len(), 8);
        for p in &pts {
            let d = [[1000.0, 600.0], [1000.0, 2100.0]].iter().map(|s: &[f32; 2]| ((p[0] - s[0]).powi(2) + (p[1] - s[1]).powi(2)).sqrt()).fold(f32::MAX, f32::min);
            assert!((d - SPAWN_RING).abs() < 1e-2);
        }
        // Near an edge, the points off the map are left out.
        assert_eq!(spawn_points(&[[100.0, 100.0]], (2000.0, 2000.0)).len(), 1);
    }

    /// Over the player's install: the Zombies map parses back with its spawn points, the stock
    /// objects untouched.
    #[test]
    #[ignore]
    fn the_zombies_map_builds_from_the_install() {
        let zh = std::path::PathBuf::from(std::env::var("ZHBO_ZH_DIR").expect("ZHBO_ZH_DIR"));
        let (files, summary) = files(&zh).unwrap();
        eprintln!("{summary}");
        let file = ChunkFile::parse(&files[0].1).unwrap();
        let objs = objects(&file).unwrap();
        let spawners: Vec<&MapObject> = objs.iter().filter(|o| o.name == zhbo_core::names::ZOMBIE_SPAWNER).collect();
        assert!(spawners.len() >= 4, "{summary}");
        assert!(spawners.iter().all(|o| o.prop("originalOwner") == Some(&Value::Ascii("team".into()))));
        assert_eq!(files.len(), 2, "the map and its preview");
    }
}
