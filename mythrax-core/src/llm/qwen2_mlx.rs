use std::collections::HashMap;
use anyhow::Result;
use mlx_rs::{Array, StreamOrDevice};
use mlx_rs::module::Module;
use mlx_rs::nn::{Embedding, QuantizedEmbedding, RmsNorm, QuantizedLinear, Linear};
use crate::llm::mlx_weights::{get_quantized_linear, get_rms_norm, get_linear, get_embedding};
use mlx_rs::ops::indexing::TryIndexMutOp;

#[derive(Debug, Clone)]
pub enum QwenEmbedding {
    Dense(Embedding),
    Quantized(QuantizedEmbedding),
}

impl QwenEmbedding {
    pub fn forward(&mut self, x: &Array) -> Result<Array, mlx_rs::error::Exception> {
        match self {
            Self::Dense(emb) => emb.forward(x),
            Self::Quantized(qemb) => {
                use mlx_rs::ops::indexing::IndexOp;
                let s = x.shape();
                let x_flat = x.flatten(None, None)?;
                
                let w = qemb.inner.weight.value.index(&x_flat);
                let scales = qemb.scales.value.index(&x_flat);
                let biases = qemb.biases.value.index(&x_flat);
                
                w.eval()?;
                scales.eval()?;
                biases.eval()?;
                
                let out = mlx_rs::ops::dequantize_device(
                    &w,
                    &scales,
                    &biases,
                    qemb.group_size,
                    qemb.bits,
                    StreamOrDevice::gpu()
                )?;
                
                let mut ret_shape = s.to_vec();
                ret_shape.push(-1);
                out.reshape(&ret_shape)
            }
        }
    }

    pub fn as_linear(&self, x: &Array) -> Result<Array, mlx_rs::error::Exception> {
        match self {
            Self::Dense(emb) => emb.as_linear(x),
            Self::Quantized(qemb) => qemb.as_linear(x),
        }
    }
}


pub struct Qwen2Attention {
    pub q_proj: QuantizedLinear,
    pub k_proj: QuantizedLinear,
    pub v_proj: QuantizedLinear,
    pub o_proj: QuantizedLinear,
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
        group_size: i32,
        bits: i32,
    ) -> Result<Self> {
        let q_proj = get_quantized_linear(weights, &format!("{}.q_proj", prefix), true, group_size, bits)?;
        let k_proj = get_quantized_linear(weights, &format!("{}.k_proj", prefix), true, group_size, bits)?;
        let v_proj = get_quantized_linear(weights, &format!("{}.v_proj", prefix), true, group_size, bits)?;
        let o_proj = get_quantized_linear(weights, &format!("{}.o_proj", prefix), false, group_size, bits)?;
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

    pub fn forward(
        &mut self,
        x: &Array,
        mask: Option<&Array>,
        position_offset: i32,
        cache: Option<&mut (Array, Array)>,
    ) -> Result<Array> {
        let q = self.q_proj.forward(x)?;
        let k = self.k_proj.forward(x)?;
        let v = self.v_proj.forward(x)?;

        let shape = q.shape();
        let batch = shape[0];
        let seq_len = shape[1];

        // Reshape to [batch, seq_len, heads, head_dim]
        let q = q.reshape(&[batch, seq_len, self.num_heads, self.head_dim])?;
        let k = k.reshape(&[batch, seq_len, self.num_kv_heads, self.head_dim])?;
        let v = v.reshape(&[batch, seq_len, self.num_kv_heads, self.head_dim])?;

        // Apply RoPE to Q and K
        let q = mlx_rs::fast::rope_device(
            &q,
            self.head_dim,
            false,
            Some(self.rope_theta),
            1.0,
            position_offset,
            None,
            StreamOrDevice::gpu(),
        ).map_err(|e| anyhow::anyhow!("RoPE Q failed: {:?}", e))?;

        let k = mlx_rs::fast::rope_device(
            &k,
            self.head_dim,
            false,
            Some(self.rope_theta),
            1.0,
            position_offset,
            None,
            StreamOrDevice::gpu(),
        ).map_err(|e| anyhow::anyhow!("RoPE K failed: {:?}", e))?;

        // Transpose to [batch, heads, seq_len, head_dim]
        let q = q.transpose_axes_device(&[0, 2, 1, 3], StreamOrDevice::gpu())?;
        let mut k = k.transpose_axes_device(&[0, 2, 1, 3], StreamOrDevice::gpu())?;
        let mut v = v.transpose_axes_device(&[0, 2, 1, 3], StreamOrDevice::gpu())?;

        // Concatenate along the sequence axis (axis 2) if cache is present
        if let Some((cached_k, cached_v)) = cache {
            if cached_k.shape()[2] > 0 {
                k = mlx_rs::ops::concatenate_axis_device(&[cached_k.clone(), k], 2, StreamOrDevice::gpu())?;
                v = mlx_rs::ops::concatenate_axis_device(&[cached_v.clone(), v], 2, StreamOrDevice::gpu())?;
            }
            *cached_k = k.clone();
            *cached_v = v.clone();
        }

        // Repeat KV heads if using GQA
        if self.num_heads != self.num_kv_heads {
            let reps = self.num_heads / self.num_kv_heads;
            k = mlx_rs::ops::repeat_axis_device::<f32>(k, reps as i32, 1, StreamOrDevice::gpu())?;
            v = mlx_rs::ops::repeat_axis_device::<f32>(v, reps as i32, 1, StreamOrDevice::gpu())?;
        }

        let scale = 1.0 / (self.head_dim as f32).sqrt();

        // SDP attention
        let out = mlx_rs::fast::scaled_dot_product_attention_device(
            &q,
            &k,
            &v,
            scale,
            mask.map(|m| m.into()),
            StreamOrDevice::gpu(),
        ).map_err(|e| anyhow::anyhow!("SDP Attention failed: {:?}", e))?;

        // Transpose back to [batch, seq_len, num_heads * head_dim]
        let out = out.transpose_axes_device(&[0, 2, 1, 3], StreamOrDevice::gpu())?;
        let out = out.reshape(&[batch, seq_len, self.num_heads * self.head_dim])?;

        let out = self.o_proj.forward(&out)?;
        Ok(out)
    }
}

