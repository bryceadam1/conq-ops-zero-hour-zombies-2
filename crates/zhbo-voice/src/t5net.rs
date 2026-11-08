//! T5's network over the Hugging Face checkpoint's weights (flan-t5: gated-GELU, untied head),
//! with what candle's own model does not give: the logits at every decoded position, and the
//! decoder's self-attention cache as a value ([`State`]) that can be copied and grown per
//! hypothesis. Option scoring (`options`) needs both: every option at a step is run as a batch on
//! top of one shared state. The layers follow transformers' `modeling_t5.py`.

use candle_core::{DType, Device, Module, Result, Tensor, D};
use candle_nn::{embedding, linear_no_bias, Embedding, Linear, VarBuilder};
use candle_transformers::models::t5::Config;

/// T5's layer norm: a root-mean-square scale with no mean and no bias.
struct RmsNorm {
    weight: Tensor,
    eps: f64,
}

impl RmsNorm {
    fn load(size: usize, eps: f64, vb: VarBuilder) -> Result<RmsNorm> {
        Ok(RmsNorm { weight: vb.get(size, "weight")?, eps })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let var = x.sqr()?.mean_keepdim(D::Minus1)?;
        x.broadcast_div(&(var + self.eps)?.sqrt()?)?.broadcast_mul(&self.weight)
    }
}

struct Attention {
    q: Linear,
    k: Linear,
    v: Linear,
    o: Linear,
    heads: usize,
    d_kv: usize,
}

impl Attention {
    fn load(cfg: &Config, vb: VarBuilder) -> Result<Attention> {
        let inner = cfg.num_heads * cfg.d_kv;
        Ok(Attention {
            q: linear_no_bias(cfg.d_model, inner, vb.pp("q"))?,
            k: linear_no_bias(cfg.d_model, inner, vb.pp("k"))?,
            v: linear_no_bias(cfg.d_model, inner, vb.pp("v"))?,
            o: linear_no_bias(inner, cfg.d_model, vb.pp("o"))?,
            heads: cfg.num_heads,
            d_kv: cfg.d_kv,
        })
    }

    /// `[n, len, d_model]` to `[n, heads, len, d_kv]`.
    fn split(&self, x: &Tensor) -> Result<Tensor> {
        let (n, len, _) = x.dims3()?;
        x.reshape((n, len, self.heads, self.d_kv))?.transpose(1, 2)?.contiguous()
    }

    /// Attend from queries `x` to keys and values already split, with `bias` (`[heads, q, k]`,
    /// position bias and mask) added; T5 does not scale the scores.
    fn attend(&self, x: &Tensor, k: &Tensor, v: &Tensor, bias: Option<&Tensor>) -> Result<Tensor> {
        let q = self.split(&self.q.forward(x)?)?;
        let mut scores = q.matmul(&k.transpose(2, 3)?.contiguous()?)?;
        if let Some(b) = bias {
            scores = scores.broadcast_add(b)?;
        }
        let w = candle_nn::ops::softmax_last_dim(&scores)?;
        let (n, _, len, _) = q.dims4()?;
        self.o.forward(&w.matmul(v)?.transpose(1, 2)?.reshape((n, len, self.heads * self.d_kv))?)
    }
}

struct FeedForward {
    wi_0: Linear,
    wi_1: Linear,
    wo: Linear,
}

impl FeedForward {
    fn load(cfg: &Config, vb: VarBuilder) -> Result<FeedForward> {
        Ok(FeedForward {
            wi_0: linear_no_bias(cfg.d_model, cfg.d_ff, vb.pp("wi_0"))?,
            wi_1: linear_no_bias(cfg.d_model, cfg.d_ff, vb.pp("wi_1"))?,
            wo: linear_no_bias(cfg.d_ff, cfg.d_model, vb.pp("wo"))?,
        })
    }

    /// Gated GELU (`gelu_new`, the tanh form).
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        self.wo.forward(&(self.wi_0.forward(x)?.gelu()? * self.wi_1.forward(x)?)?)
    }
}

struct EncoderBlock {
    attn: Attention,
    ln_attn: RmsNorm,
    ff: FeedForward,
    ln_ff: RmsNorm,
}

struct DecoderBlock {
    attn: Attention,
    ln_attn: RmsNorm,
    cross: Attention,
    ln_cross: RmsNorm,
    ff: FeedForward,
    ln_ff: RmsNorm,
}

/// T5's relative position bucket (`_relative_position_bucket`): `rel` is key minus query.
fn bucket(rel: i64, bidirectional: bool, buckets: usize, max_distance: usize) -> usize {
    let (mut buckets, mut out, mut rel) = (buckets as i64, 0i64, rel);
    if bidirectional {
        buckets /= 2;
        if rel > 0 {
            out += buckets;
        }
        rel = rel.abs();
    } else {
        rel = -rel.min(0);
    }
    let exact = buckets / 2;
    out += if rel < exact {
        rel
    } else {
        let large = exact + ((rel as f64 / exact as f64).ln() / (max_distance as f64 / exact as f64).ln() * (buckets - exact) as f64) as i64;
        large.min(buckets - 1)
    };
    out as usize
}

