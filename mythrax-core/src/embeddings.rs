use anyhow::{Result, Context};
use tokenizers::Tokenizer;
use std::path::Path;
use std::env;
use std::sync::Mutex;
use std::sync::Arc;
use std::sync::OnceLock;

static GLOBAL_EMBEDDER: OnceLock<Result<Arc<LocalEmbedder>, String>> = OnceLock::new();

pub struct LocalEmbedder {
    session: Mutex<ort::session::Session>,
    tokenizer: Tokenizer,
}

impl LocalEmbedder {
    pub fn get_global() -> Result<Arc<Self>> {
        let res = GLOBAL_EMBEDDER.get_or_init(|| {
            Self::new().map(Arc::new).map_err(|e| e.to_string())
        });
        match res {
            Ok(emb) => Ok(emb.clone()),
            Err(err) => Err(anyhow::anyhow!("Failed to initialize global embedder: {}", err)),
        }
    }

    pub fn new() -> Result<Self> {
        let home = env::var("HOME").context("HOME env var not set")?;
        let base_path = Path::new(&home).join(".mythrax/models");
        
        let model_path = base_path.join("nomic-embed-text-v1.5.onnx");
        let tokenizer_path = base_path.join("tokenizer.json");

        if !model_path.exists() || !tokenizer_path.exists() {
            anyhow::bail!("ONNX model or tokenizer files not found in ~/.mythrax/models/");
        }

        // Initialize ONNX Runtime session
        // Note: ort 2.0 uses session builder
        let session = ort::session::Session::builder()
            .map_err(|e| anyhow::anyhow!("Failed to create session builder: {}", e))?
            .with_intra_threads(2)
            .map_err(|e| anyhow::anyhow!("Failed to set intra threads: {}", e))?
            .commit_from_file(&model_path)
            .map_err(|e| anyhow::anyhow!("Failed to load ONNX model session: {}", e))?;

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;

        Ok(Self { session: Mutex::new(session), tokenizer })
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Nomic Embed Text requires a prefix for search queries vs document indices:
        // "search_query: " or "search_document: "
        let formatted_text = if text.contains(':') {
            text.to_string()
        } else {
            format!("search_document: {}", text)
        };

        let encoding = self.tokenizer.encode(formatted_text, true)
            .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

        let ids = encoding.get_ids();
        let mask = encoding.get_attention_mask();
        let mut seq_len = ids.len();

        if seq_len == 0 {
            return Ok(vec![0.0; 768]); // Default dimension
        }

        // Truncate sequence length to a maximum of 2048 to prevent quadratic memory usage in self-attention layers
        if seq_len > 2048 {
            seq_len = 2048;
        }

        // Convert token IDs to i64 for ONNX
        let input_ids_data: Vec<i64> = ids.iter().take(seq_len).map(|&x| x as i64).collect();
        let attention_mask_data: Vec<i64> = mask.iter().take(seq_len).map(|&x| x as i64).collect();
        let token_type_ids_data: Vec<i64> = vec![0; seq_len];

        // Create 2D inputs [batch_size = 1, seq_len]
        let input_ids = ort::value::Tensor::from_array((vec![1, seq_len], input_ids_data))?;
        let attention_mask = ort::value::Tensor::from_array((vec![1, seq_len], attention_mask_data))?;
        let token_type_ids = ort::value::Tensor::from_array((vec![1, seq_len], token_type_ids_data))?;

        // Run inference
        let mut session_lock = self.session.lock().map_err(|e| anyhow::anyhow!("Failed to lock session: {}", e))?;
        let outputs = session_lock.run(ort::inputs![
            "input_ids" => input_ids,
            "attention_mask" => attention_mask,
            "token_type_ids" => token_type_ids,
        ]).map_err(|e| anyhow::anyhow!("ONNX inference failed: {}", e))?;

        // Nomic-embed-text outputs token embeddings under "last_hidden_state"
        let output_tensor = outputs.get("last_hidden_state")
            .context("Failed to get last_hidden_state output")?;

        let (shape, data) = output_tensor.try_extract_tensor::<f32>()
            .map_err(|e| anyhow::anyhow!("Failed to extract tensor data: {}", e))?;
        
        // Shape is [batch=1, seq_len, hidden_dim=768]
        if shape.len() != 3 || shape[0] != 1 || shape[1] as usize != seq_len {
            anyhow::bail!("Unexpected embedding output shape: {:?}", shape);
        }

        let hidden_dim = shape[2] as usize; // 768

        // Perform mean pooling: sum token embeddings and divide by active token count
        let mut sum_embeddings = vec![0.0; hidden_dim];
        let mut active_tokens = 0.0;

        for (i, &m) in mask.iter().enumerate().take(seq_len) {
            if m == 1 {
                active_tokens += 1.0;
                let offset = i * hidden_dim;
                for j in 0..hidden_dim {
                    sum_embeddings[j] += data[offset + j];
                }
            }
        }

        if active_tokens > 0.0 {
            for val in &mut sum_embeddings {
                *val /= active_tokens;
            }
        }

        // L2 Normalize the pooled embedding
        let mut l2_norm: f32 = 0.0;
        for &val in &sum_embeddings {
            l2_norm += val * val;
        }
        l2_norm = l2_norm.sqrt();

        if l2_norm > 0.0 {
            for val in &mut sum_embeddings {
                *val /= l2_norm;
            }
        }

        Ok(sum_embeddings)
    }

