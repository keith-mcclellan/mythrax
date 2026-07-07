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

pub async fn mine_transcript(
    session: &str,
    transcript_path: &str,
    backend: &dyn StorageBackend,
    store: &MarkdownStore,
    ignore: &WatchIgnoreList,
) -> Result<usize> {
    use std::io::Seek;

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

    let all_eps = backend.get_all_episodes().await.unwrap_or_default();
    let mut has_previous_user_input = all_eps.iter().any(|ep| {
        ep.session_id.as_deref() == Some(session)
            && (ep.node_type.as_deref() == Some("user_input")
                || ep.node_type.as_deref() == Some("user_feedback"))
    });

    let mut buf = String::new();
    loop {
        buf.clear();
        let bytes_read = match reader.read_line(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };

        let next_offset = current_offset + bytes_read as u64;
        let line_str = buf.trim_end_matches('\n').trim_end_matches('\r');

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
                    let ep = EpisodeSave {
                        title,
                        content: extracted,
                        entities: vec![],
                        scope: Some("general".to_string()),
                        vault_path: None,
                        source_episode: None,
                        session_id: Some(session.to_string()),
                        task_id: None,
                        discovery_tokens: None,
                        facts: None,
                        concepts: None,
                        files_read: None,
                        files_modified: None,
                        node_type: Some(type_val),
                        confidence: None,
                    };    
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
        current_offset = next_offset;
    }

    let _ = backend.save_stm(session, "transcript_offset", &current_offset.to_string()).await;

    // 2.0 dual-durability journaling
    backend.journal_state(&store.vault_root, Some(session))
        .await
        .context("Failed to journal state after transcript mining")?;

    Ok(saved_count)
}
