use anyhow::Result;
use chrono::Utc;
use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::StorageBackend;
use crate::db::SurrealBackend;
use crate::db::CognitiveTask;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ToolCall {
    pub name: String,
    #[serde(default)]
    pub args: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TranscriptStep {
    pub step_index: Option<usize>,
    pub source: Option<String>,
    pub r#type: Option<String>,
    pub status: Option<String>,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

pub async fn handle_reflect(
    session_id: &str,
    transcript_path: &str,
    backend: &dyn StorageBackend,
) -> Result<String> {
    let path = Path::new(transcript_path);
    if !path.exists() {
        return Ok("skipped_missing".to_string());
    }

    let file_content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Ok("skipped_missing".to_string()),
    };

    let mut turns = 0;
    let mut tool_calls_count = 0;
    let mut steps = Vec::new();

    for line in file_content.lines() {
        if let Ok(step) = serde_json::from_str::<TranscriptStep>(line) {
            let t = step.r#type.as_deref().unwrap_or("");
            if t == "USER_INPUT" || t == "PLANNER_RESPONSE" {
                turns += 1;
            }
            if let Some(ref calls) = step.tool_calls {
                tool_calls_count += calls.len();
            }
            steps.push(step);
        }
    }

    // Complexity gate: If ≤ 3 tool calls OR ≤ 5 turns, skip reflection
    if turns <= 5 || tool_calls_count <= 3 {
        return Ok("skipped_trivial".to_string());
    }

    // Construct the distillation prompt
    let mut transcript_summary = String::new();
    for step in &steps {
        let type_str = step.r#type.as_deref().unwrap_or("");
        if let Some(ref content) = step.content {
            // Cap content of each step to prevent oversized prompts
            let clipped = if content.len() > 1000 {
                format!("{}... [truncated]", &content[..1000])
            } else {
                content.clone()
            };
            transcript_summary.push_str(&format!("{}: {}\n", type_str, clipped));
        }
        if let Some(ref calls) = step.tool_calls {
            for call in calls {
                transcript_summary.push_str(&format!("Tool call: {}\n", call.name));
            }
        }
    }

    let user_prompt = format!(
        "Analyze the following coding session transcript and distill it into a causal experience experience episode.
Identify:
1. Outcome: was it a success, partial success, failure, or abandoned?
2. Causal explanation: a 1-3 sentence description explaining WHY this outcome occurred.
3. Key lessons: list of specific actionable lessons learned.
4. Error patterns: any error messages or compiler/runtime error patterns encountered.
5. Files modified: paths of files edited.

Transcript:
{}

Return JSON format matching:
{{
  \"outcome\": \"success|partial|failure|abandoned\",
  \"causal_explanation\": \"WHY\",
  \"lessons\": [\"lesson 1\"],
  \"error_patterns\": [\"pattern 1\"],
  \"files_modified\": [\"file1\"]
}}",
        transcript_summary
    );

    let system_instruction = "You are a transcript distillation agent that outputs JSON only.";
    
    if let Some(surreal_backend) = backend.as_any().downcast_ref::<SurrealBackend>() {
        let task_id = format!("cognitive_task:{}", Uuid::new_v4());
        let task = CognitiveTask {
            id: task_id,
            task_type: "reflection_distillation".to_string(),
            prompt: user_prompt,
            system_instruction: system_instruction.to_string(),
            expected_format: "Json".to_string(),
            priority: "Normal".to_string(),
            created_at: Utc::now(),
            status: "Pending".to_string(),
            result: None,
            ttl_minutes: 10,
            injected_at: None,
            session_id: Some(session_id.to_string()),
        };
        surreal_backend.create_cognitive_task(&task).await?;
        Ok("reflection_queued".to_string())
    } else {
        anyhow::bail!("SurrealBackend required for cognitive callback")
    }
}
