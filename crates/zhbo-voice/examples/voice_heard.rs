//! Training rows as the game will hear them (docs/COMMANDS.md section 11): spoken training inputs
//! run through Whisper, its prompt and the snapper, labelled with their command.
//!
//! ```text
//! cargo run --release -p zhbo-voice --example voice_heard -- <whisper dir> <data dir> [rows] [seed] [out] [renamed]
//! ```
//!
//! macOS only. Takes `rows` of `<data dir>/train.tsv` spread through it (with `renamed`, only the
//! English-renamed copies), speaks each with one of the voices `voice_listen` does not test with,
//! at a speaking rate drawn from 140–230 words a minute, snaps it with the data's `words.txt`, and
//! writes `<data dir>/<out>` (default `heard.tsv`: `heard`, label, id); `train_t5.py` adds every
//! `heard*.tsv` to the training rows. Only rows whose transcript still has every name of the label
//! are kept: where Whisper lost a name, the row would teach the model to make one up.
//! `voice_heard filter <data dir>` applies that to the `heard*.tsv` already there. The translator then learns Whisper's habits (`nine` for 9, `ok`, a name a letter
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

/// Whether `heard` has every name of `label` whole.
fn names_kept(heard: &str, label: &str, vocab: &zhbo_core::command::Vocabulary) -> bool {
    let padded = format!(" {} ", zhbo_core::command::normal::words(heard).join(" "));
    zhbo_core::command::sample::names_in(label, vocab).iter().all(|n| padded.contains(&format!(" {n} ")))
}

fn main() -> Result<(), zhbo_voice::Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "filter") {
        let data = Path::new(args.get(1).ok_or("usage: voice_heard filter <data dir>")?);
        let vocabs = common::vocabularies(data)?;
        for entry in std::fs::read_dir(data)? {
            let path = entry?.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !(name.starts_with("heard") && name.ends_with(".tsv")) {
                continue;
            }
            let text = std::fs::read_to_string(&path)?;
            let kept: Vec<&str> = text
                .lines()
                .filter(|l| {
                    let f: Vec<&str> = l.split('\t').collect();
                    f.len() >= 3 && vocabs.get(f[2]).is_some_and(|v| names_kept(f[0], f[1], v))
                })
                .collect();
            eprintln!("{name}: kept {} of {}", kept.len(), text.lines().count());
            std::fs::write(&path, kept.join("\n") + "\n")?;
        }
        return Ok(());
    }
    if args.len() < 2 {
        return Err("usage: voice_heard <whisper dir> <data dir> [rows] [seed] [out] [renamed]".into());
    }
    let data = Path::new(&args[1]);
    let limit: usize = args.get(2).and_then(|n| n.parse().ok()).unwrap_or(6000);
    let mut seed: u32 = args.get(3).and_then(|n| n.parse().ok()).unwrap_or(20261108);
    let out_name = args.get(4).map_or("heard.tsv", String::as_str);
    let renamed_only = args.get(5).is_some_and(|a| a == "renamed");
    let vocabs = common::vocabularies(data)?;
    let all: Vec<(String, String, String)> = std::fs::read_to_string(data.join("train.tsv"))?
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            // The canonical lines are inputs too; a spoken paraphrase is what a player says.
            (f.len() >= 3 && f[0] != f[1] && (!renamed_only || f[2].ends_with('e'))).then(|| (f[0].to_string(), f[1].to_string(), f[2].to_string()))
        })
        .collect();
    let stride = (all.len() / limit.max(1)).max(1);
    let mut ears = Listener::load(Path::new(&args[0]), candle_core::Device::Cpu)?;
    let words = std::fs::read_to_string(data.join("words.txt"))?;
    let snapper = Snapper::new(words.lines().map(str::to_string), &Tables::parse(&[normal::SHIPPED]));
    let scratch = std::env::temp_dir();
    let mut out = std::io::BufWriter::new(std::fs::File::create(data.join(out_name))?);
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
        // A row `say` cannot speak (a stray control character in a paraphrase) is skipped.
        let pcm = match common::speak(input, voice, Some(rate), &scratch) {
            Ok(pcm) => pcm,
            Err(e) => {
                eprintln!("skipped '{}': {e}", input.escape_debug());
                continue;
            }
        };
        let heard = snapper.snap(&hear::tidy(&ears.hear(&pcm, &hear::prompt(v))?.text), v);
        if heard.is_empty() || heard.contains('\t') || !names_kept(&heard, label, v) {
            continue;
        }
        writeln!(out, "{heard}\t{label}\t{id}")?;
        written += 1;
        if written.is_multiple_of(500) {
            eprintln!("{written} rows");
            out.flush()?;
        }
    }
    eprintln!("wrote {written} rows to {}", data.join(out_name).display());
    Ok(())
}
