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
    content: Option<String>,
    message: Option<NestedMessage>,
}

#[derive(Deserialize)]
struct NestedMessage {
    role: Option<String>,
    content: Option<String>,
}

pub async fn mine_transcript(
    session: &str,
    transcript_path: &str,
    backend: &Arc<dyn StorageBackend>,
    store: &Arc<MarkdownStore>,
    ignore: &WatchIgnoreList,
) -> Result<usize> {
    let file = File::open(transcript_path)
        .context(format!("Failed to open transcript file at {}", transcript_path))?;
    let reader = BufReader::new(file);
    let mut saved_count = 0;

    for line in reader.lines() {
        let line_str = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        if let Ok(msg) = serde_json::from_str::<SimpleMessage>(&line_str) {
            let role = msg.role.clone().or_else(|| msg.message.as_ref().and_then(|m| m.role.clone()));
            let content = msg.content.clone().or_else(|| msg.message.as_ref().and_then(|m| m.content.clone()));

            if let (Some(r), Some(c)) = (role, content) {
                // We mine user turns, tool outputs, computer outputs, and tool_results
                let normalized_role = r.to_lowercase();
                if normalized_role == "user" 
                    || normalized_role == "tool" 
                    || normalized_role == "tool_result" 
                    || normalized_role == "computer" 
                {
                    let title = format!("Verbatim {} Turn ({})", r, session);
                    let ep = EpisodeSave {
                        title,
                        content: c,
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
};

                    save_episode_bidirectional(&ep, backend, store, ignore)
                        .await
                        .context("Failed to save episode bidirectionally during transcript mining")?;
                    
                    saved_count += 1;
                }
            }
        }
    }

    // 2.0 dual-durability journaling
    backend.journal_state(&store.vault_root, Some(session))
        .await
        .context("Failed to journal state after transcript mining")?;

    Ok(saved_count)
}
