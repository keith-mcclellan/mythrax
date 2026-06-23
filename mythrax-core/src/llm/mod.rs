use crate::db::StorageBackend;
use anyhow::{Context, Result};
use std::sync::OnceLock;
use tokio::sync::Semaphore;

/// Process-global semaphore that limits concurrent local LLM requests to 1.
/// This prevents memory pressure when running on a machine with a constrained
/// model context window (e.g. 16k tokens on Apple Silicon).
static LOCAL_LLM_SEMAPHORE: OnceLock<Semaphore> = OnceLock::new();

fn local_llm_semaphore() -> &'static Semaphore {
    LOCAL_LLM_SEMAPHORE.get_or_init(|| Semaphore::new(1))
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
        if let Ok(mock) = std::env::var("MYTHRAX_MOCK_LLM") {
            if mock == "true" {
                if prompt.contains("Wisdom") || prompt.contains("rules") || prompt.contains("Wisdom Rules") {
                    return Ok(r#"[{"target_pattern": "test_pattern", "action_to_avoid": "test_action", "causal_explanation": "test_causal", "prescribed_remedy": "test_remedy"}]"#.to_string());
                } else {
                    return Ok(r#"[{"name": "test_concept", "content": "test_explanation"}]"#.to_string());
                }
            }
        }

        let config = db.get_llm_config().await?;
        
        let response_text = match config.active_provider.as_str() {
            "local" => {
                let url = "http://127.0.0.1:8080/v1/chat/completions";
                let mut messages = Vec::new();
                if let Some(sys) = system_instruction {
                    messages.push(serde_json::json!({
                        "role": "system",
                        "content": sys
                    }));
                }
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": prompt
                }));

                let payload = serde_json::json!({
                    "model": config.model,
                    "messages": messages,
                    "temperature": 0.2,
                    "max_tokens": 16384
                });

                // Serialize all local LLM calls — only one request in-flight at a time
                let _permit = local_llm_semaphore().acquire().await
                    .map_err(|e| anyhow::anyhow!("LLM semaphore error: {}", e))?;

                let req = self.client.post(url).json(&payload);
                let resp = send_with_retry(&self.client, req).await?;
                let json: serde_json::Value = resp.json().await?;
                tracing::debug!("Local LLM raw response: {}", json);
                let content = json["choices"][0]["message"]["content"].clone();
                if content.is_null() {
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
                }
            }
            "cloud" => {
                match config.cloud_provider.as_str() {
                    "gemini" => {
                        let api_key = config.api_key.clone().unwrap_or_else(|| std::env::var("GEMINI_API_KEY").unwrap_or_default());
                        let url = format!(
                            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                            config.model, api_key
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
                            "model": config.model,
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
            if let Ok(res) = db.get_wisdom(parent_hypothesis, tier, 5, 0, 0.55).await {
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
                if attempt >= 3 {
                    anyhow::bail!("HTTP request failed with status {}: {}", status, body_text);
                }
            }
            Err(e) => {
                tracing::warn!("HTTP request error on attempt {}: {}", attempt, e);
                if attempt >= 3 {
                    return Err(e.into());
                }
            }
        }
        attempt += 1;
        let base_ms = 200.0;
        let factor = (2.0f64).powi(attempt);
        let jitter = (tokio::time::Instant::now().elapsed().as_nanos() % 100) as f64;
        let sleep_duration = std::time::Duration::from_millis((base_ms * factor + jitter) as u64);
        tokio::time::sleep(sleep_duration).await;
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
