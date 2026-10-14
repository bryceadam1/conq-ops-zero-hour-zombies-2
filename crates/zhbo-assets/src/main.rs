//! zhbo-assets - work with Black Ops II sound banks in the player's own install.
//!
//! ```text
//! zhbo-assets banks                               list sound banks
//! zhbo-assets list <bank>                         entries: id, duration, channels, rate
//! zhbo-assets export <bank> <out-dir> [--min S] [--max S]
//!                                                 decode entries (optionally by duration) to WAV
//! ```
//! `<bank>` is a file name in `sound\` such as `mpl_common.all.sabs`.
//!
//! BO2 banks key entries by a hash only; the names live in the fastfiles' sound alias tables, which
//! are encrypted, and this tool does not decrypt them. Export to a private folder, listen, and note the
//! ids you want. Exported audio is the player's own game data: keep it local, never redistribute it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
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

fn sound_dir() -> Result<PathBuf, String> {
    let bo2 = zhbo_requirements::check_requirements().black_ops2?;
    Ok(bo2.path.join("sound"))
}

fn run(args: &[String]) -> Result<(), String> {
    let cmd = args.first().map(String::as_str).unwrap_or("");
    match cmd {
        "banks" => {
            let mut names: Vec<String> = fs::read_dir(sound_dir()?)
                .map_err(|e| e.to_string())?
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            for n in names {
                println!("{n}");
            }
            Ok(())
        }
        "list" => {
            let bank = open_bank(args.get(1).ok_or("list <bank>")?)?;
            println!("{:>10}  {:>7}  {:>2}  {:>6}  {}", "id", "seconds", "ch", "rate", "format");
            for e in bank.entries() {
                println!(
                    "{:#010x}  {:>7.2}  {:>2}  {:>6}  {}",
                    e.id,
                    e.duration_secs().unwrap_or(0.0),
                    e.channels,
                    e.sample_rate().unwrap_or(0),
                    match e.format {
                        FORMAT_FLAC => "flac",
                        FORMAT_PCM16 => "pcm16",
                        _ => "other",
                    }
                );
            }
            Ok(())
        }
        "export" => {
            let bank = open_bank(args.get(1).ok_or("export <bank> <out-dir>")?)?;
            let out = PathBuf::from(args.get(2).ok_or("export <bank> <out-dir>")?);
            let min = flag(args, "--min").unwrap_or(0.0);
            let max = flag(args, "--max").unwrap_or(f32::INFINITY);
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            let mut n = 0;
            for e in bank.entries() {
                let d = e.duration_secs().unwrap_or(0.0);
                if d < min || d > max {
                    continue;
                }
                let data = bank.read(e).map_err(|err| err.to_string())?;
                let wav = match e.format {
                    FORMAT_FLAC => flac_to_wav(&data)?,
                    FORMAT_PCM16 => wav_bytes(&data, e.channels as u16, e.sample_rate().unwrap_or(48000)),
                    _ => continue,
                };
                fs::write(out.join(format!("{:08x}.wav", e.id)), wav).map_err(|err| err.to_string())?;
                n += 1;
            }
            println!("exported {n} entries to {}", out.display());
            Ok(())
        }
        _ => Err("usage: zhbo-assets <banks|list|export> ...".into()),
    }
}

fn flag(args: &[String], name: &str) -> Option<f32> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok())
}

fn open_bank(name: &str) -> Result<SoundBank, String> {
    let path = if Path::new(name).is_file() { PathBuf::from(name) } else { sound_dir()?.join(name) };
    SoundBank::open(&path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Decode a FLAC stream to 16-bit PCM WAV (Zero Hour's Miles audio plays 16-bit WAV).
pub fn flac_to_wav(flac: &[u8]) -> Result<Vec<u8>, String> {
    let mut reader = claxon::FlacReader::new(std::io::Cursor::new(flac)).map_err(|e| e.to_string())?;
    let info = reader.streaminfo();
    let channels = info.channels as u16;
    let rate = info.sample_rate;
    let shift = info.bits_per_sample as i32 - 16;
    let mut pcm: Vec<u8> = Vec::new();
    for s in reader.samples() {
        let s = s.map_err(|e| e.to_string())?;
        let v = if shift >= 0 { s >> shift } else { s << -shift };
        pcm.extend_from_slice(&(v.clamp(i16::MIN as i32, i16::MAX as i32) as i16).to_le_bytes());
    }
    Ok(wav_bytes(&pcm, channels, rate))
}

pub fn wav_bytes(pcm: &[u8], channels: u16, rate: u32) -> Vec<u8> {
    let mut w = Vec::with_capacity(44 + pcm.len());
    let block = channels * 2;
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&channels.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * block as u32).to_le_bytes());
    w.extend_from_slice(&block.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(pcm);
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_is_well_formed() {
        let w = wav_bytes(&[0, 0, 1, 0], 1, 48000);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(w[4..8].try_into().unwrap()), 36 + 4);
        assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), 48000);
        assert_eq!(w.len(), 48);
    }
}
