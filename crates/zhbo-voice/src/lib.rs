//! Voice commands (docs/COMMANDS.md section 11): the fine-tuned translator from what a player says
//! to the canonical command language, on candle, with the grammar-constrained decoder.
//!
//! * [`Translator`] loads a fine-tuned T5 (`model.safetensors`, `config.json`, `tokenizer.json`, as
//!   `tools/voice/train/train_t5.py` writes them; the launcher downloads them at install) and
//!   decodes greedily, optionally under the grammar's mask ([`constrain`]).
//! * [`constrain`] turns the grammar's word-level completions into a token mask.
//! * [`hear`] is the speech recogniser in front of it: Whisper, prompted with the match's names.
//! * [`snap`] respells what Whisper heard into the match's words before the translator reads it.
//! * [`pipeline`] runs the three in turn; [`audio`] brings a microphone's or a file's sound to
//!   Whisper's 16 kHz mono. The `conquerops-voice` binary is the game's process for all of it.

pub mod audio;
pub mod constrain;
pub mod hear;
pub mod pipeline;
pub mod snap;
mod whisper;

use std::path::Path;

use candle_core::{DType, Device, Tensor, D};
use candle_nn::VarBuilder;
use candle_transformers::models::t5;
use tokenizers::Tokenizer;
use zhbo_core::command::Vocabulary;

/// The task prefix the model was trained with.
pub const PREFIX: &str = "command: ";
/// The longest translation, in tokens.
pub const MAX_TOKENS: usize = 64;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// One translation and how sure the decoder was of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Translation {
    pub text: String,
    /// The chosen tokens' summed log-probability under the model's own distribution.
    pub log_prob: f32,
    /// The most probability the grammar's mask took away at any one step (0 unconstrained): high
    /// means the model wanted something the grammar does not allow (section 9's confidence check).
    pub masked_max: f32,
    /// The mask left nothing to say (the model would have to abstain).
    pub stuck: bool,
}

/// One decoding step, for tracing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub wanted: u32,
    pub wanted_p: f32,
    pub took: u32,
    pub took_p: f32,
}

pub struct Translator {
    model: t5::T5ForConditionalGeneration,
    tokenizer: Tokenizer,
    device: Device,
    start: u32,
    eos: u32,
}

impl Translator {
    /// Load a fine-tuned model from `dir` onto `device`.
    pub fn load(dir: &Path, device: Device) -> Result<Translator, Error> {
        let mut config: t5::Config = serde_json::from_str(&std::fs::read_to_string(dir.join("config.json"))?)?;
        let tokenizer = Tokenizer::from_file(dir.join("tokenizer.json"))?;
        let weights = dir.join("model.safetensors");
        // The weights decide whether the output head is tied: a checkpoint with its own
        // `lm_head.weight` was trained with it (transformers keeps an untied head even where the
        // config claims a tie, and decoding through the shared embeddings then produces nonsense).
        let tensors = unsafe { candle_core::safetensors::MmapedSafetensors::new(&weights)? };
        if tensors.get("lm_head.weight").is_ok() {
            config.tie_word_embeddings = false;
        }
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[weights], DType::F32, &device)? };
        let model = t5::T5ForConditionalGeneration::load(vb, &config)?;
        let start = config.decoder_start_token_id.unwrap_or(config.pad_token_id) as u32;
        Ok(Translator { model, tokenizer, device, start, eos: config.eos_token_id as u32 })
    }

    pub fn tokenizer(&self) -> &Tokenizer {
        &self.tokenizer
    }

    /// Translate `heard` greedily; under the grammar's mask over `vocab` when given.
    pub fn translate(&mut self, heard: &str, vocab: Option<&Vocabulary>) -> Result<Translation, Error> {
        self.translate_traced(heard, vocab, &mut |_| {})
    }

    /// [`Translator::translate`], with each step reported to `trace`: the model's own choice and
    /// its probability, and the token taken.
    pub fn translate_traced(&mut self, heard: &str, vocab: Option<&Vocabulary>, trace: &mut dyn FnMut(Step)) -> Result<Translation, Error> {
        let input = self.tokenizer.encode(format!("{PREFIX}{heard}"), true)?;
        let ids = Tensor::new(input.get_ids(), &self.device)?.unsqueeze(0)?;
        self.model.clear_kv_cache();
        let encoded = self.model.encode(&ids)?;
        let mut mask = match vocab {
            Some(v) => Some(constrain::Mask::new(v, &self.tokenizer)?),
            None => None,
        };
        let mut out: Vec<u32> = vec![self.start];
        let mut log_prob = 0.0f32;
        let mut masked_max = 0.0f32;
        let mut stuck = false;
        for step in 0..MAX_TOKENS {
            // With the cache, only the newest token goes in after the first step.
            let feed = if step == 0 { Tensor::new(out.as_slice(), &self.device)? } else { Tensor::new(&out[out.len() - 1..], &self.device)? };
            let logits = self.model.decode(&feed.unsqueeze(0)?, &encoded)?.squeeze(0)?;
            let probs = candle_nn::ops::softmax(&logits, D::Minus1)?.to_vec1::<f32>()?;
            let next = match &mut mask {
                None => argmax(&probs, None),
                Some(m) => {
                    let allowed = m.allowed(self.eos)?;
                    if allowed.is_empty() {
                        stuck = true;
                        break;
                    }
                    let kept: f32 = allowed.iter().map(|&t| probs.get(t as usize).copied().unwrap_or(0.0)).sum();
                    masked_max = masked_max.max(1.0 - kept);
                    argmax(&probs, Some(&allowed))
                }
            };
            let wanted = argmax(&probs, None);
            trace(Step { wanted, wanted_p: probs[wanted as usize], took: next, took_p: probs.get(next as usize).copied().unwrap_or(0.0) });
            log_prob += probs.get(next as usize).copied().unwrap_or(1e-12).max(1e-12).ln();
            if next == self.eos {
                break;
            }
            if let Some(m) = &mut mask {
                m.advance(next)?;
            }
            out.push(next);
        }
        let text = self.tokenizer.decode(&out[1..], true)?.trim().to_string();
        Ok(Translation { text, log_prob, masked_max, stuck })
    }
}

/// The most probable token, among `allowed` when given.
fn argmax(probs: &[f32], allowed: Option<&[u32]>) -> u32 {
    match allowed {
        Some(a) => a.iter().copied().max_by(|&x, &y| probs.get(x as usize).unwrap_or(&0.0).total_cmp(probs.get(y as usize).unwrap_or(&0.0))).unwrap_or(0),
        None => probs.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map_or(0, |(i, _)| i as u32),
    }
}
