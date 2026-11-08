//! Whisper in front of the translator, end to end, on spoken test commands (docs/COMMANDS.md
//! section 11):
//!
//! ```text
//! cargo run --release -p zhbo-voice --example voice_listen -- <whisper dir> <model dir> <data dir> [rows]
//! ```
//!
//! macOS only: each test input is spoken by `say`, in turn by several English voices, into 16 kHz
//! audio. Whisper hears it without a prompt and prompted with the row's names (`hear::prompt`),
//! the prompted transcription is also snapped (`snap`, with the model's `words.txt`), and the
//! constrained translator reads each. Reports how often the transcription
//! has every name of the label in it, how often the command means the label, and the time Whisper
//! takes. Synthetic speech is cleaner than a player's, so this is the ceiling, not the floor.

mod common;

use std::path::Path;
use std::time::Instant;

use common::same_meaning;
use zhbo_core::command::sample::names_in;
use zhbo_core::command::normal::{self, Tables};
use zhbo_voice::hear::{self, Listener};
use zhbo_voice::snap::Snapper;
use zhbo_voice::Translator;

const VOICES: [&str; 8] = ["Samantha", "Daniel", "Karen", "Moira", "Rishi", "Tessa", "Reed (English (US))", "Shelley (English (US))"];

fn main() -> Result<(), zhbo_voice::Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        return Err("usage: voice_listen <whisper dir> <model dir> <data dir> [rows]".into());
    }
    let data = Path::new(&args[2]);
    let limit: usize = args.get(3).and_then(|n| n.parse().ok()).unwrap_or(120);
    let vocabs = common::vocabularies(data)?;
    // A spread of commands: every so many rows rather than the first few vocabularies.
    let all = common::test_rows(data, usize::MAX)?;
    let stride = (all.len() / limit.max(1)).max(1);
    let rows: Vec<_> = all.into_iter().step_by(stride).take(limit).collect();
    let mut ears = Listener::load(Path::new(&args[0]), candle_core::Device::Cpu)?;
    // VOICE_WINDOW=<frames> or `full`: the silence after the audio in the encoder's window.
    if let Ok(w) = std::env::var("VOICE_WINDOW") {
        ears.window = w.parse().ok();
    }
    println!("encoder window: {}", ears.window.map_or("30 s".to_string(), |w| format!("the audio and {w} frames")));
    let words = std::fs::read_to_string(Path::new(&args[1]).join("words.txt"))?;
    let snapper = Snapper::new(words.lines().map(str::to_string), &Tables::parse(&[normal::SHIPPED]));
    let mut model = Translator::load(Path::new(&args[1]), candle_core::Device::Cpu)?;
    let scratch = std::env::temp_dir();
    // [no prompt, prompted, prompted and snapped]: names heard whole, commands meaning the
    // label, seconds hearing (and its parts).
    const WAYS: [&str; 3] = ["no prompt", "prompted", "snapped"];
    let mut names = [0usize; 3];
    let mut right = [0usize; 3];
    let mut took = [0f64; 3];
    let mut parts = [[0f64; 2]; 3];
    let mut wrong: Vec<String> = Vec::new();
    for (i, (input, label, id)) in rows.iter().enumerate() {
        let Some(v) = vocabs.get(id) else { continue };
        let voice = VOICES[i % VOICES.len()];
        let pcm = common::speak(input, voice, None, &scratch)?;
        let wanted = names_in(label, v);
        let names_prompt = hear::prompt(v);
        let mut prompted = String::new();
        for k in 0..WAYS.len() {
            let text = if k == 2 {
                snapper.snap(&prompted, v)
            } else {
                let t = Instant::now();
                let heard = ears.hear(&pcm, if k == 0 { "" } else { &names_prompt })?;
                took[k] += t.elapsed().as_secs_f64();
                parts[k][0] += heard.encode_seconds as f64;
                parts[k][1] += heard.decode_seconds as f64;
                hear::tidy(&heard.text)
            };
            if k == 1 {
                prompted = text.clone();
            }
            names[k] += wanted.iter().all(|n| text.contains(n.as_str())) as usize;
            let out = model.translate(&text, Some(v))?;
            let same = same_meaning(&out.text, label, v);
            right[k] += same as usize;
            if k == 2 && !same && wrong.len() < 25 {
                wrong.push(format!("  [{voice}] {input}\n    heard {prompted}\n    snapped {text}\n    want  {label}\n    got   {}", out.text));
            }
        }
    }
    let n = rows.len().max(1) as f64;
    println!("{} spoken test rows, {} voices, CPU", rows.len(), VOICES.len());
    for (k, name) in WAYS.iter().enumerate() {
        println!(
            "{name:>9}: names heard {:.3}, same meaning {:.3}, {:.0} ms hearing ({:.0} encoding, {:.0} decoding)",
            names[k] as f64 / n,
            right[k] as f64 / n,
            took[k] / n * 1000.0,
            parts[k][0] / n * 1000.0,
            parts[k][1] / n * 1000.0
        );
    }
    println!("snapped errors (first 25):\n{}", wrong.join("\n"));
    Ok(())
}