/// One utterance's encoding: each decoder layer's cross-attention keys and values.
pub struct Encoded {
    cross: Vec<(Tensor, Tensor)>,
}

/// A decoding state: each decoder layer's self-attention keys and values for the positions so far
/// (`[1, heads, len, d_kv]`).
#[derive(Clone)]
pub struct State {
    kv: Vec<(Tensor, Tensor)>,
    pub len: usize,
}

pub struct T5 {
    shared: Embedding,
    encoder: Vec<EncoderBlock>,
    encoder_norm: RmsNorm,
    encoder_bias: Embedding,
    decoder: Vec<DecoderBlock>,
    decoder_norm: RmsNorm,
    decoder_bias: Embedding,
    head: Option<Linear>,
    cfg: Config,
    device: Device,
}

impl T5 {
    pub fn load(vb: VarBuilder, cfg: &Config) -> Result<T5> {
        let eps = cfg.layer_norm_epsilon;
        let (enc, dec) = (vb.pp("encoder"), vb.pp("decoder"));
        let encoder = (0..cfg.num_layers)
            .map(|i| {
                let b = enc.pp(format!("block.{i}"));
                Ok(EncoderBlock {
                    attn: Attention::load(cfg, b.pp("layer.0.SelfAttention"))?,
                    ln_attn: RmsNorm::load(cfg.d_model, eps, b.pp("layer.0.layer_norm"))?,
                    ff: FeedForward::load(cfg, b.pp("layer.1.DenseReluDense"))?,
                    ln_ff: RmsNorm::load(cfg.d_model, eps, b.pp("layer.1.layer_norm"))?,
                })
            })
            .collect::<Result<_>>()?;
        let decoder = (0..cfg.num_decoder_layers.unwrap_or(cfg.num_layers))
            .map(|i| {
                let b = dec.pp(format!("block.{i}"));
                Ok(DecoderBlock {
                    attn: Attention::load(cfg, b.pp("layer.0.SelfAttention"))?,
                    ln_attn: RmsNorm::load(cfg.d_model, eps, b.pp("layer.0.layer_norm"))?,
                    cross: Attention::load(cfg, b.pp("layer.1.EncDecAttention"))?,
                    ln_cross: RmsNorm::load(cfg.d_model, eps, b.pp("layer.1.layer_norm"))?,
                    ff: FeedForward::load(cfg, b.pp("layer.2.DenseReluDense"))?,
                    ln_ff: RmsNorm::load(cfg.d_model, eps, b.pp("layer.2.layer_norm"))?,
                })
            })
            .collect::<Result<_>>()?;
        let rel = |vb: VarBuilder| embedding(cfg.relative_attention_num_buckets, cfg.num_heads, vb.pp("block.0.layer.0.SelfAttention.relative_attention_bias"));
        Ok(T5 {
            shared: embedding(cfg.vocab_size, cfg.d_model, vb.pp("shared"))?,
            encoder,
            encoder_norm: RmsNorm::load(cfg.d_model, eps, enc.pp("final_layer_norm"))?,
            encoder_bias: rel(enc.clone())?,
            decoder,
            decoder_norm: RmsNorm::load(cfg.d_model, eps, dec.pp("final_layer_norm"))?,
            decoder_bias: rel(dec.clone())?,
            head: if cfg.tie_word_embeddings { None } else { Some(linear_no_bias(cfg.d_model, cfg.vocab_size, vb.pp("lm_head"))?) },
            cfg: cfg.clone(),
            device: vb.device().clone(),
        })
    }

    /// The position bias `[heads, queries, keys]` for queries at `q0..q0 + nq` over keys
    /// `0..nk`, with the decoder's causal mask.
    fn bias(&self, table: &Embedding, q0: usize, nq: usize, nk: usize, bidirectional: bool) -> Result<Tensor> {
        let (buckets, distance) = (self.cfg.relative_attention_num_buckets, self.cfg.relative_attention_max_distance);
        let ids: Vec<u32> = (0..nq).flat_map(|i| (0..nk).map(move |j| bucket(j as i64 - (q0 + i) as i64, bidirectional, buckets, distance) as u32)).collect();
        let ids = Tensor::from_vec(ids, (nq, nk), &self.device)?;
        let mut bias = table.forward(&ids)?.permute((2, 0, 1))?.contiguous()?;
        if !bidirectional {
            let mask: Vec<f32> = (0..nq).flat_map(|i| (0..nk).map(move |j| if j > q0 + i { f32::NEG_INFINITY } else { 0.0 })).collect();
            bias = bias.broadcast_add(&Tensor::from_vec(mask, (1, nq, nk), &self.device)?)?;
        }
        Ok(bias)
    }

