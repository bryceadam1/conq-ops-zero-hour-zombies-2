//! Speech recognition (docs/COMMANDS.md section 11): Whisper on candle, prompted with the match's
//! names so proper nouns come out spelled as the vocabulary spells them.
//!
//! [`Listener`] loads a Whisper checkpoint as Hugging Face publishes it (`model.safetensors`,
//! `config.json`, `tokenizer.json`; the launcher downloads it at install) and transcribes one
//! utterance of 16 kHz mono audio greedily. The prompt goes in as the "previous text"
//! (`<|startofprev|>`), Whisper's own way of carrying context.
//!
//! A checkpoint trained to write commands (`tools/voice/train/train_whisper.py`, section 11 step
//! 13) is read by [`Listener::command`] instead: the same prompt and window, its tokens under the
//! grammar's mask (`constrain`), so what comes out is the canonical command itself; or, where it
//! hears a name the prompt lacks, the command up to it and `<unknown: name>`.

use std::path::Path;

use candle_core::{Device, Tensor, D};
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::{self, Config};
use tokenizers::Tokenizer;
use zhbo_core::command::Vocabulary;

use crate::bias::{self, Bias};
use crate::Error;

/// The audio Whisper expects.
pub const SAMPLE_RATE: usize = whisper::SAMPLE_RATE;
/// The silence after the audio in the encoder's window, in frames (3 s). Against the full 30 s
/// window it cut base.en's encoder from 380 to 95 ms on spoken commands with no loss in what was
/// heard (`voice_listen`); whisper.cpp's `audio_ctx` is the same idea.
pub const WINDOW_SILENCE: usize = 300;
/// The longest transcription, in tokens (a command is a sentence).
const MAX_TOKENS: usize = 96;
/// Tokens a second of speech may come to, at most: fast speech is about four words a second, a
/// word a token or two. Whisper can loop, writing a phrase again and again past what was said;
/// the audio's length bounds it.
const TOKENS_PER_SECOND: f32 = 10.0;

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

/// One utterance as a command model reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    /// The canonical command, up to an unknown name if there was one.
    pub text: String,
    /// The name heard that the prompt lacks (`<unknown: name>`).
    pub unknown: Option<String>,
    /// The most probability the grammar's mask took away at one step (the confidence check).
    pub masked_max: f32,
    /// The mask left nothing to say.
    pub stuck: bool,
    pub no_speech: f32,
    pub encode_seconds: f32,
    pub decode_seconds: f32,
}

/// What a command model writes before a name it can't find.
const UNKNOWN_OPEN: &str = " <unknown:";
/// The most tokens an unknown name may take (a name is a few words; past that the model loops).
const UNKNOWN_TOKENS: usize = 12;

pub struct Listener {
    /// The encoder's window: `None` for Whisper's full 30 s, or the audio's own length plus this
    /// many frames (100 a second) of silence after it. A command lasts a few seconds, and the
    /// encoder's cost grows with its window.
    pub window: Option<usize>,
    /// Hypotheses kept while writing: 1 is greedy.
    pub beam: usize,
    /// Stop the beam once its best hypothesis ends, instead of when `beam` have.
    pub stop_first: bool,
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
            beam: 1,
            stop_first: false,
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

    /// Whisper's tokenizer (a `Bias` is built over it).
    pub fn tokenizer(&self) -> &Tokenizer {
        &self.tokenizer
    }

    /// Transcribe `pcm` (16 kHz mono, at most 30 s), with `prompt` as the text before it.
    pub fn hear(&mut self, pcm: &[f32], prompt: &str) -> Result<Heard, Error> {
        self.hear_biased(pcm, prompt, None)
    }

    /// The same, with `bias`'s phrases made likelier as it writes (`bias`), over a beam of
    /// `self.beam` hypotheses.
    pub fn hear_biased(&mut self, pcm: &[f32], prompt: &str, bias: Option<&Bias>) -> Result<Heard, Error> {
        let clock = std::time::Instant::now();
        let features = self.encode(pcm)?;
        let encode_seconds = clock.elapsed().as_secs_f32();
        self.decode(pcm, prompt, &features, clock, encode_seconds, bias)
    }

