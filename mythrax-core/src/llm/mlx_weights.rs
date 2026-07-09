use crate::llm::qwen2_mlx::QwenEmbedding;
use anyhow::{Context, Result};
use mlx_rs::builder::Builder;
use mlx_rs::module::Param;
use mlx_rs::nn::{
    Embedding, LayerNorm, LayerNormBuilder, Linear, LinearBuilder, QuantizedEmbedding,
    QuantizedLinear, QuantizedLinearBuilder, RmsNorm, RmsNormBuilder, build_quantized_linear,
};
use mlx_rs::{Array, Dtype};
use std::collections::HashMap;

pub fn get_linear(
    weights: &HashMap<String, Array>,
    prefix: &str,
    has_bias: bool,
) -> Result<Linear> {
    let weight_key = format!("{}.weight", prefix);
    let weight = weights
        .get(&weight_key)
        .with_context(|| format!("Weight key {} not found", weight_key))?
        .clone();
    let sh = weight.shape();
    let out_dims = sh[0];
    let in_dims = sh[1];

    let bias = if has_bias {
        let bias_key = format!("{}.bias", prefix);
        let b = weights
            .get(&bias_key)
            .with_context(|| format!("Bias key {} not found", bias_key))?
            .clone();
        Some(b)
    } else {
        None
    };

    let mut layer = LinearBuilder::new(in_dims, out_dims)
        .bias(has_bias)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build linear: {:?}", e))?;
    layer.weight.value = weight;
    layer.bias.value = bias;
    Ok(layer)
}

pub fn get_rms_norm(weights: &HashMap<String, Array>, key: &str, eps: f32) -> Result<RmsNorm> {
    let weight = weights
        .get(key)
        .with_context(|| format!("RmsNorm weight key {} not found", key))?
        .clone();
    let dims = weight.shape()[0];
    let mut norm = RmsNormBuilder::new(dims)
        .eps(eps)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build RmsNorm: {:?}", e))?;
    norm.weight.value = weight;
    Ok(norm)
}

pub fn get_layer_norm(
    weights: &HashMap<String, Array>,
    prefix: &str,
    eps: f32,
) -> Result<LayerNorm> {
    let weight_key = format!("{}.weight", prefix);
    let bias_key = format!("{}.bias", prefix);
    let weight = weights
        .get(&weight_key)
        .with_context(|| format!("LayerNorm weight key {} not found", weight_key))?
        .clone();
    let bias = weights
        .get(&bias_key)
        .with_context(|| format!("LayerNorm bias key {} not found", bias_key))?
        .clone();
    let dims = weight.shape()[0];
    let mut norm = LayerNormBuilder::new(dims)
        .eps(eps)
        .affine(true)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build LayerNorm: {:?}", e))?;
    norm.weight.value = Some(weight);
    norm.bias.value = Some(bias);
    Ok(norm)
}

pub fn get_quantized_linear(
    weights: &HashMap<String, Array>,
    prefix: &str,
    has_bias: bool,
    group_size: i32,
    bits: i32,
) -> Result<QuantizedLinear> {
    let weight_key = format!("{}.weight", prefix);
    let scales_key = format!("{}.scales", prefix);
    let biases_key = format!("{}.biases", prefix);

    let weight = weights
        .get(&weight_key)
        .with_context(|| format!("Quantized weight key {} not found", weight_key))?
        .clone();
    let scales = weights
        .get(&scales_key)
        .with_context(|| format!("Quantized scales key {} not found", scales_key))?
        .clone();
    let biases = weights
        .get(&biases_key)
        .with_context(|| format!("Quantized biases key {} not found", biases_key))?
        .clone();

    let out_dims = scales.shape()[0];
    let packed_cols = weight.shape()[1];
    let in_dims = packed_cols * (32 / bits);

    let bias = if has_bias {
        let bias_key = format!("{}.bias", prefix);
        let b = weights
            .get(&bias_key)
            .with_context(|| format!("Bias key {} not found", bias_key))?
            .clone();
        Some(b)
    } else {
        None
    };

    let builder = QuantizedLinearBuilder::new(in_dims, out_dims)
        .group_size(group_size)
        .bits(bits)
        .bias(has_bias);

    let mut ql = build_quantized_linear(builder)
        .map_err(|e| anyhow::anyhow!("Failed to build quantized linear: {:?}", e))?;

    ql.inner.weight.value = weight;
    ql.scales.value = scales;
    ql.biases.value = biases;
    ql.inner.bias.value = bias;
    Ok(ql)
}

