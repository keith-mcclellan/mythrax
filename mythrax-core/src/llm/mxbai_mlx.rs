#![cfg(feature = "mlx")]

use std::collections::HashMap;
use std::path::Path;
use anyhow::{Result, Context};
use tokenizers::Tokenizer;
use mlx_rs::{Array, StreamOrDevice};
use mlx_rs::nn::{Embedding, RmsNorm, Linear};
use mlx_rs::module::Module;
use crate::llm::mlx_weights::{get_linear, get_rms_norm, get_embedding, load_model_weights};

pub struct Qwen2Attention {
    pub q_proj: Linear,
    pub k_proj: Linear,
    pub v_proj: Linear,
    pub o_proj: Linear,
    pub num_heads: i32,
    pub num_kv_heads: i32,
    pub head_dim: i32,
    pub rope_theta: f32,
}

impl Qwen2Attention {
    pub fn new(
        weights: &HashMap<String, Array>,
        prefix: &str,
        num_heads: i32,
        num_kv_heads: i32,
        head_dim: i32,
        rope_theta: f32,
    ) -> Result<Self> {
        let q_proj = get_linear(weights, &format!("{}.q_proj", prefix), true)?;
        let k_proj = get_linear(weights, &format!("{}.k_proj", prefix), true)?;
        let v_proj = get_linear(weights, &format!("{}.v_proj", prefix), true)?;
        let o_proj = get_linear(weights, &format!("{}.o_proj", prefix), false)?;
        Ok(Self {
            q_proj,
            k_proj,
            v_proj,
            o_proj,
            num_heads,
            num_kv_heads,
            head_dim,
            rope_theta,
        })
    }

    pub fn forward(&mut self, x: &Array, mask: Option<&Array>) -> Result<Array> {
        let q = self.q_proj.forward(x)
            .map_err(|e| anyhow::anyhow!("Q proj failed: {:?}", e))?;
        let k = self.k_proj.forward(x)
            .map_err(|e| anyhow::anyhow!("K proj failed: {:?}", e))?;
        let v = self.v_proj.forward(x)
            .map_err(|e| anyhow::anyhow!("V proj failed: {:?}", e))?;

        let shape = q.shape();
        let batch = shape[0];
        let seq_len = shape[1];

        let q = q.reshape(&[batch, seq_len, self.num_heads, self.head_dim])?;
        let k = k.reshape(&[batch, seq_len, self.num_kv_heads, self.head_dim])?;
        let v = v.reshape(&[batch, seq_len, self.num_kv_heads, self.head_dim])?;

        // Apply RoPE
        let q = mlx_rs::fast::rope_device(
            &q,
            self.head_dim,
            false,
            Some(self.rope_theta),
            1.0,
            0,
            None,
            StreamOrDevice::gpu(),
        ).map_err(|e| anyhow::anyhow!("RoPE Q failed: {:?}", e))?;

        let k = mlx_rs::fast::rope_device(
            &k,
            self.head_dim,
            false,
            Some(self.rope_theta),
            1.0,
            0,
            None,
            StreamOrDevice::gpu(),
        ).map_err(|e| anyhow::anyhow!("RoPE K failed: {:?}", e))?;

        // Transpose to [batch, heads, seq_len, head_dim]
        let q = q.transpose_axes_device(&[0, 2, 1, 3], StreamOrDevice::gpu())?;
        let mut k = k.transpose_axes_device(&[0, 2, 1, 3], StreamOrDevice::gpu())?;
        let mut v = v.transpose_axes_device(&[0, 2, 1, 3], StreamOrDevice::gpu())?;

        // Repeat KV heads if using GQA
        if self.num_heads != self.num_kv_heads {
            let reps = self.num_heads / self.num_kv_heads;
            k = mlx_rs::ops::repeat_axis_device::<half::f16>(k, reps as i32, 1, StreamOrDevice::gpu())?;
            v = mlx_rs::ops::repeat_axis_device::<half::f16>(v, reps as i32, 1, StreamOrDevice::gpu())?;
        }

        let scale = 1.0 / (self.head_dim as f32).sqrt();

        // Cast mask to query dtype to avoid type promotion failure (bfloat16 vs float32)
        let cast_mask = match mask {
            Some(m) => Some(m.as_dtype(q.dtype()).map_err(|e| anyhow::anyhow!("Mask cast failed: {:?}", e))?),
            None => None,
        };

        // SDP attention
        let out = mlx_rs::fast::scaled_dot_product_attention_device(
            &q,
            &k,
            &v,
            scale,
            cast_mask.as_ref().map(|m| m.into()),
            StreamOrDevice::gpu(),
        ).map_err(|e| anyhow::anyhow!("SDP Attention failed: {:?}", e))?;

        // Transpose back to [batch, seq_len, num_heads * head_dim]
        let out = out.transpose_axes_device(&[0, 2, 1, 3], StreamOrDevice::gpu())?;
        let out = out.reshape(&[batch, seq_len, self.num_heads * self.head_dim])?;

        let out = self.o_proj.forward(&out)
            .map_err(|e| anyhow::anyhow!("O proj failed: {:?}", e))?;
        Ok(out)
    }
}

