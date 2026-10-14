//! zhbo-assets - inspect and export Black Ops II content from the player's own install.
//!
//! ```text
//! zhbo-assets banks                                   list sound banks
//! zhbo-assets list <bank>                             sound entries: id, duration, channels, rate
//! zhbo-assets export <bank> <out-dir> [--min S] [--max S]
//!                                                     decode sound entries to WAV
//! zhbo-assets paks                                    list image packs
//! zhbo-assets images <pak> [--formats 1,13] [--max-size N]
//!                                                     image index: hash, format, size
//! zhbo-assets sheets <pak> <out-dir> [--formats ..] [--max-size N] [--hashes a,b] [--per-sheet 64] [--cell 96]
//!                                                     numbered contact sheets + index, to identify art by eye
//! zhbo-assets image <pak> <hash> <out.png>            export one image
//! zhbo-assets zones                                   list fastfiles
//! zhbo-assets zone <ff>                               decrypt a fastfile; asset counts by type
//! zhbo-assets find <text> [<ff> ...]                  names containing <text>, in every (or the given) zone
//! zhbo-assets model <ff> <name> [--png out] [--bones] read a model; stats, preview image, skeleton
//! zhbo-assets dump <ff> <text> [before] [after]        hex dump around a string in a zone
//! zhbo-assets pose <out.png> <frame> <ff>:<anim> <ff>:<model>[@<tag>] ...
//!                                                     render models posed by an animation frame
//! ```
//! `<bank>`/`<pak>`/`<ff>` are file names inside the BO2 install (`sound\`, `zone\all\`) or full paths.
//! Everything exported is the player's own game data: keep it local, never redistribute it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use zhbo_assets::{images, sounds};

mod preview;
use zhbo_formats::fastfile;
use zhbo_formats::ipak::Ipak;
use zhbo_formats::iwi::Format;
use zhbo_formats::sab::{SoundBank, FORMAT_FLAC, FORMAT_PCM16};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("zhbo-assets: {e}");
            ExitCode::FAILURE
        }
    }
}

fn bo2_dir() -> Result<PathBuf, String> {
    Ok(zhbo_requirements::check_requirements().black_ops2?.path)
}

fn resolve(name: &str, sub: &[&str]) -> Result<PathBuf, String> {
    if Path::new(name).is_file() {
        return Ok(PathBuf::from(name));
    }
    let mut p = bo2_dir()?;
    for s in sub {
        p = p.join(s);
    }
    Ok(p.join(name))
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn list_dir(dir: PathBuf, ext: &[&str]) -> Result<(), String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| ext.iter().any(|x| n.ends_with(x)))
        .collect();
    names.sort();
    names.iter().for_each(|n| println!("{n}"));
    Ok(())
}

/// Image filter from `--formats` (IWi codes) and `--max-size` (largest side).
struct Filter {
    formats: Option<Vec<u8>>,
    max_size: u16,
    hashes: Option<Vec<u32>>,
}

impl Filter {
    fn from_args(args: &[String]) -> Self {
        Self {
            formats: flag(args, "--formats").map(|f| f.split(',').filter_map(|c| c.trim().parse().ok()).collect()),
            max_size: flag(args, "--max-size").and_then(|v| v.parse().ok()).unwrap_or(u16::MAX),
            hashes: flag(args, "--hashes")
                .map(|h| h.split(',').filter_map(|x| u32::from_str_radix(x.trim().trim_start_matches("0x"), 16).ok()).collect()),
        }
    }

    fn wants_hash(&self, hash: u32) -> bool {
        self.hashes.as_ref().map_or(true, |h| h.contains(&hash))
    }

