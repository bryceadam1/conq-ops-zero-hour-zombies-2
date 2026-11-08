//! Whisper's network over the Hugging Face checkpoint's weights, with the decoder's self-attention
//! cached. The prompt (the match's names, a hundred or more tokens) is read once per utterance and
//! each new token costs one position, where candle's own model (which caches only the
//! cross-attention) reads the whole sequence again at every step: 1.1 s against ~0.2 s on the
//! CPU for base.en. The layers follow OpenAI's `whisper/model.py` (and candle's port of it).

use candle_core::{DType, Device, Module, Result, Tensor};
use candle_nn::{embedding, layer_norm, linear, linear_no_bias, Conv1d, Conv1dConfig, Embedding, LayerNorm, LayerNormConfig, Linear, VarBuilder};
use candle_transformers::models::whisper::Config;

struct Attention {
    query: Linear,
    key: Linear,
    value: Linear,
    out: Linear,
    heads: usize,
    /// Self-attention keeps earlier positions (the decoder's); the encoder's sees all at once.
    keep: bool,
    /// Keys and values so far: every earlier position for self-attention, the audio for
    /// cross-attention.
    cache: Option<(Tensor, Tensor)>,
}

impl Attention {
    fn load(state: usize, heads: usize, keep: bool, vb: VarBuilder) -> Result<Attention> {
        Ok(Attention {
            query: linear(state, state, vb.pp("q_proj"))?,
            key: linear_no_bias(state, state, vb.pp("k_proj"))?,
            value: linear(state, state, vb.pp("v_proj"))?,
            out: linear(state, state, vb.pp("out_proj"))?,
            heads,
            keep,
            cache: None,
        })
    }

    fn heads(&self, x: &Tensor) -> Result<Tensor> {
        let (b, n, s) = x.dims3()?;
        x.reshape((b, n, self.heads, s / self.heads))?.transpose(1, 2)
    }

    /// Attend from `x` to `x` itself (`source` None: earlier positions included, from the
    /// cache, causally) or to `source` (the audio, projected once and cached).
    fn forward(&mut self, x: &Tensor, source: Option<&Tensor>) -> Result<Tensor> {
        let q = self.query.forward(x)?;
        let (k, v, past) = match source {
            Some(audio) => match &self.cache {
                Some((k, v)) => (k.clone(), v.clone(), None),
                None => {
                    let (k, v) = (self.key.forward(audio)?, self.value.forward(audio)?);
                    self.cache = Some((k.clone(), v.clone()));
                    (k, v, None)
                }
            },
            None if !self.keep => (self.key.forward(x)?, self.value.forward(x)?, None),
            None => {
                let (k, v) = (self.key.forward(x)?, self.value.forward(x)?);
                let past = self.cache.as_ref().map_or(0, |(k, _)| k.dim(1).unwrap_or(0));
                let (k, v) = match &self.cache {
                    Some((pk, pv)) => (Tensor::cat(&[pk, &k], 1)?, Tensor::cat(&[pv, &v], 1)?),
                    None => (k, v),
                };
                self.cache = Some((k.clone(), v.clone()));
                (k, v, Some(past))
            }
        };
        let (_, n, s) = q.dims3()?;
        let scale = ((s / self.heads) as f64).powf(-0.25);
        let q = (self.heads(&q)? * scale)?;
        let k = (self.heads(&k)?.transpose(2, 3)? * scale)?;
        let v = self.heads(&v)?.contiguous()?;
        let mut qk = q.contiguous()?.matmul(&k.contiguous()?)?;
        // New position i (after `past` cached ones) sees positions up to past + i.
        if let (Some(past), true) = (past, n > 1) {
            let total = past + n;
            let mask: Vec<f32> = (0..n).flat_map(|i| (0..total).map(move |j| if j > past + i { f32::NEG_INFINITY } else { 0.0 })).collect();
            qk = qk.broadcast_add(&Tensor::from_vec(mask, (n, total), x.device())?)?;
        }
        let w = candle_nn::ops::softmax_last_dim(&qk)?;
        self.out.forward(&w.matmul(&v)?.transpose(1, 2)?.flatten_from(2)?)
    }
}

struct Block {
    attn: Attention,
    attn_ln: LayerNorm,
    cross: Option<(Attention, LayerNorm)>,
    fc1: Linear,
    fc2: Linear,
    mlp_ln: LayerNorm,
}

fn norm(size: usize, vb: VarBuilder) -> Result<LayerNorm> {
    layer_norm(size, LayerNormConfig { eps: 1e-5, ..Default::default() }, vb)
}

impl Block {
    fn load(state: usize, heads: usize, cross: bool, vb: VarBuilder) -> Result<Block> {
        Ok(Block {
            attn: Attention::load(state, heads, cross, vb.pp("self_attn"))?,
            attn_ln: norm(state, vb.pp("self_attn_layer_norm"))?,
            cross: if cross { Some((Attention::load(state, heads, false, vb.pp("encoder_attn"))?, norm(state, vb.pp("encoder_attn_layer_norm"))?)) } else { None },
            fc1: linear(state, state * 4, vb.pp("fc1"))?,
            fc2: linear(state * 4, state, vb.pp("fc2"))?,
            mlp_ln: norm(state, vb.pp("final_layer_norm"))?,
        })
    }

