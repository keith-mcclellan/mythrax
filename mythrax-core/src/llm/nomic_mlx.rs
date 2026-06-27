use std::collections::HashMap;
use anyhow::{Result, Context};
use mlx_rs::{Array, StreamOrDevice};
use mlx_rs::module::Module;
use mlx_rs::nn::{Embedding, LayerNorm, Linear};
use mlx_rs::ops::indexing::TryIndexOp;
use crate::llm::mlx_weights::{get_linear, get_layer_norm};

pub struct NomicBertAttention {
    wqkv: Linear,
    out_proj: Linear,
    num_heads: i32,
    head_dim: i32,
}

impl NomicBertAttention {
    pub fn new(weights: &HashMap<String, Array>, prefix: &str) -> Result<Self> {
        let wqkv = get_linear(weights, &format!("{}.Wqkv", prefix), false)?;
        let out_proj = get_linear(weights, &format!("{}.out_proj", prefix), false)?;
        Ok(Self {
            wqkv,
            out_proj,
            num_heads: 12,
            head_dim: 64,
        })
    }

    pub fn forward(&mut self, x: &Array, mask: Option<&Array>) -> Result<Array> {
        let qkv = self.wqkv.forward(x)?;
        let shape = qkv.shape();
        let batch = shape[0];
        let seq_len = shape[1];

        // Split into Q, K, V using IndexOp tuples
        let q = qkv.try_index((.., .., 0..768))
            .map_err(|e| anyhow::anyhow!("Slice Q failed: {:?}", e))?;
        let k = qkv.try_index((.., .., 768..1536))
            .map_err(|e| anyhow::anyhow!("Slice K failed: {:?}", e))?;
        let v = qkv.try_index((.., .., 1536..2304))
            .map_err(|e| anyhow::anyhow!("Slice V failed: {:?}", e))?;

        // Reshape to [batch, seq_len, num_heads, head_dim]
        let q = q.reshape(&[batch, seq_len, self.num_heads, self.head_dim])
            .map_err(|e| anyhow::anyhow!("Reshape Q failed: {:?}", e))?;
        let k = k.reshape(&[batch, seq_len, self.num_heads, self.head_dim])
            .map_err(|e| anyhow::anyhow!("Reshape K failed: {:?}", e))?;
        let v = v.reshape(&[batch, seq_len, self.num_heads, self.head_dim])
            .map_err(|e| anyhow::anyhow!("Reshape V failed: {:?}", e))?;

        let q = q.add(&Array::from(0.0f32)).map_err(|e| anyhow::anyhow!("Contiguous Q failed: {:?}", e))?;
        let k = k.add(&Array::from(0.0f32)).map_err(|e| anyhow::anyhow!("Contiguous K failed: {:?}", e))?;


        // Manual RoPE implementation
        let half = self.head_dim / 2;
        let q1 = q.try_index((.., .., .., 0..half))
            .map_err(|e| anyhow::anyhow!("Slice Q1 failed: {:?}", e))?;
        let q2 = q.try_index((.., .., .., half..self.head_dim))
            .map_err(|e| anyhow::anyhow!("Slice Q2 failed: {:?}", e))?;
        let neg_q2 = q2.negative()
            .map_err(|e| anyhow::anyhow!("Neg Q2 failed: {:?}", e))?;
        let rotate_half_q = mlx_rs::ops::concatenate_axis_device(&[neg_q2, q1], 3, StreamOrDevice::gpu())
            .map_err(|e| anyhow::anyhow!("Concat rotate Q failed: {:?}", e))?;

        let k1 = k.try_index((.., .., .., 0..half))
            .map_err(|e| anyhow::anyhow!("Slice K1 failed: {:?}", e))?;
        let k2 = k.try_index((.., .., .., half..self.head_dim))
            .map_err(|e| anyhow::anyhow!("Slice K2 failed: {:?}", e))?;
        let neg_k2 = k2.negative()
            .map_err(|e| anyhow::anyhow!("Neg K2 failed: {:?}", e))?;
        let rotate_half_k = mlx_rs::ops::concatenate_axis_device(&[neg_k2, k1], 3, StreamOrDevice::gpu())
            .map_err(|e| anyhow::anyhow!("Concat rotate K failed: {:?}", e))?;

        // Compute cos and sin values for RoPE
        let seq_len_u = seq_len as usize;
        let head_dim_u = self.head_dim as usize;
        let half_u = half as usize;
        let mut cos_val = vec![0.0f32; seq_len_u * head_dim_u];
        let mut sin_val = vec![0.0f32; seq_len_u * head_dim_u];
        for pos in 0..seq_len_u {
            for i in 0..half_u {
                let exponent = -2.0 * (i as f32) / (self.head_dim as f32);
                let theta = 1000.0f32.powf(exponent);
                let angle = (pos as f32) * theta;
                let c = angle.cos();
                let s = angle.sin();
                
                cos_val[pos * head_dim_u + i] = c;
                sin_val[pos * head_dim_u + i] = s;
                cos_val[pos * head_dim_u + half_u + i] = c;
                sin_val[pos * head_dim_u + half_u + i] = s;
            }
        }

        let cos_arr = Array::from_slice(&cos_val, &[1, seq_len as i32, 1, self.head_dim]);
        let sin_arr = Array::from_slice(&sin_val, &[1, seq_len as i32, 1, self.head_dim]);

        let q = q.multiply(&cos_arr)?
            .add(&rotate_half_q.multiply(&sin_arr)?)
            .map_err(|e| anyhow::anyhow!("Apply RoPE Q failed: {:?}", e))?;
        let k = k.multiply(&cos_arr)?
            .add(&rotate_half_k.multiply(&sin_arr)?)
            .map_err(|e| anyhow::anyhow!("Apply RoPE K failed: {:?}", e))?;



        let q = q.transpose_axes_device(&[0, 2, 1, 3], StreamOrDevice::gpu())
            .map_err(|e| anyhow::anyhow!("Transpose Q failed: {:?}", e))?;
        let k = k.transpose_axes_device(&[0, 2, 1, 3], StreamOrDevice::gpu())
            .map_err(|e| anyhow::anyhow!("Transpose K failed: {:?}", e))?;
        let v = v.transpose_axes_device(&[0, 2, 1, 3], StreamOrDevice::gpu())
            .map_err(|e| anyhow::anyhow!("Transpose V failed: {:?}", e))?;

        let scale = 1.0 / (self.head_dim as f32).sqrt();

        let mask_opt = if let Some(m) = mask {
            let one = Array::from(1.0f32);
            let inv_m = one.subtract(m).map_err(|e| anyhow::anyhow!("Sub mask failed: {:?}", e))?;
            let add_mask = inv_m.multiply(&Array::from(-1e9f32)).map_err(|e| anyhow::anyhow!("Mul mask failed: {:?}", e))?;
            let add_mask_reshaped = add_mask.reshape(&[batch, 1, 1, seq_len])
                .map_err(|e| anyhow::anyhow!("Reshape mask failed: {:?}", e))?;
            Some(add_mask_reshaped)
        } else {
            None
        };

        let out = mlx_rs::fast::scaled_dot_product_attention_device(
            &q,
            &k,
            &v,
            scale,
            mask_opt.as_ref().map(|x| x.into()),
            StreamOrDevice::gpu(),
        ).map_err(|e| anyhow::anyhow!("SDP Attention failed: {:?}", e))?;

        let out = out.transpose_axes_device(&[0, 2, 1, 3], StreamOrDevice::gpu())
            .map_err(|e| anyhow::anyhow!("Transpose out failed: {:?}", e))?;
        let out = out.reshape(&[batch, seq_len, 768])
            .map_err(|e| anyhow::anyhow!("Reshape out failed: {:?}", e))?;



        let out = self.out_proj.forward(&out)
            .map_err(|e| anyhow::anyhow!("Attention out projection failed: {:?}", e))?;
        Ok(out)
    }
}

