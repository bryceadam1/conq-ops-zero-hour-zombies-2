//! From speech to a command (docs/COMMANDS.md section 11): Whisper prompted with the match's
//! names, the snapper, and the constrained translator.

use std::path::Path;

use candle_core::Device;
use zhbo_core::command::normal::{self, Tables};
use zhbo_core::command::Vocabulary;

use crate::hear::{self, Listener};
use crate::snap::Snapper;
use crate::{Error, Translator};

/// The voice models' folders under the voice directory, from the launcher's manifest.
pub const MANIFEST: &str = include_str!("../../../data/ConquerOps/voice.ini");

/// The `Dir` of a `[section]` of the manifest.
pub fn model_dir(manifest: &str, section: &str) -> Option<String> {
    let mut inside = false;
    for line in manifest.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if line.starts_with('[') {
            inside = line == format!("[{section}]");
        } else if inside {
            if let Some(dir) = line.strip_prefix("Dir").and_then(|r| r.trim_start().strip_prefix('=')) {
                return Some(dir.trim().to_string());
            }
        }
    }
    None
}

/// What one utterance came to.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    /// Whisper's words, tidied, and after snapping.
    pub heard: String,
    pub snapped: String,
    /// The canonical command (empty if the mask left nothing to say).
    pub command: String,
    /// The most probability the grammar's mask took away at one step: the confidence check.
    pub masked_max: f32,
    pub stuck: bool,
    /// How likely Whisper thought there was no speech.
    pub no_speech: f32,
}

pub struct Pipeline {
    ears: Listener,
    snapper: Snapper,
    model: Translator,
    /// Decoding by options with this beam instead of the token mask (`options`), when set.
    options: Option<(crate::options::OptionDecoder, usize)>,
}

impl Pipeline {
    /// The models in `voice_dir` (the launcher's `voice/`), with the synonym tables the game
    /// reads (`tables`: the shipped file, the player's after it).
    pub fn load(voice_dir: &Path, tables: &Tables, device: Device) -> Result<Pipeline, Error> {
        let dir = |section: &str| model_dir(MANIFEST, section).map(|d| voice_dir.join(d)).ok_or_else(|| format!("voice.ini has no [{section}] Dir"));
        let (whisper, translator) = (dir("Whisper")?, dir("Translator")?);
        let words = std::fs::read_to_string(translator.join("words.txt"))?;
        Ok(Pipeline {
            ears: Listener::load(&whisper, device.clone())?,
            snapper: Snapper::new(words.lines().map(str::to_string), tables),
            model: Translator::load(&translator, device.clone())?,
            options: match std::env::var("ZHBO_VOICE_OPTIONS").ok().and_then(|b| b.parse::<usize>().ok()).filter(|&b| b > 0) {
                Some(beam) => Some((crate::options::OptionDecoder::load(&translator, device)?, beam)),
                None => None,
            },
        })
    }

    /// The shipped synonym tables alone.
    pub fn shipped_tables() -> Tables {
        Tables::parse(&[normal::SHIPPED])
    }

    /// Hear `pcm` (16 kHz mono) and translate it over `vocab`.
    pub fn run(&mut self, pcm: &[f32], vocab: &Vocabulary) -> Result<Outcome, Error> {
        let said = self.ears.hear(pcm, &hear::prompt(vocab))?;
        let heard = hear::tidy(&said.text);
        let snapped = self.snapper.snap(&heard, vocab);
        if let Some((decoder, beam)) = &self.options {
            let s = decoder.decode(&snapped, vocab, *beam, 0.0)?;
            // The confidence check reads the weakest option: 1 - its mean token probability.
            return Ok(Outcome { heard, snapped, command: s.text, masked_max: 1.0 - s.weakest, stuck: s.unknown.is_some(), no_speech: said.no_speech });
        }
        let t = self.model.translate(&snapped, vocab, true)?;
        Ok(Outcome { heard, snapped, command: t.text, masked_max: t.masked_max, stuck: t.stuck, no_speech: said.no_speech })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_names_both_models() {
        assert!(model_dir(MANIFEST, "Whisper").is_some());
        assert!(model_dir(MANIFEST, "Translator").is_some());
        assert_eq!(model_dir("[A]\nDir = x\n[B]\nDir = y", "B").as_deref(), Some("y"));
    }
}
