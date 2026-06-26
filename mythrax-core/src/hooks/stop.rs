use std::sync::Arc;
use anyhow::{Context, Result};

use crate::db::backend::StorageBackend;
use crate::store::MarkdownStore;
use crate::vault::watcher::WatchIgnoreList;
use crate::hooks::shell::count_human_messages;

pub fn should_save(prev: usize, new: usize) -> bool {
    if new == 0 {
        return false;
    }
    let prev_intervals = prev / 15;
    let new_intervals = new / 15;
    new_intervals > prev_intervals
}

pub async fn mine_if_due(
    session: &str,
    transcript_path: &str,
    stop_hook_active: bool,
    backend: &Arc<dyn StorageBackend>,
    store: &Arc<MarkdownStore>,
    ignore: &WatchIgnoreList,
) -> Result<Option<usize>> {
    if stop_hook_active {
        return Ok(None); // no-op if stop hook is active (prevents re-entry)
    }

    // 1. Get the current count of human messages from the transcript file
    let new_count = count_human_messages(transcript_path);

    // 2. Retrieve the last saved human message count from the persistent STM
    let stm_data = backend.get_stm(session, Some("last_human_message_count"))
        .await
        .unwrap_or_default();
    
    let prev_count = stm_data.get("last_human_message_count")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);

    // 3. Check if we crossed a 15-message interval boundary
    if should_save(prev_count, new_count) {
        // 4. Run the transcript mining to capture the raw turns verbatim
        let count = crate::hooks::precompact::mine_transcript(session, transcript_path, backend, store, ignore)
            .await
            .context("Failed to mine transcript in stop hook")?;

        // 5. Update the persistent state in STM with the new count
        backend.save_stm(session, "last_human_message_count", &new_count.to_string())
            .await
            .context("Failed to save human message count in STM")?;

        Ok(Some(count))
    } else {
        Ok(None)
    }
}