    fn forward(&mut self, x: &Tensor, audio: Option<&Tensor>) -> Result<Tensor> {
        let mut x = (x + self.attn.forward(&self.attn_ln.forward(x)?, None)?)?;
        if let Some((attn, ln)) = &mut self.cross {
            x = (&x + attn.forward(&ln.forward(&x)?, audio)?)?;
        }
        let mlp = self.fc2.forward(&self.fc1.forward(&self.mlp_ln.forward(&x)?)?.gelu_erf()?)?;
        x + mlp
    }

    fn forget(&mut self) {
        self.attn.cache = None;
        if let Some((attn, _)) = &mut self.cross {
            attn.cache = None;
        }
    }
}

/// The encoder's fixed sinusoidal positions.
fn sinusoids(length: usize, channels: usize, device: &Device) -> Result<Tensor> {
    let step = 10000f32.ln() / (channels / 2 - 1) as f32;
    let inv: Vec<f32> = (0..channels / 2).map(|i| (i as f32 * -step).exp()).collect();
    let inv = Tensor::new(inv.as_slice(), device)?.unsqueeze(0)?;
    let t = Tensor::arange(0, length as u32, device)?.to_dtype(DType::F32)?.unsqueeze(1)?;
    let scaled = t.broadcast_mul(&inv)?;
    Tensor::cat(&[scaled.sin()?, scaled.cos()?], 1)
}

pub struct Whisper {
    conv1: Conv1d,
    conv2: Conv1d,
    positions: Tensor,
    encoder: Vec<Block>,
    encoder_ln: LayerNorm,
    tokens: Embedding,
    text_positions: Tensor,
    decoder: Vec<Block>,
    decoder_ln: LayerNorm,
    /// Positions already in the decoder's cache.
    written: usize,
}

impl Whisper {
    pub fn load(vb: VarBuilder, cfg: &Config) -> Result<Whisper> {
        let (state, enc, dec) = (cfg.d_model, vb.pp("model.encoder"), vb.pp("model.decoder"));
        let conv = |inputs, stride, vb: VarBuilder| -> Result<Conv1d> {
            let weight = vb.get((state, inputs, 3), "weight")?;
            Ok(Conv1d::new(weight, Some(vb.get(state, "bias")?), Conv1dConfig { padding: 1, stride, dilation: 1, groups: 1, cudnn_fwd_algo: None }))
        };
        Ok(Whisper {
            conv1: conv(cfg.num_mel_bins, 1, enc.pp("conv1"))?,
            conv2: conv(state, 2, enc.pp("conv2"))?,
            positions: sinusoids(cfg.max_source_positions, state, vb.device())?,
            encoder: (0..cfg.encoder_layers).map(|i| Block::load(state, cfg.encoder_attention_heads, false, enc.pp(format!("layers.{i}")))).collect::<Result<_>>()?,
            encoder_ln: norm(state, enc.pp("layer_norm"))?,
            tokens: embedding(cfg.vocab_size, state, dec.pp("embed_tokens"))?,
            text_positions: dec.get((cfg.max_target_positions, state), "embed_positions.weight")?,
            decoder: (0..cfg.decoder_layers).map(|i| Block::load(state, cfg.decoder_attention_heads, true, dec.pp(format!("layers.{i}")))).collect::<Result<_>>()?,
            decoder_ln: norm(state, dec.pp("layer_norm"))?,
            written: 0,
        })
    }

    /// The audio's features from its mel spectrogram (`[1, mels, frames]`, frames even and at
    /// most 3000), and a fresh decoder for it.
    pub fn encode(&mut self, mel: &Tensor) -> Result<Tensor> {
        for block in self.encoder.iter_mut().chain(self.decoder.iter_mut()) {
            block.forget();
        }
        self.written = 0;
        let x = self.conv1.forward(mel)?.gelu_erf()?;
        let x = self.conv2.forward(&x)?.gelu_erf()?.transpose(1, 2)?;
        let mut x = x.broadcast_add(&self.positions.narrow(0, 0, x.dim(1)?)?)?;
        for block in &mut self.encoder {
            x = block.forward(&x, None)?;
        }
        self.encoder_ln.forward(&x)
    }

    /// The next-token logits after `tokens` (the ones not yet given), over `audio`.
    pub fn decode(&mut self, tokens: &[u32], audio: &Tensor) -> Result<Tensor> {
        let x = Tensor::new(tokens, audio.device())?.unsqueeze(0)?;
        let mut x = self.tokens.forward(&x)?.broadcast_add(&self.text_positions.narrow(0, self.written, tokens.len())?)?;
        self.written += tokens.len();
        for block in &mut self.decoder {
            x = block.forward(&x, Some(audio))?;
        }
        let x = self.decoder_ln.forward(&x)?;
        let last = x.narrow(1, tokens.len() - 1, 1)?.squeeze(1)?;
        last.matmul(&self.tokens.embeddings().t()?)?.squeeze(0)
    }
}
