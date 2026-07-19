use std::path::{Path, PathBuf};
use anyhow::Result;
use serde::{Serialize, Deserialize};
use chrono::Utc;
use uuid::Uuid;
use crate::db::StorageBackend;
use crate::contracts::{EpisodeSave, WikiNode, WisdomRule, Tier};
use crate::db::cognitive_tasks::CognitiveTask;
use crate::llm::LLMClient;
use crate::db::backend::SurrealBackend;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DistilledConversation {
    pub conversation_id: String,
    pub title: String,
    pub scope: String,
    pub timestamp: String,
    pub decisions: Vec<String>,
    pub constraints_discovered: Vec<String>,
    pub code_changes: Vec<String>,
    pub commands_run: Vec<String>,
    pub errors_resolved: Vec<String>,
    pub user_preferences: Vec<String>,
    pub summary: String,
    pub key_takeaways: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ToolCall {
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TranscriptStep {
    pub step_index: usize,
    pub source: String,
    pub r#type: String,
    pub status: String,
    pub created_at: String,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

pub fn is_edit_tool(tool_name: &str) -> bool {
    matches!(tool_name, "write_to_file" | "edit_file" | "replace_file_content" | "multi_replace_file_content" | "write")
}

pub fn has_edit_calls(step: &TranscriptStep) -> bool {
    step.tool_calls.as_ref().map_or(false, |calls| {
        calls.iter().any(|c| is_edit_tool(&c.name))
    })
}

pub fn has_tool_calls(step: &TranscriptStep) -> bool {
    step.tool_calls.as_ref().map_or(false, |calls| {
        calls.iter().any(|c| !is_edit_tool(&c.name))
    })
}

pub fn chunk_transcript(steps: &[TranscriptStep]) -> Vec<Vec<TranscriptStep>> {
    let mut chunks = Vec::new();
    let mut current_chunk = Vec::new();
    let mut user_input_count = 0;
    
    for step in steps {
        let is_user_input = step.r#type == "USER_INPUT";
        let step_has_edit = has_edit_calls(step);
        let step_has_tool = has_tool_calls(step);
        
        let should_split = if current_chunk.is_empty() {
            false
        } else {
            let last_step = current_chunk.last().unwrap();
            let last_has_edit = has_edit_calls(last_step);
            let last_has_tool = has_tool_calls(last_step);
            
            (is_user_input && user_input_count >= 10)
                || (step_has_edit && !last_has_edit)
                || (step_has_tool && !last_has_tool)
                || (!step_has_edit && last_has_edit)
                || (!step_has_tool && last_has_tool)
        };
        
        if should_split {
            chunks.push(current_chunk);
            current_chunk = Vec::new();
            user_input_count = 0;
        }
        
        if is_user_input {
            user_input_count += 1;
        }
        current_chunk.push(step.clone());
    }
    
    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }
    
    chunks
}

pub fn enforce_symbol_integrity(input: &str, output: &str) -> String {
    if let Some(start_idx) = input.find("### Key Code Symbols & Paths") {
        let sub = &input[start_idx..];
        let mut end_idx = sub.len();
        for (i, line) in sub.lines().enumerate() {
            if i > 0 && (line.starts_with('#') || line.starts_with("---")) {
                if let Some(pos) = sub.find(line) {
                    end_idx = pos;
                    break;
                }
            }
        }
        let symbol_section = sub[..end_idx].trim_end().to_string();
        
        if let Some(out_start) = output.find("### Key Code Symbols & Paths") {
            let out_sub = &output[out_start..];
            let mut out_end = out_sub.len();
            for (i, line) in out_sub.lines().enumerate() {
                if i > 0 && (line.starts_with('#') || line.starts_with("---")) {
                    if let Some(pos) = out_sub.find(line) {
                        out_end = pos;
                        break;
                    }
                }
            }
            let mut final_out = output[..out_start].to_string();
            final_out.push_str(&symbol_section);
            final_out.push_str(&out_sub[out_end..]);
            final_out
        } else {
            let mut final_out = output.to_string();
            if !final_out.ends_with('\n') {
                final_out.push('\n');
            }
            final_out.push_str("\n");
            final_out.push_str(&symbol_section);
            final_out
        }
    } else {
        output.to_string()
    }
}

pub async fn run_summarization_task(
    db: &dyn StorageBackend,
    client: &LLMClient,
    content: &str,
) -> Result<String> {
    let sys_prompt = "You are a code summarizer. You MUST enforce the Symbol Integrity prompt contract: if there is a '### Key Code Symbols & Paths' section in the context, you must maintain it verbatim in your response.";
    let user_prompt = format!("Summarize the following content:\n\n{}", content);
    
    // Try cognitive callback first if not bootstrapping in CLI mode
    if std::env::var("MYTHRAX_BOOTSTRAPPING").is_err() {
        if let Some(surreal_backend) = db.as_any().downcast_ref::<SurrealBackend>() {
            let task_id = format!("cognitive_task:{}", Uuid::new_v4());
            let task = CognitiveTask {
                id: task_id.clone(),
                task_type: "Summarization".to_string(),
                prompt: user_prompt.clone(),
                system_instruction: sys_prompt.to_string(),
                expected_format: "Any".to_string(),
                priority: "Normal".to_string(),
                created_at: Utc::now(),
                status: "Pending".to_string(),
                result: None,
                ttl_minutes: 10,
                injected_at: None,
            };
            
            if surreal_backend.create_cognitive_task(&task).await.is_ok() {
                let start = std::time::Instant::now();
                let timeout = std::time::Duration::from_secs(60);
                while start.elapsed() < timeout {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    if let Ok(Some(updated)) = surreal_backend.get_cognitive_task(&task_id).await {
                        if updated.status == "Completed" {
                            if let Some(res) = updated.result {
                                return Ok(enforce_symbol_integrity(content, &res));
                            }
                        }
                    }
                }
                tracing::warn!("Cognitive callback timed out, falling back to LargeLocal");
            }
        }
    }
    
    // Fallback to LargeLocal
    let profile = crate::contracts::TaskProfile::new(crate::contracts::TaskArchetype::Summarization);
    let raw_res = client.routed_completion(db, &profile, Some(sys_prompt), &user_prompt).await?;
    Ok(enforce_symbol_integrity(content, &raw_res))
}

pub async fn recursive_summarize(
    db: &dyn StorageBackend,
    client: &LLMClient,
    chunks_content: &[String],
) -> Result<String> {
    if chunks_content.is_empty() {
        return Ok("".to_string());
    }
    
    let mut current_summaries = Vec::new();
    for chunk in chunks_content {
        let summary = run_summarization_task(db, client, chunk).await?;
        current_summaries.push(summary);
    }
    
    while current_summaries.len() > 1 {
        let mut next_level = Vec::new();
        for chunk_pair in current_summaries.chunks(2) {
            let combined = chunk_pair.join("\n\n---\n\n");
            let summary = run_summarization_task(db, client, &combined).await?;
            next_level.push(summary);
        }
        current_summaries = next_level;
    }
    
    Ok(current_summaries.into_iter().next().unwrap_or_default())
}

pub async fn distill_transcript_file(
    db: &dyn StorageBackend,
    client: &LLMClient,
    path: &Path,
    conversation_id: &str,
    scope: &str,
) -> Result<Vec<DistilledConversation>> {
    let content = std::fs::read_to_string(path)?;
    let mut steps = Vec::new();
    for line in content.lines() {
        if let Ok(step) = serde_json::from_str::<TranscriptStep>(line) {
            steps.push(step);
        }
    }
    
    let step_chunks = chunk_transcript(&steps);
    let mut distilled_chunks = Vec::new();
    
    for chunk in step_chunks {
        let mut commands_run = Vec::new();
        let mut code_changes = Vec::new();
        let mut errors_resolved = Vec::new();
        let mut chunk_text = String::new();
        
        for step in &chunk {
            if let Some(ref text) = step.content {
                chunk_text.push_str(text);
                chunk_text.push('\n');
            }
            if let Some(ref calls) = step.tool_calls {
                for call in calls {
                    if call.name == "run_command" {
                        if let Some(cmd) = call.args.get("CommandLine").and_then(|v| v.as_str()) {
                            commands_run.push(cmd.to_string());
                        }
                    } else if is_edit_tool(&call.name) {
                        if let Some(file) = call.args.get("TargetFile").and_then(|v| v.as_str()) {
                            code_changes.push(file.to_string());
                        }
                    }
                }
            }
            if step.status == "ERROR" {
                if let Some(ref text) = step.content {
                    errors_resolved.push(text.to_string());
                }
            }
        }
        
        // Formulate LLM distillation prompt to parse semantic elements
        let prompt = format!(
            "Analyze the following segment of a coding transcript. Extract:
            1. Decisions: key design, architecture, or scope decisions.
            2. Constraints: any constraints or requirements discovered.
            3. User Preferences: any user stated preferences.
            4. Summary: a one paragraph summary.
            5. Takeaways: key takeaways.

            Transcript Content:
            {}
            
            Return JSON format matching:
            {{
              \"title\": \"Segment Title\",
              \"scope\": \"general\",
              \"decisions\": [\"dec1\"],
              \"constraints_discovered\": [\"con1\"],
              \"user_preferences\": [\"pref1\"],
              \"summary\": \"concise summary\",
              \"key_takeaways\": [\"takeaway1\"]
            }}",
            chunk_text
        );
        
        let profile = crate::contracts::TaskProfile::new(crate::contracts::TaskArchetype::Reasoning);
        let sys_msg = "You are a transcript distillation agent that outputs JSON only.";
        
        let mut res_json_str = None;
        if std::env::var("MYTHRAX_BOOTSTRAPPING").is_err() {
            if let Some(surreal_backend) = db.as_any().downcast_ref::<SurrealBackend>() {
                let task_id = format!("cognitive_task:{}", Uuid::new_v4());
                let task = CognitiveTask {
                    id: task_id.clone(),
                    task_type: "InsightExtraction".to_string(),
                    prompt: prompt.clone(),
                    system_instruction: sys_msg.to_string(),
                    expected_format: "Json".to_string(),
                    priority: "Normal".to_string(),
                    created_at: Utc::now(),
                    status: "Pending".to_string(),
                    result: None,
                    ttl_minutes: 10,
                    injected_at: None,
                };
                
                if surreal_backend.create_cognitive_task(&task).await.is_ok() {
                    let start = std::time::Instant::now();
                    let timeout = std::time::Duration::from_secs(60);
                    while start.elapsed() < timeout {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        if let Ok(Some(updated)) = surreal_backend.get_cognitive_task(&task_id).await {
                            if updated.status == "Completed" {
                                if let Some(res) = updated.result {
                                    res_json_str = Some(res);
                                    break;
                                }
                            }
                        }
                    }
                    if res_json_str.is_none() {
                        tracing::warn!("InsightExtraction cognitive callback timed out, falling back to LargeLocal");
                    }
                }
            }
        }
        
        let res_json_str = match res_json_str {
            Some(s) => s,
            None => client.routed_completion(db, &profile, Some(sys_msg), &prompt).await?,
        };
        
        #[derive(Deserialize, Default)]
        struct DistilledResponse {
            title: Option<String>,
            scope: Option<String>,
            decisions: Option<Vec<String>>,
            constraints_discovered: Option<Vec<String>>,
            user_preferences: Option<Vec<String>>,
            summary: Option<String>,
            key_takeaways: Option<Vec<String>>,
        }
        
        let parsed: DistilledResponse = serde_json::from_str(&res_json_str).unwrap_or_default();
        
        let distilled = DistilledConversation {
            conversation_id: conversation_id.to_string(),
            title: parsed.title.unwrap_or_else(|| format!("Distilled Segment - {}", conversation_id)),
            scope: parsed.scope.unwrap_or_else(|| scope.to_string()),
            timestamp: chunk.first().map(|s| s.created_at.clone()).unwrap_or_else(|| Utc::now().to_rfc3339()),
            decisions: parsed.decisions.unwrap_or_default(),
            constraints_discovered: parsed.constraints_discovered.unwrap_or_default(),
            code_changes,
            commands_run,
            errors_resolved,
            user_preferences: parsed.user_preferences.unwrap_or_default(),
            summary: parsed.summary.unwrap_or_default(),
            key_takeaways: parsed.key_takeaways.unwrap_or_default(),
        };
        
        distilled_chunks.push(distilled);
    }
    
    Ok(distilled_chunks)
}

pub fn extract_decisions(content: &str) -> String {
    let mut decisions = Vec::new();
    let mut in_decisions_section = false;
    
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.to_lowercase().starts_with("#") && trimmed.to_lowercase().contains("decision") {
            in_decisions_section = true;
            continue;
        } else if trimmed.to_lowercase().starts_with("#") && in_decisions_section {
            in_decisions_section = false;
        }
        
        if in_decisions_section {
            if !trimmed.is_empty() {
                decisions.push(line.to_string());
            }
        } else {
            let lower = trimmed.to_lowercase();
            if (trimmed.starts_with("-") || trimmed.starts_with("*") || (trimmed.chars().next().map_or(false, |c| c.is_ascii_digit()))) 
               && (lower.contains("decision") || lower.contains("decided") || lower.contains("decide")) {
                decisions.push(line.to_string());
            }
        }
    }
    
    if decisions.is_empty() {
        "No explicit decisions extracted.".to_string()
    } else {
        decisions.join("\n")
    }
}

pub fn extract_completed_tasks(content: &str) -> String {
    let mut completed = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("- [x]") || trimmed.starts_with("- [X]") 
           || trimmed.starts_with("* [x]") || trimmed.starts_with("* [X]") {
            completed.push(line.to_string());
        }
    }
    if completed.is_empty() {
        "No completed tasks found in checklist.".to_string()
    } else {
        format!("Completed Checklist Items:\n{}", completed.join("\n"))
    }
}

