use crate::db::StorageBackend;
use anyhow::{Context, Result};
use std::sync::OnceLock;
use tokio::sync::Semaphore;
use std::sync::{Arc, Mutex, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::HashMap;
use std::path::PathBuf;

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
    ) -> Result<String> {
        if let Ok(mock) = std::env::var("MYTHRAX_MOCK_LLM") {
            if mock == "true" {
                if prompt.contains("Analyze the following dialog") {
                    return Ok(r#"{"target_pattern": "test_pattern", "action_to_avoid": "test_action", "causal_explanation": "test_causal", "prescribed_remedy": "test_remedy"}"#.to_string());
                } else if prompt.contains("Validate if these should merge") {
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
                } else {
                    return Ok(r#"[{"name": "test_concept", "content": "test_explanation"}]"#.to_string());
                }
            }
        }

        let config = db.get_llm_config().await?;
        
        let response_text = match active_provider {
            "local" => {
                let url = "http://127.0.0.1:8080/v1/chat/completions";
                let truncated_prompt = if prompt.len() > 100_000 {
                    let truncated = truncate_to_boundary(prompt, 100_000);
                    format!("{}... [Truncated due to local context limits]", truncated)
                } else {
                    prompt.to_string()
                };

                let mut messages = Vec::new();
                if let Some(sys) = system_instruction {
                    messages.push(serde_json::json!({
                        "role": "system",
                        "content": sys
                    }));
                }
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": truncated_prompt
                }));

                let payload = serde_json::json!({
                    "model": model,
                    "messages": messages,
                    "temperature": 0.2,
                    "max_tokens": 8192
                });

                // Serialize all local LLM calls — only one request in-flight at a time
                let _permit = metal_inference_semaphore().acquire().await
                    .map_err(|e| anyhow::anyhow!("LLM semaphore error: {}", e))?;

                let req = self.client.post(url).json(&payload);
                let resp = send_with_retry(&self.client, req).await?;
                let json: serde_json::Value = resp.json().await?;
                tracing::debug!("Local LLM raw response: {}", json);
                let content = json["choices"][0]["message"]["content"].clone();
                
                let result = if content.is_null() {
                    // Gemma 4 / thinking models emit `reasoning` instead of `content`
                    // when finish_reason=length (hit max_tokens mid-thought) or when
                    // the model uses a separate reasoning field.
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

                // Pause for 5 seconds to give GPU/cache recovery time before releasing semaphore
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                
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
    let mut cleaned = content.trim().to_string();
    if cleaned.starts_with("```") {
        if let Some(first_newline_pos) = cleaned.find('\n') {
            cleaned = cleaned[first_newline_pos + 1..].to_string();
        }
        if cleaned.ends_with("```") {
            cleaned = cleaned[..cleaned.len() - 3].trim().to_string();
        } else if cleaned.ends_with("```\n") {
            cleaned = cleaned[..cleaned.len() - 4].trim().to_string();
        }
    }
    cleaned
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
        let jitter = (tokio::time::Instant::now().elapsed().as_nanos() % 100) as f64;
        let delay_ms = (base_ms * factor + jitter).min(5000.0);
        let sleep_duration = std::time::Duration::from_millis(delay_ms as u64);
        tokio::time::sleep(sleep_duration).await;
    }
}

/// Truncates a string to a maximum character count, respecting boundary
/// awareness (paragraphs, lines, words) to avoid cutting mid-sentence.
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
}

/// A concrete implementation of InferenceEngine for MLX-based models.
pub struct InProcessMlxEngine {
    name: String,
    warmed_up: bool,
    stop_tokens: Vec<String>,
    execution_mode: String,
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
    ) -> Self {
        Self {
            name,
            warmed_up,
            stop_tokens,
            execution_mode,
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
}

impl DynamicModelBroker {
    /// Creates a new DynamicModelBroker.
    pub async fn new(_model_dir: PathBuf) -> Result<Self> {
        Ok(Self {
            models: Arc::new(Mutex::new(HashMap::new())),
            embedding_model_loaded: AtomicBool::new(false),
            config_model: Arc::new(Mutex::new(None)),
            last_weak_ref: Arc::new(Mutex::new(None)),
            corrupt_mock: false,
            active_tier: Arc::new(Mutex::new(None)),
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
            while weak_ref.upgrade().is_some() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            tracing::info!("Evicted model tier {:?} successfully deallocated from VRAM", t);
        }

        // 3. Now load the new model safely
        let mut models = self.models.lock().unwrap();
        // Check again in case another thread loaded it while we were waiting
        let model = if let Some(model) = models.get(&tier) {
            model.clone()
        } else {
            // Determine the model name based on config
            let config_model = self.config_model.lock().unwrap();
            let model_name = match config_model.as_ref() {
                Some(name) => name.clone(),
                None => "Qwen2.5-Coder-7B-Instruct-MLX-4bit".to_string(),
            };
            
            // Create a new model instance
            let engine = Arc::new(InProcessMlxEngine::new(
                model_name,
                true, // warmed_up: true
                vec!["<|eot_id|>".to_string(), "\"\"".to_string()], // stop_tokens
                "gpu".to_string(), // execution_mode
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
    pub fn get_weak_llm_reference(&self) -> Weak<dyn InferenceEngine> {
        let last_weak_ref = self.last_weak_ref.lock().unwrap();
        last_weak_ref.clone().unwrap_or_else(|| {
            // Return a dummy weak reference if none exists
            let dummy: Arc<dyn InferenceEngine> = Arc::new(InProcessMlxEngine::new(
                "dummy".to_string(),
                false,
                vec![],
                "cpu".to_string(),
            ));
            Arc::downgrade(&dummy)
        })
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
    pub async fn new_corrupt_mock() -> Result<Self> {
        Ok(Self {
            models: Arc::new(Mutex::new(HashMap::new())),
            embedding_model_loaded: AtomicBool::new(false),
            config_model: Arc::new(Mutex::new(None)),
            last_weak_ref: Arc::new(Mutex::new(None)),
            corrupt_mock: true,
            active_tier: Arc::new(Mutex::new(None)),
        })
    }

    /// Acquires an LLM model with a warmup fallback mechanism.
    pub async fn acquire_llm_with_warmup_fallback(&self, tier: ModelTier) -> Result<Arc<dyn InferenceEngine>> {
        match self.acquire_llm(tier).await {
            Ok(model) => Ok(model),
            Err(_) => {
                let mut models = self.models.lock().unwrap();
                models.clear();
                
                let fallback_model: Arc<dyn InferenceEngine> = Arc::new(InProcessMlxEngine::new(
                    "fallback-cpu-model".to_string(),
                    true,
                    vec!["<|eot_id|>".to_string(), "\"\"".to_string()],
                    "cpu".to_string(),
                ));
                
                models.insert(tier, fallback_model.clone());
                
                let mut last_weak_ref = self.last_weak_ref.lock().unwrap();
                *last_weak_ref = Some(Arc::downgrade(&fallback_model));
                
                Ok(fallback_model)
            }
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
