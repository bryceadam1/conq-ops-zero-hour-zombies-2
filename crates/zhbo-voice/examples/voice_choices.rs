//! Multiple-choice training rows (docs/COMMANDS.md section 11, "Multiple choice"):
//!
//! ```text
//! cargo run --release -p zhbo-voice --example voice_choices -- <data dir> [seed]
//! ```
//!
//! From `train.tsv` and `heard*.tsv` (to `choices_train.tsv`) and `dev.tsv` (to `choices_dev.tsv`):
//! each row's label is cut into the grammar's steps (`parse::phrases`); a step at random, and a
//! name's step when there is one, become a question (`choice::question`) whose answer is the
//! step's phrase. The wrong options are the step's real ones, the hardest first: an option one
//! of whose words-prefix is the other's (`the` and `the torwren crossing`), then the nearest in
//! sound (`snap::distance`), then any; 2 to 8 options, the answer at a random letter. Each line
//! is the question, the answer's letter and the vocabulary id; `train_t5.py` reads them as written.

mod common;

use std::io::Write;
use std::path::Path;

use zhbo_core::command::parse::{completions_with, phrases};
use zhbo_core::command::Vocabulary;
use zhbo_voice::choice::{question, LETTERS, MAX_OPTIONS};
use zhbo_voice::constrain::voice_numbers;
use zhbo_voice::snap::distance;

struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        self.next() as usize % n.max(1)
    }
}

/// Whether one phrase's words begin the other's.
fn overlaps(a: &str, b: &str) -> bool {
    let (x, y): (Vec<&str>, Vec<&str>) = (a.split(' ').collect(), b.split(' ').collect());
    let n = x.len().min(y.len());
    n > 0 && x[..n] == y[..n]
}

/// The question for step `at` of `steps`, or `None` if it had no choice.
fn ask(heard: &str, steps: &[String], at: usize, vocab: &Vocabulary, numbers: &[&str], rng: &mut Rng) -> Option<(String, &'static str)> {
    let c = completions_with(&steps[..at].join(" "), vocab, numbers);
    let answer = steps.get(at).cloned().unwrap_or_default();
    let mut pool: Vec<String> = c.next.iter().filter(|p| **p != answer).cloned().collect();
    if c.complete && !answer.is_empty() {
        pool.push(String::new());
    }
    if pool.is_empty() {
        return None;
    }
    // Hardest first: overlapping phrases, then by sound; a little shuffle among equals.
    let key = |p: &String| -> (u8, i64) {
        let o = !p.is_empty() && !answer.is_empty() && overlaps(p, &answer);
        let d = if p.is_empty() || answer.is_empty() { 99.0 } else { distance(p, &answer) / p.len().max(answer.len()) as f32 };
        (if o { 0 } else { 1 }, (d * 1000.0) as i64)
    };
    pool.sort_by_key(|p| key(p));
    let k = 2 + rng.below(MAX_OPTIONS - 1);
    let hard = (k - 1).min(3).min(pool.len());
    let mut wrong: Vec<String> = pool.drain(..hard).collect();
    while wrong.len() < k - 1 && !pool.is_empty() {
        let i = rng.below(pool.len());
        wrong.push(pool.swap_remove(i));
    }
    let slot = rng.below(wrong.len() + 1);
    let mut options = wrong;
    options.insert(slot, answer);
    Some((question(heard, &steps[..at], &options), LETTERS[slot]))
}

fn main() -> Result<(), zhbo_voice::Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data = Path::new(args.first().ok_or("usage: voice_choices <data dir> [seed]")?);
    let mut rng = Rng(args.get(1).and_then(|s| s.parse().ok()).unwrap_or(20261109));
    let vocabs = common::vocabularies(data)?;
    let numbers = voice_numbers();
    let refs: Vec<&str> = numbers.iter().map(String::as_str).collect();
    for (out_name, inputs) in [("choices_train.tsv", vec!["train.tsv".to_string()]), ("choices_dev.tsv", vec!["dev.tsv".to_string()])] {
        let mut sources = inputs;
        if out_name == "choices_train.tsv" {
            for e in std::fs::read_dir(data)? {
                let name = e?.file_name().to_string_lossy().to_string();
                if name.starts_with("heard") && name.ends_with(".tsv") {
                    sources.push(name);
                }
            }
        }
        let mut out = std::io::BufWriter::new(std::fs::File::create(data.join(out_name))?);
        let (mut rows, mut questions) = (0usize, 0usize);
        for src in &sources {
            for line in std::fs::read_to_string(data.join(src))?.lines() {
                let f: Vec<&str> = line.split('\t').collect();
                let (Some(heard), Some(label), Some(id)) = (f.first(), f.get(1), f.get(2)) else { continue };
                let Some(v) = vocabs.get(*id) else { continue };
                let Some(steps) = phrases(label, v, &refs) else { continue };
                rows += 1;
                // The end counts as a step: the question after the last phrase.
                let mut at = vec![rng.below(steps.len() + 1)];
                let names: Vec<usize> = (0..steps.len()).filter(|&i| v.spoken_names().contains(&steps[i]) || v.types.iter().any(|n| n.singular == steps[i])).collect();
                if !names.is_empty() {
                    at.push(names[rng.below(names.len())]);
                }
                at.dedup();
                for a in at {
                    if let Some((q, letter)) = ask(heard, &steps, a, v, &refs, &mut rng) {
                        writeln!(out, "{}\t{letter}\t{id}", q.replace('\n', "\\n").replace('\t', " "))?;
                        questions += 1;
                    }
                }
            }
        }
        eprintln!("{out_name}: {questions} questions from {rows} rows");
    }
    Ok(())
}
