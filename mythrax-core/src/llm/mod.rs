use crate::db::StorageBackend;
use anyhow::{Result, Context};

pub struct LLMClient {
    client: reqwest::Client,
}

impl LLMClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

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
                    "temperature": 0.2
                });

                let req = self.client.post(url).json(&payload);
                let resp = send_with_retry(&self.client, req).await?;
                let json: serde_json::Value = resp.json().await?;
                json["choices"][0]["message"]["content"]
                    .as_str()
                    .context("Invalid local completion response")?
                    .to_string()
            }
            "cloud" => {
                match config.cloud_provider.as_str() {
                    "gemini" => {
                        let api_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
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
                        let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
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
    async fn propose_hypotheses(&self, db: &dyn StorageBackend, _parent_id: &str, parent_hypothesis: &str) -> Result<String> {
        let prompt = format!(
            "Based on the parent hypothesis: \"{}\", propose two alternative child hypotheses to refine it.\n\
             Return a JSON array of objects, each containing:\n\
             - \"node_id\": a unique sequential string ID (e.g. \"1\", \"2\")\n\
             - \"hypothesis\": description of the refinement\n\
             - \"score\": float utility expectation from 0.0 to 100.0 (assign higher score to the better candidate)\n\n\
             Output format MUST be a raw JSON array only.",
            parent_hypothesis
        );
        self.completion(db, Some("You are an ideation assistant that outputs raw JSON arrays."), &prompt).await
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
        let factor = (2.0f64).powi(attempt as i32);
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