pub struct Qwen2MLP {
    pub gate_proj: Linear,
    pub up_proj: Linear,
    pub down_proj: Linear,
}

impl Qwen2MLP {
    pub fn new(weights: &HashMap<String, Array>, prefix: &str) -> Result<Self> {
        let gate_proj = get_linear(weights, &format!("{}.gate_proj", prefix), false)?;
        let up_proj = get_linear(weights, &format!("{}.up_proj", prefix), false)?;
        let down_proj = get_linear(weights, &format!("{}.down_proj", prefix), false)?;
        Ok(Self { gate_proj, up_proj, down_proj })
    }

    pub fn forward(&mut self, x: &Array) -> Result<Array> {
        let g = self.gate_proj.forward(x)
            .map_err(|e| anyhow::anyhow!("Gate proj failed: {:?}", e))?;
        let u = self.up_proj.forward(x)
            .map_err(|e| anyhow::anyhow!("Up proj failed: {:?}", e))?;
        let silu_g = g.multiply(&mlx_rs::ops::sigmoid(&g)?)?;
        let activated = silu_g.multiply(&u)?;
        let out = self.down_proj.forward(&activated)
            .map_err(|e| anyhow::anyhow!("Down proj failed: {:?}", e))?;
        Ok(out)
    }
}

pub struct Qwen2DecoderLayer {
    pub input_layernorm: RmsNorm,
    pub self_attn: Qwen2Attention,
    pub post_attention_layernorm: RmsNorm,
    pub mlp: Qwen2MLP,
}

impl Qwen2DecoderLayer {
    pub fn new(
        weights: &HashMap<String, Array>,
        prefix: &str,
        num_heads: i32,
        num_kv_heads: i32,
        head_dim: i32,
        rope_theta: f32,
        rms_norm_eps: f32,
    ) -> Result<Self> {
        let input_layernorm = get_rms_norm(weights, &format!("{}.input_layernorm.weight", prefix), rms_norm_eps)?;
        let self_attn = Qwen2Attention::new(weights, &format!("{}.self_attn", prefix), num_heads, num_kv_heads, head_dim, rope_theta)?;
        let post_attention_layernorm = get_rms_norm(weights, &format!("{}.post_attention_layernorm.weight", prefix), rms_norm_eps)?;
        let mlp = Qwen2MLP::new(weights, &format!("{}.mlp", prefix))?;
        Ok(Self {
            input_layernorm,
            self_attn,
            post_attention_layernorm,
            mlp,
        })
    }
}

pub struct Qwen2Model {
    pub embed_tokens: Embedding,
    pub layers: Vec<Qwen2DecoderLayer>,
    pub norm: RmsNorm,
}

impl Qwen2Model {
    pub fn new(
        weights: &HashMap<String, Array>,
        num_layers: i32,
        num_heads: i32,
        num_kv_heads: i32,
        head_dim: i32,
        rope_theta: f32,
        rms_norm_eps: f32,
    ) -> Result<Self> {
        let embed_tokens = match get_embedding(weights, "model.embed_tokens", 0, 0)? {
            crate::llm::qwen2_mlx::QwenEmbedding::Dense(emb) => emb,
            _ => anyhow::bail!("Expected dense embedding weights"),
        };
        let norm = get_rms_norm(weights, "model.norm.weight", rms_norm_eps)?;
        let mut layers = Vec::new();
        for i in 0..num_layers {
            layers.push(Qwen2DecoderLayer::new(weights, &format!("model.layers.{}", i), num_heads, num_kv_heads, head_dim, rope_theta, rms_norm_eps)?);
        }
        Ok(Self { embed_tokens, layers, norm })
    }