    /// The audio's features from Whisper's encoder (`[1, positions, d_model]`), over the
    /// listener's window.
    pub fn encode(&mut self, pcm: &[f32]) -> Result<Tensor, Error> {
        let n_mels = self.config.num_mel_bins;
        let samples = window_samples(pcm, self.window);
        let mel = log_mel(&samples, &self.filters, n_mels);
        let width = mel.len() / n_mels;
        let mel = Tensor::from_vec(mel, (1, n_mels, width), &self.device)?;
        Ok(self.model.encode(&mel)?)
    }

    /// The decoder's tokens before the first it writes: the prompt as the previous text, then the
    /// start of the transcription.
    fn prefix(&self, prompt: &str) -> Result<Vec<u32>, Error> {
        let mut tokens = Vec::new();
        if !prompt.is_empty() {
            // The prompt may use half the decoder's context, its end kept.
            let ids = self.tokenizer.encode(format!(" {}", prompt.trim()), false)?.get_ids().to_vec();
            let room = self.config.max_target_positions / 2 - 1;
            tokens.push(self.prev);
            tokens.extend_from_slice(&ids[ids.len().saturating_sub(room)..]);
        }
        tokens.extend_from_slice(&self.start);
        Ok(tokens)
    }

    /// A command model's reading of `pcm` over `vocab` (prompted with `prompt(vocab)`), greedy,
    /// under the grammar's mask if `constrained`. `<unknown:` starts where the model itself would
    /// write it first, mask or not: as one more allowed token it became the best of what the mask
    /// left wherever the model wanted something the grammar refuses (6 of 300 spoken commands,
    /// every name in the prompt, against none unmasked). After it the name is free text up to
    /// `>` (at most `UNKNOWN_TOKENS`), and the command ends there.
    pub fn command(&mut self, pcm: &[f32], vocab: &Vocabulary, constrained: bool) -> Result<Command, Error> {
        let clock = std::time::Instant::now();
        let features = self.encode(pcm)?;
        let encode_seconds = clock.elapsed().as_secs_f32();
        let mut tokens = self.prefix(&prompt(vocab))?;
        let first = tokens.len();
        let open = self.tokenizer.encode(UNKNOWN_OPEN, false)?.get_ids().to_vec();
        let mut mask = if constrained { Some(crate::constrain::Mask::new(vocab, &self.tokenizer)?) } else { None };
        let (mut masked_max, mut stuck, mut no_speech) = (0f32, false, 0f32);
        // Where `<unknown:` began, once it has.
        let mut unknown_at: Option<usize> = None;
        let budget = ((pcm.len() as f32 / SAMPLE_RATE as f32) * TOKENS_PER_SECOND * 2.0) as usize + 16;
        for step in 0..MAX_TOKENS.min(budget) {
            let feed = if step == 0 { &tokens[..] } else { &tokens[tokens.len() - 1..] };
            let logits = self.model.decode(feed, &features)?;
            let probs = candle_nn::ops::softmax(&logits, D::Minus1)?.to_vec1::<f32>()?;
            if step == 0 {
                no_speech = self.no_speech_token.and_then(|t| probs.get(t as usize).copied()).unwrap_or(0.0);
            }
            let text_only = |t: &u32| *t <= self.eot && !self.config.suppress_tokens.contains(t);
            let best = |allowed: &mut dyn Iterator<Item = u32>| allowed.max_by(|&a, &b| probs[a as usize].total_cmp(&probs[b as usize]));
            let written = tokens.len() - first;
            let next = match (unknown_at, &mut mask) {
                // The rest of `<unknown:`, then the name in any text.
                (Some(at), _) if written - at < open.len() => open[written - at],
                (Some(at), _) if written - at >= open.len() + UNKNOWN_TOKENS => break,
                (Some(_), _) | (None, None) => best(&mut (0..=self.eot).filter(text_only)).unwrap_or(self.eot),
                (None, Some(m)) => {
                    let wanted = best(&mut (0..=self.eot).filter(text_only));
                    let mut allowed = m.allowed(self.eot)?;
                    if wanted == Some(open[0]) {
                        allowed.push(open[0]);
                    }
                    let kept: f32 = allowed.iter().map(|&t| probs.get(t as usize).copied().unwrap_or(0.0)).sum();
                    masked_max = masked_max.max(1.0 - kept);
                    match best(&mut allowed.into_iter()) {
                        Some(t) => t,
                        None => {
                            stuck = true;
                            break;
                        }
                    }
                }
            };
            if next == self.eot {
                break;
            }
            tokens.push(next);
            if unknown_at.is_none() && next == open[0] && tokens[first..].ends_with(&open[..1]) {
                unknown_at = Some(written);
            } else if unknown_at.is_none() {
                if let Some(m) = &mut mask {
                    m.advance(next)?;
                }
            } else if self.tokenizer.decode(&[next], false)?.contains('>') {
                break;
            }
        }
        let said = &tokens[first..];
        let (text, unknown) = match unknown_at {
            Some(at) => {
                let name = self.tokenizer.decode(&said[(at + open.len()).min(said.len())..], true)?;
                (self.tokenizer.decode(&said[..at], true)?, Some(name.trim().trim_end_matches('>').trim().to_string()))
            }
            None => (self.tokenizer.decode(said, true)?, None),
        };
        let decode_seconds = clock.elapsed().as_secs_f32() - encode_seconds;
        Ok(Command { text: text.trim().to_string(), unknown, masked_max, stuck, no_speech, encode_seconds, decode_seconds })
    }