pub struct NomicBertMLP {
    fc11: Linear,
    fc12: Linear,
    fc2: Linear,
}

impl NomicBertMLP {
    pub fn new(weights: &HashMap<String, Array>, prefix: &str) -> Result<Self> {
        let fc11 = get_linear(weights, &format!("{}.fc11", prefix), false)?;
        let fc12 = get_linear(weights, &format!("{}.fc12", prefix), false)?;
        let fc2 = get_linear(weights, &format!("{}.fc2", prefix), false)?;
        Ok(Self { fc11, fc12, fc2 })
    }

    pub fn forward(&mut self, x: &Array) -> Result<Array> {
        let h1 = self.fc11.forward(x)
            .map_err(|e| anyhow::anyhow!("MLP fc11 failed: {:?}", e))?;
        let h2 = self.fc12.forward(x)
            .map_err(|e| anyhow::anyhow!("MLP fc12 failed: {:?}", e))?;
        let silu_h2 = h2.multiply(&mlx_rs::ops::sigmoid(&h2)?)
            .map_err(|e| anyhow::anyhow!("MLP Swish/SiLU failed: {:?}", e))?;
        let activated = h1.multiply(&silu_h2)
            .map_err(|e| anyhow::anyhow!("MLP Activation multiply failed: {:?}", e))?;
        let out = self.fc2.forward(&activated)
            .map_err(|e| anyhow::anyhow!("MLP fc2 failed: {:?}", e))?;
        Ok(out)
    }
}

