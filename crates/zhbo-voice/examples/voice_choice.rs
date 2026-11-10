//! Decoding by multiple choice where the generator is unsure (docs/COMMANDS.md section 11,
//! "Multiple choice"):
//!
//! ```text
//! cargo run --release -p zhbo-voice --example voice_choice -- <model dir> <choice model dir> <data dir> [rows]
//! ```
//!
//! Over the held-out typed rows, for several thresholds (the generator's top option's share under
//! which it asks; 0 never asks, above 1 always asks): the meaning right, how often it asked and how
//! often the answer overruled the generator, and the time per command. The choice model reads
//! the question as written (its `prompt.txt` is `{heard}`).

mod common;

use std::path::Path;
use std::time::Instant;

use common::same_meaning;
use zhbo_voice::options::OptionDecoder;

fn main() -> Result<(), zhbo_voice::Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        return Err("usage: voice_choice <model dir> <choice model dir> <data dir> [rows]".into());
    }
    let data = Path::new(&args[2]);
    let limit: usize = args.get(3).and_then(|n| n.parse().ok()).unwrap_or(200);
    let vocabs = common::vocabularies(data)?;
    let all = common::test_rows(data, usize::MAX)?;
    let stride = (all.len() / limit.max(1)).max(1);
    let rows: Vec<_> = all.into_iter().step_by(stride).take(limit).collect();
    let generator = OptionDecoder::load(Path::new(&args[0]), candle_core::Device::Cpu)?;
    let chooser = OptionDecoder::load(Path::new(&args[1]), candle_core::Device::Cpu)?;
    let thresholds = [0.0f32, 0.9, 0.99, 1.01];
    let mut right = [0usize; 4];
    let (mut asked, mut overruled, mut steps_won, mut steps_lost) = ([0usize; 4], [0usize; 4], [0usize; 4], [0usize; 4]);
    let mut took = [0f64; 4];
    let mut n = 0usize;
    for (input, label, id) in &rows {
        let Some(v) = vocabs.get(id) else { continue };
        n += 1;
        let base = generator.decode_choice(input, v, &chooser, 0.0)?;
        let base_ok = same_meaning(&base.text, label, v);
        for (k, &sure) in thresholds.iter().enumerate() {
            let t = Instant::now();
            let c = if k == 0 { base.clone() } else { generator.decode_choice(input, v, &chooser, sure)? };
            took[k] += t.elapsed().as_secs_f64();
            let ok = same_meaning(&c.text, label, v);
            right[k] += ok as usize;
            asked[k] += c.asked;
            overruled[k] += c.overruled;
            steps_won[k] += (ok && !base_ok) as usize;
            steps_lost[k] += (!ok && base_ok) as usize;
        }
    }
    let d = n.max(1) as f64;
    println!("{n} held-out rows, CPU");
    for (k, sure) in thresholds.iter().enumerate() {
        println!(
            "ask below {sure:.2}: same meaning {:.3}, {:.1} questions and {:.2} overruled per command, {} fixed and {} broken against never asking, {:.0} ms",
            right[k] as f64 / d,
            asked[k] as f64 / d,
            overruled[k] as f64 / d,
            steps_won[k],
            steps_lost[k],
            took[k] / d * 1000.0
        );
    }
    Ok(())
}
