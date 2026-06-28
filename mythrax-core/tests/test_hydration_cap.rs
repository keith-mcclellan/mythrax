use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use serde_json::json;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::store::MarkdownStore;
use mythrax_core::api::ApiState;
use mythrax_core::mcp_routes::call_mcp_tool;
use mythrax_core::contracts::EpisodeSave;

#[tokio::test]
async fn test_get_full_hydration_cap() -> Result<()> {
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("episodes"))?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let store = std::sync::Arc::new(MarkdownStore::new(&vault_root)?);

    let state = ApiState {
        backend: backend.clone(),
        auth_token: "secret".to_string(),
        store: store.clone(),
        ignore_list: std::sync::Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new()),
        dream_tx: None,
    };

    // Create an episode with very large content (> 10000 characters)
    let large_content = "A".repeat(12000);
    let ep_save = EpisodeSave {
        title: "Very Large Episode".to_string(),
        content: large_content.clone(),
        scope: Some("general".to_string()),
        vault_path: Some("episodes/large_ep.md".to_string()),
        ..Default::default()
    };
    let ep_id = backend.save_episode(&ep_save).await?;

    // Call get_full tool via consolidated read tool
    let args = json!({
        "action": "get_full",
        "ids": [ep_id]
    });

    let resp = call_mcp_tool(&state, "read", args).await?;
    
    // Parse the output content:
    let content_arr = resp.get("content").unwrap().as_array().unwrap();
    let text = content_arr[0].get("text").unwrap().as_str().unwrap();
    
    let search_results: Vec<mythrax_core::contracts::SearchResult> = serde_json::from_str(text)?;
    let result = &search_results[0];
    
    // It should have truncated content and the truncation marker
    assert!(result.content.len() < 12000);
    assert!(result.content.contains("truncated 2000 chars"));

    Ok(())
}
