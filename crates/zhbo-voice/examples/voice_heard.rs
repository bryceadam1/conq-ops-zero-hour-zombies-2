//! Training rows as the game will hear them (docs/COMMANDS.md section 11): spoken training inputs
//! run through Whisper, its prompt and the snapper, labelled with their command.
//!
//! ```text
//! cargo run --release -p zhbo-voice --example voice_heard -- <whisper dir> <model dir> <data dir> [rows] [seed]
//! ```
//!
//! macOS only. Takes `rows` of `<data dir>/train.tsv` spread through it, speaks each with one of
//! the voices `voice_listen` does not test with, at a speaking rate drawn from 140–230 words a
//! minute, and writes `<data dir>/heard.tsv` (`heard`, label, id), which `train_t5.py` adds to the
//! training rows. The translator then learns Whisper's habits (`nine` for 9, `ok`, a name a letter
//! off that the snapper left) instead of meeting them first in a match.

mod common;

use std::io::Write;
use std::path::Path;

use zhbo_core::command::normal::{self, Tables};
use zhbo_voice::hear::{self, Listener};
use zhbo_voice::snap::Snapper;

/// The voices for training, none of `voice_listen`'s.
const VOICES: [&str; 14] = [
    "Eddy (English (UK))",
    "Eddy (English (US))",
    "Flo (English (UK))",
    "Flo (English (US))",
    "Fred",
    "Grandma (English (UK))",
    "Grandpa (English (US))",
    "Kathy",
    "Ralph",
    "Reed (English (UK))",
    "Rocko (English (UK))",
    "Rocko (English (US))",
    "Sandy (English (US))",
    "Shelley (English (UK))",
];

fn main() -> Result<(), zhbo_voice::Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        return Err("usage: voice_heard <whisper dir> <model dir> <data dir> [rows] [seed]".into());
    }
    let data = Path::new(&args[2]);
    let limit: usize = args.get(3).and_then(|n| n.parse().ok()).unwrap_or(6000);
    let mut seed: u32 = args.get(4).and_then(|n| n.parse().ok()).unwrap_or(20261108);
    let vocabs = common::vocabularies(data)?;
    let all: Vec<(String, String, String)> = std::fs::read_to_string(data.join("train.tsv"))?
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            // The canonical lines are inputs too; a spoken paraphrase is what a player says.
            (f.len() >= 3 && f[0] != f[1]).then(|| (f[0].to_string(), f[1].to_string(), f[2].to_string()))
        })
        .collect();
    let stride = (all.len() / limit.max(1)).max(1);
    let mut ears = Listener::load(Path::new(&args[0]), candle_core::Device::Cpu)?;
    let words = std::fs::read_to_string(Path::new(&args[1]).join("words.txt"))?;
    let snapper = Snapper::new(words.lines().map(str::to_string), &Tables::parse(&[normal::SHIPPED]));
    let scratch = std::env::temp_dir();
    let mut out = std::io::BufWriter::new(std::fs::File::create(data.join("heard.tsv"))?);
    let mut next = || {
        // xorshift: the voice and rate for each row, reproducible from the seed.
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let mut written = 0usize;
    for (input, label, id) in all.iter().step_by(stride).take(limit) {
        let Some(v) = vocabs.get(id) else { continue };
        let voice = VOICES[next() as usize % VOICES.len()];
        let rate = 140 + next() % 91;
        let pcm = common::speak(input, voice, Some(rate), &scratch)?;
        let heard = snapper.snap(&hear::tidy(&ears.hear(&pcm, &hear::prompt(v))?.text), v);
        if heard.is_empty() || heard.contains('\t') {
            continue;
        }
        writeln!(out, "{heard}\t{label}\t{id}")?;
        written += 1;
        if written.is_multiple_of(500) {
            eprintln!("{written} rows");
            out.flush()?;
        }
    }
    eprintln!("wrote {written} rows to {}", data.join("heard.tsv").display());
    Ok(())
}
