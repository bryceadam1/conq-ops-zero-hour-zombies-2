//! Decoding by options (docs/COMMANDS.md section 11, "Option scoring"): the model never writes a
//! token of its own choosing. At each step the grammar lists what may come next (the same
//! completions the menu offers), every option is run through the decoder as if the model had
//! written it, and the options are compared by how likely the model finds them, token by token.
//!
//! Against the token mask (`constrain`), which takes the likeliest allowed token and so decides a
//! name by its first piece, an option is judged whole. A small beam keeps the next best few, so a
//! short option (`the`) cannot win only by being a prefix of a longer one (`the torwren
//! crossing`): finished commands compete on their full probability, the end token included.
//!
//! When even the best option is unlikely (its mean token probability under `unknown_below`), the
//! model does not want any of them: decoding stops and the model writes on freely, and that text
//! comes back as [`Scored::unknown`], for the game to say what it did not understand.

use std::path::Path;

use candle_core::{Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::t5::Config;
use tokenizers::Tokenizer;
use zhbo_core::command::parse::completions_with;
use zhbo_core::command::Vocabulary;

use crate::constrain::voice_numbers;
use crate::t5net::{Encoded, State, T5};
use crate::{prompt, Error, PROMPT};

/// The longest command, in phrases.
const MAX_PHRASES: usize = 48;
/// The longest free text after an unknown, in tokens.
const MAX_FREE: usize = 32;
/// Options run through the decoder at once.
const BATCH: usize = 16;

/// A decoded command and how sure the model was.
#[derive(Clone, Debug, PartialEq)]
pub struct Scored {
    /// The command (empty if nothing fit from the start).
    pub text: String,
    pub log_prob: f32,
    /// The weakest chosen option's mean token probability: low means the model would rather
    /// have said something the grammar does not allow.
    pub weakest: f32,
    /// What the model wrote where no option was likely enough, if it came to that.
    pub unknown: Option<String>,
}

#[derive(Clone)]
struct Hypothesis {
    words: Vec<String>,
    state: State,
    /// The distribution of the next token (log-probabilities).
    next: Vec<f32>,
    score: f32,
    weakest: f32,
}

pub struct OptionDecoder {
    net: T5,
    tokenizer: Tokenizer,
    device: Device,
    start: u32,
    eos: u32,
    pad: u32,
    template: String,
    numbers: Vec<String>,
}

impl OptionDecoder {
    pub fn load(dir: &Path, device: Device) -> Result<OptionDecoder, Error> {
        let config: Config = serde_json::from_str(&std::fs::read_to_string(dir.join("config.json"))?)?;
        let tokenizer = Tokenizer::from_file(dir.join("tokenizer.json"))?;
        let weights = dir.join("model.safetensors");
        let mut config = config;
        let tensors = unsafe { candle_core::safetensors::MmapedSafetensors::new(&weights)? };
        if tensors.get("lm_head.weight").is_ok() {
            config.tie_word_embeddings = false;
        }
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[weights], candle_core::DType::F32, &device)? };
        let template = std::fs::read_to_string(dir.join("prompt.txt")).map(|t| t.trim().to_string()).unwrap_or_else(|_| PROMPT.to_string());
        Ok(OptionDecoder {
            net: T5::load(vb, &config)?,
            tokenizer,
            device,
            start: config.decoder_start_token_id.unwrap_or(config.pad_token_id) as u32,
            eos: config.eos_token_id as u32,
            pad: config.pad_token_id as u32,
            template,
            numbers: voice_numbers(),
        })
    }

    fn encode(&self, heard: &str, vocab: &Vocabulary) -> Result<Encoded, Error> {
        let ids = self.tokenizer.encode(prompt(&self.template, heard, vocab), true)?.get_ids().to_vec();
        Ok(self.net.encode(&ids)?)
    }

    /// The state after the start token, and the first token's distribution.
    fn first(&self, enc: &Encoded) -> Result<(State, Vec<f32>), Error> {
        let (logp, mut states) = self.net.decode(enc, &self.net.start()?, &Tensor::new(&[[self.start]], &self.device)?)?;
        Ok((states.remove(0), logp.i((0, 0))?.to_vec1()?))
    }

    /// Greedy and free from `state`, at most `max` tokens: the text and the tokens' log-probability.
    fn free(&self, enc: &Encoded, mut state: State, mut next: Vec<f32>, max: usize) -> Result<(Vec<u32>, f32), Error> {
        let (mut out, mut lp) = (Vec::new(), 0.0);
        for _ in 0..max {
            let t = argmax(&next);
            lp += next[t as usize];
            if t == self.eos {
                break;
            }
            out.push(t);
            let (logp, mut states) = self.net.decode(enc, &state, &Tensor::new(&[[t]], &self.device)?)?;
            state = states.remove(0);
            next = logp.i((0, 0))?.to_vec1()?;
        }
        Ok((out, lp))
    }

    /// Free greedy decoding, unconstrained (the network checked against candle's).
    pub fn greedy(&self, heard: &str, vocab: &Vocabulary) -> Result<String, Error> {
        let enc = self.encode(heard, vocab)?;
        let (state, next) = self.first(&enc)?;
        let (tokens, _) = self.free(&enc, state, next, 64)?;
        Ok(self.tokenizer.decode(&tokens, true)?.trim().to_string())
    }

    /// Every option after `h` that can still make the beam, scored: (its tokens' log-probability,
    /// its token count, the grown hypothesis; `None` for the end of the command). `top` holds the
    /// step's best scores so far (at most `beam`), and is kept up to date.
    ///
    /// An option's score is at most its first token's log-probability (every later token's is at
    /// most 0), and that is known before running it. So options go in order of their first
    /// token's, single-token ones cost nothing, and once an option's bound is under the beam's
    /// lowest kept score, it and every option after it are skipped: the beam's choice is exact.
    fn expand(&self, enc: &Encoded, h: &Hypothesis, vocab: &Vocabulary, beam: usize, top: &mut Vec<f32>) -> Result<Vec<(f32, usize, Option<Hypothesis>)>, Error> {
        let refs: Vec<&str> = self.numbers.iter().map(String::as_str).collect();
        let c = completions_with(&h.words.join(" "), vocab, &refs);
        let mut options: Vec<(String, Vec<u32>)> = Vec::new();
        if c.complete {
            options.push((String::new(), vec![self.eos]));
        }
        for p in &c.next {
            let ids = self.tokenizer.encode(p.as_str(), false)?.get_ids().to_vec();
            if !ids.is_empty() {
                options.push((p.clone(), ids));
            }
        }
        // A name being given (`name them …`) has no list: the model spells its next word itself.
        if c.name {
            if let Some((word, tokens)) = self.free_word(enc, h.state.clone(), h.next.clone())? {
                if zhbo_core::command::vocab::is_word(&word) && word != "then" {
                    options.push((word, tokens));
                }
            }
        }
        let bound = |o: &(String, Vec<u32>)| h.score + h.next[o.1[0] as usize];
        options.sort_by(|a, b| bound(b).total_cmp(&bound(a)));
        let keep = |top: &mut Vec<f32>, score: f32| {
            let at = top.partition_point(|&s| s >= score);
            top.insert(at, score);
            top.truncate(beam.max(1));
        };
        let floor = |top: &Vec<f32>| if top.len() < beam.max(1) { f32::NEG_INFINITY } else { top[top.len() - 1] };
        let mut out = Vec::new();
        let mut pending: Vec<(String, Vec<u32>)> = Vec::new();
        let mut i = 0;
        while i < options.len() || !pending.is_empty() {
            // Gather the next batch of options that can still make it.
            while i < options.len() && pending.len() < BATCH {
                let o = &options[i];
                i += 1;
                if bound(o) < floor(top) {
                    i = options.len();
                    break;
                }
                if o.1.len() == 1 {
                    // One token: its score is known already; a phrase's state still needs the
                    // decoder (its next distribution), so only the end is settled here.
                    if o.0.is_empty() {
                        let lp = h.next[self.eos as usize];
                        keep(top, h.score + lp);
                        out.push((lp, 1, None));
                        continue;
                    }
                }
                pending.push(o.clone());
            }
            if pending.is_empty() {
                break;
            }
            let batch = std::mem::take(&mut pending);
            let width = batch.iter().map(|(_, t)| t.len()).max().unwrap_or(1);
            let mut rows: Vec<u32> = Vec::with_capacity(batch.len() * width);
            let mut targets: Vec<u32> = Vec::with_capacity(batch.len() * width);
            for (_, t) in &batch {
                for j in 0..width {
                    rows.push(*t.get(j).unwrap_or(&self.pad));
                    targets.push(*t.get(j + 1).unwrap_or(&self.pad));
                }
            }
            let tokens = Tensor::from_vec(rows, (batch.len(), width), &self.device)?;
            let (logp, states) = self.net.decode(enc, &h.state, &tokens)?;
            // Each position's log-probability of the option's next token, gathered on the device.
            let picked: Vec<Vec<f32>> = logp.gather(&Tensor::from_vec(targets, (batch.len(), width, 1), &self.device)?, 2)?.squeeze(2)?.to_vec2()?;
            for (r, (phrase, t)) in batch.into_iter().enumerate() {
                let n = t.len();
                let lp = h.next[t[0] as usize] + picked[r][..n - 1].iter().sum::<f32>();
                keep(top, h.score + lp);
                let next: Vec<f32> = logp.i((r, n - 1))?.to_vec1()?;
                let mut words = h.words.clone();
                words.push(phrase);
                let grown = Hypothesis { words, state: states[r].keep(h.state.len + n)?, next, score: h.score + lp, weakest: h.weakest.min((lp / n as f32).exp()) };
                out.push((lp, n, Some(grown)));
            }
        }
        Ok(out)
    }

    /// The model's own next word, greedy: its tokens up to the next word's start.
    fn free_word(&self, enc: &Encoded, mut state: State, mut next: Vec<f32>) -> Result<Option<(String, Vec<u32>)>, Error> {
        let mut tokens = Vec::new();
        for _ in 0..8 {
            let t = argmax(&next);
            let piece = self.tokenizer.id_to_token(t).unwrap_or_default();
            if t == self.eos || (!tokens.is_empty() && piece.starts_with('▁')) {
                break;
            }
            tokens.push(t);
            let (logp, mut states) = self.net.decode(enc, &state, &Tensor::new(&[[t]], &self.device)?)?;
            state = states.remove(0);
            next = logp.i((0, 0))?.to_vec1()?;
        }
        if tokens.is_empty() {
            return Ok(None);
        }
        let word = self.tokenizer.decode(&tokens, true)?.trim().to_string();
        Ok(Some((word, tokens)))
    }

    /// Decode `heard` over `vocab` by options, keeping the `beam` best; stop with the model's own
    /// text where the best option's mean token probability is under `unknown_below` (0: never).
    pub fn decode(&self, heard: &str, vocab: &Vocabulary, beam: usize, unknown_below: f32) -> Result<Scored, Error> {
        let enc = self.encode(heard, vocab)?;
        let (state, next) = self.first(&enc)?;
        let mut live = vec![Hypothesis { words: Vec::new(), state, next, score: 0.0, weakest: 1.0 }];
        let mut finished: Vec<Hypothesis> = Vec::new();
        for _ in 0..MAX_PHRASES {
            let mut grown: Vec<(f32, Hypothesis, bool)> = Vec::new();
            let mut top: Vec<f32> = Vec::new();
            live.sort_by(|a, b| b.score.total_cmp(&a.score));
            for h in &live {
                let options = self.expand(&enc, h, vocab, beam, &mut top)?;
                let best = options.iter().map(|(lp, n, _)| (lp / *n as f32).exp()).fold(0.0f32, f32::max);
                if options.is_empty() || best < unknown_below {
                    // Nothing the model wants here: its own words, for the game to report.
                    if live.len() == 1 && finished.is_empty() {
                        let (tokens, _) = self.free(&enc, h.state.clone(), h.next.clone(), MAX_FREE)?;
                        let unknown = self.tokenizer.decode(&tokens, true)?.trim().to_string();
                        return Ok(Scored { text: h.words.join(" "), log_prob: h.score, weakest: best.min(h.weakest), unknown: Some(unknown) });
                    }
                    continue;
                }
                for (lp, n, g) in options {
                    match g {
                        Some(g) => grown.push((g.score, g, false)),
                        None => {
                            let mut done = h.clone();
                            done.score += lp;
                            done.weakest = done.weakest.min((lp / n as f32).exp());
                            grown.push((done.score, done, true));
                        }
                    }
                }
            }
            grown.sort_by(|a, b| b.0.total_cmp(&a.0));
            grown.truncate(beam.max(1));
            live.clear();
            for (_, h, done) in grown {
                if done {
                    finished.push(h);
                } else {
                    live.push(h);
                }
            }
            // A finished command no live one can beat (scores only fall) ends the search.
            let best_done = finished.iter().map(|h| h.score).fold(f32::NEG_INFINITY, f32::max);
            if live.is_empty() || live.iter().all(|h| h.score <= best_done) {
                break;
            }
        }
        let best = finished.into_iter().chain(live).max_by(|a, b| a.score.total_cmp(&b.score));
        Ok(match best {
            Some(h) => Scored { text: h.words.join(" "), log_prob: h.score, weakest: h.weakest, unknown: None },
            None => Scored { text: String::new(), log_prob: f32::NEG_INFINITY, weakest: 0.0, unknown: Some(String::new()) },
        })
    }
}