    fn accepts(&self, h: &zhbo_formats::iwi::Header) -> bool {
        let code = match h.format {
            Format::Argb32 => 1,
            Format::Rgb24 => 2,
            Format::Dxt1 => 11,
            Format::Dxt3 => 12,
            Format::Dxt5 => 13,
            Format::Bc5 => 14,
            Format::Other(c) => c,
        };
        self.formats.as_ref().map_or(true, |f| f.contains(&code)) && h.width.max(h.height) <= self.max_size
    }
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str).unwrap_or("") {
        "banks" => list_dir(bo2_dir()?.join("sound"), &[".sabs", ".sabl"]),
        "paks" => list_dir(bo2_dir()?.join("zone").join("all"), &[".ipak"]),
        "zones" => list_dir(bo2_dir()?.join("zone").join("all"), &[".ff"]),
        "zone" => {
            let zone = open_zone(args.get(1).ok_or("zone <ff>")?)?;
            let index = zone.index().map_err(|e| e.to_string())?;
            println!("zone {}: {} bytes, {} script strings, {} assets", zone.name, zone.data.len(), index.script_strings.len(), index.assets.len());
            let mut counts = std::collections::BTreeMap::new();
            for &(t, _) in &index.assets {
                *counts.entry(t).or_insert(0usize) += 1;
            }
            for (t, n) in counts {
                println!("{:>6}  {}", n, fastfile::asset_type_name(t));
            }
            if args.iter().any(|a| a == "--order") {
                // Runs of asset types in stream order: what a parser must walk to reach an asset.
                println!("order:");
                let mut runs: Vec<(u32, usize, usize)> = Vec::new(); // (type, first index, count)
                for (i, &(t, _)) in index.assets.iter().enumerate() {
                    match runs.last_mut() {
                        Some(r) if r.0 == t => r.2 += 1,
                        _ => runs.push((t, i, 1)),
                    }
                }
                for (t, first, n) in runs {
                    println!("{:>6} {:>6}  {}", first, n, fastfile::asset_type_name(t));
                }
            }
            Ok(())
        }
        "model" => {
            let usage = "model <ff> <name> [--png out.png]";
            let zone = load_full_zone(args.get(1).ok_or(usage)?)?;
            let name = args.get(2).ok_or(usage)?;
            let model = zhbo_formats::xmodel::find(&zone, name).map_err(|e| e.to_string())?;
            println!("{}: {} bones, {} surfaces, {} lods, bounds {:?}..{:?}", model.name, model.bones.len(), model.surfaces.len(), model.lods.len(), model.mins, model.maxs);
            for (i, l) in model.lods.iter().enumerate() {
                let surfs = &model.surfaces[l.first_surface..l.first_surface + l.surface_count];
                let tris: usize = surfs.iter().map(|s| s.triangles.len()).sum();
                let verts: usize = surfs.iter().map(|s| s.vertices.len()).sum();
                println!("  lod{i}: dist {:.0}, {} surfaces, {verts} vertices, {tris} triangles", l.distance, l.surface_count);
            }
            let names: Vec<&str> = model.bones.iter().take(12).map(|b| b.name.as_str()).collect();
            println!("  bones: {} ...", names.join(" "));
            let paks = image_paks()?;
            for (i, m) in model.materials.iter().enumerate().take(model.lods[0].surface_count) {
                match m {
                    None => println!("  surface {i}: material shared or unreadable"),
                    Some(m) => {
                        println!("  surface {i}: {}", m.name);
                        for t in &m.textures {
                            match &t.image {
                                Some(img) => {
                                    let find = |h: u32| paks.iter().find(|(_, _, g)| g.contains_key(&h)).map(|(n, _, _)| n.as_str()).unwrap_or("-");
                                    println!(
                                        "      semantic {:>2}: {} {}x{} hash {:#010x} in {}, stream hash {:#010x} ({} parts) in {}",
                                        t.semantic, img.name, img.width, img.height, img.hash, find(img.hash), img.stream_hash, img.streamed_parts, find(img.stream_hash)
                                    );
                                }
                                None => println!("      semantic {:>2}: (shared image)", t.semantic),
                            }
                        }
                    }
                }
            }
            if args.iter().any(|a| a == "--bones") {
                for (i, b) in model.bones.iter().enumerate() {
                    println!("  bone {i:>3} {:<24} parent {:>4} pos {:>7.2?} rot {:>6.3?}", b.name, b.parent.map_or(-1, |p| p as i64), b.position, b.rotation);
                }
                for (i, s) in model.surfaces.iter().enumerate().take(model.lods[0].surface_count) {
                    let mut bones: Vec<u16> = s.vertices.iter().flat_map(|v| v.weights.iter().map(|w| w.0)).collect();
                    bones.sort_unstable();
                    bones.dedup();
                    let c = s.vertices.iter().fold([0.0f32; 3], |a, v| [a[0] + v.position[0], a[1] + v.position[1], a[2] + v.position[2]]);
                    let n = s.vertices.len().max(1) as f32;
                    println!("  surface {i}: {} verts, bones {bones:?}, centroid {:>7.2?}", s.vertices.len(), [c[0] / n, c[1] / n, c[2] / n]);
                }
            }
            if let Some(out) = flag(args, "--png") {
                let textures = color_maps(&model, &paks);
                preview::render(&model, &textures, Path::new(out)).map_err(|e| e.to_string())?;
                println!("wrote {out}");
            }
            Ok(())
        }
        "pose" => {
            // pose <out.png> <frame|fraction> <anim-ff>:<anim> <ff>:<model>[@<parent-bone>] ...
            let usage = "pose <out.png> <frame> <ff>:<anim> <ff>:<model>[@<tag>] ...";
            let out = args.get(1).ok_or(usage)?;
            let frame: f32 = args.get(2).and_then(|v| v.parse().ok()).ok_or(usage)?;
            let split = |s: &str| -> Result<(String, String), String> {
                s.split_once(':').map(|(a, b)| (a.to_string(), b.to_string())).ok_or_else(|| usage.to_string())
            };
            let mut zones: ZoneCache = Default::default();
            let (anim_ff, anim_name) = split(args.get(3).ok_or(usage)?)?;
            load_zone(&mut zones, &anim_ff)?;
            let anim = zhbo_formats::xanim::find(&zones[&anim_ff], &anim_name).map_err(|e| e.to_string())?;
            println!("{}: {} frames at {} fps, {} bones", anim.name, anim.num_frames, anim.frame_rate, anim.bones.len());
            let paks = image_paks()?;
            let mut models = Vec::new();
            for spec in &args[4..] {
                let (ff, rest) = split(spec)?;
                let (name, tag) = match rest.split_once('@') {
                    Some((n, t)) => (n.to_string(), Some(t.to_string())),
                    None => (rest, None),
                };
                load_zone(&mut zones, &ff)?;
                let model = zhbo_formats::xmodel::find(&zones[&ff], &name).map_err(|e| e.to_string())?;
                let textures = color_maps(&model, &paks);
                models.push((model, textures, tag));
            }
            let refs: Vec<(&zhbo_formats::xmodel::Model, &preview::Textures, Option<&str>)> =
                models.iter().map(|(m, x, t)| (m, x, t.as_deref())).collect();
            let meshes = preview::pose(&refs, &anim, frame);
            preview::render_meshes(&meshes, Path::new(out)).map_err(|e| e.to_string())?;
            println!("wrote {out}");
            Ok(())
        }
        "walk" => {
            // Fully load zones (every asset, every pointer) and report.
            let names: Vec<String> = if args.len() > 1 {
                args[1..].to_vec()
            } else {
                let mut v: Vec<String> = fs::read_dir(bo2_dir()?.join("zone").join("all"))
                    .map_err(|e| e.to_string())?
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.ends_with(".ff"))
                    .collect();
                v.sort();
                v
            };
            let mut failed = 0;
            for name in names {
                let zone = open_zone(&name)?;
                match zhbo_formats::t6zone::load(&zone) {
                    Ok(z) => println!("{name:<36} ok   {:>6} assets, {} bytes left over", z.assets.len(), z.trailing_bytes),
                    Err(e) => {
                        failed += 1;
                        println!("{name:<36} FAIL {e}");
                    }
                }
            }
            if failed > 0 {
                return Err(format!("{failed} zones failed"));
            }
            Ok(())
        }
        "anim" => {
            // anim <ff> <name>: tracks per bone.
            let zone = load_full_zone(args.get(1).ok_or("anim <ff> <name>")?)?;
            let anim = zhbo_formats::xanim::find(&zone, args.get(2).ok_or("anim <ff> <name>")?).map_err(|e| e.to_string())?;
            println!("{}: {} frames at {} fps, loop {}, {} bones", anim.name, anim.num_frames, anim.frame_rate, anim.looping, anim.bones.len());
            for b in &anim.bones {
                let r = b.rotation.as_ref().map_or("-".to_string(), |t| format!("{} keys, first {:.3?}", t.len(), t[0].1));
                let t = b.translation.as_ref().map_or("-".to_string(), |t| format!("{} keys, first {:.2?}", t.len(), t[0].1));
                println!("  {:<24} rot {r:<44} pos {t}", b.name);
            }
            for n in &anim.notifies {
                println!("  notify {:.3} {}", n.time, n.name);
            }
            Ok(())
        }
        "dump" => {
            // Hex dump around each occurrence of a string in a zone, for format work.
            let zone = open_zone(args.get(1).ok_or("dump <ff> <text> [before] [after]")?)?;
            let needle = args.get(2).ok_or("dump <ff> <text> [before] [after]")?.as_bytes();
            let before: usize = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(256);
            let after: usize = args.get(4).and_then(|v| v.parse().ok()).unwrap_or(64);
            let d = &zone.data;
            let mut start = 0;
            while let Some(rel) = d[start..].windows(needle.len()).position(|w| w == needle) {
                let at = start + rel;
                start = at + 1;
                println!("== at {at:#x}");
                let from = at.saturating_sub(before) & !15;
                for row in (from..(at + after).min(d.len())).step_by(16) {
                    let bytes = &d[row..(row + 16).min(d.len())];
                    let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02x}")).collect();
                    let text: String = bytes.iter().map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '.' }).collect();
                    println!("{row:08x}  {:<47}  {text}", hex.join(" "));
                }
            }
            Ok(())
        }
        "find" => {
            let needle = args.get(1).ok_or("find <text> [zone.ff ...]")?.to_ascii_lowercase();
            let zones: Vec<PathBuf> = if args.len() > 2 {
                args[2..].iter().map(|z| resolve(z, &["zone", "all"])).collect::<Result<_, _>>()?
            } else {
                let mut v: Vec<PathBuf> = fs::read_dir(bo2_dir()?.join("zone").join("all"))
                    .map_err(|e| e.to_string())?
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "ff"))
                    .collect();
                v.sort();
                v
            };
            for path in zones {
                let zone = match fs::read(&path).map_err(|e| e.to_string()).and_then(|f| fastfile::read(&f).map_err(|e| e.to_string())) {
                    Ok(z) => z,
                    Err(e) => {
                        eprintln!("{}: {e}", path.display());
                        continue;
                    }
                };
                let mut seen = std::collections::BTreeSet::new();
                for s in zone.data.split(|&b| b == 0) {
                    if s.len() >= 4 && s.iter().all(|&b| (0x20..0x7f).contains(&b)) {
                        let text = String::from_utf8_lossy(s);
                        if text.to_ascii_lowercase().contains(&needle) && seen.insert(text.to_string()) {
                            println!("{}: {text}", zone.name);
                        }
                    }
                }
            }
            Ok(())
        }
        "list" => {
            let bank = open_bank(args.get(1).ok_or("list <bank>")?)?;
            println!("{:>10}  {:>7}  {:>2}  {:>6}  format", "id", "seconds", "ch", "rate");
            for e in bank.entries() {
                let fmt = match e.format {
                    FORMAT_FLAC => "flac",
                    FORMAT_PCM16 => "pcm16",
                    _ => "other",
                };
                println!("{:#010x}  {:>7.2}  {:>2}  {:>6}  {fmt}", e.id, e.duration_secs().unwrap_or(0.0), e.channels, e.sample_rate().unwrap_or(0));
            }
            Ok(())
        }
        "export" => {
            let bank = open_bank(args.get(1).ok_or("export <bank> <out-dir>")?)?;
            let out = PathBuf::from(args.get(2).ok_or("export <bank> <out-dir>")?);
            let min: f32 = flag(args, "--min").and_then(|v| v.parse().ok()).unwrap_or(0.0);
            let max: f32 = flag(args, "--max").and_then(|v| v.parse().ok()).unwrap_or(f32::INFINITY);
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            let mut n = 0;
            for e in bank.entries() {
                let d = e.duration_secs().unwrap_or(0.0);
                if d < min || d > max {
                    continue;
                }
                if let Some(wav) = sounds::entry_to_wav(&bank, e)? {
                    fs::write(out.join(format!("{:08x}.wav", e.id)), wav).map_err(|err| err.to_string())?;
                    n += 1;
                }
            }
            println!("exported {n} entries to {}", out.display());
            Ok(())
        }
        "images" => {
            let pak = open_pak(args.get(1).ok_or("images <pak>")?)?;
            let filter = Filter::from_args(args);
            for (hash, parts) in images::group_by_name(&pak) {
                if let Some(h) = images::header(&pak, &parts).filter(|h| filter.accepts(h)) {
                    println!("{hash:08x}  {:?}  {}x{}  parts {}", h.format, h.width, h.height, parts.len());
                }
            }
            Ok(())
        }
        "sheets" => {
            let pak = open_pak(args.get(1).ok_or("sheets <pak> <out-dir>")?)?;
            let out = PathBuf::from(args.get(2).ok_or("sheets <pak> <out-dir>")?);
            let filter = Filter::from_args(args);
            let per: usize = flag(args, "--per-sheet").and_then(|v| v.parse().ok()).unwrap_or(64);
            let cell: u32 = flag(args, "--cell").and_then(|v| v.parse().ok()).unwrap_or(96);
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            let mut batch = Vec::new();
            let mut index = String::new();
            let mut sheet_no = 0;
            let groups = images::group_by_name(&pak);
            let flush = |batch: &mut Vec<_>, index: &mut String, sheet_no: &mut usize| -> Result<(), String> {
                if batch.is_empty() {
                    return Ok(());
                }
                let s = images::sheet(batch, cell, 8);
                images::write_png(&out.join(format!("sheet_{sheet_no:03}.png")), &s)?;
                fs::write(out.join(format!("sheet_{sheet_no:03}.txt")), &*index).map_err(|e| e.to_string())?;
                batch.clear();
                index.clear();
                *sheet_no += 1;
                Ok(())
            };
            for (hash, parts) in &groups {
                if !filter.wants_hash(*hash) {
                    continue;
                }
                let Some(h) = images::header(&pak, parts).filter(|h| filter.accepts(h)) else { continue };
                let Ok(img) = images::load(&pak, parts) else { continue };
                index.push_str(&format!("{:3} {hash:08x} {:?} {}x{}\n", batch.len(), h.format, h.width, h.height));
                batch.push(img);
                if batch.len() == per {
                    flush(&mut batch, &mut index, &mut sheet_no)?;
                }
            }
            flush(&mut batch, &mut index, &mut sheet_no)?;
            println!("wrote {sheet_no} sheets to {} (cells are numbered left-to-right, 8 per row)", out.display());
            Ok(())
        }
        "image" => {
            let pak = open_pak(args.get(1).ok_or("image <pak> <hash> <out.png>")?)?;
            let hash = u32::from_str_radix(args.get(2).ok_or("image <pak> <hash> <out.png>")?.trim_start_matches("0x"), 16)
                .map_err(|e| e.to_string())?;
            let out = PathBuf::from(args.get(3).ok_or("image <pak> <hash> <out.png>")?);
            let groups = images::group_by_name(&pak);
            let parts = groups.get(&hash).ok_or("no image with that hash")?;
            images::write_png(&out, &images::load(&pak, parts)?)
        }
        _ => Err("usage: zhbo-assets <banks|list|export|paks|images|sheets|image> ...".into()),
    }
}