pub struct Qwen2MLP {
    pub gate_proj: QuantizedLinear,
    pub up_proj: QuantizedLinear,
    pub down_proj: QuantizedLinear,
}

impl Qwen2MLP {
    pub fn new(
        weights: &HashMap<String, Array>,
        prefix: &str,
        group_size: i32,
        bits: i32,
    ) -> Result<Self> {
        let gate_proj = get_quantized_linear(weights, &format!("{}.gate_proj", prefix), false, group_size, bits)?;
        let up_proj = get_quantized_linear(weights, &format!("{}.up_proj", prefix), false, group_size, bits)?;
        let down_proj = get_quantized_linear(weights, &format!("{}.down_proj", prefix), false, group_size, bits)?;
        Ok(Self { gate_proj, up_proj, down_proj })
    }

    pub fn forward(&mut self, x: &Array) -> Result<Array> {
        let g = self.gate_proj.forward(x)?;
        let u = self.up_proj.forward(x)?;
        let silu_g = g.multiply(&mlx_rs::ops::sigmoid(&g)?)?;
        let activated = silu_g.multiply(&u)?;
        let out = self.down_proj.forward(&activated)?;
        Ok(out)
    }
}

pub struct Qwen3MoEMLP {
    pub gate: Linear,
    pub experts: Vec<Qwen2MLP>,
    pub shared_expert: Qwen2MLP,
    pub num_experts_per_tok: i32,
}

impl Qwen3MoEMLP {
    pub fn new(
        weights: &HashMap<String, Array>,
        prefix: &str,
        num_experts: i32,
        num_experts_per_tok: i32,
        group_size: i32,
        bits: i32,
    ) -> Result<Self> {
        let gate = get_linear(weights, &format!("{}.gate", prefix), false)?;
        
        let mut experts = Vec::new();
        for i in 0..num_experts {
            let expert = Qwen2MLP::new(weights, &format!("{}.experts.{}", prefix, i), group_size, bits)?;
            experts.push(expert);
        }
        
        let shared_expert = Qwen2MLP::new(weights, &format!("{}.shared_expert", prefix), group_size, bits)?;
        
        Ok(Self {
            gate,
            experts,
            shared_expert,
            num_experts_per_tok,
        })
    }

