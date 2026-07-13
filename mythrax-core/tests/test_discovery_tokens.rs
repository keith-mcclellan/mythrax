use mythrax_core::mcp_routes::{handle_pre_invocation_hook, CHARS_PER_TOKEN};
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::api::ApiState;
use mythrax_core::contracts::EpisodeSave;
use tempfile::tempdir;
use std::sync::Arc;

#[tokio::test]
async fn test_episode_save_roundtrips_discovery_tokens() {
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    // 1. Check with Some value
    let ep_some = EpisodeSave {
        created_at: None,
        title: "Some Discovery".to_string(),
        content: "Test content".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/some_discovery.md".to_string()),
        source_episode: None,
        session_id: Some("session-1".to_string()),
        task_id: None,
        discovery_tokens: Some(1234),
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,
    
        confidence: None,
        ..Default::default()
    };
    let id_some = backend.save_episode(&ep_some).await.unwrap();

    // 2. Check with None value
    let ep_none = EpisodeSave {
        created_at: None,
        title: "None Discovery".to_string(),
        content: "Test content 2".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/none_discovery.md".to_string()),
        source_episode: None,
        session_id: Some("session-1".to_string()),
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,
    
        confidence: None,
        ..Default::default()
    };
    let id_none = backend.save_episode(&ep_none).await.unwrap();

    let all = backend.get_all_episodes().await.unwrap();
    
    let some_retrieved = all.iter().find(|e| e.id.as_ref().unwrap() == &id_some).unwrap();
    assert_eq!(some_retrieved.discovery_tokens, Some(1234));

    let none_retrieved = all.iter().find(|e| e.id.as_ref().unwrap() == &id_none).unwrap();
    assert_eq!(none_retrieved.discovery_tokens, None);
}

#[test]
fn test_read_token_estimate_matches_formula() {
    // observation_tokens = ceil((title.len() + content.len()) / CHARS_PER_TOKEN)
    // with CHARS_PER_TOKEN = 4
    assert_eq!(CHARS_PER_TOKEN, 4);

    let title = "Hello"; // len 5
    let content = "World!"; // len 6
    // total len = 11. ceil(11/4) = 3 tokens.
    
    let calc_tokens = |t: &str, c: &str| -> u32 {
        let len = t.len() + c.len();
        ((len + CHARS_PER_TOKEN - 1) / CHARS_PER_TOKEN) as u32
    };

    assert_eq!(calc_tokens(title, content), 3);
}

#[tokio::test]
async fn test_token_economics_savings() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    // Episode 1: title (9) + content (391) = 400 chars. read_tokens = 100. discovery_tokens = Some(1000).
    let ep1 = EpisodeSave {
        created_at: None,
        title: "Episode 1".to_string(),
        content: "a".repeat(391),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/ep1.md".to_string()),
        source_episode: None,
        session_id: Some("test_session".to_string()),
        task_id: None,
        discovery_tokens: Some(1000),
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,
    
        confidence: None,
        ..Default::default()
    };
    let id1 = backend.save_episode(&ep1).await.unwrap();

    // Episode 2: title (9) + content (391) = 400 chars. read_tokens = 100. discovery_tokens = Some(500).
    let ep2 = EpisodeSave {
        created_at: None,
        title: "Episode 2".to_string(),
        content: "b".repeat(391),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/ep2.md".to_string()),
        source_episode: None,
        session_id: Some("test_session".to_string()),
        task_id: None,
        discovery_tokens: Some(500),
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,
    
        confidence: None,
        ..Default::default()
    };
    let id2 = backend.save_episode(&ep2).await.unwrap();

    // Episode 3: has None/zero discovery tokens (will not be in distilled_context_nodes, so not hydrated)
    let ep3 = EpisodeSave {
        created_at: None,
        title: "Episode 3".to_string(),
        content: "c".repeat(391),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/ep3.md".to_string()),
        source_episode: None,
        session_id: Some("test_session".to_string()),
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,
    
        confidence: None,
        ..Default::default()
    };
    let _id3 = backend.save_episode(&ep3).await.unwrap();

    // Put distilled_context_nodes in STM to hydrate exactly ep1 and ep2 (sum of read tokens = 200, discovery = 1500)
    let node_ids = vec![id1, id2];
    let node_ids_json = serde_json::to_string(&node_ids).unwrap();
    backend.save_stm("test_session", "distilled_context_nodes", &node_ids_json).await.unwrap();

    // Insert a pending handoff so that subagent path is triggered in handle_pre_invocation_hook
    backend.db.query("INSERT INTO handoff {
        parent_conversation_id: 'parent_123',
        subagent_conversation_id: 'test_session',
        summary: 'handoff summary',
        handoff_file_path: 'handoff.md',
        scope: 'general',
        status: 'PENDING',
        created_at: time::now()
    };").await.unwrap();

    let state = ApiState {
        backend: Arc::new(backend),
        auth_token: "secret".to_string(),
        store: Arc::new(mythrax_core::store::MarkdownStore::new(temp_dir.path().to_path_buf()).unwrap()),
        ignore_list: Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new()),
        dream_tx: None,
        shutdown_tx: None,
    };

    let payload = serde_json::json!({
        "session_id": "test_session",
        "workspace_path": temp_dir.path().to_string_lossy()
    });

    let result = handle_pre_invocation_hook(&state, payload).await.unwrap();
    
    let econ = &result["token_economics"];
    assert_eq!(econ["total_read"].as_u64().unwrap(), 200);
    assert_eq!(econ["total_discovery"].as_u64().unwrap(), 1500);
    assert_eq!(econ["savings"].as_i64().unwrap(), 1300);
    assert_eq!(econ["savings_percent"].as_u64().unwrap(), 87);
}

#[tokio::test]
async fn test_zero_discovery_no_divide_by_zero() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    // Episode with Some(0) discovery tokens
    let ep = EpisodeSave {
        created_at: None,
        title: "Episode 1".to_string(),
        content: "Test".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/ep1.md".to_string()),
        source_episode: None,
        session_id: Some("test_session_zero".to_string()),
        task_id: None,
        discovery_tokens: Some(0),
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,
    
        confidence: None,
        ..Default::default()
    };
    let id = backend.save_episode(&ep).await.unwrap();

    let node_ids = vec![id];
    let node_ids_json = serde_json::to_string(&node_ids).unwrap();
    backend.save_stm("test_session_zero", "distilled_context_nodes", &node_ids_json).await.unwrap();

    backend.db.query("INSERT INTO handoff {
        parent_conversation_id: 'parent_123',
        subagent_conversation_id: 'test_session_zero',
        summary: 'handoff summary',
        handoff_file_path: 'handoff.md',
        scope: 'general',
        status: 'PENDING',
        created_at: time::now()
    };").await.unwrap();

    let state = ApiState {
        backend: Arc::new(backend),
        auth_token: "secret".to_string(),
        store: Arc::new(mythrax_core::store::MarkdownStore::new(temp_dir.path().to_path_buf()).unwrap()),
        ignore_list: Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new()),
        dream_tx: None,
        shutdown_tx: None,
    };

    let payload = serde_json::json!({
        "session_id": "test_session_zero",
        "workspace_path": temp_dir.path().to_string_lossy()
    });

    let result = handle_pre_invocation_hook(&state, payload).await.unwrap();
    
    let econ = &result["token_economics"];
    assert_eq!(econ["total_discovery"].as_u64().unwrap(), 0);
    assert_eq!(econ["savings_percent"].as_u64().unwrap(), 0);
}