impl OptionDecoder {
    /// Decode `heard` over `vocab` token by token under the grammar's mask, keeping the `beam`
    /// likeliest hypotheses (all the same length in tokens at each step, so compared fairly).
    /// The mask's own greedy choice is this with a beam of 1.
    pub fn decode_tokens(&self, heard: &str, vocab: &Vocabulary, beam: usize) -> Result<Scored, Error> {
        struct Hyp<'a> {
            mask: crate::constrain::Mask<'a>,
            state: State,
            next: Vec<f32>,
            tokens: Vec<u32>,
            score: f32,
            weakest: f32,
        }
        let enc = self.encode(heard, vocab)?;
        let (state, next) = self.first(&enc)?;
        let mut live = vec![Hyp { mask: crate::constrain::Mask::new(vocab, &self.tokenizer)?, state, next, tokens: Vec::new(), score: 0.0, weakest: 1.0 }];
        let mut finished: Vec<(Vec<u32>, f32, f32)> = Vec::new();
        for _ in 0..crate::MAX_TOKENS {
            // Every allowed token of every hypothesis, scored; the beam's best kept.
            let mut cands: Vec<(f32, usize, u32)> = Vec::new();
            for (i, h) in live.iter_mut().enumerate() {
                for t in h.mask.allowed(self.eos)? {
                    cands.push((h.score + h.next[t as usize], i, t));
                }
            }
            cands.sort_by(|a, b| b.0.total_cmp(&a.0));
            cands.truncate(beam.max(1));
            let mut grown: Vec<Hyp> = Vec::new();
            // The kept tokens grouped by parent, each group's states grown in one decoder pass.
            let mut by_parent: Vec<Vec<(f32, u32)>> = vec![Vec::new(); live.len()];
            for (score, i, t) in cands {
                if t == self.eos {
                    let h = &live[i];
                    finished.push((h.tokens.clone(), score, h.weakest.min(h.next[t as usize].exp())));
                } else {
                    by_parent[i].push((score, t));
                }
            }
            for (i, kids) in by_parent.into_iter().enumerate() {
                if kids.is_empty() {
                    continue;
                }
                let h = &live[i];
                let ids: Vec<u32> = kids.iter().map(|k| k.1).collect();
                let (logp, states) = self.net.decode(&enc, &h.state, &Tensor::from_vec(ids.clone(), (ids.len(), 1), &self.device)?)?;
                for (r, ((score, t), state)) in kids.into_iter().zip(states).enumerate() {
                    let mut mask = h.mask.clone();
                    mask.advance(t)?;
                    let mut tokens = h.tokens.clone();
                    tokens.push(t);
                    grown.push(Hyp { mask, state, next: logp.i((r, 0))?.to_vec1()?, tokens, score, weakest: h.weakest.min(h.next[t as usize].exp()) });
                }
            }
            live = grown;
            let best_done = finished.iter().map(|f| f.1).fold(f32::NEG_INFINITY, f32::max);
            if live.is_empty() || live.iter().all(|h| h.score <= best_done) {
                break;
            }
        }
        let best = finished.into_iter().max_by(|a, b| a.1.total_cmp(&b.1));
        Ok(match best {
            Some((tokens, score, weakest)) => Scored { text: self.tokenizer.decode(&tokens, true)?.trim().to_string(), log_prob: score, weakest, unknown: None },
            None => Scored { text: String::new(), log_prob: f32::NEG_INFINITY, weakest: 0.0, unknown: Some(String::new()) },
        })
    }
}

