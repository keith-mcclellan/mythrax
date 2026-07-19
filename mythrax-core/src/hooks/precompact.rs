use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::Arc;
use anyhow::{Context, Result};
use serde::Deserialize;

use crate::contracts::EpisodeSave;
use crate::db::backend::StorageBackend;
use crate::store::MarkdownStore;
use crate::vault::watcher::{save_episode_bidirectional, WatchIgnoreList};

#[derive(Deserialize)]
struct SimpleMessage {
    role: Option<String>,
    // content may be a flat string OR an array of content blocks (tool_result / text).
    content: Option<serde_json::Value>,
    message: Option<NestedMessage>,
}

#[derive(Deserialize)]
struct NestedMessage {
    role: Option<String>,
    content: Option<serde_json::Value>,
}

/// Extract verbatim text from a transcript message `content` field that may be EITHER
/// a flat string OR an array of content blocks (the array form used by real Claude/Codex
/// transcripts for `tool_result` and multi-block assistant/user turns). For block arrays
/// we concatenate the text of each block, including the raw payload of `tool_result`
/// blocks (recursing into nested string/array content), so tool output is captured
/// verbatim rather than silently dropped.
fn extract_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(blocks) => {
            let mut parts: Vec<String> = Vec::new();
            for block in blocks {
                // Plain string element.
                if let serde_json::Value::String(s) = block {
                    parts.push(s.clone());
                    continue;
                }
                // text block: { "type": "text", "text": "..." }
                if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                    parts.push(t.to_string());
                }
                // tool_result block: { "type": "tool_result", "content": <string|array> }
                if let Some(inner) = block.get("content") {
                    let inner_text = extract_text(inner);
                    if !inner_text.is_empty() {
                        parts.push(inner_text);
                    }
                }
                // tool_use / generic blocks carrying an "input"/"output" string payload.
                for key in ["output", "input"] {
                    if let Some(s) = block.get(key).and_then(|v| v.as_str()) {
                        parts.push(s.to_string());
                    }
                }
            }
            parts.join("\n")
        }
        serde_json::Value::Object(_) => {
            // A single content block object (not wrapped in an array).
            extract_text(&serde_json::Value::Array(vec![value.clone()]))
        }
        _ => String::new(),
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct BootstrapCheckpoint {
    session_id: String,
    last_processed_index: usize,
}

