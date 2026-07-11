use std::sync::Mutex;
use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use serde_json::json;
use mythrax_core::db::{SurrealBackend, StorageBackend, backend::parse_record_id};
use mythrax_core::store::MarkdownStore;
use mythrax_core::api::ApiState;
use mythrax_core::mcp_routes::call_mcp_tool;
use mythrax_core::contracts::{EpisodeSave, IndexRow, SearchResult};

static TEST_MUTEX: Mutex<()> = Mutex::new(());

async fn setup_test_state() -> Result<(ApiState, std::sync::Arc<SurrealBackend>, tempfile::TempDir)> {
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;
    fs::create_dir_all(vault_root.join("episodes"))?;

    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;
    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.db.query(mythrax_core::db::schema::INIT_SCHEMA).await?.check()?;
    backend.init().await?;
    
    let store = std::sync::Arc::new(MarkdownStore::new(&vault_root)?);

    let state = ApiState {
        backend: backend.clone(),
        auth_token: "secret".to_string(),
        store,
        ignore_list: std::sync::Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new()),
        dream_tx: None,
    };

    Ok((state, backend, tmp))
}

#[tokio::test]
async fn test_search_index_omits_content() -> Result<()> {
    let _guard = match TEST_MUTEX.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };

    let (state, backend, _tmp) = setup_test_state().await?;

    // Save a few test episodes
    for i in 1..=3 {
        let ep = EpisodeSave {
        created_at: None,
            title: format!("Episode Title {}", i),
            content: format!("This is the long content for episode number {}. It contains details that should not be returned in a compact index query.", i),
            entities: vec![],
            scope: Some("general".to_string()),
            vault_path: Some(format!("notes/ep_{}.md", i)),
            source_episode: None,
            session_id: None,
            task_id: None,
            discovery_tokens: None,
            facts: None,
            concepts: None,
            files_read: None,
            files_modified: None,
            node_type: None,
        
            confidence: None,};
        backend.save_episode(&ep).await?;
    }

    // Call search_index
    let mcp_res = call_mcp_tool(&state, "read", json!({
        "action": "search_index",
        "query": "Episode",
        "limit": 10
    })).await?;

    // Extract the text content from the MCP response
    let content_arr = mcp_res.get("content").and_then(|v| v.as_array()).unwrap();
    let text = content_arr[0].get("text").and_then(|v| v.as_str()).unwrap();
    
    let index_rows: Vec<IndexRow> = serde_json::from_str(text)?;
    assert!(!index_rows.is_empty());

    for row in &index_rows {
        assert!(!row.id.is_empty());
        assert!(!row.title.is_empty());
        assert!(!row.subtitle.is_empty());
        // Verify subtitle is a truncated version of content (max 120 chars) and does not contain the full body
        assert!(row.subtitle.len() <= 123); // 120 + "..."
        
        // Convert to a raw JSON value to verify that content/embedding fields are absent
        let val = serde_json::to_value(row)?;
        assert!(val.get("content").is_none());
        assert!(val.get("embedding").is_none());
    }

    Ok(())
}

#[tokio::test]
async fn test_get_full_hydrates() -> Result<()> {
    let _guard = match TEST_MUTEX.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };

    let (state, backend, _tmp) = setup_test_state().await?;

    let ep = EpisodeSave {
        created_at: None,
        title: "Unique Hydration Test".to_string(),
        content: "Detailed secret documentation that must be hydrated.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/hydration.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,
    
        confidence: None,};
    let ep_id = backend.save_episode(&ep).await?;

    // Call search_index first to get the ID
    let mcp_res = call_mcp_tool(&state, "read", json!({
        "action": "search_index",
        "query": "Hydration",
        "limit": 1
    })).await?;

    let text = mcp_res["content"][0]["text"].as_str().unwrap();
    let index_rows: Vec<IndexRow> = serde_json::from_str(text)?;
    assert_eq!(index_rows.len(), 1);
    assert_eq!(index_rows[0].id, ep_id);

    // Call get_full to hydrate it
    let full_res = call_mcp_tool(&state, "read", json!({
        "action": "get_full",
        "ids": [ep_id.clone()]
    })).await?;

    let full_text = full_res["content"][0]["text"].as_str().unwrap();
    let results: Vec<SearchResult> = serde_json::from_str(full_text)?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, ep_id);
    assert_eq!(results[0].content, "Detailed secret documentation that must be hydrated.");

    Ok(())
}