    fn decode(&mut self, pcm: &[f32], prompt: &str, features: &Tensor, clock: std::time::Instant, encode_seconds: f32, bias: Option<&Bias>) -> Result<Heard, Error> {
        let beam = self.beam.max(1);
        let prefix = self.prefix(prompt)?;
        // Text only: every special token (timestamps, task, language) sits at or after the end
        // of text, and the checkpoint's own suppressions stay suppressed.
        let mut text = vec![true; self.eot as usize + 1];
        for &t in &self.config.suppress_tokens {
            if let Some(x) = text.get_mut(t as usize) {
                *x = false;
            }
        }
        struct Hyp {
            said: Vec<u32>,
            /// The log-probability with the bias's changes (what the beam ranks), and without.
            score: f32,
            log_prob: f32,
            state: bias::State,
        }
        let mut live = vec![Hyp { said: Vec::new(), score: 0.0, log_prob: 0.0, state: bias::State::default() }];
        let mut done: Vec<Hyp> = Vec::new();
        let mut no_speech = 0f32;
        let budget = ((pcm.len() as f32 / SAMPLE_RATE as f32) * TOKENS_PER_SECOND) as usize + 8;
        for step in 0..MAX_TOKENS.min(budget) {
            // The cache holds what came before: the whole start first, then one token a row.
            let feed = if step == 0 {
                Tensor::new(prefix.as_slice(), &self.device)?.unsqueeze(0)?
            } else {
                let last: Vec<u32> = live.iter().map(|h| *h.said.last().unwrap_or(&self.eot)).collect();
                Tensor::new(last.as_slice(), &self.device)?.unsqueeze(1)?
            };
            let logits = self.model.decode_rows(&feed, features)?;
            let rows = candle_nn::ops::log_softmax(&logits, D::Minus1)?.to_vec2::<f32>()?;
            if step == 0 {
                no_speech = self.no_speech_token.and_then(|t| rows[0].get(t as usize)).map_or(0.0, |l| l.exp());
            }
            // Each row's best few continuations, by log-probability plus the bias's change.
            let mut options: Vec<(f32, usize, u32, f32)> = Vec::new();
            for (b, (h, lp)) in live.iter().zip(&rows).enumerate() {
                let (rest, better) = bias.map_or((0.0, Vec::new()), |x| x.boosts(h.state));
                let mut mine: Vec<(f32, u32)> = Vec::with_capacity(beam + 1 + better.len());
                for (t, &l) in lp.iter().enumerate().take(text.len()) {
                    if text[t] {
                        mine.push((l + rest, t as u32));
                    }
                }
                let k = (beam + 1).min(mine.len());
                if k < mine.len() {
                    mine.select_nth_unstable_by(k, |a, c| c.0.total_cmp(&a.0));
                    mine.truncate(k);
                }
                for &(t, change) in &better {
                    if text.get(t as usize).copied().unwrap_or(false) {
                        mine.retain(|m| m.1 != t);
                        mine.push((lp[t as usize] + change, t));
                    }
                }
                options.extend(mine.into_iter().map(|(sc, t)| (h.score + sc, b, t, lp[t as usize])));
            }
            options.sort_by(|a, c| c.0.total_cmp(&a.0));
            // Best first: an end of text finishes its row, anything else carries one on, until the
            // beam is full again.
            let mut next: Vec<Hyp> = Vec::new();
            let mut parents: Vec<u32> = Vec::new();
            let best_ends = options.first().is_some_and(|o| o.2 == self.eot);
            for (score, b, t, l) in options {
                if next.len() == beam {
                    break;
                }
                let h = &live[b];
                if t == self.eot {
                    if done.len() < beam {
                        done.push(Hyp { said: h.said.clone(), score, log_prob: h.log_prob + l, state: h.state });
                    }
                    continue;
                }
                let mut said = h.said.clone();
                said.push(t);
                let state = bias.map_or(bias::State::default(), |x| x.advance(h.state, t));
                next.push(Hyp { said, score, log_prob: h.log_prob + l, state });
                parents.push(b as u32);
            }
            if done.len() >= beam || next.is_empty() || (self.stop_first && best_ends) {
                break;
            }
            self.model.reorder(&parents)?;
            live = next;
        }
        // Out of budget: what is still going counts as said.
        if done.is_empty() {
            done = live;
        }
        // Whisper's own ranking: the score over the length (end of text counted).
        let best = done
            .into_iter()
            .max_by(|a, c| (a.score / (a.said.len() + 1) as f32).total_cmp(&(c.score / (c.said.len() + 1) as f32)))
            .ok_or("no hypothesis")?;
        let text = unrepeat(self.tokenizer.decode(&best.said, true)?.trim());
        let decode_seconds = clock.elapsed().as_secs_f32() - encode_seconds;
        Ok(Heard { text, mean_log_prob: best.log_prob / (best.said.len() + 1) as f32, no_speech, encode_seconds, decode_seconds })
    }
}

