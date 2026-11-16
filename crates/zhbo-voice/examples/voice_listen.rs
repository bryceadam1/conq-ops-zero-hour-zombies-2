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
//!
//! `VOICE_COMMAND=<checkpoint dir>`: also a Whisper trained to write commands
//! (`train_whisper.py`) on the same audio, read by `Listener::command` with the grammar's mask
//! and without, no translator after it.
//!
//! `VOICE_AUDIO=<dir>`: the rows' audio from `<dir>/<i>.wav` and its voices from
//! `<dir>/manifest.tsv` instead of `say` (`tools/voice/tts/listen_kokoro.py`: Kokoro voices no
//! model trained on).

mod common;

use std::path::Path;
use std::time::Instant;

use common::same_meaning;
use zhbo_core::command::sample::names_in;
use zhbo_core::command::normal::{self, Tables};
use zhbo_voice::hear::{self, Listener};
use zhbo_voice::snap::Snapper;
use zhbo_voice::Translator;

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
    let audio_dir = std::env::var("VOICE_AUDIO").ok().map(std::path::PathBuf::from);
    let audio_voices: Vec<String> = match &audio_dir {
        Some(d) => std::fs::read_to_string(d.join("manifest.tsv"))?.lines().map(|l| l.split('\t').nth(1).unwrap_or("").to_string()).collect(),
        None => Vec::new(),
    };
    let mut translate_time = 0f64;
    // [no prompt, prompted, prompted and snapped]: names heard whole, commands meaning the
    // label, seconds hearing (and its parts).
    const WAYS: [&str; 3] = ["no prompt", "prompted", "snapped"];
    let mut names = [0usize; 3];
    let mut right = [0usize; 3];
    let mut took = [0f64; 3];
    let mut parts = [[0f64; 2]; 3];
    let mut wrong: Vec<String> = Vec::new();
    let mut free_right = 0usize;
    // The English-named commands (the renamed copies, vocabulary ids ending in `e`): a match's
    // names are English, so these are told apart.
    let (mut english, mut english_right) = (0usize, 0usize);
    // VOICE_OPTIONS=<beam>: also decode the snapped text by options (`options`).
    let options = match std::env::var("VOICE_OPTIONS").ok().and_then(|b| b.parse::<usize>().ok()) {
        Some(beam) => Some((zhbo_voice::options::OptionDecoder::load(Path::new(&args[1]), candle_core::Device::Cpu)?, beam)),
        None => None,
    };
    let (mut options_right, mut options_time) = (0usize, 0f64);
    let (mut tokens_right, mut tokens_time) = (0usize, 0f64);
    // VOICE_CHOICE=<share>: also decode by options, asking the model's multiple-choice question
    // where its top option holds less than that share (`OptionDecoder::decode_choice`).
    let choice = match std::env::var("VOICE_CHOICE").ok().and_then(|s| s.parse::<f32>().ok()) {
        Some(sure) => Some((zhbo_voice::options::OptionDecoder::load(Path::new(&args[1]), candle_core::Device::Cpu)?, sure)),
        None => None,
    };
    let (mut choice_right, mut choice_time, mut choice_asked) = (0usize, 0f64, 0usize);
    let mut direct = match std::env::var("VOICE_COMMAND") {
        Ok(dir) => Some(Listener::load(Path::new(&dir), candle_core::Device::Cpu)?),
        Err(_) => None,
    };
    // [masked, free]: right, unknown flagged, seconds; and the masked misses.
    let (mut direct_right, mut direct_unknown, mut direct_time) = ([0usize; 2], [0usize; 2], [0f64; 2]);
    let (mut direct_english, mut direct_wrong) = (0usize, Vec::new());
    for (i, (input, label, id)) in rows.iter().enumerate() {
        let Some(v) = vocabs.get(id) else { continue };
        let (voice, pcm) = match &audio_dir {
            Some(d) => (audio_voices[i].as_str(), zhbo_voice::audio::read_wav(&std::fs::read(d.join(format!("{i}.wav")))?)?),
            None => {
                let voice = common::TEST_VOICES[i % common::TEST_VOICES.len()];
                (voice, common::speak(input, voice, None, &scratch)?)
            }
        };
        let wanted = names_in(label, v);
        let names_prompt = hear::prompt(v);
        if let Some(ears) = &mut direct {
            for (k, constrained) in [true, false].into_iter().enumerate() {
                let t = Instant::now();
                let c = ears.command(&pcm, v, constrained)?;
                direct_time[k] += t.elapsed().as_secs_f64();
                direct_unknown[k] += c.unknown.is_some() as usize;
                let same = c.unknown.is_none() && same_meaning(&c.text, label, v);
                direct_right[k] += same as usize;
                if constrained && id.ends_with('e') {
                    direct_english += same as usize;
                }
                if constrained && !same && direct_wrong.len() < 25 {
                    let unknown = c.unknown.map(|u| format!(" <unknown: {u}>")).unwrap_or_default();
                    direct_wrong.push(format!("  [{voice}] {input}\n    want  {label}\n    got   {}{unknown} (mask took {:.2})", c.text, c.masked_max));
                }
            }
        }
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
            let t = Instant::now();
            let out = model.translate(&text, v, true)?;
            if k == 2 {
                translate_time += t.elapsed().as_secs_f64();
            }
            let same = same_meaning(&out.text, label, v);
            right[k] += same as usize;
            if let (2, Some((decoder, sure))) = (k, &choice) {
                let t = Instant::now();
                let c = decoder.decode_choice(&text, v, decoder, *sure)?;
                choice_time += t.elapsed().as_secs_f64();
                choice_asked += c.asked;
                choice_right += same_meaning(&c.text, label, v) as usize;
            }
            if k == 2 {
                // The same, unconstrained: a model trained elsewhere without the mask compares here.
                free_right += same_meaning(&model.translate(&text, v, false)?.text, label, v) as usize;
                if let Some((decoder, beam)) = &options {
                    let t = Instant::now();
                    options_right += same_meaning(&decoder.decode(&text, v, *beam, 0.0)?.text, label, v) as usize;
                    options_time += t.elapsed().as_secs_f64();
                    let t = Instant::now();
                    tokens_right += same_meaning(&decoder.decode_tokens(&text, v, *beam)?.text, label, v) as usize;
                    tokens_time += t.elapsed().as_secs_f64();
                }
            }
            if k == 2 && id.ends_with('e') {
                english += 1;
                english_right += same as usize;
            }
            if k == 2 && !same && wrong.len() < 25 {
                wrong.push(format!("  [{voice}] {input}\n    heard {prompted}\n    snapped {text}\n    want  {label}\n    got   {}", out.text));
            }
        }
    }
    let n = rows.len().max(1) as f64;
    let voices = if audio_dir.is_some() { "the manifest's" } else { "say's" };
    println!("{} spoken test rows, {} voices ({voices}), CPU", rows.len(), common::TEST_VOICES.len());
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
    println!("  snapped, translating under the mask: {:.0} ms a command", translate_time / n * 1000.0);
    println!("  snapped, unconstrained: same meaning {:.3}", free_right as f64 / n);
    println!("  snapped, English-named commands only: same meaning {:.3} of {english}", english_right as f64 / english.max(1) as f64);
    if let Some((_, sure)) = &choice {
        println!(
            "  snapped, choice below {sure:.2}: same meaning {:.3}, {:.1} questions per command, {:.0} ms translating",
            choice_right as f64 / n,
            choice_asked as f64 / n,
            choice_time / n * 1000.0
        );
    }
    if let Some((_, beam)) = &options {
        println!("  snapped, options (beam {beam}): same meaning {:.3}, {:.0} ms translating", options_right as f64 / n, options_time / n * 1000.0);
        println!("  snapped, token beam {beam}: same meaning {:.3}, {:.0} ms translating", tokens_right as f64 / n, tokens_time / n * 1000.0);
    }
    println!("snapped errors (first 25):\n{}", wrong.join("\n"));
    if direct.is_some() {
        for (k, name) in ["masked", "free"].iter().enumerate() {
            println!(
                "command model, {name}: same meaning {:.3}, unknown flagged {:.3}, {:.0} ms a command",
                direct_right[k] as f64 / n,
                direct_unknown[k] as f64 / n,
                direct_time[k] / n * 1000.0
            );
        }
        println!("  masked, English-named commands only: same meaning {:.3} of {english}", direct_english as f64 / english.max(1) as f64);
        println!("command model errors, masked (first 25):\n{}", direct_wrong.join("\n"));
    }
    Ok(())
}
