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
            let zone = open_zone(args.get(1).ok_or(usage)?)?;
            let index = zone.index().map_err(|e| e.to_string())?;
            let name = args.get(2).ok_or(usage)?;
            let model = zhbo_formats::xmodel::find(&zone.data, &index.script_strings, name).map_err(|e| e.to_string())?;
            println!("{}: {} bones, {} surfaces, {} lods, bounds {:?}..{:?}", model.name, model.bones.len(), model.surfaces.len(), model.lods.len(), model.mins, model.maxs);
            for (i, l) in model.lods.iter().enumerate() {
                let surfs = &model.surfaces[l.first_surface..l.first_surface + l.surface_count];
                let tris: usize = surfs.iter().map(|s| s.triangles.len()).sum();
                let verts: usize = surfs.iter().map(|s| s.vertices.len()).sum();
                println!("  lod{i}: dist {:.0}, {} surfaces, {verts} vertices, {tris} triangles", l.distance, l.surface_count);
            }
            let names: Vec<&str> = model.bones.iter().take(12).map(|b| b.name.as_str()).collect();
            println!("  bones: {} ...", names.join(" "));
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
                preview::render(&model, Path::new(out)).map_err(|e| e.to_string())?;
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
            let (z, s) = &zones[&anim_ff];
            let anim = zhbo_formats::xanim::find(&z.data, s, &anim_name).map_err(|e| e.to_string())?;
            println!("{}: {} frames at {} fps, {} bones", anim.name, anim.num_frames, anim.frame_rate, anim.bones.len());
            let mut models = Vec::new();
            for spec in &args[4..] {
                let (ff, rest) = split(spec)?;
                let (name, tag) = match rest.split_once('@') {
                    Some((n, t)) => (n.to_string(), Some(t.to_string())),
                    None => (rest, None),
                };
                load_zone(&mut zones, &ff)?;
                let (z, s) = &zones[&ff];
                models.push((zhbo_formats::xmodel::find(&z.data, s, &name).map_err(|e| e.to_string())?, tag));
            }
            let refs: Vec<(&zhbo_formats::xmodel::Model, Option<&str>)> = models.iter().map(|(m, t)| (m, t.as_deref())).collect();
            let meshes = preview::pose(&refs, &anim, frame);
            preview::render_meshes(&meshes, Path::new(out)).map_err(|e| e.to_string())?;
            println!("wrote {out}");
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

/// Decrypted zones with their script strings, by file name.
type ZoneCache = std::collections::HashMap<String, (fastfile::Zone, Vec<String>)>;

fn load_zone(zones: &mut ZoneCache, name: &str) -> Result<(), String> {
    if !zones.contains_key(name) {
        let z = open_zone(name)?;
        let strings = z.index().map_err(|e| e.to_string())?.script_strings;
        zones.insert(name.to_string(), (z, strings));
    }
    Ok(())
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