/// `text` with a run of words said again straight after itself kept once: Whisper's loop
/// (`attack-move to the bridge, attack-move to the bridge, …`). Only runs of three words or more,
/// so a repeated word (`garrison, garrison`) is left as said.
pub fn unrepeat(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let norm = |w: &str| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    let mut out: Vec<&str> = Vec::new();
    let mut looped = false;
    let mut i = 0;
    while i < words.len() {
        out.push(words[i]);
        i += 1;
        // After each word, drop as many straight repeats of the run that ends here as follow.
        'runs: for len in (3..=out.len()).rev() {
            let run: Vec<String> = out[out.len() - len..].iter().map(|w| norm(w)).collect();
            let mut skipped = false;
            while i + len <= words.len() && words[i..i + len].iter().map(|w| norm(w)).eq(run.iter().cloned()) {
                i += len;
                skipped = true;
            }
            if skipped {
                looped = true;
                break 'runs;
            }
        }
    }
    // A loop cut off mid-way leaves the run's start again at the end: drop it. Only after a loop:
    // a command may name a place twice (`near the bridge, get over to the bridge`).
    let n = if looped { out.len() } else { 0 };
    'tail: for len in (3..=n / 2).rev() {
        let tail: Vec<String> = out[n - len..].iter().map(|w| norm(w)).collect();
        for run in len + 1..=n - len {
            if out[n - len - run..n - len].iter().take(len).map(|w| norm(w)).eq(tail.iter().cloned()) {
                out.truncate(n - len);
                break 'tail;
            }
        }
    }
    out.join(" ")
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

