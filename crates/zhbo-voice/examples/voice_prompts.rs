//! Whisper's prompt for each vocabulary of a corpus draw (docs/COMMANDS.md section 11, step 13):
//!
//! ```text
//! cargo run --release -p zhbo-voice --example voice_prompts -- <corpus dir>
//! ```
//!
//! Reads `<dir>/vocab.tsv` (`voice_corpus corpus`) and writes `<dir>/prompts.tsv`: each vocabulary
//! id and `hear::prompt` of it, the text the runtime puts before the audio as Whisper's previous
//! text, so `train_whisper.py` trains on exactly what the game sends.

use std::fs;
use std::path::Path;

use zhbo_core::command::Vocabulary;
use zhbo_voice::hear::prompt;

fn main() {
    let dir = std::env::args().nth(1).expect("usage: voice_prompts <corpus dir>");
    let dir = Path::new(&dir);
    let text = fs::read_to_string(dir.join("vocab.tsv")).expect("vocab.tsv");
    let mut out = Vec::new();
    for line in text.lines() {
        let Some((id, rest)) = line.split_once('\t') else { continue };
        let vocab = Vocabulary::from_line(rest).unwrap_or_else(|| panic!("vocab.tsv: {id} does not parse"));
        out.push(format!("{id}\t{}", prompt(&vocab)));
    }
    fs::write(dir.join("prompts.tsv"), out.join("\n") + "\n").expect("prompts.tsv");
    println!("{} prompts in {}", out.len(), dir.join("prompts.tsv").display());
}