    pub fn forward(&mut self, ids: &Array, mask: Option<&Array>) -> Result<Array> {
        let mut x = self.embed_tokens.forward(ids)
            .map_err(|e| anyhow::anyhow!("Embed forward failed: {:?}", e))?;
        for layer in &mut self.layers {
            let h = layer.input_layernorm.forward(&x)
                .map_err(|e| anyhow::anyhow!("Input norm failed: {:?}", e))?;
            let attn = layer.self_attn.forward(&h, mask)?;
            x = x.add(&attn)?;

            let h = layer.post_attention_layernorm.forward(&x)
                .map_err(|e| anyhow::anyhow!("Post attn norm failed: {:?}", e))?;
            let mlp = layer.mlp.forward(&h)?;
            x = x.add(&mlp)?;
        }
        let out = self.norm.forward(&x)
            .map_err(|e| anyhow::anyhow!("Final norm failed: {:?}", e))?;
        Ok(out)
    }
}

pub struct MxbaiReranker {
    pub model: Qwen2Model,
    pub tokenizer: Tokenizer,
}

impl MxbaiReranker {
    pub fn load(model_dir: &Path) -> Result<Self> {
        println!("!!! LOADING CROSS-ENCODER MODEL FROM DISK: {:?} !!!", model_dir);
        let config_path = model_dir.join("config.json");
        let config_str = std::fs::read_to_string(config_path)?;
        let config: serde_json::Value = serde_json::from_str(&config_str)?;

        let num_layers = config["num_hidden_layers"].as_i64().context("num_hidden_layers not found")? as i32;
        let num_heads = config["num_attention_heads"].as_i64().context("num_attention_heads not found")? as i32;
        let num_kv_heads = config["num_key_value_heads"].as_i64().context("num_key_value_heads not found")? as i32;
        let hidden_size = config["hidden_size"].as_i64().context("hidden_size not found")? as i32;
        let head_dim = hidden_size / num_heads;
        let rope_theta = config["rope_theta"].as_f64().unwrap_or(1000000.0) as f32;
        let rms_norm_eps = config["rms_norm_eps"].as_f64().unwrap_or(1e-6) as f32;

        let weights = load_model_weights(model_dir)?;
        let model = Qwen2Model::new(&weights, num_layers, num_heads, num_kv_heads, head_dim, rope_theta, rms_norm_eps)?;

        let tokenizer = match Tokenizer::from_file(model_dir.join("tokenizer.json")) {
            Ok(t) => t,
            Err(_) => {
                let parent_dir = model_dir.parent().unwrap();
                let paths = [
                    parent_dir.join("mlx-community_Qwen3.6-35B-A3B-4bit/tokenizer.json"),
                    parent_dir.join("mlx-community_Qwen2.5-0.5B-Instruct-4bit/tokenizer.json"),
                ];
                let mut found_tok = None;
                for p in &paths {
                    if p.exists() {
                        if let Ok(t) = Tokenizer::from_file(p) {
                            found_tok = Some(t);
                            break;
                        }
                    }
                }
                found_tok.context("Failed to load any valid Qwen2 tokenizer")?
            }
        };

        Ok(Self { model, tokenizer })
    }