/// The match's names as Whisper's prompt: the words it should expect, most specific first. The
/// command model was trained on this one.
pub fn prompt(vocab: &Vocabulary) -> String {
    prompt_with(vocab, false, false)
}

/// The same, with each type also in the singular (`torhal, torhals`) where `singular`, and
/// opening with fillers (`Um, uh,`) where `fillers`: Whisper writes the previous text's style,
/// and otherwise leaves out or respells what a player says between words.
pub fn prompt_with(vocab: &Vocabulary, singular: bool, fillers: bool) -> String {
    let names = if singular {
        let mut out: Vec<String> = vocab.types.iter().flat_map(|n| [n.singular.clone(), n.plural.clone()]).collect();
        out.extend(vocab.spoken_names().into_iter().skip(vocab.types.len()));
        out
    } else {
        vocab.spoken_names()
    };
    let lead = if fillers { "Um, uh, commands for" } else { "Commands for" };
    format!("{lead} {}.", names.join(", "))
}

/// The samples the encoder's window covers: the audio and `silence` frames of silence after it,
/// zero-padded to a whole number of frame pairs (the encoder halves the frames), or with `None`
/// Whisper's full 30 s. As whisper-t5-bridge's `audio.window_samples`, so a bridge trained there
/// sees the same states.
pub fn window_samples(pcm: &[f32], silence: Option<usize>) -> Vec<f32> {
    let hop = whisper::HOP_LENGTH;
    let n = match silence {
        Some(frames) => (pcm.len() + frames * hop).div_ceil(2 * hop) * 2 * hop,
        None => whisper::N_SAMPLES,
    }
    .min(whisper::N_SAMPLES);
    let mut out = vec![0f32; n];
    let take = pcm.len().min(n);
    out[..take].copy_from_slice(&pcm[..take]);
    out
}

/// Whisper's log-mel spectrogram exactly as openai/whisper's `log_mel_spectrogram`: a 400-point
/// periodic Hann STFT, hop 160, centred with reflect padding, its last frame dropped; the power
/// spectrum through `filters` (`n_mels` rows of 201); log10 clamped at 1e-10, floored at the
/// window's maximum less 8, then `(x + 4) / 4`. Row-major `[n_mels, samples / 160]`. (candle's
/// `pcm_to_mel` frames the audio differently, and its states drifted from the reference by a few
/// percent: `the_log_mel_is_openais`.)
pub fn log_mel(samples: &[f32], filters: &[f32], n_mels: usize) -> Vec<f32> {
    use rustfft::num_complex::Complex;
    let (n_fft, hop) = (whisper::N_FFT, whisper::HOP_LENGTH);
    let bins = n_fft / 2 + 1;
    let half = n_fft / 2;
    let n = samples.len();
    // Reflect padding: `x[-k] = x[k]`, without repeating the edge.
    let at = |i: isize| -> f32 {
        let i = if i < 0 { -i } else if i >= n as isize { 2 * (n as isize - 1) - i } else { i };
        samples.get(i.clamp(0, n as isize - 1) as usize).copied().unwrap_or(0.0)
    };
    let hann: Vec<f32> = (0..n_fft).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n_fft as f32).cos()).collect();
    let frames = n / hop;
    let fft = rustfft::FftPlanner::<f32>::new().plan_fft_forward(n_fft);
    let mut power = vec![0f32; bins * frames];
    let mut buf = vec![Complex::new(0f32, 0f32); n_fft];
    for t in 0..frames {
        let start = (t * hop) as isize - half as isize;
        for (i, b) in buf.iter_mut().enumerate() {
            *b = Complex::new(at(start + i as isize) * hann[i], 0.0);
        }
        fft.process(&mut buf);
        for k in 0..bins {
            power[k * frames + t] = buf[k].norm_sqr();
        }
    }
    let mut mel = vec![0f32; n_mels * frames];
    for m in 0..n_mels {
        let row = &filters[m * bins..(m + 1) * bins];
        for t in 0..frames {
            let s: f32 = row.iter().enumerate().map(|(k, w)| w * power[k * frames + t]).sum();
            mel[m * frames + t] = s.max(1e-10).log10();
        }
    }
    let top = mel.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    for x in &mut mel {
        *x = ((*x).max(top - 8.0) + 4.0) / 4.0;
    }
    mel
}

