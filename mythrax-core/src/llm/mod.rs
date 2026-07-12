use crate::db::StorageBackend;
use anyhow::{Context, Result};
use std::sync::OnceLock;
use tokio::sync::Semaphore;
use std::sync::{Arc, Mutex, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::HashMap;
use std::path::PathBuf;

#[cfg(feature = "mlx")]
use tokenizers::Tokenizer;
#[cfg(feature = "mlx")]
use mlx_rs::{Array, StreamOrDevice};

#[cfg(feature = "mlx")]
pub mod mlx_weights;
#[cfg(feature = "mlx")]
pub mod nomic_mlx;
#[cfg(feature = "mlx")]
pub mod qwen2_mlx;
#[cfg(feature = "mlx")]
pub mod mxbai_mlx;

#[cfg(feature = "mlx")]
pub use mxbai_mlx::MxbaiReranker;

#[cfg(not(feature = "mlx"))]
pub struct Qwen2Model;
#[cfg(not(feature = "mlx"))]
pub struct Tokenizer;
#[cfg(not(feature = "mlx"))]
pub struct MxbaiReranker;

/// Process-global semaphores that limit concurrent GPU inference and embedding requests.
static METAL_INFERENCE_SEMAPHORE: OnceLock<Semaphore> = OnceLock::new();
static METAL_EMBEDDING_SEMAPHORE: OnceLock<Semaphore> = OnceLock::new();

pub fn metal_inference_semaphore() -> &'static Semaphore {
    METAL_INFERENCE_SEMAPHORE.get_or_init(|| Semaphore::new(1))
}

pub fn metal_embedding_semaphore() -> &'static Semaphore {
    METAL_EMBEDDING_SEMAPHORE.get_or_init(|| Semaphore::new(1))
}

pub struct LLMClient {
    client: reqwest::Client,
}

