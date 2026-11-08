//! Decoding by options against the token mask (docs/COMMANDS.md section 11, "Option scoring"):
//!
//! ```text
//! cargo run --release -p zhbo-voice --example voice_options -- <model dir> <data dir> [rows]
//! ```
//!
//! Over the held-out typed rows: whether our T5 network's free greedy output equals candle's (the
//! port's check), then the meaning right and the time per command for the token mask, options
//! with a beam of 1 and of 4, and how the weakest chosen option's probability tells right answers
//! from wrong ones (the "unknown" signal).

mod common;

use std::path::Path;
use std::time::Instant;

use common::same_meaning;
use zhbo_voice::options::OptionDecoder;
use zhbo_voice::Translator;

fn main() -> Result<(), zhbo_voice::Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return Err("usage: voice_options <model dir> <data dir> [rows]".into());
    }
    let (model_dir, data) = (Path::new(&args[0]), Path::new(&args[1]));
    let limit: usize = args.get(2).and_then(|n| n.parse().ok()).unwrap_or(300);
    let vocabs = common::vocabularies(data)?;
    let all = common::test_rows(data, usize::MAX)?;
    let stride = (all.len() / limit.max(1)).max(1);
    let rows: Vec<_> = all.into_iter().step_by(stride).take(limit).collect();
    let mut mask = Translator::load(model_dir, candle_core::Device::Cpu)?;
    let opts = OptionDecoder::load(model_dir, candle_core::Device::Cpu)?;
    let (mut same_net, mut n) = (0usize, 0usize);
    let mut right = [0usize; 5];
    let mut took = [0f64; 5];
    // The beam-4 answers' weakest option probability, and whether each was right.
    let mut weakest: Vec<(f32, bool)> = Vec::new();
    let mut lost: Vec<String> = Vec::new();
    for (input, label, id) in &rows {
        let Some(v) = vocabs.get(id) else { continue };
        n += 1;
        let free = mask.translate(input, v, false)?.text;
        same_net += (opts.greedy(input, v)? == free) as usize;
        let t = Instant::now();
        let masked = mask.translate(input, v, true)?.text;
        took[0] += t.elapsed().as_secs_f64();
        right[0] += same_meaning(&masked, label, v) as usize;
        for (k, beam) in [(1usize, 1usize), (2, 4)] {
            let t = Instant::now();
            let s = opts.decode(input, v, beam, 0.0)?;
            took[k] += t.elapsed().as_secs_f64();
            let ok = same_meaning(&s.text, label, v);
            right[k] += ok as usize;
            if beam == 4 {
                weakest.push((s.weakest, ok));
                if !ok && same_meaning(&masked, label, v) && lost.len() < 10 {
                    lost.push(format!("  {input}\n    want {label}\n    mask {masked}\n    opts {} (weakest {:.2})", s.text, s.weakest));
                }
            }
        }
        for (k, beam) in [(3usize, 1usize), (4, 4)] {
            let t = Instant::now();
            let s = opts.decode_tokens(input, v, beam)?;
            took[k] += t.elapsed().as_secs_f64();
            right[k] += same_meaning(&s.text, label, v) as usize;
        }
    }
    let d = n.max(1) as f64;
    println!("{n} held-out rows, CPU");
    println!("our network's greedy equals candle's: {same_net} of {n}");
    for (k, name) in ["token mask", "options, beam 1", "options, beam 4", "tokens, beam 1", "tokens, beam 4"].iter().enumerate() {
        println!("{name:>16}: same meaning {:.3}, {:.0} ms per command", right[k] as f64 / d, took[k] / d * 1000.0);
    }
    let (r, w) = (weakest.iter().filter(|x| x.1).count().max(1) as f64, weakest.iter().filter(|x| !x.1).count().max(1) as f64);
    for t in [0.05f32, 0.1, 0.2, 0.3, 0.5] {
        let a = weakest.iter().filter(|x| x.1 && x.0 < t).count() as f64 / r;
        let b = weakest.iter().filter(|x| !x.1 && x.0 < t).count() as f64 / w;
        println!("unknown below {t:.2}: on {a:.3} of right answers, {b:.3} of wrong ones");
    }
    println!("right by the mask, wrong by options (first 10):\n{}", lost.join("\n"));
    Ok(())
}