fn open_bank(name: &str) -> Result<SoundBank, String> {
    let path = resolve(name, &["sound"])?;
    SoundBank::open(&path).map_err(|e| format!("{}: {e}", path.display()))
}

type Paks = Vec<(String, Ipak, std::collections::BTreeMap<u32, Vec<zhbo_formats::ipak::IpakEntry>>)>;

/// The colour map of each surface's material, decoded from the image packs.
fn color_maps(model: &zhbo_formats::xmodel::Model, paks: &Paks) -> preview::Textures {
    model
        .materials
        .iter()
        .map(|m| {
            let img = m.as_ref()?.color_map()?;
            let (_, pak, groups) = paks.iter().find(|(_, _, g)| g.contains_key(&img.stream_hash))?;
            images::load(pak, &groups[&img.stream_hash]).ok().map(std::rc::Rc::new)
        })
        .collect()
}

/// Every image pack with its entries grouped by name hash.
fn image_paks() -> Result<Paks, String> {
    let dir = bo2_dir()?.join("zone").join("all");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".ipak"))
        .collect();
    names.sort();
    let mut out = Vec::new();
    for n in names {
        let pak = open_pak(&n)?;
        let groups = images::group_by_name(&pak);
        out.push((n, pak, groups));
    }
    Ok(out)
}

/// Fully loaded zones, by file name.
type ZoneCache = std::collections::HashMap<String, zhbo_formats::t6zone::Zone>;

fn load_zone(zones: &mut ZoneCache, name: &str) -> Result<(), String> {
    if !zones.contains_key(name) {
        zones.insert(name.to_string(), load_full_zone(name)?);
    }
    Ok(())
}

/// Decrypt a fastfile and load every asset in it.
fn load_full_zone(name: &str) -> Result<zhbo_formats::t6zone::Zone, String> {
    let zone = open_zone(name)?;
    zhbo_formats::t6zone::load(&zone).map_err(|e| format!("{name}: {e}"))
}

fn open_zone(name: &str) -> Result<fastfile::Zone, String> {
    let path = resolve(name, &["zone", "all"])?;
    let file = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    fastfile::read(&file).map_err(|e| format!("{}: {e}", path.display()))
}

fn open_pak(name: &str) -> Result<Ipak, String> {
    let path = resolve(name, &["zone", "all"])?;
    Ipak::open(&path).map_err(|e| format!("{}: {e}", path.display()))
}