    pub fn score_pairs(&mut self, query: &str, passages: &[&str]) -> Result<Vec<f32>> {
        use mlx_rs::ops::indexing::TryIndexOp;

        if passages.is_empty() {
            return Ok(Vec::new());
        }

        // 1. Compute null logits for query-only baseline
        let null_text = format!("query: {} document: ", query);
        let null_encoding = self.tokenizer.encode(null_text, false)
            .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;
        let mut null_ids: Vec<i32> = null_encoding.get_ids().iter().map(|&x| x as i32).collect();
        let null_seq_len = null_ids.len();
        let null_seq_len_bucketed = ((null_seq_len + 63) / 64) * 64;
        let null_pad_len = null_seq_len_bucketed - null_seq_len;
        for _ in 0..null_pad_len {
            null_ids.push(151643); // pad_token_id
        }
        let null_ids_array = mlx_rs::Array::from_slice(&null_ids, &[1, null_seq_len_bucketed as i32]);
        let null_seq_len_i32 = null_seq_len_bucketed as i32;
        let null_causal_mask = mlx_rs::ops::full::<f32>(&[null_seq_len_i32, null_seq_len_i32], &mlx_rs::Array::from(f32::NEG_INFINITY))
            .map_err(|e| anyhow::anyhow!("Full failed: {:?}", e))?;
        let null_causal_mask = mlx_rs::ops::triu_device(&null_causal_mask, 1, StreamOrDevice::gpu())
            .map_err(|e| anyhow::anyhow!("Triu failed: {:?}", e))?;
        let null_mask_4d = null_causal_mask.reshape(&[1, 1, null_seq_len_i32, null_seq_len_i32])
            .map_err(|e| anyhow::anyhow!("Reshape failed: {:?}", e))?;
        let null_out = self.model.forward(&null_ids_array, Some(&null_mask_4d))?;
        let null_last_hidden = null_out.try_index((0, (null_seq_len - 1) as i32, ..))?;

        let embed_w = self.model.embed_tokens.weight.value.clone();
        let w_0 = embed_w.try_index((15, ..))?; // "0" token
        let w_1 = embed_w.try_index((16, ..))?; // "1" token

        let null_logit_0 = null_last_hidden.multiply(&w_0)?.sum_axes(&[-1], false)?;
        let null_logit_1 = null_last_hidden.multiply(&w_1)?.sum_axes(&[-1], false)?;
        let nl0 = null_logit_0.as_dtype(mlx_rs::Dtype::Float32)?.as_slice::<f32>()[0];
        let nl1 = null_logit_1.as_dtype(mlx_rs::Dtype::Float32)?.as_slice::<f32>()[0];

        // 2. Compute logits sequentially for each passage to preserve correct positional RoPE indices
        let mut scores = Vec::with_capacity(passages.len());
        for passage in passages {
            let truncated_passage = if passage.len() > 1000 {
                let mut end = 1000;
                while end > 0 && !passage.is_char_boundary(end) {
                    end -= 1;
                }
                &passage[..end]
            } else {
                passage
            };
            let text = format!("query: {} document: {}", query, truncated_passage);
            let encoding = self.tokenizer.encode(text, false)
                .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;
            let mut ids: Vec<i32> = encoding.get_ids().iter().map(|&x| x as i32).collect();
            let seq_len = ids.len();
            let seq_len_bucketed = ((seq_len + 63) / 64) * 64;
            let pad_len = seq_len_bucketed - seq_len;
            for _ in 0..pad_len {
                ids.push(151643); // pad_token_id
            }
            let ids_array = mlx_rs::Array::from_slice(&ids, &[1, seq_len_bucketed as i32]);
            let seq_len_i32 = seq_len_bucketed as i32;
            let causal_mask = mlx_rs::ops::full::<f32>(&[seq_len_i32, seq_len_i32], &mlx_rs::Array::from(f32::NEG_INFINITY))
                .map_err(|e| anyhow::anyhow!("Full failed: {:?}", e))?;
            let causal_mask = mlx_rs::ops::triu_device(&causal_mask, 1, StreamOrDevice::gpu())
                .map_err(|e| anyhow::anyhow!("Triu failed: {:?}", e))?;
            let mask_4d = causal_mask.reshape(&[1, 1, seq_len_i32, seq_len_i32])
                .map_err(|e| anyhow::anyhow!("Reshape failed: {:?}", e))?;
            let out = self.model.forward(&ids_array, Some(&mask_4d))?;
            let last_hidden = out.try_index((0, (seq_len - 1) as i32, ..))?;

            let logit_0 = last_hidden.multiply(&w_0)?.sum_axes(&[-1], false)?;
            let logit_1 = last_hidden.multiply(&w_1)?.sum_axes(&[-1], false)?;
            let raw_l0 = logit_0.as_dtype(mlx_rs::Dtype::Float32)?.as_slice::<f32>()[0];
            let raw_l1 = logit_1.as_dtype(mlx_rs::Dtype::Float32)?.as_slice::<f32>()[0];

            // Apply null calibration
            let l0 = raw_l0 - nl0;
            let l1 = raw_l1 - nl1;

            // Stable softmax
            let max_l = l0.max(l1);
            let exp_l0 = (l0 - max_l).exp();
            let exp_l1 = (l1 - max_l).exp();
            let prob_1 = exp_l1 / (exp_l0 + exp_l1);
            scores.push(prob_1);
        }

        Ok(scores)
    }
}