pub fn get_embedding(
    weights: &HashMap<String, Array>,
    prefix: &str,
    group_size: i32,
    bits: i32,
) -> Result<QwenEmbedding> {
    let weight_key = format!("{}.weight", prefix);
    let weight = weights
        .get(&weight_key)
        .with_context(|| format!("Embedding weight key {} not found", weight_key))?
        .clone();

    let scales_key = format!("{}.scales", prefix);
    if let Some(scales) = weights.get(&scales_key) {
        // Quantized Embedding path
        let biases_key = format!("{}.biases", prefix);
        let biases = weights
            .get(&biases_key)
            .with_context(|| format!("Quantized biases key {} not found", biases_key))?
            .clone();

        let zero_u32 = Array::from_slice(&[0u32], &[]);
        let clean_weight = weight
            .add(&zero_u32)
            .map_err(|e| anyhow::anyhow!("Embed weight contiguity failed: {:?}", e))?;
        let clean_scales = scales
            .add(&Array::from(0.0f32))
            .map_err(|e| anyhow::anyhow!("Embed scales contiguity failed: {:?}", e))?;
        let clean_biases = biases
            .add(&Array::from(0.0f32))
            .map_err(|e| anyhow::anyhow!("Embed biases contiguity failed: {:?}", e))?;

        clean_weight.eval().unwrap();
        clean_scales.eval().unwrap();
        clean_biases.eval().unwrap();

        let inner = Embedding {
            weight: Param::new(clean_weight),
        };

        let qe = QuantizedEmbedding {
            group_size,
            bits,
            scales: Param::new(clean_scales),
            biases: Param::new(clean_biases),
            inner,
        };
        Ok(QwenEmbedding::Quantized(qe))
    } else {
        // Standard (unquantized/dense) Embedding path
        let mut emb = Embedding::new(weight.shape()[0], weight.shape()[1])
            .map_err(|e| anyhow::anyhow!("Failed to build standard Embedding: {:?}", e))?;
        emb.weight.value = weight;
        Ok(QwenEmbedding::Dense(emb))
    }
}

pub fn load_model_weights(model_dir: &std::path::Path) -> Result<HashMap<String, Array>> {
    let index_path = model_dir.join("model.safetensors.index.json");
    if index_path.exists() {
        let content = std::fs::read_to_string(&index_path)?;
        let index: serde_json::Value = serde_json::from_str(&content)?;
        let weight_map = index
            .get("weight_map")
            .context("weight_map not found in index")?;
        let weight_map = weight_map
            .as_object()
            .context("weight_map is not a JSON object")?;

        let mut shard_files = std::collections::HashSet::new();
        for shard_val in weight_map.values() {
            if let Some(shard_str) = shard_val.as_str() {
                shard_files.insert(shard_str.to_string());
            }
        }

        let mut all_weights = HashMap::new();
        for shard in shard_files {
            let shard_path = model_dir.join(shard);
            let weights = Array::load_safetensors(&shard_path).map_err(|e| {
                anyhow::anyhow!("Failed to load shard {}: {:?}", shard_path.display(), e)
            })?;
            for (k, v) in weights {
                let cast_v = if v.dtype() == Dtype::Bfloat16 || v.dtype() == Dtype::Float32 {
                    v.as_dtype(Dtype::Float16)?
                } else {
                    v
                };
                all_weights.insert(k, cast_v);
            }
        }
        Ok(all_weights)
    } else {
        let safetensors_path = model_dir.join("model.safetensors");
        let weights = Array::load_safetensors(&safetensors_path)
            .map_err(|e| anyhow::anyhow!("Failed to load safetensors: {:?}", e))?;
        let mut cast_weights = HashMap::new();
        for (k, v) in weights {
            let cast_v = if v.dtype() == Dtype::Bfloat16 || v.dtype() == Dtype::Float32 {
                v.as_dtype(Dtype::Float16)?
            } else {
                v
            };
            cast_weights.insert(k, cast_v);
        }
        Ok(cast_weights)
    }
}