pub async fn mine_transcript(
    session: &str,
    transcript_path: &str,
    backend: &dyn StorageBackend,
    store: &MarkdownStore,
    ignore: &WatchIgnoreList,
) -> Result<usize> {
    use std::io::Seek;

    let checkpoint_path = store.vault_root.join(".mythrax/bootstrap_checkpoint.json");
    let mut last_processed_index = None;
    if checkpoint_path.exists() {
        if let Ok(c_content) = std::fs::read_to_string(&checkpoint_path) {
            if let Ok(cp) = serde_json::from_str::<BootstrapCheckpoint>(&c_content) {
                if cp.session_id == session {
                    last_processed_index = Some(cp.last_processed_index);
                }
            }
        }
    }

    let mut start_offset: u64 = 0;
    if let Ok(stm_map) = backend.get_stm(session, Some("transcript_offset")).await {
        if let Some(offset_str) = stm_map.get("transcript_offset") {
            if let Ok(parsed) = offset_str.parse::<u64>() {
                start_offset = parsed;
            }
        }
    }

    let mut file = File::open(transcript_path)
        .context(format!("Failed to open transcript file at {}", transcript_path))?;
    
    let metadata = file.metadata()?;
    if metadata.len() < start_offset {
        start_offset = 0;
    }

    if start_offset > 0 {
        file.seek(std::io::SeekFrom::Start(start_offset))?;
    }

    let mut reader = BufReader::new(file);
    let mut current_offset = start_offset;
    let mut saved_count = 0;
    let mut prev_saved_id: Option<String> = None;
    let mut tool_sequence = Vec::new();

    let all_eps = backend.get_all_episodes().await.unwrap_or_default();
    let mut has_previous_user_input = all_eps.iter().any(|ep| {
        ep.session_id.as_deref() == Some(session)
            && (ep.node_type.as_deref() == Some("user_input")
                || ep.node_type.as_deref() == Some("user_feedback"))
    });

    let mut buf = String::new();
    let mut line_idx = 0;
    loop {
        buf.clear();
        let bytes_read = match reader.read_line(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };

        let next_offset = current_offset + bytes_read as u64;
        let line_str = buf.trim_end_matches('\n').trim_end_matches('\r');

        if let Some(lpi) = last_processed_index {
            if line_idx <= lpi {
                line_idx += 1;
                current_offset = next_offset;
                continue;
            }
        }

        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line_str) {
            // Extract tool calls
            if let Some(tool_calls_arr) = val.get("tool_calls").and_then(|v| v.as_array()) {
                for tc in tool_calls_arr {
                    if let Some(name) = tc.get("name").and_then(|v| v.as_str()) {
                        tool_sequence.push(name.to_string());
                    }
                }
            }

            // Check for checklist items in the content (WU-3.3)
            let content_opt = val.get("content").and_then(|c| c.as_str())
                .or_else(|| val.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()));
            if let Some(content_str) = content_opt {
                let mut checklist_lines = Vec::new();
                for line in content_str.lines() {
                    if line.contains("- [ ]") || line.contains("- [x]") {
                        checklist_lines.push(line.trim().to_string());
                    }
                }
                if !checklist_lines.is_empty() {
                    let checklist_str = checklist_lines.join("\n");
                    let _ = backend.save_stm(session, "checklist", &checklist_str).await;
                    
                    let ep = EpisodeSave::builder("Active Task Checklist".to_string(), checklist_str)
                        .scope(Some("general".to_string()))
                        .session_id(Some(session.to_string()))
                        .node_type(Some("task_checklist".to_string()))
                        .build();
                    let store_arc = Arc::new(crate::store::MarkdownStore {
                        vault_root: store.vault_root.clone(),
                    });
                    if let Ok(saved_id) = save_episode_bidirectional(&ep, backend, &store_arc, ignore).await {
                        if let Some(ref prev_id) = prev_saved_id {
                            let _ = backend.relate_followed_by(prev_id, &saved_id).await;
                        }
                        prev_saved_id = Some(saved_id);
                        saved_count += 1;
                    }
                }
            }
        }

        if let Ok(msg) = serde_json::from_str::<SimpleMessage>(&line_str) {
            let role = msg.role.clone().or_else(|| msg.message.as_ref().and_then(|m| m.role.clone()));
            let content = msg.content.clone().or_else(|| msg.message.as_ref().and_then(|m| m.content.clone()));

            if let (Some(r), Some(c)) = (role, content) {
                let normalized_role = r.to_lowercase();
                if normalized_role == "user"
                    || normalized_role == "assistant"
                    || normalized_role == "tool"
                    || normalized_role == "tool_result"
                    || normalized_role == "computer"
                    || normalized_role == "system"
                {
                    let extracted = extract_text(&c);
                    if extracted.trim().is_empty() {
                        current_offset = next_offset;
                        continue;
                    }
                    if normalized_role == "assistant" && extracted.trim().chars().count() <= 20 {
                        current_offset = next_offset;
                        continue;
                    }
                    let type_val = match normalized_role.as_str() {
                        "user" => {
                            if !has_previous_user_input {
                                has_previous_user_input = true;
                                "user_input".to_string()
                            } else {
                                "user_feedback".to_string()
                            }
                        }
                        "assistant" => "agent_thought".to_string(),
                        "tool" | "tool_result" | "computer" => "tool_execution".to_string(),
                        "system" => "system_log".to_string(),
                        _ => "agent_thought".to_string(),
                    };
                    let title = format!("Verbatim {} Turn ({})", r, session);
                    let ep = EpisodeSave::builder(title, extracted)
                        .scope(Some("general".to_string()))
                        .session_id(Some(session.to_string()))
                        .node_type(Some(type_val))
                        .build();    
                    let store_arc = Arc::new(crate::store::MarkdownStore {
                        vault_root: store.vault_root.clone(),
                    });
                    let saved_id = save_episode_bidirectional(&ep, backend, &store_arc, ignore)
                        .await
                        .context("Failed to save episode bidirectionally during transcript mining")?;
                    
                    if let Some(ref prev_id) = prev_saved_id {
                        if let Err(e) = backend.relate_followed_by(prev_id, &saved_id).await {
                            tracing::warn!("Failed to link mined sequential episodes: {:?}", e);
                        }
                    }
                    prev_saved_id = Some(saved_id);
                    saved_count += 1;
                }
            }
        }
        // Save checkpoint progress
        let checkpoint = BootstrapCheckpoint {
            session_id: session.to_string(),
            last_processed_index: line_idx,
        };
        let checkpoint_dir = store.vault_root.join(".mythrax");
        let _ = std::fs::create_dir_all(&checkpoint_dir);
        if let Ok(json_str) = serde_json::to_string(&checkpoint) {
            let _ = std::fs::write(checkpoint_dir.join("bootstrap_checkpoint.json"), json_str);
        }

        line_idx += 1;
        current_offset = next_offset;
    }

    let _ = backend.save_stm(session, "transcript_offset", &current_offset.to_string()).await;

    // Run n-gram analysis on the extracted tool sequence
    let _ = analyze_tool_calls_ngrams(backend, &tool_sequence).await;

    // 2.0 dual-durability journaling
    backend.journal_state(&store.vault_root, Some(session))
        .await
        .context("Failed to journal state after transcript mining")?;

    Ok(saved_count)
}

async fn analyze_tool_calls_ngrams(backend: &dyn StorageBackend, tool_sequence: &[String]) -> Result<()> {
    if tool_sequence.len() < 2 {
        return Ok(());
    }

    let mut counts: std::collections::HashMap<Vec<String>, usize> = std::collections::HashMap::new();

    for len in 2..=4 {
        if tool_sequence.len() < len {
            continue;
        }
        for i in 0..=(tool_sequence.len() - len) {
            let ngram = tool_sequence[i..(i + len)].to_vec();
            *counts.entry(ngram).or_insert(0) += 1;
        }
    }

    for (ngram, count) in counts {
        if count >= 2 {
            let chain_str = ngram.join(" -> ");
            let uuid = uuid::Uuid::new_v4().to_string();
            let rule = crate::contracts::WisdomRule {
                id: Some(format!("wisdom:{}", uuid)),
                target_pattern: format!("Tool sequence: {}", chain_str),
                action_to_avoid: format!("Avoid manually repeating this tool sequence: {}", chain_str),
                causal_explanation: format!("This tool chain was used {} times in the session transcript.", count),
                prescribed_remedy: format!("Automate this sequence using a combined batch route or helper tool: {}", chain_str),
                tier: crate::contracts::Tier::Project,
                scope: "general".to_string(),
                vault_path: None,
                embedding: None,
                source_episodes: vec![],
                generator_name: "TranscriptNgramMiner".to_string(),
                similarity: Some(1.0),
                utility: Some(1.0),
                status: Some("active".to_string()),
                superseded_at: None,
                superseded_by: None,
                rule_type: Some("frequent_tool_chain".to_string()),
                severity: Some("info".to_string()),
                blocking: Some(false),
                importance: Some(5.0),
            };

            let _ = backend.save_wisdom_rule(&rule).await;
        }
    }

    Ok(())
}
