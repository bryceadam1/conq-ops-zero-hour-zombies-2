//! Speech recognition (docs/COMMANDS.md section 11): Whisper on candle, prompted with the match's
//! names so proper nouns come out spelled as the vocabulary spells them.
//!
//! [`Listener`] loads a Whisper checkpoint as Hugging Face publishes it (`model.safetensors`,
//! `config.json`, `tokenizer.json`; the launcher downloads it at install) and transcribes one
//! utterance of 16 kHz mono audio greedily. The prompt goes in as the "previous text"
//! (`<|startofprev|>`), Whisper's own way of carrying context.

use std::path::Path;

use candle_core::{Device, Tensor, D};
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::{self, audio, Config};
use tokenizers::Tokenizer;
use zhbo_core::command::Vocabulary;

use crate::Error;

/// The audio Whisper expects.
pub const SAMPLE_RATE: usize = whisper::SAMPLE_RATE;
/// The silence after the audio in the encoder's window, in frames (3 s). Against the full 30 s
/// window it cut base.en's encoder from 380 to 95 ms on spoken commands with no loss in what was
/// heard (`voice_listen`); whisper.cpp's `audio_ctx` is the same idea.
pub const WINDOW_SILENCE: usize = 300;
/// The longest transcription, in tokens (a command is a sentence).
const MAX_TOKENS: usize = 96;

/// One utterance as heard.
#[derive(Clone, Debug, PartialEq)]
pub struct Heard {
    pub text: String,
    /// The mean log-probability of the chosen tokens.
    pub mean_log_prob: f32,
    /// How likely Whisper thought the audio held no speech at all.
    pub no_speech: f32,
    /// Seconds spent reading the audio (the encoder) and writing the text (the decoder).
    pub encode_seconds: f32,
    pub decode_seconds: f32,
}

pub struct Listener {
    /// The encoder's window: `None` for Whisper's full 30 s, or the audio's own length plus this
    /// many frames (100 a second) of silence after it. A command lasts a few seconds, and the
    /// encoder's cost grows with its window.
    pub window: Option<usize>,
    model: crate::whisper::Whisper,
    tokenizer: Tokenizer,
    config: Config,
    filters: Vec<f32>,
    device: Device,
    /// The tokens that start a transcription after the prompt.
    start: Vec<u32>,
    prev: u32,
    eot: u32,
    no_speech_token: Option<u32>,
}

impl Listener {
    pub fn load(dir: &Path, device: Device) -> Result<Listener, Error> {
        let config: Config = serde_json::from_str(&std::fs::read_to_string(dir.join("config.json"))?)?;
        let tokenizer = Tokenizer::from_file(dir.join("tokenizer.json"))?;
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[dir.join("model.safetensors")], whisper::DTYPE, &device)? };
        let model = crate::whisper::Whisper::load(vb, &config)?;
        let id = |t: &str| tokenizer.token_to_id(t).ok_or_else(|| format!("the tokenizer has no {t}"));
        let mut start = vec![id(whisper::SOT_TOKEN)?];
        // Multilingual checkpoints take a language and a task; the English-only ones (`.en`, one
        // token fewer) go straight on.
        if config.vocab_size >= 51865 {
            start.push(id("<|en|>")?);
            start.push(id(whisper::TRANSCRIBE_TOKEN)?);
        }
        start.push(id(whisper::NO_TIMESTAMPS_TOKEN)?);
        let no_speech_token = whisper::NO_SPEECH_TOKENS.iter().find_map(|t| tokenizer.token_to_id(t));
        Ok(Listener {
            window: Some(WINDOW_SILENCE),
            filters: mel_filters(SAMPLE_RATE, whisper::N_FFT, config.num_mel_bins),
            prev: id("<|startofprev|>")?,
            eot: id(whisper::EOT_TOKEN)?,
            model,
            tokenizer,
            config,
            device,
            start,
            no_speech_token,
        })
    }

    /// Transcribe `pcm` (16 kHz mono, at most 30 s), with `prompt` as the text before it.
    pub fn hear(&mut self, pcm: &[f32], prompt: &str) -> Result<Heard, Error> {
        let clock = std::time::Instant::now();
        let n_mels = self.config.num_mel_bins;
        let mel = audio::pcm_to_mel(&self.config, pcm, &self.filters);
        let frames = mel.len() / n_mels;
        // The window, in frames: even (the encoder halves it), at most 30 s. `pcm_to_mel` pads
        // with silence, so the frames after the audio are silence's.
        let width = match self.window {
            None => whisper::N_FRAMES,
            Some(extra) => ((pcm.len() / whisper::HOP_LENGTH + extra + 1) & !1).min(whisper::N_FRAMES),
        };
        let mut window = vec![0f32; n_mels * width];
        for m in 0..n_mels {
            let take = frames.min(width);
            window[m * width..m * width + take].copy_from_slice(&mel[m * frames..m * frames + take]);
        }
        let mel = Tensor::from_vec(window, (1, n_mels, width), &self.device)?;
        let features = self.model.encode(&mel)?;
        let encode_seconds = clock.elapsed().as_secs_f32();

        let mut tokens = Vec::new();
        if !prompt.is_empty() {
            // The prompt may use half the decoder's context, its end kept.
            let ids = self.tokenizer.encode(format!(" {}", prompt.trim()), false)?.get_ids().to_vec();
            let room = self.config.max_target_positions / 2 - 1;
            tokens.push(self.prev);
            tokens.extend_from_slice(&ids[ids.len().saturating_sub(room)..]);
        }
        tokens.extend_from_slice(&self.start);
        let first = tokens.len();
        let (mut sum, mut no_speech) = (0f32, 0f32);
        for step in 0..MAX_TOKENS {
            // The cache holds what came before: the whole start first, then one token a step.
            let feed = if step == 0 { &tokens[..] } else { &tokens[tokens.len() - 1..] };
            let logits = self.model.decode(feed, &features)?;
            let probs = candle_nn::ops::softmax(&logits, D::Minus1)?.to_vec1::<f32>()?;
            if step == 0 {
                no_speech = self.no_speech_token.and_then(|t| probs.get(t as usize).copied()).unwrap_or(0.0);
            }
            // Text only: every special token (timestamps, task, language) sits at or after the end
            // of text, and the checkpoint's own suppressions stay suppressed.
            let next = (0..=self.eot)
                .filter(|t| !self.config.suppress_tokens.contains(t))
                .max_by(|&a, &b| probs[a as usize].total_cmp(&probs[b as usize]))
                .unwrap_or(self.eot);
            sum += probs[next as usize].max(1e-12).ln();
            if next == self.eot {
                break;
            }
            tokens.push(next);
        }
        let said = &tokens[first..];
        let text = self.tokenizer.decode(said, true)?.trim().to_string();
        let decode_seconds = clock.elapsed().as_secs_f32() - encode_seconds;
        Ok(Heard { text, mean_log_prob: sum / (said.len() + 1) as f32, no_speech, encode_seconds, decode_seconds })
    }
}