    pub fn count_tokens(&self, text: &str) -> Result<usize> {
        let encoding = self.tokenizer.encode(text, true)
            .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;
        Ok(encoding.get_ids().len())
    }

    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let mut all_embeddings = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(32) {
            let chunk_embeddings = self.embed_sub_batch(chunk)?;
            all_embeddings.extend(chunk_embeddings);
        }
        Ok(all_embeddings)
    }

    fn embed_sub_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let formatted_texts: Vec<String> = texts.iter().map(|text| {
            if text.contains(':') {
                text.clone()
            } else {
                format!("search_document: {}", text)
            }
        }).collect();

        let encodings = self.tokenizer.encode_batch(formatted_texts, true)
            .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

        let max_len = encodings.iter()
            .map(|enc| enc.get_ids().len())
            .max()
            .unwrap_or(0)
            .min(2048)
            .max(1);

        let batch_size = encodings.len();

        let mut input_ids_data = Vec::with_capacity(batch_size * max_len);
        let mut attention_mask_data = Vec::with_capacity(batch_size * max_len);
        let mut token_type_ids_data = Vec::with_capacity(batch_size * max_len);

        for enc in &encodings {
            let ids = enc.get_ids();
            let mask = enc.get_attention_mask();
            let len = ids.len();
            let take_len = std::cmp::min(len, max_len);

            input_ids_data.extend(ids.iter().take(take_len).map(|&id| id as i64));
            attention_mask_data.extend(mask.iter().take(take_len).map(|&m| m as i64));
            token_type_ids_data.extend(std::iter::repeat(0i64).take(take_len));

            let padding_len = max_len - take_len;
            if padding_len > 0 {
                input_ids_data.resize(input_ids_data.len() + padding_len, 0);
                attention_mask_data.resize(attention_mask_data.len() + padding_len, 0);
                token_type_ids_data.resize(token_type_ids_data.len() + padding_len, 0);
            }
        }

        let input_ids = ort::value::Tensor::from_array((vec![batch_size as i64, max_len as i64], input_ids_data))?;
        let attention_mask = ort::value::Tensor::from_array((vec![batch_size as i64, max_len as i64], attention_mask_data))?;
        let token_type_ids = ort::value::Tensor::from_array((vec![batch_size as i64, max_len as i64], token_type_ids_data))?;

        let mut session_lock = self.session.lock().map_err(|e| anyhow::anyhow!("Failed to lock session: {}", e))?;
        let outputs = session_lock.run(ort::inputs![
            "input_ids" => input_ids,
            "attention_mask" => attention_mask,
            "token_type_ids" => token_type_ids,
        ]).map_err(|e| anyhow::anyhow!("ONNX inference failed: {}", e))?;

        let output_tensor = outputs.get("last_hidden_state")
            .context("Failed to get last_hidden_state output")?;

        let (shape, data) = output_tensor.try_extract_tensor::<f32>()
            .map_err(|e| anyhow::anyhow!("Failed to extract tensor data: {}", e))?;

        if shape.len() != 3 || shape[0] as usize != batch_size || shape[1] as usize != max_len {
            anyhow::bail!("Unexpected embedding output shape: {:?}", shape);
        }

        let hidden_dim = shape[2] as usize;

        let mut batch_embeddings = Vec::with_capacity(batch_size);
        for b in 0..batch_size {
            let enc = &encodings[b];
            let mask = enc.get_attention_mask();
            let len = mask.len();
            let take_len = std::cmp::min(len, max_len);

            let mut sum_embeddings = vec![0.0; hidden_dim];
            let mut active_tokens = 0.0;
            let batch_offset = b * max_len * hidden_dim;

            for i in 0..take_len {
                if mask[i] == 1 {
                    active_tokens += 1.0;
                    let token_offset = batch_offset + i * hidden_dim;
                    for j in 0..hidden_dim {
                        sum_embeddings[j] += data[token_offset + j];
                    }
                }
            }

            if active_tokens > 0.0 {
                let inv_active = 1.0 / active_tokens;
                for val in &mut sum_embeddings {
                    *val *= inv_active;
                }
            }

            let l2_norm_sq: f32 = sum_embeddings.iter().map(|&x| x * x).sum();
            let l2_norm = l2_norm_sq.sqrt();
            if l2_norm > 0.0 {
                let inv_norm = 1.0 / l2_norm;
                for val in &mut sum_embeddings {
                    *val *= inv_norm;
                }
            }
            batch_embeddings.push(sum_embeddings);
        }

        Ok(batch_embeddings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_embeddings() {
        // Verify we can load the model and run embedding inference
        if let Ok(embedder) = LocalEmbedder::new() {
            let vec = embedder.embed("Test embedding query").unwrap();
            assert_eq!(vec.len(), 768);
            
            // Check L2 Normalized (sum of squares is ~1.0)
            let sum_sq: f32 = vec.iter().map(|&x| x * x).sum();
            assert!((sum_sq - 1.0).abs() < 1e-4);
        } else {
            println!("Skipping embeddings test: model files not present in ~/.mythrax/models/");
        }
    }
}