    /// Encode `ids` (the tokenized prompt, with its end token) for decoding.
    pub fn encode(&self, ids: &[u32]) -> Result<Encoded> {
        let x = Tensor::new(ids, &self.device)?.unsqueeze(0)?;
        let mut x = self.shared.forward(&x)?;
        let bias = self.bias(&self.encoder_bias, 0, ids.len(), ids.len(), true)?;
        for b in &self.encoder {
            let h = b.ln_attn.forward(&x)?;
            let (k, v) = (b.attn.split(&b.attn.k.forward(&h)?)?, b.attn.split(&b.attn.v.forward(&h)?)?);
            x = (&x + b.attn.attend(&h, &k, &v, Some(&bias))?)?;
            x = (&x + b.ff.forward(&b.ln_ff.forward(&x)?)?)?;
        }
        let out = self.encoder_norm.forward(&x)?;
        let cross = self.decoder.iter().map(|b| Ok((b.cross.split(&b.cross.k.forward(&out)?)?, b.cross.split(&b.cross.v.forward(&out)?)?))).collect::<Result<_>>()?;
        Ok(Encoded { cross })
    }

    /// The empty state, before the decoder's start token.
    pub fn start(&self) -> Result<State> {
        let empty = Tensor::zeros((1, self.cfg.num_heads, 0, self.cfg.d_kv), DType::F32, &self.device)?;
        Ok(State { kv: self.decoder.iter().map(|_| (empty.clone(), empty.clone())).collect(), len: 0 })
    }

    /// Run `tokens` (`[n, m]`: n continuations of `state`, padded at their ends) through the
    /// decoder. Returns the log-probabilities at every position (`[n, m, vocab]`: row r's
    /// position j is the distribution of the token after its j-th), and the n grown states (each
    /// `state.len + m` long; a padded row's state is cut with [`State::keep`]).
    pub fn decode(&self, enc: &Encoded, state: &State, tokens: &Tensor) -> Result<(Tensor, Vec<State>)> {
        let (n, m) = tokens.dims2()?;
        let mut x = self.shared.forward(tokens)?;
        let bias = self.bias(&self.decoder_bias, state.len, m, state.len + m, false)?;
        let mut grown: Vec<(Tensor, Tensor)> = Vec::with_capacity(self.decoder.len());
        for (b, ((pk, pv), (ck, cv))) in self.decoder.iter().zip(state.kv.iter().zip(&enc.cross)) {
            let h = b.ln_attn.forward(&x)?;
            let (k, v) = (b.attn.split(&b.attn.k.forward(&h)?)?, b.attn.split(&b.attn.v.forward(&h)?)?);
            let expand = |t: &Tensor| t.broadcast_as((n, t.dim(1)?, t.dim(2)?, t.dim(3)?))?.contiguous();
            let (k, v) = (Tensor::cat(&[&expand(pk)?, &k], 2)?, Tensor::cat(&[&expand(pv)?, &v], 2)?);
            x = (&x + b.attn.attend(&h, &k, &v, Some(&bias))?)?;
            let h = b.ln_cross.forward(&x)?;
            x = (&x + b.cross.attend(&h, &expand(ck)?, &expand(cv)?, None)?)?;
            x = (&x + b.ff.forward(&b.ln_ff.forward(&x)?)?)?;
            grown.push((k, v));
        }
        let x = self.decoder_norm.forward(&x)?;
        let logits = match &self.head {
            Some(head) => head.forward(&x)?,
            None => (x * (self.cfg.d_model as f64).powf(-0.5))?.broadcast_matmul(&self.shared.embeddings().t()?)?,
        };
        let logp = candle_nn::ops::log_softmax(&logits, D::Minus1)?;
        let states = (0..n)
            .map(|r| Ok(State { kv: grown.iter().map(|(k, v)| Ok((k.narrow(0, r, 1)?, v.narrow(0, r, 1)?))).collect::<Result<_>>()?, len: state.len + m }))
            .collect::<Result<_>>()?;
        Ok((logp, states))
    }
}

impl State {
    /// The state with only its first `len` positions (a padded row's own).
    pub fn keep(&self, len: usize) -> Result<State> {
        Ok(State { kv: self.kv.iter().map(|(k, v)| Ok((k.narrow(2, 0, len)?, v.narrow(2, 0, len)?))).collect::<Result<_>>()?, len })
    }
}

#[cfg(test)]
mod tests {
    use super::bucket;

    #[test]
    fn buckets_are_t5s() {
        // transformers' _relative_position_bucket with 32 buckets, distance 128.
        assert_eq!(bucket(0, true, 32, 128), 0);
        assert_eq!(bucket(3, true, 32, 128), 19);
        assert_eq!(bucket(-3, true, 32, 128), 3);
        assert_eq!(bucket(-200, true, 32, 128), 15);
        assert_eq!(bucket(-5, false, 32, 128), 5);
        assert_eq!(bucket(5, false, 32, 128), 0);
        assert_eq!(bucket(-40, false, 32, 128), 23);
        assert_eq!(bucket(-1000, false, 32, 128), 31);
    }
}
