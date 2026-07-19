use anyhow::Result;
use chrono::Utc;
use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::StorageBackend;
use crate::db::SurrealBackend;
use crate::db::CognitiveTask;
use crate::contracts::{EpisodeSave, WisdomRule, Tier};

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

pub async fn harvest_completed_reflections(backend: &SurrealBackend) -> Result<()> {
    let sql = "SELECT * FROM cognitive_task WHERE task_type = 'reflection_distillation' AND status = 'Completed';";
    let mut res = backend.db.query(sql).await?;
    let tasks_raw: Vec<crate::db::cognitive_tasks::CognitiveTaskRaw> = res.take(0)?;
    let tasks: Vec<CognitiveTask> = tasks_raw.into_iter().map(CognitiveTask::from).collect();
    
    for task in tasks {
        if let Some(ref result_str) = task.result {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(result_str) {
                let outcome = parsed["outcome"].as_str().map(|s| s.to_string());
                let causal = parsed["causal_explanation"].as_str().map(|s| s.to_string());
                let mut files = vec![];
                if let Some(arr) = parsed["files_modified"].as_array() {
                    for v in arr {
                        if let Some(s) = v.as_str() {
                            files.push(s.to_string());
                        }
                    }
                }
                
                let title = format!("Session Reflection: {}", task.session_id.as_deref().unwrap_or("Unknown"));
                let content = format!("Lessons:\n{:?}\nError Patterns:\n{:?}", parsed["lessons"], parsed["error_patterns"]);

                let ep = EpisodeSave::builder(title, content)
                    .node_type(Some("experience".to_string()))
                    .session_id(task.session_id.clone())
                    .outcome(outcome.clone())
                    .causal_explanation(causal.clone())
                    .files_modified(Some(files))
                    .build();

                let ep_id = backend.save_episode_db(&ep).await?;

                if outcome.as_deref() == Some("failure") {
                    if let Some(embedder) = &backend.embedder {
                        let text_to_embed = format!("{} {:?}", causal.unwrap_or_default(), parsed["lessons"]);
                        if let Ok(vec) = embedder.embed(&text_to_embed) {
                            let rule_sql = "SELECT * FROM wisdom WHERE rule_type = 'pruned_hypothesis' AND status = 'active';";
                             if let Ok(mut rule_res) = backend.db.query(rule_sql).await {
                                if let Ok(rules_raw) = rule_res.take::<Vec<crate::db::backend::WisdomRaw>>(0) {
                                    let rules: Vec<WisdomRule> = rules_raw.into_iter().map(|r| r.into_wisdom_rule()).collect();
                                    let mut matched = false;
                                    for mut rule in rules {
                                        if let Some(ref rule_emb) = rule.embedding {
                                            let sim = crate::math::cosine_similarity(&vec, rule_emb);
                                            if sim >= 0.80 {
                                                let current_imp = rule.importance.unwrap_or(0.0);
                                                rule.importance = Some((current_imp + 0.2).min(1.0));
                                                let _ = backend.save_wisdom_rule_db(&rule).await;
                                                matched = true;
                                                break;
                                            }
                                        }
                                    }
                                    if !matched {
                                        let causal_str = parsed["causal_explanation"].as_str().unwrap_or("").to_string();
                                        let lessons_str = if let Some(arr) = parsed["lessons"].as_array() {
                                            let items: Vec<String> = arr.iter()
                                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                                .collect();
                                            items.join(", ")
                                        } else {
                                            parsed["lessons"].as_str().unwrap_or("").to_string()
                                        };
                                        let new_rule = WisdomRule {
                                            id: None,
                                            target_pattern: format!("PRUNED: {}", causal_str),
                                            action_to_avoid: "Repeat failed approach".to_string(),
                                            causal_explanation: causal_str,
                                            prescribed_remedy: format!("Lessons: {}", lessons_str),
                                            tier: Tier::Working,
                                            scope: "general".to_string(),
                                            vault_path: None,
                                            source_episodes: vec![ep_id],
                                            generator_name: "reflect_harvester".to_string(),
                                            embedding: Some(vec),
                                            utility: Some(50.0),
                                            status: Some("active".to_string()),
                                            superseded_at: None,
                                            superseded_by: None,
                                            severity: Some("low".to_string()),
                                            blocking: Some(false),
                                            rule_type: Some("pruned_hypothesis".to_string()),
                                            importance: Some(0.2), // Initial importance
                                            ..Default::default()
                                        };
                                        let _ = backend.save_wisdom_rule_db(&new_rule).await;
                                    }
                                }
                             }
                        }
                    }
                }
                
                let del_sql = "DELETE type::record('cognitive_task', $id);";
                if let Some(id_part) = task.id.strip_prefix("cognitive_task:") {
                    let _ = backend.db.query(del_sql).bind(("id", id_part)).await;
                }
            }
        }
    }
    Ok(())
}