    pub fn forward(&mut self, x: &Array) -> Result<Array> {
        let shape = x.shape();
        let batch = shape[0];
        let seq_len = shape[1];
        let dim = shape[2];

        // Reshape x to [batch * seq_len, dim]
        let x_flat = x.reshape(&[batch * seq_len, dim])?;

        // 1. Router logits and softmax scores
        let router_logits = self.gate.forward(&x_flat)?;
        let scores = mlx_rs::ops::softmax_axis_device(&router_logits, -1, false, StreamOrDevice::gpu())?;

        // 2. Select top-k experts
        let num_experts = self.experts.len() as i32;
        let k = self.num_experts_per_tok;
        
        let sorted_indices = mlx_rs::ops::argsort_axis_device(&scores, -1, StreamOrDevice::gpu())?;
        use mlx_rs::ops::indexing::TryIndexOp;
        let top_k_indices = sorted_indices.try_index((.., (num_experts - k)..num_experts))?;
        let top_k_gates = mlx_rs::ops::indexing::take_along_axis_device(&scores, &top_k_indices, -1, StreamOrDevice::gpu())?;

        let sum_gates = top_k_gates.sum_axes_device(&[-1], true, StreamOrDevice::gpu())?;
        let top_k_gates = top_k_gates.divide(&sum_gates)?;

        // 3. Initialize output FFN tensor with zeros
        let mut ffn_out = Array::zeros::<f32>(&[batch * seq_len, dim])?;

        // 4. Vectorized routing for each expert
        for e in 0..num_experts {
            let expert_mask_expanded = top_k_indices.eq_device(&Array::from(e as i32), StreamOrDevice::gpu())?;
            let expert_mask = expert_mask_expanded.any_axes_device(&[1], false, StreamOrDevice::gpu())?;

            let any_selected = expert_mask.any_device(false, StreamOrDevice::gpu())?;
            any_selected.eval()?;
            if any_selected.as_slice::<bool>()[0] {
                let x_expert = x_flat.try_index(expert_mask.clone())?;
                let out_expert = self.experts[e as usize].forward(&x_expert)?;

                let gate_masked = expert_mask_expanded.as_dtype(mlx_rs::Dtype::Float32)?
                    .multiply(&top_k_gates)?;
                let expert_gates = gate_masked.sum_axes_device(&[1], false, StreamOrDevice::gpu())?;
                let active_gates = expert_gates.try_index(expert_mask.clone())?;

                let out_expert_weighted = out_expert.multiply(&active_gates.reshape(&[-1, 1])?)?;

                let current_vals = ffn_out.try_index(expert_mask.clone())?;
                let updated_vals = current_vals.add(&out_expert_weighted)?;
                ffn_out.try_index_mut(expert_mask.clone(), updated_vals)?;
            }
        }

        // 5. Add shared expert output
        let shared_out = self.shared_expert.forward(&x_flat)?;
        let ffn_out = ffn_out.add(&shared_out)?;

        let out = ffn_out.reshape(&[batch, seq_len, dim])?;
        Ok(out)
    }
}

pub enum MLPLayer {
    Dense(Qwen2MLP),
    MoE(Qwen3MoEMLP),
}

impl MLPLayer {
    pub fn forward(&mut self, x: &Array) -> Result<Array> {
        match self {
            MLPLayer::Dense(mlp) => mlp.forward(x),
            MLPLayer::MoE(moe) => moe.forward(x),
        }
    }
}

