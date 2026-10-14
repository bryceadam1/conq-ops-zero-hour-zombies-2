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
//! ```
//! `<bank>`/`<pak>` are file names inside the BO2 install (`sound\`, `zone\all\`) or full paths.
//! Everything exported is the player's own game data: keep it local, never redistribute it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use zhbo_assets::{images, sounds};
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

fn open_pak(name: &str) -> Result<Ipak, String> {
    let path = resolve(name, &["zone", "all"])?;
    Ipak::open(&path).map_err(|e| format!("{}: {e}", path.display()))
}