impl Default for LLMClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LLMClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    #[allow(dead_code)]
    pub async fn prompt(&self, db: &dyn StorageBackend, prompt: &str) -> Result<String> {
        self.completion(db, None, prompt).await
    }

    pub async fn completion(
        &self,
        db: &dyn StorageBackend,
        system_instruction: Option<&str>,
        prompt: &str,
    ) -> Result<String> {
        let config = db.get_llm_config().await?;
        self.completion_explicit(
            db,
            &config.active_provider,
            &config.cloud_provider,
            &config.model,
            system_instruction,
            prompt,
            false,
        )
        .await
    }

    pub async fn completion_explicit(
        &self,
        db: &dyn StorageBackend,
        active_provider: &str,
        cloud_provider: &str,
        model: &str,
        system_instruction: Option<&str>,
        prompt: &str,
        enable_thinking: bool,
    ) -> Result<String> {
        if let Ok(mock) = std::env::var("MYTHRAX_MOCK_LLM") {
            if mock == "true" {
                if prompt.contains("Analyze the following dialog") {
                    return Ok(r#"{"target_pattern": "test_pattern", "action_to_avoid": "test_action", "causal_explanation": "test_causal", "prescribed_remedy": "test_remedy"}"#.to_string());
                } else if prompt.contains("Validate if these should merge") {
                    if std::env::var("MYTHRAX_MOCK_MALFORMED_MERGE").is_ok() {
                        return Ok(r#"{"should_merge": true}"#.to_string());
                    }
                    return Ok(r#"{"should_merge": true, "suggested_name": "git-workflow", "reason": "Redundant playbooks"}"#.to_string());
                } else if prompt.contains("Playbooks to Merge") {
                    return Ok("---\nname: meta-git-workflow\ndescription: Consolidated git meta skill\ngenerator_name: MetaSkillSynthesizer\n---\n\nConsolidated instructions here.\n".to_string());
                } else if prompt.contains("meta-skill synthesizer") || prompt.contains("Context Data:") {
                    return Ok("---\nname: meta-test-scope\ndescription: Synthesized meta skill\ngenerator_name: MetaSkillSynthesizer\n---\n\nSynthesized instructions here.\n".to_string());
                } else if prompt.contains("Wisdom") || prompt.contains("rules") || prompt.contains("Wisdom Rules") {
                    if prompt.contains("aesthetic") || prompt.contains("procedural") || prompt.contains("rule_type") || prompt.contains("Events:") {
                        return Ok(r#"[{"target_pattern": "test_pattern", "action_to_avoid": "test_action", "causal_explanation": "test_causal", "prescribed_remedy": "test_remedy", "rule_type": "procedural"}]"#.to_string());
                    } else {
                        return Ok(r#"[{"target_pattern": "test_pattern", "action_to_avoid": "test_action", "causal_explanation": "test_causal", "prescribed_remedy": "test_remedy"}]"#.to_string());
                    }
                } else if prompt.contains("TOC") || prompt.contains("Table of Contents") {
                    return Ok(r#"[{"title": "test_title", "start_phrase": "Some document"}]"#.to_string());
                } else if prompt.contains("Insights:") {
                    return Ok("Here is an architectural compaction summary containing a code block:\n\n```rust\npub fn test_fn() {}\n```".to_string());
                } else if prompt.contains("consistency checker") || prompt.contains("NEW INSIGHT") {
                    return Ok(r#"{"contradicts": true, "conflicting_field": "database", "resolution": "We should use SurrealDB for the database because Postgres was deprecated.", "confidence": 0.95}"#.to_string());
                } else if prompt.contains("knowledge generalizer") || prompt.contains("emerged independently") {
                    return Ok(r#"{"target_pattern": "test_graduated_pattern", "action_to_avoid": "avoid_test", "causal_explanation": "why_test", "prescribed_remedy": "do_test", "confidence": 0.95}"#.to_string());
                } else {
                    return Ok(r#"[{"name": "test_concept", "content": "test_explanation"}]"#.to_string());
                }
            }
        }

        let config = db.get_llm_config().await?;
        
        let response_text = match active_provider {
            "local" => {
                #[cfg(feature = "mlx")]
                {
                    let is_external = model.contains("35B") || model.contains("3.6") || model.contains("gemma-4-26b");
                    if !is_external {
                        if let Some(broker) = DYNAMIC_MODEL_BROKER.get() {
                            let tier = match model {
                                m if m.contains("0.5B") || m.contains("1.5B") || m.contains("Tier1") => ModelTier::Tier1,
                                m if m.contains("35B") || m.contains("3.6") || m.contains("Tier3") || m.contains("Tier2") || m.contains("a3b") || m.contains("a4b") => ModelTier::Tier3,
                                _ => ModelTier::Tier2,
                            };
                            tracing::info!("mlx feature active: routing local inference in-process via DynamicModelBroker for tier {:?}", tier);
                            let engine = broker.acquire_llm(tier).await?;
                            let raw = engine.generate(prompt, system_instruction).await?;
                            return Ok(strip_think_block(&raw));
                        }
                    }
                }

                // Fallback to HTTP request when mlx feature is disabled or broker is uninitialized
                tracing::debug!("mlx feature disabled or broker not initialized: routing local inference to mlx-lm HTTP server at :8080");

                let url_str = std::env::var("MYTHRAX_COMPLETIONS_URL")
                    .unwrap_or_else(|_| "http://127.0.0.1:8080/v1/chat/completions".to_string());
                let url = &url_str;
                let truncated_prompt = if prompt.len() > 100_000 {
                    let truncated = truncate_to_boundary(prompt, 100_000);
                    format!("{}... [Truncated due to local context limits]", truncated)
                } else {
                    prompt.to_string()
                };

                let thinking_directive = if enable_thinking { "/think" } else { "/no_think" };
                let effective_system = match system_instruction {
                    Some(sys) => format!("{thinking_directive}\n{sys}"),
                    None => thinking_directive.to_string(),
                };

                let temperature = if enable_thinking { 0.6_f32 } else { 0.2_f32 };

                let messages = vec![
                    serde_json::json!({ "role": "system", "content": effective_system }),
                    serde_json::json!({ "role": "user",   "content": truncated_prompt }),
                ];

                let payload = serde_json::json!({
                    "model": model,
                    "messages": messages,
                    "temperature": temperature,
                    "max_tokens": 8192
                });

                let _permit = metal_inference_semaphore().acquire().await
                    .map_err(|e| anyhow::anyhow!("LLM semaphore error: {}", e))?;

                let req = self.client.post(url).json(&payload);
                let resp = send_with_retry(&self.client, req).await?;
                let json: serde_json::Value = resp.json().await?;
                tracing::debug!("Local LLM raw response: {}", json);
                let content = json["choices"][0]["message"]["content"].clone();
                
                let raw = if content.is_null() {
                    let alt = json["choices"][0]["message"]["reasoning"]
                        .as_str()
                        .or_else(|| json["choices"][0]["message"]["reasoning_content"].as_str())
                        .or_else(|| json["choices"][0]["text"].as_str());
                    if let Some(text) = alt {
                        text.to_string()
                    } else {
                        anyhow::bail!("Invalid local completion response. Raw JSON: {}", json);
                    }
                } else {
                    content.as_str()
                        .with_context(|| format!("Invalid local completion response. Raw JSON: {}", json))?
                        .to_string()
                };

                let result = strip_think_block(&raw);

                let delay_ms = db.get_llm_config().await
                    .map(|cfg| cfg.llm_post_inference_delay_ms.unwrap_or(5000))
                    .unwrap_or(5000);
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                
                result
            }
            "cloud" => {
                match cloud_provider {
                    "gemini" => {
                        let api_key = config.api_key.clone().unwrap_or_else(|| std::env::var("GEMINI_API_KEY").unwrap_or_default());
                        let url = format!(
                            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                            model, api_key
                        );
                        
                        let mut payload = serde_json::json!({
                            "contents": [
                                {
                                    "parts": [
                                        {
                                            "text": prompt
                                        }
                                    ]
                                }
                            ],
                            "generationConfig": {
                                "temperature": 0.2
                            }
                        });

                        if let Some(sys) = system_instruction {
                            payload["systemInstruction"] = serde_json::json!({
                                "parts": [
                                    {
                                        "text": sys
                                    }
                                ]
                            });
                        }

                        let req = self.client.post(&url).json(&payload);
                        let resp = send_with_retry(&self.client, req).await?;
                        let json: serde_json::Value = resp.json().await?;
                        json["candidates"][0]["content"]["parts"][0]["text"]
                            .as_str()
                            .context("Invalid Gemini response")?
                            .to_string()
                    }
                    "anthropic" | "claude" => {
                        let api_key = config.api_key.clone().unwrap_or_else(|| std::env::var("ANTHROPIC_API_KEY").unwrap_or_default());
                        let url = "https://api.anthropic.com/v1/messages";
                        
                        let mut payload = serde_json::json!({
                            "model": model,
                            "max_tokens": 4096,
                            "messages": [
                                {
                                    "role": "user",
                                    "content": prompt
                                }
                            ],
                            "temperature": 0.2
                        });

                        if let Some(sys) = system_instruction {
                            payload["system"] = serde_json::json!(sys);
                        }

                        let req = self.client.post(url)
                            .header("x-api-key", api_key)
                            .header("anthropic-version", "2023-06-01")
                            .json(&payload);
                        let resp = send_with_retry(&self.client, req).await?;
                        let json: serde_json::Value = resp.json().await?;
                        json["content"][0]["text"]
                            .as_str()
                            .context("Invalid Anthropic response")?
                            .to_string()
                    }
                    other => anyhow::bail!("Unsupported cloud provider: {}", other),
                }
            }
            other => anyhow::bail!("Unsupported active provider: {}", other),
        };

        Ok(strip_code_fences(&response_text))
    }
}

impl crate::cognitive::arbor::ArborLlmClient for LLMClient {
    async fn propose_hypotheses(
        &self,
        db: &dyn StorageBackend,
        _parent_id: &str,
        parent_hypothesis: &str,
        target_files: &[(String, String)],
    ) -> Result<String> {
        let mut files_context = String::new();
        for (path, content) in target_files {
            files_context.push_str(&format!("--- FILE: {} ---\n{}\n\n", path, content));
        }

        // Query Wisdom Rules semantically for all tiers
        let mut rules = Vec::new();
        for tier in &["pinned", "permanent", "dynamic"] {
            if let Ok(res) = db.get_wisdom(parent_hypothesis, Some(*tier), 5, 0, 0.55).await {
                rules.extend(res.results);
            }
        }
        // Sort by blended score descending
        rules.sort_by(|a, b| {
            let score_a = a.similarity.unwrap_or(1.0) * (0.7 + 0.3 * a.utility.unwrap_or(1.0));
            let score_b = b.similarity.unwrap_or(1.0) * (0.7 + 0.3 * b.utility.unwrap_or(1.0));
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });
        rules.truncate(5);

        let mut wisdom_injection = String::new();
        if !rules.is_empty() {
            wisdom_injection.push_str("\n\nHere are some relevant Wisdom Rules to follow during code generation:\n");
            for (idx, r) in rules.iter().enumerate() {
                wisdom_injection.push_str(&format!(
                    "{}. [Rule: {}]\n   - Action to Avoid: {}\n   - Why: {}\n   - Remedy: {}\n",
                    idx + 1, r.target_pattern, r.action_to_avoid, r.causal_explanation, r.prescribed_remedy
                ));
            }
        }

        let prompt = format!(
            "You are an autonomous codebase researcher. We are modifying the following files:\n\n\
             {}\n\
             Based on the parent hypothesis: \"{}\", propose two alternative refinements.\n\
             For each refinement, suggest sequential node_id (e.g. \"1\", \"2\"), description, expected utility score (0.0 to 100.0), and a 'code_changes' map containing relative file paths to their COMPLETE updated file contents.\n\n\
             Return a JSON array of objects with exactly this structure:\n\
             [\n\
               {{\n\
                 \"node_id\": \"1\",\n\
                 \"hypothesis\": \"...description...\",\n\
                 \"score\": 95.0,\n\
                 \"code_changes\": {{\n\
                   \"relative/file/path.rs\": \"...full updated content of the file...\"\n\
                 }}\n\
               }}\n\
             ]\n\n\
             Output format MUST be a raw JSON array only, without any markdown formatting or code block wrapping.",
            files_context, parent_hypothesis
        );

        let system_prompt = format!(
            "You are an ideation assistant that outputs raw JSON arrays.{}",
            wisdom_injection
        );

        self.completion(db, Some(&system_prompt), &prompt).await
    }

    async fn evaluate_run(&self, db: &dyn StorageBackend, run_logs: &str) -> Result<String> {
        self.completion(db, Some("You are a critic assistant that evaluates run logs and outputs JSON."), run_logs).await
    }

    async fn abstract_insights(&self, db: &dyn StorageBackend, parent_insight: Option<&str>, child_insight: &str) -> Result<String> {
        let prompt = format!(
            "Parent context/insight: {:?}\n\
             Child run insight: {}\n\
             Summarize and merge these into a single updated insight containing all key takeaways.",
            parent_insight, child_insight
        );
        self.completion(db, Some("You are a summarization assistant."), &prompt).await
    }
}


pub fn strip_code_fences(content: &str) -> String {
    let mut cleaned = content.trim();
    if cleaned.starts_with("```") {
        if cleaned.contains('\n') {
            if let Some(first_newline_pos) = cleaned.find('\n') {
                cleaned = &cleaned[first_newline_pos + 1..];
            }
            if cleaned.ends_with("```") {
                cleaned = &cleaned[..cleaned.len() - 3];
            } else if cleaned.ends_with("```\n") {
                cleaned = &cleaned[..cleaned.len() - 4];
            }
        } else {
            if cleaned.starts_with("```json") {
                cleaned = &cleaned["```json".len()..];
            } else if cleaned.starts_with("```markdown") {
                cleaned = &cleaned["```markdown".len()..];
            } else if cleaned.starts_with("```") {
                cleaned = &cleaned["```".len()..];
            }
            if cleaned.ends_with("```") {
                cleaned = &cleaned[..cleaned.len() - 3];
            }
        }
    } else {
        if let Some(start_idx) = cleaned.find("```") {
            let fence_slice = &cleaned[start_idx..];
            let prefix = if fence_slice.starts_with("```json") {
                "```json"
            } else if fence_slice.starts_with("```markdown") {
                "```markdown"
            } else {
                "```"
            };
            let content_start = start_idx + prefix.len();
            let rest = &cleaned[content_start..];
            if let Some(end_idx) = rest.rfind("```") {
                cleaned = &rest[..end_idx];
            } else {
                cleaned = rest;
            }
        }
    }
    cleaned.trim().to_string()
}

async fn send_with_retry(
    _client: &reqwest::Client,
    req_builder: reqwest::RequestBuilder,
) -> Result<reqwest::Response> {
    let mut attempt = 0;
    loop {
        let req = req_builder.try_clone().ok_or_else(|| anyhow::anyhow!("Request builder not cloneable"))?;
        match req.send().await {
            Ok(resp) if resp.status().is_success() => return Ok(resp),
            Ok(resp) => {
                let status = resp.status();
                let body_text = resp.text().await.unwrap_or_default();
                tracing::warn!("HTTP request failed with status {}: {}", status, body_text);
                if attempt >= 5 {
                    anyhow::bail!("HTTP request failed with status {}: {}", status, body_text);
                }
            }
            Err(e) => {
                let err_str = e.to_string();
                let is_connection_refused = e.is_connect() || err_str.contains("Connection refused") || err_str.contains("connection refused");
                if is_connection_refused {
                    tracing::warn!(
                        "WARNING: Local LLM connection refused. If the server crashed, run: brew services restart mlx-lm"
                    );
                } else {
                    tracing::warn!("HTTP request error on attempt {}: {}", attempt, e);
                }
                if attempt >= 5 {
                    return Err(e.into());
                }
            }
        }
        attempt += 1;
        let base_ms = 500.0;
        let factor = (2.0f64).powi(attempt);
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let jitter = calculate_lcg_jitter(attempt, ns);
        let delay_ms = (base_ms * factor + jitter).min(5000.0);
        let sleep_duration = std::time::Duration::from_millis(delay_ms as u64);
        tokio::time::sleep(sleep_duration).await;
    }
}

pub fn calculate_lcg_jitter(attempt: i32, ns: u128) -> f64 {
    let mut x = (ns ^ (attempt as u128)) as u64;
    x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    (x % 100) as f64
}

/// Truncates a string to a maximum character count, respecting boundary
/// awareness (paragraphs, lines, words) to avoid cutting mid-sentence.
/// Remove the Qwen3 `<think>…</think>` reasoning block from a model response.
///
/// Qwen3 always wraps its chain-of-thought inside `<think>` tags before the
/// final answer. Callers of `completion_explicit` only want the answer text,
/// so we strip the block unconditionally regardless of whether thinking was
/// explicitly requested. This also handles the rare case where `/no_think` is
/// set but the model still emits a partial trace.
fn strip_think_block(s: &str) -> String {
    // Fast path: no thinking block present.
    if !s.contains("<think>") {
        return s.trim().to_string();
    }
    // Find the closing tag; if malformed (no closing tag) return everything
    // after the opening tag so we don't silently drop the whole response.
    match s.find("</think>") {
        Some(end) => s[end + "</think>".len()..].trim().to_string(),
        None => {
            // Partial / truncated thinking block — strip what we can.
            match s.find("<think>") {
                Some(start) if start > 0 => s[..start].trim().to_string(),
                _ => s.trim().to_string(),
            }
        }
    }
}

fn truncate_to_boundary(s: &str, max_chars: usize) -> &str {
    // Check if the string is within limits
    if s.chars().count() <= max_chars {
        return s;
    }
    
    // Find the byte index at the character limit
    let limit_byte_idx = match s.char_indices().nth(max_chars) {
        Some((idx, _)) => idx,
        None => return s,
    };
    
    let candidate = &s[..limit_byte_idx];
    
    // Scan backward to find a clean boundary
    // 1. Try paragraph boundary (\n\n) within the last 5000 characters
    if let Some(para_idx) = candidate.rfind("\n\n") {
        if limit_byte_idx - para_idx < 5000 {
            return &candidate[..para_idx];
        }
    }
    
    // 2. Try line boundary (\n) within the last 2000 characters
    if let Some(line_idx) = candidate.rfind('\n') {
        if limit_byte_idx - line_idx < 2000 {
            return &candidate[..line_idx];
        }
    }
    
    // 3. Try word boundary (space) within the last 500 characters
    if let Some(space_idx) = candidate.rfind(' ') {
        if limit_byte_idx - space_idx < 500 {
            return &candidate[..space_idx];
        }
    }
    
    // Fallback to exact character boundary truncation
    candidate
}

/// Represents the tier of the LLM model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelTier {
    Tier1,
    Tier2,
    Tier3,
}

/// Trait for inference engines.
pub trait InferenceEngine: Send + Sync {
    fn name(&self) -> String;
    fn is_warmed_up(&self) -> bool;
    fn stop_tokens(&self) -> Vec<String>;
    fn execution_mode(&self) -> String;
    fn generate(&self, _prompt: &str, _system_instruction: Option<&str>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send>> {
        Box::pin(async move {
            Err(anyhow::anyhow!("Local inference is not enabled on this platform"))
        })
    }
}

/// A concrete implementation of InferenceEngine for MLX-based models.
pub struct InProcessMlxEngine {
    name: String,
    warmed_up: bool,
    stop_tokens: Vec<String>,
    execution_mode: String,
    #[cfg(feature = "mlx")]
    model: Option<std::sync::Arc<tokio::sync::Mutex<qwen2_mlx::Qwen2Model>>>,
    #[cfg(feature = "mlx")]
    tokenizer: Option<std::sync::Arc<Tokenizer>>,
}

// Safety: This struct contains no raw pointers or unsafe data that would violate Send/Sync
// contracts in the context of this mock implementation.
unsafe impl Send for InProcessMlxEngine {}
unsafe impl Sync for InProcessMlxEngine {}

impl InProcessMlxEngine {
    pub fn new(
        name: String,
        warmed_up: bool,
        stop_tokens: Vec<String>,
        execution_mode: String,
        #[cfg(feature = "mlx")]
        model: Option<std::sync::Arc<tokio::sync::Mutex<qwen2_mlx::Qwen2Model>>>,
        #[cfg(feature = "mlx")]
        tokenizer: Option<std::sync::Arc<Tokenizer>>,
    ) -> Self {
        Self {
            name,
            warmed_up,
            stop_tokens,
            execution_mode,
            #[cfg(feature = "mlx")]
            model,
            #[cfg(feature = "mlx")]
            tokenizer,
        }
    }
}

impl InferenceEngine for InProcessMlxEngine {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn is_warmed_up(&self) -> bool {
        self.warmed_up
    }

    fn stop_tokens(&self) -> Vec<String> {
        self.stop_tokens.clone()
    }

    fn execution_mode(&self) -> String {
        self.execution_mode.clone()
    }

    #[cfg(feature = "mlx")]
    fn generate(&self, prompt: &str, system_instruction: Option<&str>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send>> {
        let model_opt = self.model.clone();
        let tokenizer_opt = self.tokenizer.clone();
        let prompt = prompt.to_string();
        let system_instruction = system_instruction.map(|s| s.to_string());

        Box::pin(async move {
            if model_opt.is_none() || tokenizer_opt.is_none() {
                return Ok("Mock local generation response".to_string());
            }

            let model_arc = model_opt.unwrap();
            let tokenizer = tokenizer_opt.unwrap();

            // Apply Chat Template (Qwen style)
            let formatted_prompt = match system_instruction {
                Some(sys) => format!(
                    "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
                    sys, prompt
                ),
                None => format!(
                    "<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
                    prompt
                ),
            };

            // Tokenize prompt
            let tokens = tokenizer.encode(formatted_prompt, true)
                .map_err(|e| anyhow::anyhow!("Tokenizer error: {}", e))?
                .get_ids()
                .to_vec();

            let mut input_ids = tokens.clone();
            let mut generated_text = String::new();

            let mut model = model_arc.lock().await;

            // Initialize KV cache
            let num_layers = model.layers.len();
            let mut kv_cache = Vec::with_capacity(num_layers);
            for layer in &model.layers {
                let kv = (
                    mlx_rs::ops::zeros::<f32>(&[1, layer.self_attn.num_kv_heads, 0, layer.self_attn.head_dim])
                        .map_err(|e| anyhow::anyhow!("zeros cached_k build failed: {:?}", e))?,
                    mlx_rs::ops::zeros::<f32>(&[1, layer.self_attn.num_kv_heads, 0, layer.self_attn.head_dim])
                        .map_err(|e| anyhow::anyhow!("zeros cached_v build failed: {:?}", e))?,
                );
                kv_cache.push(kv);
            }

            // Run autoregressive generation
            for index in 0..1024 {
                let input_array = if index == 0 {
                    let tokens_i32: Vec<i32> = input_ids.iter().map(|&x| x as i32).collect();
                    Array::from_slice(&tokens_i32, &[1, tokens_i32.len() as i32])
                } else {
                    let last_token = input_ids[input_ids.len() - 1] as i32;
                    Array::from_slice(&[last_token], &[1, 1])
                };

                let position_offset = if index == 0 {
                    0
                } else {
                    (input_ids.len() - 1) as i32
                };

                // Forward pass to get logits of shape [1, seq_len, vocab_size]
                let logits = model.forward(&input_array, None, position_offset, &mut kv_cache)?;
                let seq_len = logits.shape()[1];

                // Extract last token's logits [vocab_size]
                use mlx_rs::ops::indexing::TryIndexOp;
                let last_logits = logits.try_index((0, (seq_len - 1) as i32))?;

                // Argmax to sample the most likely token (greedy)
                let next_token_arr = mlx_rs::ops::indexing::argmax_axis_device(last_logits, 0, false, StreamOrDevice::gpu())?;
                next_token_arr.eval()
                    .map_err(|e| anyhow::anyhow!("MLX logits eval failed: {:?}", e))?;
                
                let next_token = next_token_arr.as_slice::<u32>()[0];
                input_ids.push(next_token);

                // Decode and check stop tokens
                let decoded = tokenizer.decode(&[next_token], true)
                    .map_err(|e| anyhow::anyhow!("Tokenizer decode error: {}", e))?;
                
                if decoded.contains("<|im_end|>") || decoded.contains("<|endoftext|>") || decoded.is_empty() {
                    break;
                }
                
                generated_text.push_str(&decoded);
            }

            Ok(generated_text)
        })
    }
}

impl Drop for InProcessMlxEngine {
    fn drop(&mut self) {
        #[cfg(feature = "mlx")]
        {
            mlx_rs::transforms::compile::clear_cache();
            tracing::info!("Cleared MLX compile and Metal caches");
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelConfig {
    pub embeddings: String,
    pub tier1_fast_llm: String,
    pub tier2_coder_llm: String,
    pub tier3_reasoning_llm: String,
    pub max_context_window: usize,
}

pub static DYNAMIC_MODEL_BROKER: std::sync::OnceLock<Arc<DynamicModelBroker>> = std::sync::OnceLock::new();

/// DynamicModelBroker manages the lifecycle of LLM models.
pub struct DynamicModelBroker {
    models: Arc<Mutex<HashMap<ModelTier, Arc<dyn InferenceEngine>>>>,
    embedding_model_loaded: AtomicBool,
    config_model: Arc<Mutex<Option<String>>>,
    last_weak_ref: Arc<Mutex<Option<Weak<dyn InferenceEngine>>>>,
    corrupt_mock: bool,
    active_tier: Arc<Mutex<Option<ModelTier>>>,
    #[allow(dead_code)]
    models_dir: PathBuf,
}

impl DynamicModelBroker {
    /// Creates a new DynamicModelBroker.
    pub async fn new(model_dir: PathBuf) -> Result<Self> {
        Ok(Self {
            models: Arc::new(Mutex::new(HashMap::new())),
            embedding_model_loaded: AtomicBool::new(false),
            config_model: Arc::new(Mutex::new(None)),
            last_weak_ref: Arc::new(Mutex::new(None)),
            corrupt_mock: false,
            active_tier: Arc::new(Mutex::new(None)),
            models_dir: model_dir,
        })
    }

    /// Preloads the embedding model.
    pub async fn preload_embedding_model(&self, _model_name: &str) -> Result<()> {
        self.embedding_model_loaded.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Checks if the embedding model is loaded.
    pub fn is_embedding_model_loaded(&self) -> bool {
        self.embedding_model_loaded.load(Ordering::SeqCst)
    }

    /// Acquires an LLM model for the specified tier.
    pub async fn acquire_llm(&self, tier: ModelTier) -> Result<Arc<dyn InferenceEngine>> {
        if self.corrupt_mock {
            return Err(anyhow::anyhow!("Mock corruption: Failed to acquire model"));
        }

        // 1. Identify and evict all other LLM models to free VRAM
        let mut evict_list = Vec::new();
        {
            let mut models = self.models.lock().unwrap();
            // If the requested tier is already loaded, we can just return it immediately
            if let Some(model) = models.get(&tier) {
                return Ok(model.clone());
            }
            
            // Otherwise, we evict all other models
            for (t, m) in models.iter() {
                if *t != tier {
                    evict_list.push((*t, Arc::downgrade(m)));
                }
            }
            for (t, _) in &evict_list {
                models.remove(t);
            }
        } // Release lock before waiting to avoid deadlock!

        // 2. Block until the strong reference count of all evicted models drops to 0 (weak upgrade returns None)
        for (t, weak_ref) in evict_list {
            let start_wait = tokio::time::Instant::now();
            while weak_ref.upgrade().is_some() {
                if start_wait.elapsed() >= std::time::Duration::from_secs(30) {
                    tracing::warn!("Timeout waiting for evicted model tier {:?} to deallocate from VRAM", t);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            if weak_ref.upgrade().is_none() {
                tracing::info!("Evicted model tier {:?} successfully deallocated from VRAM", t);
            }
        }

        // 3. Determine the model name and download paths WITHOUT holding the lock
        let model_name = match tier {
            ModelTier::Tier1 => "mlx-community/Qwen2.5-0.5B-Instruct-4bit".to_string(),
            ModelTier::Tier2 => {
                let config_model = self.config_model.lock().unwrap();
                config_model.as_ref().cloned().unwrap_or_else(|| "mlx-community/Qwen3.6-35B-A3B-4bit".to_string())
            }
            ModelTier::Tier3 => "mlx-community/Qwen2.5-0.5B-Instruct-4bit".to_string(),
        };

        #[cfg(feature = "mlx")]
        let model_subdir = self.models_dir.join(model_name.replace("/", "_"));
        #[cfg(feature = "mlx")]
        std::fs::create_dir_all(&model_subdir)?;

        #[cfg(feature = "mlx")]
        {
            if std::env::var("MYTHRAX_TEST_MOCK").is_err() {
                // 1. Download config.json if missing
                let config_path = model_subdir.join("config.json");
                if !config_path.exists() {
                    let config_url = format!("https://huggingface.co/{}/resolve/main/config.json", model_name);
                    download_file_if_missing(&config_url, &config_path).await?;
                }

                // 2. Download tokenizer.json if missing
                let tokenizer_path = model_subdir.join("tokenizer.json");
                if !tokenizer_path.exists() {
                    let tokenizer_url = format!("https://huggingface.co/{}/resolve/main/tokenizer.json", model_name);
                    download_file_if_missing(&tokenizer_url, &tokenizer_path).await?;
                }

                // 3. Check for model.safetensors.index.json
                let index_path = model_subdir.join("model.safetensors.index.json");
                let index_url = format!("https://huggingface.co/{}/resolve/main/model.safetensors.index.json", model_name);
                
                let is_sharded = match reqwest::Client::new().head(&index_url).send().await {
                    Ok(resp) => resp.status().is_success(),
                    Err(_) => false,
                };

                if is_sharded {
                    download_file_if_missing(&index_url, &index_path).await?;
                    let content = std::fs::read_to_string(&index_path)?;
                    let index: serde_json::Value = serde_json::from_str(&content)?;
                    let weight_map = index.get("weight_map").context("weight_map not found in index")?;
                    let weight_map = weight_map.as_object().context("weight_map is not a JSON object")?;

                    let mut shard_files = std::collections::HashSet::new();
                    for shard_val in weight_map.values() {
                        if let Some(shard_str) = shard_val.as_str() {
                            shard_files.insert(shard_str.to_string());
                        }
                    }

                    for shard in shard_files {
                        let shard_path = model_subdir.join(&shard);
                        let shard_url = format!("https://huggingface.co/{}/resolve/main/{}", model_name, shard);
                        download_file_if_missing(&shard_url, &shard_path).await?;
                    }
                } else {
                    let safetensors_path = model_subdir.join("model.safetensors");
                    let safetensors_url = format!("https://huggingface.co/{}/resolve/main/model.safetensors", model_name);
                    download_file_if_missing(&safetensors_url, &safetensors_path).await?;
                }
            }
        }

        // 4. Now load the new model safely under lock
        let mut models = self.models.lock().unwrap();
        // Check again in case another thread loaded it while we were waiting
        let model = if let Some(model) = models.get(&tier) {
            model.clone()
        } else {
            #[cfg(feature = "mlx")]
            let (model_opt, tok_opt) = {
                let is_mock = {
                    #[cfg(any(test, debug_assertions))]
                    {
                        std::env::var("MYTHRAX_TEST_MOCK").is_ok() || std::env::var("MYTHRAX_MOCK_LLM").is_ok()
                    }
                    #[cfg(not(any(test, debug_assertions)))]
                    {
                        false
                    }
                };
                if is_mock {
                    (None, None)
                } else {
                    tracing::info!("Loading Qwen2 model from {} onto Metal", model_subdir.display());
                    let config_content = std::fs::read_to_string(&model_subdir.join("config.json"))?;
                    let config_json: serde_json::Value = serde_json::from_str(&config_content)?;

                    let num_layers = config_json.get("num_hidden_layers").and_then(|v| v.as_i64()).unwrap_or(28) as i32;
                    let num_heads = config_json.get("num_attention_heads").and_then(|v| v.as_i64()).unwrap_or(12) as i32;
                    let num_kv_heads = config_json.get("num_key_value_heads").and_then(|v| v.as_i64()).unwrap_or(2) as i32;
                    let hidden_size = config_json.get("hidden_size").and_then(|v| v.as_i64()).unwrap_or(1536) as i32;
                    let head_dim = hidden_size / num_heads;
                    let rms_norm_eps = config_json.get("rms_norm_eps").and_then(|v| v.as_f64()).unwrap_or(1e-6) as f32;
                    let vocab_size = config_json.get("vocab_size").and_then(|v| v.as_i64()).unwrap_or(151936) as i32;

                    let rope_theta = config_json.get("rope_theta")
                        .and_then(|v| v.as_f64())
                        .or_else(|| {
                            config_json.get("rope_parameters")
                                .and_then(|p| p.get("rope_theta"))
                                .and_then(|v| v.as_f64())
                        })
                        .unwrap_or(1000000.0) as f32;

                    let quantization = config_json.get("quantization")
                        .or_else(|| config_json.get("quantization_config"));
                    let bits = quantization.and_then(|q| q.get("bits").and_then(|v| v.as_i64())).unwrap_or(4) as i32;
                    let group_size = quantization.and_then(|q| q.get("group_size").and_then(|v| v.as_i64())).unwrap_or(64) as i32;

                    let num_experts = config_json.get("num_experts").and_then(|v| v.as_i64()).map(|x| x as i32);
                    let num_experts_per_tok = config_json.get("num_experts_per_tok").and_then(|v| v.as_i64()).map(|x| x as i32);

                    let weights = mlx_weights::load_model_weights(&model_subdir)?;
                    let weights_model = qwen2_mlx::Qwen2Model::new(
                        &weights,
                        num_layers,
                        num_heads,
                        num_kv_heads,
                        head_dim,
                        rope_theta,
                        rms_norm_eps,
                        vocab_size,
                        hidden_size,
                        group_size,
                        bits,
                        num_experts,
                        num_experts_per_tok,
                    )?;
                    
                    let tokenizer = Tokenizer::from_file(&model_subdir.join("tokenizer.json"))
                        .map_err(|e| anyhow::anyhow!("Tokenizer load error: {}", e))?;
                    
                    (Some(std::sync::Arc::new(tokio::sync::Mutex::new(weights_model))), Some(std::sync::Arc::new(tokenizer)))
                }
            };

            #[cfg(not(feature = "mlx"))]
            let (_model_opt, _tok_opt): (Option<std::sync::Arc<tokio::sync::Mutex<Qwen2Model>>>, Option<std::sync::Arc<Tokenizer>>) = (None, None);

            // Create a new model instance
            let engine = Arc::new(InProcessMlxEngine::new(
                model_name.clone(),
                true, // warmed_up: true
                vec!["<|eot_id|>".to_string(), "\"\"".to_string()], // stop_tokens
                "gpu".to_string(), // execution_mode
                #[cfg(feature = "mlx")]
                model_opt,
                #[cfg(feature = "mlx")]
                tok_opt,
            ));
            
            // Store the model
            models.insert(tier, engine.clone());
            engine
        };

        // Update the last weak reference
        let mut last_weak_ref = self.last_weak_ref.lock().unwrap();
        *last_weak_ref = Some(Arc::downgrade(&model));
        
        // Update the active tier
        let mut active_tier = self.active_tier.lock().unwrap();
        *active_tier = Some(tier);
        
        Ok(model)
    }

    /// Gets the active model tier.
    pub fn active_tier(&self) -> Option<ModelTier> {
        *self.active_tier.lock().unwrap()
    }

    /// Gets a weak reference to the last acquired LLM.
    pub fn get_weak_llm_reference(&self) -> Option<Weak<dyn InferenceEngine>> {
        let last_weak_ref = self.last_weak_ref.lock().unwrap();
        last_weak_ref.clone()
    }

    /// Evicts unused models from the cache.
    pub async fn evict_unused_models(&self) {
        let mut models = self.models.lock().unwrap();
        models.retain(|_, model| Arc::strong_count(model) > 1);
    }

    /// Updates the configuration model name.
    pub async fn update_config_model(&self, model_name: &str) -> Result<()> {
        let mut config_model = self.config_model.lock().unwrap();
        *config_model = Some(model_name.to_string());
        Ok(())
    }

    /// Creates a new corrupt mock broker.
    #[cfg(any(test, debug_assertions))]
    pub async fn new_corrupt_mock() -> Result<Self> {
        Ok(Self {
            models: Arc::new(Mutex::new(HashMap::new())),
            embedding_model_loaded: AtomicBool::new(false),
            config_model: Arc::new(Mutex::new(None)),
            last_weak_ref: Arc::new(Mutex::new(None)),
            corrupt_mock: true,
            active_tier: Arc::new(Mutex::new(None)),
            models_dir: PathBuf::new(),
        })
    }

    /// Acquires an LLM model with a warmup fallback mechanism.
    pub async fn acquire_llm_with_warmup_fallback(&self, tier: ModelTier) -> Result<Arc<dyn InferenceEngine>> {
        match self.acquire_llm(tier).await {
            Ok(model) => Ok(model),
            #[cfg(any(test, debug_assertions))]
            Err(_e) => {
                let mut models = self.models.lock().unwrap();
                models.clear();
                
                let fallback_model: Arc<dyn InferenceEngine> = Arc::new(InProcessMlxEngine::new(
                    "fallback-cpu-model".to_string(),
                    true,
                    vec!["<|eot_id|>".to_string(), "\"\"".to_string()],
                    "cpu".to_string(),
                    #[cfg(feature = "mlx")]
                    None,
                    #[cfg(feature = "mlx")]
                    None,
                ));
                
                models.insert(tier, fallback_model.clone());
                
                let mut last_weak_ref = self.last_weak_ref.lock().unwrap();
                *last_weak_ref = Some(Arc::downgrade(&fallback_model));
                
                Ok(fallback_model)
            }
            #[cfg(not(any(test, debug_assertions)))]
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_json_strip() {
        let input = "```json\n{\n  \"key\": \"value\"\n}\n```";
        let expected = "{\n  \"key\": \"value\"\n}";
        assert_eq!(strip_code_fences(input), expected);

        let input_no_lang = "```\nsome non-json content\n```";
        let expected_no_lang = "some non-json content";
        assert_eq!(strip_code_fences(input_no_lang), expected_no_lang);
    }
}

#[cfg(feature = "mlx")]
async fn download_file_if_missing(url: &str, path: &std::path::Path) -> Result<()> {
    let is_mock = {
        #[cfg(any(test, debug_assertions))]
        {
            std::env::var("MYTHRAX_TEST_MOCK").is_ok() || std::env::var("MYTHRAX_MOCK_LLM").is_ok()
        }
        #[cfg(not(any(test, debug_assertions)))]
        {
            false
        }
    };
    if is_mock {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, b"dummy")?;
        return Ok(());
    }
    if path.exists() {
        if let Ok(metadata) = std::fs::metadata(path) {
            if metadata.len() > 0 {
                return Ok(());
            }
        }
    }
    tracing::info!("Downloading model asset from {} to {}...", url, path.display());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    use futures_util::StreamExt;
    let response = reqwest::get(url).await?;
    let status = response.status();
    if !status.is_success() && !status.is_redirection() {
        anyhow::bail!(
            "Failed to download model from {}. HTTP status: {}. \
             Supply a pre-downloaded model asset file at {}.",
            url, status, path.display()
        );
    }
    let mut file = std::fs::File::create(path)?;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        std::io::copy(&mut std::io::Cursor::new(chunk), &mut file)?;
    }
    tracing::info!("Successfully downloaded asset to {}", path.display());
    Ok(())
}