pub struct Qwen2DecoderLayer {
    pub self_attn: Qwen2Attention,
    pub mlp: MLPLayer,
    pub input_layernorm: RmsNorm,
    pub post_attention_layernorm: RmsNorm,
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
        group_size: i32,
        bits: i32,
        num_experts: Option<i32>,
        num_experts_per_tok: Option<i32>,
    ) -> Result<Self> {
        let self_attn = Qwen2Attention::new(
            weights,
            &format!("{}.self_attn", prefix),
            num_heads,
            num_kv_heads,
            head_dim,
            rope_theta,
            group_size,
            bits,
        )?;
        
        let mlp = if let Some(n_exp) = num_experts {
            MLPLayer::MoE(Qwen3MoEMLP::new(
                weights,
                &format!("{}.mlp", prefix),
                n_exp,
                num_experts_per_tok.unwrap_or(8),
                group_size,
                bits,
            )?)
        } else {
            MLPLayer::Dense(Qwen2MLP::new(weights, &format!("{}.mlp", prefix), group_size, bits)?)
        };

        let input_layernorm = get_rms_norm(weights, &format!("{}.input_layernorm.weight", prefix), rms_norm_eps)?;
        let post_attention_layernorm = get_rms_norm(weights, &format!("{}.post_attention_layernorm.weight", prefix), rms_norm_eps)?;
        Ok(Self {
            self_attn,
            mlp,
            input_layernorm,
            post_attention_layernorm,
        })
    }

    pub fn forward(
        &mut self,
        x: &Array,
        mask: Option<&Array>,
        position_offset: i32,
        cache: Option<&mut (Array, Array)>,
    ) -> Result<Array> {
        let residual = x;
        let norm_x = self.input_layernorm.forward(x)?;
        let attn_out = self.self_attn.forward(&norm_x, mask, position_offset, cache)?;
        let x = residual.add(&attn_out)?;

        let residual = &x;
        let norm_x = self.post_attention_layernorm.forward(&x)?;
        let mlp_out = self.mlp.forward(&norm_x)?;
        let out = residual.add(&mlp_out)?;
        Ok(out)
    }
}