pub async fn ingest_artifacts_in_dir(
    db: &dyn StorageBackend,
    dir_path: &Path,
    conversation_id: &str,
    scope: &str,
) -> Result<()> {
    if !dir_path.exists() {
        return Ok(());
    }
    
    if let Ok(entries) = std::fs::read_dir(dir_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let file_name = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
                let rel_path_str = path.to_string_lossy().to_string();
                
                if file_name == "walkthrough.md" {
                    let content = std::fs::read_to_string(&path)?;
                    let save = EpisodeSave::builder("Walkthrough".to_string(), content)
                        .scope(Some(scope.to_string()))
                        .vault_path(Some(rel_path_str))
                        .node_type(Some("walkthrough".to_string()))
                        .session_id(Some(conversation_id.to_string()))
                        .build();
                    db.save_episode(&save).await?;
                } else if file_name == "implementation_plan.md" {
                    let content = std::fs::read_to_string(&path)?;
                    let decisions_text = extract_decisions(&content);
                    let save = EpisodeSave::builder("Decisions from Implementation Plan".to_string(), decisions_text)
                        .scope(Some(scope.to_string()))
                        .vault_path(Some(rel_path_str))
                        .node_type(Some("decision".to_string()))
                        .session_id(Some(conversation_id.to_string()))
                        .build();
                    db.save_episode(&save).await?;
                } else if file_name == "task.md" {
                    let content = std::fs::read_to_string(&path)?;
                    let tasks_text = extract_completed_tasks(&content);
                    let save = EpisodeSave::builder("Completed Tasks Summary".to_string(), tasks_text)
                        .scope(Some(scope.to_string()))
                        .vault_path(Some(rel_path_str))
                        .node_type(Some("task_summary".to_string()))
                        .session_id(Some(conversation_id.to_string()))
                        .build();
                    db.save_episode(&save).await?;
                } else if (file_name.starts_with("analysis_") && file_name.ends_with(".md"))
                    || (file_name.ends_with("_critique.md")) {
                    let content = std::fs::read_to_string(&path)?;
                    let node = WikiNode {
                        id: None,
                        name: file_name.to_string(),
                        content,
                        scope: scope.to_string(),
                        vault_path: Some(rel_path_str),
                        embedding: None,
                        ..Default::default()
                    };
                    db.save_wiki_node(&node).await?;
                }
            }
        }
    }
    
    Ok(())
}

