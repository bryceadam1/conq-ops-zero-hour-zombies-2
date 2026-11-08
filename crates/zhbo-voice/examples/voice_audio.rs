//! Spoken commands as Whisper's encoder reads them, for training a model that goes from audio to
//! the command language without Whisper's decoder (docs/COMMANDS.md section 11, "Audio straight
//! to the translator"):
//!
//! ```text
//! cargo run --release -p zhbo-voice --example voice_audio -- <whisper dir> <data dir> <split> <rows> <out prefix> [seed] [shard/shards]
//! ```
//!
//! macOS only. `<split>` is `train`, `dev` or `test`. Train and dev rows (paraphrases, not the
//! canonical lines) are spoken by `say` in the training voices at drawn speaking rates; the test
//! rows are `voice_listen`'s (the same stride, voices and order), so the results compare. Each is
//! run through Whisper's encoder over the runtime's window, and its features (`[positions,
//! d_model]`) written as bf16 to `<out prefix>.bf16`, with `<out prefix>.tsv` holding each row's
//! input, label, vocabulary id, offset (in values) and positions. `shard/shards` (`0/3`) takes
//! every shards-th row from the shard-th, so several processes can share the work.

mod common;

use std::io::Write;
use std::path::Path;

use zhbo_voice::hear::Listener;

/// An `f32` as bf16 bits, rounded to nearest even.
fn bf16(x: f32) -> u16 {
    let bits = x.to_bits();
    let round = 0x7fff + ((bits >> 16) & 1);
    ((bits.wrapping_add(round)) >> 16) as u16
}

fn main() -> Result<(), zhbo_voice::Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 5 {
        return Err("usage: voice_audio <whisper dir> <data dir> <train|dev|test> <rows> <out prefix> [seed] [shard/shards]".into());
    }
    let data = Path::new(&args[1]);
    let split = args[2].as_str();
    let limit: usize = args[3].parse()?;
    let out = Path::new(&args[4]);
    let mut seed: u32 = args.get(5).and_then(|n| n.parse().ok()).unwrap_or(20261110);
    let (shard, shards) = args
        .get(6)
        .and_then(|s| s.split_once('/'))
        .and_then(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?)))
        .unwrap_or((0, 1));
    let test = split == "test";
    let all: Vec<(String, String, String)> = std::fs::read_to_string(data.join(format!("{split}.tsv")))?
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f.len() >= 3 && (test || f[0] != f[1])).then(|| (f[0].to_string(), f[1].to_string(), f[2].to_string()))
        })
        .collect();
    let stride = (all.len() / limit.max(1)).max(1);
    let rows: Vec<_> = all.into_iter().step_by(stride).take(limit).collect();
    let mut ears = Listener::load(Path::new(&args[0]), candle_core::Device::Cpu)?;
    let scratch = std::env::temp_dir().join(format!("voice_audio_{shard}"));
    std::fs::create_dir_all(&scratch)?;
    let suffix = if shards > 1 { format!(".{shard}") } else { String::new() };
    let mut values = std::io::BufWriter::new(std::fs::File::create(out.with_extension(format!("bf16{suffix}")))?);
    let mut index = std::io::BufWriter::new(std::fs::File::create(out.with_extension(format!("tsv{suffix}")))?);
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let (mut offset, mut written) = (0u64, 0usize);
    for (i, (input, label, id)) in rows.iter().enumerate() {
        // Every row draws its voice and rate, so a shard's draws match the single run's.
        let (voice, rate) = if test {
            (common::TEST_VOICES[i % common::TEST_VOICES.len()], None)
        } else {
            (common::TRAIN_VOICES[next() as usize % common::TRAIN_VOICES.len()], Some(140 + next() % 91))
        };
        if i % shards != shard {
            continue;
        }
        let pcm = match common::speak(input, voice, rate, &scratch) {
            Ok(pcm) => pcm,
            Err(e) => {
                eprintln!("skipped '{}': {e}", input.escape_debug());
                continue;
            }
        };
        let features = ears.encode(&pcm)?.squeeze(0)?;
        let (positions, width) = features.dims2()?;
        let flat = features.flatten_all()?.to_vec1::<f32>()?;
        let mut bytes = Vec::with_capacity(flat.len() * 2);
        for x in flat {
            bytes.extend_from_slice(&bf16(x).to_le_bytes());
        }
        values.write_all(&bytes)?;
        writeln!(index, "{input}\t{label}\t{id}\t{offset}\t{positions}")?;
        offset += (positions * width) as u64;
        written += 1;
        if written.is_multiple_of(500) {
            eprintln!("shard {shard}: {written} rows");
            values.flush()?;
            index.flush()?;
        }
    }
    eprintln!("shard {shard}: wrote {written} rows to {}", out.display());
    Ok(())
}