/// A transcription as the translator was trained to read it: lower case, words, commas and
/// apostrophes (Whisper writes sentences: capitals, full stops, question marks).
pub fn tidy(heard: &str) -> String {
    let kept: String = heard
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '\'' | '-' | ',') { c } else { ' ' })
        .collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ").replace(" ,", ",").trim_matches(',').trim().to_string()
}

/// The match's names as Whisper's prompt: the words it should expect, most specific first.
pub fn prompt(vocab: &Vocabulary) -> String {
    let mut names: Vec<&str> = Vec::new();
    for n in &vocab.types {
        names.push(&n.plural);
    }
    for list in [&vocab.places, &vocab.teams, &vocab.upgrades, &vocab.powers, &vocab.sciences, &vocab.modes, &vocab.weapons] {
        names.extend(list.iter().map(String::as_str));
    }
    format!("Commands for {}.", names.join(", "))
}

/// Whisper's mel filterbank: librosa's `filters.mel(sr, n_fft, n_mels)` with its defaults (the
/// Slaney mel scale and area normalisation), which OpenAI's code loads precomputed. Row-major,
/// `n_mels` rows of `n_fft / 2 + 1` bins, as candle's `pcm_to_mel` reads it.
pub fn mel_filters(sample_rate: usize, n_fft: usize, n_mels: usize) -> Vec<f32> {
    const F_SP: f64 = 200.0 / 3.0;
    const MIN_LOG_HZ: f64 = 1000.0;
    let min_log_mel = MIN_LOG_HZ / F_SP;
    let log_step = 6.4f64.ln() / 27.0;
    let to_mel = |f: f64| if f < MIN_LOG_HZ { f / F_SP } else { min_log_mel + (f / MIN_LOG_HZ).ln() / log_step };
    let to_hz = |m: f64| if m < min_log_mel { m * F_SP } else { MIN_LOG_HZ * ((m - min_log_mel) * log_step).exp() };
    let bins = n_fft / 2 + 1;
    let nyquist = sample_rate as f64 / 2.0;
    let fft_hz: Vec<f64> = (0..bins).map(|k| nyquist * k as f64 / (bins - 1) as f64).collect();
    let top = to_mel(nyquist);
    let mel_hz: Vec<f64> = (0..n_mels + 2).map(|i| to_hz(top * i as f64 / (n_mels + 1) as f64)).collect();
    let mut out = vec![0f32; n_mels * bins];
    for m in 0..n_mels {
        let (lo, mid, hi) = (mel_hz[m], mel_hz[m + 1], mel_hz[m + 2]);
        let norm = 2.0 / (hi - lo);
        for (k, &f) in fft_hz.iter().enumerate() {
            let w = ((f - lo) / (mid - lo)).min((hi - f) / (hi - mid)).max(0.0);
            out[m * bins + k] = (w * norm) as f32;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transcription_is_tidied_for_the_translator() {
        assert_eq!(tidy(" Korgor towers, move to Nixvel Hill."), "korgor towers, move to nixvel hill");
        assert_eq!(tidy("Uh, the dozer's -- build it?"), "uh, the dozer's -- build it");
    }

    #[test]
    fn the_mel_filters_are_whispers() {
        // Reference values from transformers' `WhisperFeatureExtractor(feature_size=80).mel_filters`
        // (librosa's, as OpenAI ships them); the whole matrix matched to the last bit when compared.
        let f = mel_filters(16000, 400, 80);
        assert_eq!(f.len(), 80 * 201);
        for (m, k, want) in [(0, 1, 0.024862593984176087), (10, 10, 0.019908218880980738), (40, 43, 0.014735565741439142), (79, 190, 0.0022320550527126157)] {
            assert!((f[m * 201 + k] as f64 - want).abs() < 1e-7, "filter {m} bin {k}: {} against {want}", f[m * 201 + k]);
        }
        assert!((f.iter().map(|&x| x as f64).sum::<f64>() - 1.999024102917858).abs() < 1e-5);
    }
}
