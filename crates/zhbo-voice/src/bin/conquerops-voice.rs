//! `conquerops-voice`: the voice commands' own process (docs/COMMANDS.md section 11). The game
//! starts it beside itself and talks to it over its standard input and output, one line each way
//! per message, fields split by tabs:
//!
//! ```text
//! game → voice   vocab <Vocabulary::to_line>   the match's names (sent before listening when changed)
//!                listen                         the push-to-talk key went down: record the microphone
//!                stop                           it came up: hear what was recorded
//!                hear-file <path>               hear a WAV file instead (the autotest)
//!                quit
//! voice → game   ready | unavailable <why>      once, after loading the models
//!                listening
//!                heard <masked> <stuck> <no speech> <transcript> <command>
//!                error <why>
//! ```
//!
//! Its own process keeps the models' memory and threads, and any crash, out of the game, and
//! candle out of the engine build. It never touches the simulation: what it hears comes back as a
//! command line, which the game resolves like a typed one.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use zhbo_core::command::Vocabulary;
use zhbo_voice::audio;
use zhbo_voice::pipeline::Pipeline;

/// The longest recording kept: Whisper's window.
const MAX_SECONDS: usize = 30;

fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", line.replace('\n', " "));
    let _ = out.flush();
}

/// The microphone, recording until dropped.
struct Recording {
    _stream: cpal::Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    channels: usize,
    rate: u32,
}

fn record() -> Result<Recording, String> {
    let device = cpal::default_host().default_input_device().ok_or("no microphone")?;
    let config = device.default_input_config().map_err(|e| e.to_string())?;
    let (channels, rate) = (config.channels() as usize, config.sample_rate());
    let samples = Arc::new(Mutex::new(Vec::new()));
    let cap = MAX_SECONDS * rate as usize * channels;
    let err = |e| eprintln!("conquerops-voice: microphone: {e}");
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let sink = samples.clone();
            device.build_input_stream(
                config.into(),
                move |data: &[f32], _: &_| {
                    let mut s = sink.lock().unwrap();
                    if s.len() < cap {
                        s.extend_from_slice(data);
                    }
                },
                err,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let sink = samples.clone();
            device.build_input_stream(
                config.into(),
                move |data: &[i16], _: &_| {
                    let mut s = sink.lock().unwrap();
                    if s.len() < cap {
                        s.extend(data.iter().map(|&x| x as f32 / 32768.0));
                    }
                },
                err,
                None,
            )
        }
        other => return Err(format!("microphone sample format {other:?} is not read")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(Recording { _stream: stream, samples, channels, rate })
}

fn main() {
    // The models sit in `voice/` beside this executable (the launcher fetches them there), or
    // where `--models` says.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let voice_dir = match args.iter().position(|a| a == "--models") {
        Some(i) => args.get(i + 1).map(PathBuf::from),
        None => std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("voice"))),
    };
    let Some(voice_dir) = voice_dir else {
        say("unavailable\tno voice folder");
        return;
    };
    let mut pipeline = match Pipeline::load(&voice_dir, &Pipeline::shipped_tables(), candle_core::Device::Cpu) {
        Ok(p) => p,
        Err(e) => {
            say(&format!("unavailable\t{e}"));
            return;
        }
    };
    say("ready");
    let mut vocab = Vocabulary::default();
    let mut recording: Option<Recording> = None;
    let mut heard = |pcm: &[f32], vocab: &Vocabulary| match pipeline.run(pcm, vocab) {
        Ok(o) => say(&format!("heard\t{:.3}\t{}\t{:.3}\t{}\t{}", o.masked_max, o.stuck as u8, o.no_speech, o.heard, o.command)),
        Err(e) => say(&format!("error\t{e}")),
    };
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let (word, rest) = line.split_once('\t').unwrap_or((line.as_str(), ""));
        match word {
            "vocab" => match Vocabulary::from_line(rest) {
                Some(v) => vocab = v,
                None => say("error\ta vocabulary line needs eight lists"),
            },
            "listen" => match record() {
                Ok(r) => {
                    recording = Some(r);
                    say("listening");
                }
                Err(e) => say(&format!("error\t{e}")),
            },
            "stop" => {
                let Some(r) = recording.take() else { continue };
                let raw = std::mem::take(&mut *r.samples.lock().unwrap());
                let (channels, rate) = (r.channels, r.rate);
                drop(r);
                heard(&audio::to_whisper(&raw, channels, rate), &vocab);
            }
            "hear-file" => match std::fs::read(rest).map_err(|e| e.to_string()).and_then(|b| audio::read_wav(&b).map_err(|e| e.to_string())) {
                Ok(pcm) => heard(&pcm, &vocab),
                Err(e) => say(&format!("error\t{rest}: {e}")),
            },
            "quit" => break,
            other => say(&format!("error\tunknown message {other:?}")),
        }
    }
}