/// A decoding by choice and what it took.
#[derive(Clone, Debug, PartialEq)]
pub struct Chosen {
    pub text: String,
    /// The steps where the generator asked, and those where the answer differed from its own pick.
    pub asked: usize,
    pub overruled: usize,
    /// The lowest probability an answer had, among the questions asked (1 if none).
    pub weakest_answer: f32,
}

impl OptionDecoder {
    /// Decode `heard` over `vocab` a step at a time: the grammar's options ranked by this model's
    /// first-token probability; the top one taken where it holds at least `sure` of the options'
    /// share, otherwise the top [`crate::choice::MAX_OPTIONS`] put to `mc` as a multiple-choice
    /// question and its answer taken. `mc` reads the question as written (its `prompt.txt`
    /// `{heard}`).
    pub fn decode_choice(&self, heard: &str, vocab: &Vocabulary, mc: &OptionDecoder, sure: f32) -> Result<Chosen, Error> {
        use crate::choice::{question, LETTERS, MAX_OPTIONS};
        let letters: Vec<u32> = LETTERS.iter().map(|l| mc.tokenizer.encode(*l, false).map(|e| e.get_ids()[0])).collect::<Result<_, _>>()?;
        let enc = self.encode(heard, vocab)?;
        let (state, next) = self.first(&enc)?;
        let mut h = Hypothesis { words: Vec::new(), state, next, score: 0.0, weakest: 1.0 };
        let (mut asked, mut overruled, mut weakest_answer) = (0usize, 0usize, 1.0f32);
        for _ in 0..MAX_PHRASES {
            // The likeliest options, scored whole (`expand`, pruned to the question's size).
            let mut top: Vec<f32> = Vec::new();
            let mut options = self.expand(&enc, &h, vocab, MAX_OPTIONS, &mut top)?;
            if options.is_empty() {
                break;
            }
            options.sort_by(|a, b| b.0.total_cmp(&a.0));
            options.truncate(MAX_OPTIONS);
            let total: f32 = options.iter().map(|o| o.0.exp()).sum();
            let share = options[0].0.exp() / total.max(1e-12);
            let mut pick = 0;
            if share < sure && options.len() > 1 {
                let offered: Vec<String> = options.iter().map(|o| o.2.as_ref().map_or(String::new(), |g| g.words.last().cloned().unwrap_or_default())).collect();
                let q = mc.encode(&question(heard, &h.words, &offered), vocab)?;
                let (_, answer) = mc.first(&q)?;
                let probs: Vec<f32> = letters.iter().take(offered.len()).map(|&t| answer[t as usize].exp()).collect();
                let sum: f32 = probs.iter().sum();
                pick = probs.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map_or(0, |(i, _)| i);
                asked += 1;
                overruled += (pick != 0) as usize;
                weakest_answer = weakest_answer.min(probs[pick] / sum.max(1e-12));
            }
            match options.swap_remove(pick).2 {
                Some(g) => h = g,
                None => break,
            }
        }
        let words = h.words;
        Ok(Chosen { text: words.join(" "), asked, overruled, weakest_answer })
    }
}

fn argmax(v: &[f32]) -> u32 {
    v.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map_or(0, |(i, _)| i as u32)
}