#[tokio::test]
async fn test_timeline_orders_neighbors() -> Result<()> {
    let _guard = match TEST_MUTEX.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };

    let (state, backend, _tmp) = setup_test_state().await?;

    // Save 5 episodes and update their created_at strictly sequentially
    let mut ids = Vec::new();
    for i in 1..=5 {
        let ep = EpisodeSave {
        created_at: None,
            title: format!("Timeline Episode {}", i),
            content: format!("Content for sequential timeline episode {}", i),
            entities: vec![],
            scope: Some("general".to_string()),
            vault_path: Some(format!("notes/time_{}.md", i)),
            source_episode: None,
            session_id: None,
            task_id: None,
            discovery_tokens: None,
            facts: None,
            concepts: None,
            files_read: None,
            files_modified: None,
            node_type: None,
        
            confidence: None,};
        let id = backend.save_episode(&ep).await?;
        ids.push(id);
    }

    // Assign sequential created_at timestamps
    let base_time = chrono::Utc::now();
    for (idx, id) in ids.iter().enumerate() {
        let time = base_time + chrono::Duration::seconds(idx as i64 * 10);
        let record = parse_record_id(id)?;
        backend.db.query("UPDATE $id SET created_at = $time;")
            .bind(("id", record))
            .bind(("time", time))
            .await?
            .check()?;
    }

    // Call timeline centered on the 3rd episode (index 2) with depth_before=1, depth_after=1
    let mid_id = &ids[2];
    let mcp_res = call_mcp_tool(&state, "read", json!({
        "action": "timeline",
        "anchor_id": mid_id,
        "depth_before": 1,
        "depth_after": 1
    })).await?;

    let text = mcp_res["content"][0]["text"].as_str().unwrap();
    let index_rows: Vec<IndexRow> = serde_json::from_str(text)?;
    
    // Should return exactly the 2nd and 4th episodes, chronologically ordered
    assert_eq!(index_rows.len(), 2);
    assert_eq!(index_rows[0].id, ids[1]); // Episode 2 (prior)
    assert_eq!(index_rows[1].id, ids[3]); // Episode 4 (subsequent)

    // Test with query anchor search
    let mcp_res_query = call_mcp_tool(&state, "read", json!({
        "action": "timeline",
        "query": "Timeline Episode 3",
        "depth_before": 1,
        "depth_after": 1
    })).await?;

    let text_query = mcp_res_query["content"][0]["text"].as_str().unwrap();
    let index_rows_query: Vec<IndexRow> = serde_json::from_str(text_query)?;
    assert_eq!(index_rows_query.len(), 2);
    assert_eq!(index_rows_query[0].id, ids[1]);
    assert_eq!(index_rows_query[1].id, ids[3]);

    Ok(())
}

#[tokio::test]
async fn test_index_then_full_token_savings() -> Result<()> {
    let _guard = match TEST_MUTEX.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };

    let (state, backend, _tmp) = setup_test_state().await?;

    // Create 10 episodes with very long content (e.g. lots of repeated text)
    let long_text = "This is a very long text block. ".repeat(30); // ~180 words/1000 characters
    let mut ids = Vec::new();
    for i in 1..=10 {
        let ep = EpisodeSave {
        created_at: None,
            title: format!("Big Episode Title {}", i),
            content: format!("Start of episode {}. {}", i, long_text),
            entities: vec![],
            scope: Some("general".to_string()),
            vault_path: Some(format!("notes/big_{}.md", i)),
            source_episode: None,
            session_id: None,
            task_id: None,
            discovery_tokens: None,
            facts: None,
            concepts: None,
            files_read: None,
            files_modified: None,
            node_type: None,
        
            confidence: None,};
        let id = backend.save_episode(&ep).await?;
        ids.push(id);
    }

    // 1. Full search (hydrates all matching episodes)
    let full_search_res = call_mcp_tool(&state, "read", json!({
        "action": "search",
        "query": "Big Episode",
        "include_episodes": true,
        "limit": 10
    })).await?;
    let full_search_text = full_search_res["content"][0]["text"].as_str().unwrap();
    let full_search_size = full_search_text.len();

    // 2. Progressive disclosure: search_index + get_full for only 2 episodes
    let index_res = call_mcp_tool(&state, "read", json!({
        "action": "search_index",
        "query": "Big Episode",
        "limit": 10
    })).await?;
    let index_text = index_res["content"][0]["text"].as_str().unwrap();
    let index_size = index_text.len();

    let get_full_res = call_mcp_tool(&state, "read", json!({
        "action": "get_full",
        "ids": [ids[0].clone(), ids[1].clone()]
    })).await?;
    let get_full_text = get_full_res["content"][0]["text"].as_str().unwrap();
    let get_full_size = get_full_text.len();

    let progressive_total_size = index_size + get_full_size;

    println!("Full Search Size: {} bytes", full_search_size);
    println!("Index Search Size: {} bytes", index_size);
    println!("Get Full (2 nodes) Size: {} bytes", get_full_size);
    println!("Progressive Total Size: {} bytes", progressive_total_size);

    // Progressive disclosure should be significantly smaller than full search returning all 10 hydrated
    assert!(progressive_total_size < full_search_size);

    Ok(())
}
