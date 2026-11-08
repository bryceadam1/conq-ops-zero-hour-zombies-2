//! The fine-tuned translator against the held-out set, unconstrained and under the grammar's mask
//! (docs/COMMANDS.md section 11):
//!
//! ```text
//! cargo run --release -p zhbo-voice --example voice_eval -- <model dir> <data dir> [rows]
//! ```
//!
//! Reads `<data dir>/test.tsv` (`input`, label, id) and `<data dir>/vocab.tsv` (each id's
//! vocabulary), decodes each input greedily on the CPU both ways, and reports exact match, how
//! often the unconstrained output is not even a command, the mask's confidence signal on right and
//! wrong answers, the time per command, every label the mask would refuse (there must be none), and
//! the first errors. Same meaning counts `a and b` as `b and a`.

mod common;

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use common::same_meaning;
use zhbo_core::command::{parse, Vocabulary};
use zhbo_voice::Translator;

/// Where the grammar's mask refuses `label`'s own tokens, if it does: every right answer must get
/// through, or the mask costs accuracy instead of adding it.
fn refusal(model: &Translator, label: &str, v: &Vocabulary) -> Result<Option<String>, zhbo_voice::Error> {
    let tok = model.tokenizer();
    let eos = tok.token_to_id("</s>").ok_or("the tokenizer has no </s>")?;
    let mut mask = zhbo_voice::constrain::Mask::new(v, tok)?;
    for (i, &t) in tok.encode(label, false)?.get_ids().iter().enumerate() {
        if !mask.allowed(eos)?.contains(&t) {
            return Ok(Some(format!("token {i} {:?}", tok.id_to_token(t).unwrap_or_default())));
        }
        mask.advance(t)?;
    }
    Ok((!mask.allowed(eos)?.contains(&eos)).then(|| "the end".to_string()))
}

fn main() -> Result<(), zhbo_voice::Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return Err("usage: voice_eval <model dir> <data dir> [rows]".into());
    }
    let data = Path::new(&args[1]);
    let limit: usize = args.get(2).and_then(|n| n.parse().ok()).unwrap_or(usize::MAX);
    let vocabs: BTreeMap<String, Vocabulary> = common::vocabularies(data)?;
    let rows = common::test_rows(data, limit)?;
    let mut model = Translator::load(Path::new(&args[0]), candle_core::Device::Cpu)?;
    let (mut free_right, mut free_invalid, mut masked_right, mut stuck) = (0usize, 0usize, 0usize, 0usize);
    let (mut free_same, mut masked_same) = (0usize, 0usize);
    let (mut free_time, mut masked_time) = (0.0f64, 0.0f64);
    let (mut mass_right, mut mass_wrong, mut n_wrong) = (0.0f32, 0.0f32, 0usize);
    let mut errors: Vec<String> = Vec::new();
    let mut refused: Vec<String> = Vec::new();
    // Each constrained answer's removed share and whether it meant the label.
    let mut guarded: Vec<(f32, bool)> = Vec::new();
    for (input, label, id) in &rows {
        let Some(v) = vocabs.get(id) else { continue };
        let t = Instant::now();
        let free = model.translate(input, None)?;
        free_time += t.elapsed().as_secs_f64();
        if free.text == *label {
            free_right += 1;
        }
        free_same += same_meaning(&free.text, label, v) as usize;
        if parse(&free.text, v).is_err() {
            free_invalid += 1;
        }
        let t = Instant::now();
        let masked = model.translate(input, Some(v))?;
        masked_time += t.elapsed().as_secs_f64();
        stuck += masked.stuck as usize;
        let same = same_meaning(&masked.text, label, v);
        masked_same += same as usize;
        guarded.push((masked.masked_max, same));
        if let Some(at) = refusal(&model, label, v)? {
            refused.push(format!("  {label}\n    refused at {at}"));
        }
        if masked.text == *label {
            masked_right += 1;
            mass_right += masked.masked_max;
        } else if !same {
            n_wrong += 1;
            mass_wrong += masked.masked_max;
            if errors.len() < 30 {
                errors.push(format!("  {input}\n    want {label}\n    got  {} (masked {:.2})", masked.text, masked.masked_max));
            }
        }
    }
    let n = rows.len().max(1) as f64;
    println!("{} test rows, greedy, CPU", rows.len());
    println!(
        "unconstrained: exact match {:.4}, same meaning {:.4}, not a command {:.4}, {:.1} ms per command",
        free_right as f64 / n,
        free_same as f64 / n,
        free_invalid as f64 / n,
        free_time / n * 1000.0
    );
    println!("constrained:   exact match {:.4}, same meaning {:.4}, stuck {}, {:.1} ms per command", masked_right as f64 / n, masked_same as f64 / n, stuck, masked_time / n * 1000.0);
    println!(
        "mask's largest removed share: {:.3} on right answers, {:.3} on wrong ones",
        mass_right / masked_right.max(1) as f32,
        mass_wrong / n_wrong.max(1) as f32
    );
    // The confidence guard (section 9): above the threshold the command asks before it runs.
    let (n_same, n_other) = (guarded.iter().filter(|g| g.1).count().max(1) as f64, guarded.iter().filter(|g| !g.1).count().max(1) as f64);
    for threshold in [0.05f32, 0.1, 0.2, 0.3, 0.5] {
        let asks_right = guarded.iter().filter(|g| g.1 && g.0 > threshold).count() as f64 / n_same;
        let asks_wrong = guarded.iter().filter(|g| !g.1 && g.0 > threshold).count() as f64 / n_other;
        println!("guard above {threshold:.2}: asks on {:.3} of right answers, {:.3} of wrong ones", asks_right, asks_wrong);
    }
    println!("labels the mask refuses: {}", refused.len());
    for r in refused.iter().take(10) {
        println!("{r}");
    }
    println!("constrained errors in meaning (first 30):\n{}", errors.join("\n"));
    Ok(())
}