pub struct NomicBertLayer {
    attn: NomicBertAttention,
    mlp: NomicBertMLP,
    norm1: LayerNorm,
    norm2: LayerNorm,
}

impl NomicBertLayer {
    pub fn new(weights: &HashMap<String, Array>, prefix: &str) -> Result<Self> {
        let attn = NomicBertAttention::new(weights, &format!("{}.attn", prefix))?;
        let mlp = NomicBertMLP::new(weights, &format!("{}.mlp", prefix))?;
        let norm1 = get_layer_norm(weights, &format!("{}.norm1", prefix), 1e-5)?;
        let norm2 = get_layer_norm(weights, &format!("{}.norm2", prefix), 1e-5)?;
        Ok(Self { attn, mlp, norm1, norm2 })
    }

    pub fn forward(&mut self, x: &Array, mask: Option<&Array>) -> Result<Array> {
        let attn_out = self.attn.forward(x, mask)?;
        let x_norm1 = self.norm1.forward(&x.add(&attn_out)?)
            .map_err(|e| anyhow::anyhow!("Layer norm1 failed: {:?}", e))?;
        let mlp_out = self.mlp.forward(&x_norm1)?;
        let out = self.norm2.forward(&x_norm1.add(&mlp_out)?)
            .map_err(|e| anyhow::anyhow!("Layer norm2 failed: {:?}", e))?;
        Ok(out)
    }
}

pub struct NomicBertModel {
    word_embeddings: Embedding,
    token_type_embeddings: Embedding,
    emb_ln: LayerNorm,
    layers: Vec<NomicBertLayer>,
}

impl NomicBertModel {
    pub fn new(weights: &HashMap<String, Array>) -> Result<Self> {
        let word_emb_weight = weights.get("embeddings.word_embeddings.weight")
            .context("embeddings.word_embeddings.weight not found")?
            .clone();
        let vocab_size = word_emb_weight.shape()[0];
        let mut word_embeddings = Embedding::new(vocab_size, 768)
            .map_err(|e| anyhow::anyhow!("Failed to build word embedding: {:?}", e))?;
        word_embeddings.weight.value = word_emb_weight;

        let type_emb_weight = weights.get("embeddings.token_type_embeddings.weight")
            .context("embeddings.token_type_embeddings.weight not found")?
            .clone();
        let mut token_type_embeddings = Embedding::new(2, 768)
            .map_err(|e| anyhow::anyhow!("Failed to build token type embedding: {:?}", e))?;
        token_type_embeddings.weight.value = type_emb_weight;

        let emb_ln = get_layer_norm(weights, "emb_ln", 1e-5)?;

        let mut layers = Vec::new();
        for i in 0..12 {
            let layer = NomicBertLayer::new(weights, &format!("encoder.layers.{}", i))?;
            layers.push(layer);
        }

        Ok(Self {
            word_embeddings,
            token_type_embeddings,
            emb_ln,
            layers,
        })
    }

    pub fn forward(&mut self, ids: &Array, mask: Option<&Array>) -> Result<Array> {
        let shape = ids.shape();
        let batch = shape[0];
        let seq_len = shape[1];
        
        let word_embs = self.word_embeddings.forward(ids)
            .map_err(|e| anyhow::anyhow!("Word embedding forward failed: {:?}", e))?;
        
        let token_type_ids = mlx_rs::ops::zeros::<i32>(&[batch, seq_len])
            .map_err(|e| anyhow::anyhow!("Token type IDs zero fill failed: {:?}", e))?;
        let type_embs = self.token_type_embeddings.forward(&token_type_ids)
            .map_err(|e| anyhow::anyhow!("Token type embedding forward failed: {:?}", e))?;

        let mut x = word_embs.add(&type_embs)
            .map_err(|e| anyhow::anyhow!("Embeddings add failed: {:?}", e))?;
        x = self.emb_ln.forward(&x)
            .map_err(|e| anyhow::anyhow!("Embedding LayerNorm failed: {:?}", e))?;
        
        for layer in &mut self.layers {
            x = layer.forward(&x, mask)?;
        }

        Ok(x)
    }
}