/// Whisper's mel filterbank: librosa's `filters.mel(sr, n_fft, n_mels)` with its defaults (the
/// Slaney mel scale and area normalisation), which OpenAI's code loads precomputed. Row-major,
/// `n_mels` rows of `n_fft / 2 + 1` bins, as [`log_mel`] reads it.
pub fn mel_filters(sample_rate: usize, n_fft: usize, n_mels: usize) -> Vec<f32> {
    const F_SP: f64 = 200.0 / 3.0;
    const MIN_LOG_HZ: f64 = 1000.0;
    let min_log_mel = MIN_LOG_HZ / F_SP;
    let log_step = 6.4f64.ln() / 27.0;
    let to_mel = |f: f64| {
        if f < MIN_LOG_HZ {
            f / F_SP
        } else {
            min_log_mel + (f / MIN_LOG_HZ).ln() / log_step
        }
    };
    let to_hz = |m: f64| {
        if m < min_log_mel {
            m * F_SP
        } else {
            MIN_LOG_HZ * ((m - min_log_mel) * log_step).exp()
        }
    };
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
    fn a_loop_is_kept_once() {
        let looped = "rangers and missile defenders, attack move to the bridge rangers and missile defenders, attack move to the bridge rangers and missile defenders, attack move to";
        assert_eq!(unrepeat(looped), "rangers and missile defenders, attack move to the bridge");
        assert_eq!(unrepeat("go to the bridge go to the bridge go to the bridge"), "go to the bridge");
        assert_eq!(unrepeat("garrison, garrison units near there"), "garrison, garrison units near there");
        assert_eq!(unrepeat("the 3 weakest builders"), "the 3 weakest builders");
        let twice = "our eight korselvases furthest south near the mirdorhal bridge, get over to the mirdorhal bridge";
        assert_eq!(unrepeat(twice), twice);
    }

    #[test]
    fn a_transcription_is_tidied_for_the_translator() {
        assert_eq!(tidy(" Korgor towers, move to Nixvel Hill."), "korgor towers, move to nixvel hill");
        assert_eq!(tidy("Uh, the dozer's -- build it?"), "uh, the dozer's -- build it");
    }

    #[test]
    fn the_log_mel_is_openais() {
        // Reference: whisper-t5-bridge's `audio.LogMel` (openai/whisper's computation) on the
        // same signal and window: two tones, half a second, then the 3 s of silence.
        let x: Vec<f32> = (0..8000)
            .map(|i| {
                let t = i as f32 / 16000.0;
                0.3 * (std::f32::consts::TAU * 440.0 * t).sin() + 0.1 * (std::f32::consts::TAU * 1234.0 * t).sin() * t
            })
            .collect();
        let w = window_samples(&x, Some(WINDOW_SILENCE));
        assert_eq!(w.len(), 56000);
        let mel = log_mel(&w, &mel_filters(16000, 400, 80), 80);
        let frames = mel.len() / 80;
        assert_eq!(frames, 350);
        for (m, t, want) in [(0, 0, 0.8723545074462891), (10, 5, 1.2378135919570923), (40, 49, -0.211248517036438), (30, 300, -0.672720193862915)] {
            let got = mel[m * frames + t] as f64;
            assert!((got - want).abs() < 2e-4, "mel {m} frame {t}: {got} against {want}");
        }
        let sum: f64 = mel.iter().map(|&v| v as f64).sum();
        assert!((sum + 17906.207256436348).abs() < 0.5, "sum {sum}");
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
