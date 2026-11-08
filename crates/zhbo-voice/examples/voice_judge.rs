//! A model's predictions scored as the voice examples score them (docs/COMMANDS.md section 11):
//!
//! ```text
//! cargo run --release -p zhbo-voice --example voice_judge -- <data dir> <predictions tsv>
//! ```
//!
//! The predictions are `id`, label, prediction per line (`train_bridge.py`'s
//! `test_predictions.tsv`). Reports exact match, same meaning (`a and b` as `b and a`) and how many
//! are not commands at all.

mod common;

use std::path::Path;

use common::same_meaning;
use zhbo_core::command::parse;

fn main() -> Result<(), zhbo_voice::Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return Err("usage: voice_judge <data dir> <predictions tsv>".into());
    }
    let vocabs = common::vocabularies(Path::new(&args[0]))?;
    let (mut n, mut exact, mut same, mut invalid) = (0usize, 0usize, 0usize, 0usize);
    for line in std::fs::read_to_string(&args[1])?.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let (Some(id), Some(label), Some(pred)) = (f.first(), f.get(1), f.get(2)) else { continue };
        let Some(v) = vocabs.get(*id) else { continue };
        n += 1;
        exact += (pred == label) as usize;
        same += same_meaning(pred, label, v) as usize;
        invalid += parse(pred, v).is_err() as usize;
    }
    let d = n.max(1) as f64;
    println!("{n} predictions: exact match {:.3}, same meaning {:.3}, not a command {:.3}", exact as f64 / d, same as f64 / d, invalid as f64 / d);
    Ok(())
}