fn find_skill_mds(dir: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let skill_file = path.join("SKILL.md");
                if skill_file.exists() && skill_file.is_file() {
                    paths.push(skill_file);
                } else {
                    paths.extend(find_skill_mds(&path));
                }
            }
        }
    }
    paths
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a.sqrt() * norm_b.sqrt())
    }
}

pub async fn seed_wisdom_from_rules(
    db: &dyn StorageBackend,
    vault_root: &Path,
) -> Result<usize> {
    let mut candidate_files = Vec::new();
    
    // 1. Global config paths
    let home = std::env::var("HOME").unwrap_or_default();
    let global_config = std::path::PathBuf::from(&home).join(".gemini/config");
    if global_config.exists() {
        let gemini_glob = global_config.join("GEMINI.md");
        if gemini_glob.exists() { candidate_files.push(gemini_glob); }
        let agents_glob = global_config.join("AGENTS.md");
        if agents_glob.exists() { candidate_files.push(agents_glob); }
        
        let skills_dir = global_config.join("skills");
        if skills_dir.exists() {
            candidate_files.extend(find_skill_mds(&skills_dir));
        }
    }
    
    // 2. Local workspace paths
    let local_agents = vault_root.join(".agents");
    if local_agents.exists() {
        let gemini_local = local_agents.join("GEMINI.md");
        if gemini_local.exists() { candidate_files.push(gemini_local); }
        let agents_local = local_agents.join("AGENTS.md");
        if agents_local.exists() { candidate_files.push(agents_local); }
        
        let skills_local = local_agents.join("skills");
        if skills_local.exists() {
            candidate_files.extend(find_skill_mds(&skills_local));
        }
    }
    
    let direct_gemini = vault_root.join("GEMINI.md");
    if direct_gemini.exists() { candidate_files.push(direct_gemini); }
    let direct_agents = vault_root.join("AGENTS.md");
    if direct_agents.exists() { candidate_files.push(direct_agents); }
    
    let mut saved_count = 0;
    let client = LLMClient::default();
    
    // Get existing rules to check for duplicates
    let existing_rules = db.get_all_wisdom_rules().await.unwrap_or_default();
    
    for path in candidate_files {
        let content = std::fs::read_to_string(&path)?;
        
        let parsed_rules_json = if std::env::var("MYTHRAX_MOCK_LLM").unwrap_or_default() == "true" {
            r#"[{"target_pattern": "test_pattern", "action_to_avoid": "test_action", "causal_explanation": "test_causal", "prescribed_remedy": "test_remedy"}]"#.to_string()
        } else {
            let sys_prompt = "You are a wisdom seeding assistant that parses developer rules and skill definitions. Your task is to extract actionable guidelines as WisdomRule JSON objects.";
            let user_prompt = format!(
                "Parse the following rule/skill document and extract all actionable guidelines. For each guideline, create a JSON object with:
                - target_pattern: the context or trigger (e.g. 'Parallel Test Execution', 'Metal compiler JIT', 'LTM Search')
                - action_to_avoid: what NOT to do
                - causal_explanation: the reason/why
                - prescribed_remedy: what TO do
                - rule_type: 'procedural' or 'aesthetic'

                Document Content:
                {}
                
                Output JSON list only:
                [
                  {{
                    \"target_pattern\": \"...\",
                    \"action_to_avoid\": \"...\",
                    \"causal_explanation\": \"...\",
                    \"prescribed_remedy\": \"...\",
                    \"rule_type\": \"...\"
                  }}
                ]",
                content
            );
            let profile = crate::contracts::TaskProfile::new(crate::contracts::TaskArchetype::Extraction);
            
            let mut wisdom_rule_res = None;
            if std::env::var("MYTHRAX_BOOTSTRAPPING").is_err() {
                if let Some(surreal_backend) = db.as_any().downcast_ref::<SurrealBackend>() {
                    let task_id = format!("cognitive_task:{}", Uuid::new_v4());
                    let task = CognitiveTask {
                        id: task_id.clone(),
                        task_type: "WisdomCritique".to_string(),
                        prompt: user_prompt.clone(),
                        system_instruction: sys_prompt.to_string(),
                        expected_format: "Json".to_string(),
                        priority: "Normal".to_string(),
                        created_at: Utc::now(),
                        status: "Pending".to_string(),
                        result: None,
                        ttl_minutes: 10,
                        injected_at: None,
                    };
                    
                    if surreal_backend.create_cognitive_task(&task).await.is_ok() {
                        let start = std::time::Instant::now();
                        let timeout = std::time::Duration::from_secs(60);
                        while start.elapsed() < timeout {
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            if let Ok(Some(updated)) = surreal_backend.get_cognitive_task(&task_id).await {
                                if updated.status == "Completed" {
                                    if let Some(res) = updated.result {
                                        wisdom_rule_res = Some(res);
                                        break;
                                    }
                                }
                            }
                        }
                        if wisdom_rule_res.is_none() {
                            tracing::warn!("WisdomCritique cognitive callback timed out, falling back to LargeLocal");
                        }
                    }
                }
            }
            
            match wisdom_rule_res {
                Some(s) => s,
                None => client.routed_completion(db, &profile, Some(sys_prompt), &user_prompt).await?,
            }
        };
        
        #[derive(Deserialize, Default)]
        struct RawRule {
            target_pattern: String,
            action_to_avoid: String,
            causal_explanation: String,
            prescribed_remedy: String,
            rule_type: Option<String>,
        }
        
        let extracted: Vec<RawRule> = serde_json::from_str(&parsed_rules_json).unwrap_or_default();
        
        for er in extracted {
            let rule_text = format!(
                "Pattern: {}\nAvoid: {}\nRemedy: {}\nWhy: {}",
                er.target_pattern, er.action_to_avoid, er.prescribed_remedy, er.causal_explanation
            );
            
            let embedding = db.embed(&rule_text).await.unwrap_or_default();
            
            let mut is_duplicate = false;
            for existing in &existing_rules {
                if let Some(ref ext_emb) = existing.embedding {
                    let sim = cosine_similarity(&embedding, ext_emb);
                    if sim >= 0.85 {
                        is_duplicate = true;
                        break;
                    }
                }
            }
            
            if !is_duplicate {
                let rule = WisdomRule {
                    id: None,
                    target_pattern: er.target_pattern,
                    action_to_avoid: er.action_to_avoid,
                    causal_explanation: er.causal_explanation,
                    prescribed_remedy: er.prescribed_remedy,
                    tier: Tier::Wisdom,
                    scope: "general".to_string(),
                    vault_path: Some(path.to_string_lossy().to_string()),
                    embedding: Some(embedding),
                    source_episodes: vec![],
                    generator_name: "WisdomSeeder".to_string(),
                    similarity: None,
                    utility: Some(1.0),
                    status: Some("active".to_string()),
                    superseded_at: None,
                    superseded_by: None,
                    rule_type: er.rule_type,
                    severity: None,
                    blocking: None,
                    importance: None,
                };
                
                db.save_wisdom_rule(&rule).await?;
                saved_count += 1;
            }
        }
    }
    
    Ok(saved_count)
}