pub struct Qwen2Model {
    pub embed_tokens: QwenEmbedding,
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
        _vocab_size: i32,
        _hidden_size: i32,
        group_size: i32,
        bits: i32,
        num_experts: Option<i32>,
        num_experts_per_tok: Option<i32>,
    ) -> Result<Self> {
        let embed_tokens = get_embedding(weights, "model.embed_tokens", group_size, bits)?;

        let norm = get_rms_norm(weights, "model.norm.weight", rms_norm_eps)?;

        let mut layers = Vec::new();
        for i in 0..num_layers {
            let layer = Qwen2DecoderLayer::new(
                weights,
                &format!("model.layers.{}", i),
                num_heads,
                num_kv_heads,
                head_dim,
                rope_theta,
                rms_norm_eps,
                group_size,
                bits,
                num_experts,
                num_experts_per_tok,
            )?;
            layers.push(layer);
        }

        Ok(Self {
            embed_tokens,
            layers,
            norm,
        })
    }

    pub fn forward(
        &mut self,
        ids: &Array,
        mask: Option<&Array>,
        position_offset: i32,
        kv_cache: &mut Vec<(Array, Array)>,
    ) -> Result<Array> {
        let mut x = self.embed_tokens.forward(ids)?;

        for (i, layer) in self.layers.iter_mut().enumerate() {
            let cache = if i < kv_cache.len() {
                Some(&mut kv_cache[i])
            } else {
                None
            };
            x = layer.forward(&x, mask, position_offset, cache)?;
        }

        let out = self.norm.forward(&x)?;
        let logits = self.embed_tokens.as_linear(&out)
            .map_err(|e| anyhow::anyhow!("embed_tokens.as_linear failed: {:?}", e))?;
        Ok(logits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlx_rs::Array;

    fn mock_quantized_linear_weights(weights: &mut HashMap<String, Array>, prefix: &str, in_dims: i32, out_dims: i32, group_size: i32, bits: i32) {
        let packed_cols = in_dims / (32 / bits);
        let group_count = in_dims / group_size;

        weights.insert(format!("{}.weight", prefix), Array::zeros::<u32>(&[out_dims, packed_cols]).unwrap());
        weights.insert(format!("{}.scales", prefix), Array::zeros::<f32>(&[out_dims, group_count]).unwrap());
        weights.insert(format!("{}.biases", prefix), Array::zeros::<f32>(&[out_dims, group_count]).unwrap());
    }

    #[test]
    fn test_qwen2_forward_pass() {
        let mut weights = HashMap::new();
        let vocab_size = 100;
        let hidden_size = 128;
        let num_heads = 2;
        let num_kv_heads = 1;
        let head_dim = 64;
        let group_size = 64;
        let bits = 4;
        let rms_norm_eps = 1e-6;

        // Embed tokens weight
        weights.insert("model.embed_tokens.weight".to_string(), Array::zeros::<f32>(&[vocab_size, hidden_size]).unwrap());
        // Model norm weight
        weights.insert("model.norm.weight".to_string(), Array::zeros::<f32>(&[hidden_size]).unwrap());

        // Layer 0 weights
        let layer_prefix = "model.layers.0";
        // input_layernorm
        weights.insert(format!("{}.input_layernorm.weight", layer_prefix), Array::zeros::<f32>(&[hidden_size]).unwrap());
        // post_attention_layernorm
        weights.insert(format!("{}.post_attention_layernorm.weight", layer_prefix), Array::zeros::<f32>(&[hidden_size]).unwrap());

        // self_attn quantized projections
        mock_quantized_linear_weights(&mut weights, &format!("{}.self_attn.q_proj", layer_prefix), hidden_size, hidden_size, group_size, bits);
        mock_quantized_linear_weights(&mut weights, &format!("{}.self_attn.k_proj", layer_prefix), hidden_size, num_kv_heads * head_dim, group_size, bits);
        mock_quantized_linear_weights(&mut weights, &format!("{}.self_attn.v_proj", layer_prefix), hidden_size, num_kv_heads * head_dim, group_size, bits);
        mock_quantized_linear_weights(&mut weights, &format!("{}.self_attn.o_proj", layer_prefix), hidden_size, hidden_size, group_size, bits);

        // self_attn biases for qkv
        weights.insert(format!("{}.self_attn.q_proj.bias", layer_prefix), Array::zeros::<f32>(&[hidden_size]).unwrap());
        weights.insert(format!("{}.self_attn.k_proj.bias", layer_prefix), Array::zeros::<f32>(&[num_kv_heads * head_dim]).unwrap());
        weights.insert(format!("{}.self_attn.v_proj.bias", layer_prefix), Array::zeros::<f32>(&[num_kv_heads * head_dim]).unwrap());

        // mlp projections
        mock_quantized_linear_weights(&mut weights, &format!("{}.mlp.gate_proj", layer_prefix), hidden_size, 128, group_size, bits);
        mock_quantized_linear_weights(&mut weights, &format!("{}.mlp.up_proj", layer_prefix), hidden_size, 128, group_size, bits);
        mock_quantized_linear_weights(&mut weights, &format!("{}.mlp.down_proj", layer_prefix), 128, hidden_size, group_size, bits);

        let mut model = Qwen2Model::new(
            &weights,
            1, // num_layers
            num_heads,
            num_kv_heads,
            head_dim,
            1000000.0, // rope_theta
            rms_norm_eps,
            vocab_size,
            hidden_size,
            group_size,
            bits,
            None,
            None,
        ).unwrap();

        // Run forward pass with input sequence
        let ids = Array::from_slice(&[1, 2, 3, 4], &[1, 4]);
        let mut kv_cache = vec![
            (
                mlx_rs::ops::zeros::<f32>(&[1, num_kv_heads, 0, head_dim]).unwrap(),
                mlx_rs::ops::zeros::<f32>(&[1, num_kv_heads, 0, head_dim]).unwrap(),
            )
        ];

        let logits = model.forward(&ids, None, 0, &mut kv_cache).unwrap();
        assert_eq!(logits.shape(), &[1, 4, vocab_size]);

        // Run step 2 forward pass with last token only
        let next_id = Array::from_slice(&[5], &[1, 1]);
        let logits2 = model.forward(&next_id, None, 4, &mut kv_cache).unwrap();
        assert_eq!(logits2.shape(), &[1, 1, vocab_size]);
        
        // Assert KV Cache sequence length is updated to 5
        assert_eq!(kv_cache[0].0.shape()[2], 5);
    }
}
