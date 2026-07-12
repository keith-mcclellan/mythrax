// Sprint 3 TDD Test Suite: Behavioral Enforcement Hooks + Vault Clean

use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use tempfile::tempdir;
use chrono::Utc;

use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::api::ApiState;
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::WatchIgnoreList;
use mythrax_core::contracts::{WisdomRule, EpisodeSave, Tier};
use mythrax_core::mcp_routes::handle_pre_invocation_hook;
use mythrax_core::mcp_routes::manage_handlers::handle_manage_stm;
use mythrax_core::mcp_routes::vault_handlers::handle_manage_vault;

#[tokio::test]
async fn test_post_turn_observer_and_guardrails() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().join("db");
    let backend = SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy())).await?;
    backend.init().await?;

    let store = Arc::new(MarkdownStore::new(temp_dir.path())?);
    let ignore_list = Arc::new(WatchIgnoreList::new());

    let state = ApiState {
        backend: Arc::new(backend),
        auth_token: "test".to_string(),
        store,
        ignore_list,
        dream_tx: None,
        shutdown_tx: None,
    };

    // 1. Insert a wisdom rule that is blocking
    let rule = WisdomRule {
        id: Some("wisdom:delete".to_string()),
        target_pattern: "delete".to_string(),
        action_to_avoid: "deleting files directly".to_string(),
        causal_explanation: "leads to data loss".to_string(),
        prescribed_remedy: "use trash library".to_string(),
        tier: Tier::Wisdom,
        scope: "general".to_string(),
        vault_path: None,
        embedding: None,
        source_episodes: vec![],
        generator_name: "test".to_string(),
        similarity: None,
        utility: Some(50.0),
        status: Some("active".to_string()),
        superseded_at: None,
        superseded_by: None,
        rule_type: Some("procedural".to_string()),
        severity: Some("CAUTION".to_string()),
        blocking: Some(true),
        ..Default::default()
    };
    state.backend.save_wisdom_rule(&rule).await?;

    // 2. Set distilled_context_nodes and _transcript_path
    let session_id = "sess_obs_test";
    let transcript_path = temp_dir.path().join("transcript.jsonl");
    state.backend.save_stm(session_id, "_transcript_path", &transcript_path.to_string_lossy()).await?;
    state.backend.save_stm(session_id, "distilled_context_nodes", r#"["wisdom:delete"]"#).await?;

    // Write a mock transcript turn where agent says "I will delete files"
    let mut file = File::create(&transcript_path)?;
    writeln!(file, r#"{{"step_index": 1, "source": "MODEL", "type": "PLANNER_RESPONSE", "content": "I will delete files", "tool_calls": []}}"#)?;
    drop(file);

    // 3. Run pre-invocation
    let payload = serde_json::json!({
        "session_id": session_id,
        "workspace_path": temp_dir.path().to_string_lossy()
    });
    let result = handle_pre_invocation_hook(&state, payload).await?;
    let text = result["content"][0]["text"].as_str().unwrap();

    // Verify blocking acknowledge directive is prepended
    assert!(text.contains("CRITICAL RULE ACKNOWLEDGEMENT REQUIRED"), "Should contain acknowledge directive: {}", text);
    assert!(text.contains("CAUTION"), "Should format CAUTION severity: {}", text);
    assert!(text.contains("deleting files directly"), "Should contain avoid description: {}", text);

    // Verify memory utilization is scored
    // Because we mentioned 'delete' (which matches target_pattern of wisdom:delete), memory utilization should be 100% (1/1)
    let final_stm = state.backend.get_stm(session_id, None).await?;
    assert_eq!(final_stm.get("_last_memory_utilization"), Some(&"100".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_auto_task_persistence() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().join("db");
    let backend = SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy())).await?;
    backend.init().await?;

    let store = Arc::new(MarkdownStore::new(temp_dir.path())?);
    let ignore_list = Arc::new(WatchIgnoreList::new());

    let state = ApiState {
        backend: Arc::new(backend),
        auth_token: "test".to_string(),
        store,
        ignore_list,
        dream_tx: None,
        shutdown_tx: None,
    };

    let session_id = "sess_task_test";
    let transcript_path = temp_dir.path().join("transcript.jsonl");
    state.backend.save_stm(session_id, "_transcript_path", &transcript_path.to_string_lossy()).await?;

    // Write a transcript Turn N with checklist items
    let mut file = File::create(&transcript_path)?;
    writeln!(file, r#"{{"step_index": 1, "source": "MODEL", "type": "PLANNER_RESPONSE", "content": "I need to complete these tasks:\n- [ ] Fix the memory leak\n- [ ] Add unit tests", "tool_calls": []}}"#)?;
    drop(file);

    // Run precompact (this triggers transcript mining)
    let count = mythrax_core::hooks::precompact::mine_transcript(
        session_id,
        &transcript_path.to_string_lossy(),
        state.backend.as_ref(),
        &state.store,
        &state.ignore_list
    ).await?;
    assert!(count > 0);

    // Assert that a task checklist episode is saved in DB
    let eps = state.backend.get_all_episodes().await?;
    let checklist_ep = eps.iter().find(|ep| ep.node_type.as_deref() == Some("task_checklist"));
    assert!(checklist_ep.is_some(), "Checklist episode should be created");
    let content = &checklist_ep.unwrap().content;
    assert!(content.contains("- [ ] Fix the memory leak"));
    assert!(content.contains("- [ ] Add unit tests"));

    // Check STM key
    let stm_map = state.backend.get_stm(session_id, Some("checklist")).await?;
    assert!(stm_map.contains_key("checklist"));
    assert!(stm_map.get("checklist").unwrap().contains("- [ ] Fix the memory leak"));

    Ok(())
}

#[tokio::test]
async fn test_memory_query_frequency_tracker() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().join("db");
    let backend = SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy())).await?;
    backend.init().await?;

    let store = Arc::new(MarkdownStore::new(temp_dir.path())?);
    let ignore_list = Arc::new(WatchIgnoreList::new());

    let state = ApiState {
        backend: Arc::new(backend),
        auth_token: "test".to_string(),
        store,
        ignore_list,
        dream_tx: None,
        shutdown_tx: None,
    };

    let session_id = "sess_freq_test";

    // 1. Run pre-invocation when no search has been performed
    let payload = serde_json::json!({
        "session_id": session_id,
        "workspace_path": temp_dir.path().to_string_lossy()
    });
    let result = handle_pre_invocation_hook(&state, payload.clone()).await?;
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Warning: Memory searches are stale. No search has been performed"), "Should warn when no search: {}", text);

    // 2. Perform a fresh search (update last search time to now)
    let now_unix = Utc::now().timestamp();
    state.backend.save_stm(session_id, "_last_search_time", &now_unix.to_string()).await?;

    let result2 = handle_pre_invocation_hook(&state, payload.clone()).await?;
    let text2 = result2["content"][0]["text"].as_str().unwrap();
    assert!(!text2.contains("Warning: Memory searches are stale"), "Should NOT warn when search is fresh: {}", text2);

    // 3. Stale search (last search was 6 minutes ago)
    let stale_unix = now_unix - 360;
    state.backend.save_stm(session_id, "_last_search_time", &stale_unix.to_string()).await?;

    let result3 = handle_pre_invocation_hook(&state, payload.clone()).await?;
    let text3 = result3["content"][0]["text"].as_str().unwrap();
    assert!(text3.contains("Warning: Memory searches are stale"), "Should warn when search is stale: {}", text3);

    Ok(())
}

#[tokio::test]
async fn test_citation_tracker_and_reinforcement() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().join("db");
    let backend = SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy())).await?;
    backend.init().await?;

    let store = Arc::new(MarkdownStore::new(temp_dir.path())?);
    let ignore_list = Arc::new(WatchIgnoreList::new());

    let state = ApiState {
        backend: Arc::new(backend),
        auth_token: "test".to_string(),
        store,
        ignore_list,
        dream_tx: None,
        shutdown_tx: None,
    };

    let session_id = "sess_reinforce_test";

    // 1. Create two episodes with starting importance = 5.0
    let mut ep1 = EpisodeSave::builder("Note A".to_string(), "Reinforce test content A".to_string())
        .scope(Some("general".to_string()))
        .session_id(Some(session_id.to_string()))
        .node_type(Some("agent_thought".to_string()))
        .build();
    ep1.importance = Some(5.0);
    let ep1_id = state.backend.save_episode(&ep1).await?;

    let mut ep2 = EpisodeSave::builder("Note B".to_string(), "Reinforce test content B".to_string())
        .scope(Some("general".to_string()))
        .session_id(Some(session_id.to_string()))
        .node_type(Some("agent_thought".to_string()))
        .build();
    ep2.importance = Some(5.0);
    let ep2_id = state.backend.save_episode(&ep2).await?;

    // Set them as injected context
    let nodes_json = format!("[\"{}\", \"{}\"]", ep1_id, ep2_id);
    state.backend.save_stm(session_id, "distilled_context_nodes", &nodes_json).await?;

    // Transcript turn mentions 'Note A' but not 'Note B'
    let transcript_path = temp_dir.path().join("transcript.jsonl");
    state.backend.save_stm(session_id, "_transcript_path", &transcript_path.to_string_lossy()).await?;

    let mut file = File::create(&transcript_path)?;
    writeln!(file, r#"{{"step_index": 1, "source": "MODEL", "type": "PLANNER_RESPONSE", "content": "I am looking at Note A", "tool_calls": []}}"#)?;
    drop(file);

    // 2. Run pre-invocation
    let payload = serde_json::json!({
        "session_id": session_id,
        "workspace_path": temp_dir.path().to_string_lossy()
    });
    let _ = handle_pre_invocation_hook(&state, payload).await?;

    // 3. Fetch nodes and verify importance reinforcement (EMA)
    let hydrated = state.backend.get_memory_nodes(&[ep1_id, ep2_id]).await?;
    let saved_ep1 = hydrated.episodes.iter().find(|e| e.title == "Note A").unwrap();
    let saved_ep2 = hydrated.episodes.iter().find(|e| e.title == "Note B").unwrap();

    // ep1 was cited, so its importance should increase (5.0 -> 5.5)
    // ep2 was not cited, so its importance should decrease (5.0 -> 4.6)
    assert!(saved_ep1.importance.unwrap() > 5.0, "ep1 importance should reinforce upward, got {:?}", saved_ep1.importance);
    assert!(saved_ep2.importance.unwrap() < 5.0, "ep2 importance should reinforce downward, got {:?}", saved_ep2.importance);

    Ok(())
}

#[tokio::test]
async fn test_cross_agent_broadcast_channel() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().join("db");
    let backend = SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy())).await?;
    backend.init().await?;

    let store = Arc::new(MarkdownStore::new(temp_dir.path())?);
    let ignore_list = Arc::new(WatchIgnoreList::new());

    let state = ApiState {
        backend: Arc::new(backend),
        auth_token: "test".to_string(),
        store,
        ignore_list,
        dream_tx: None,
        shutdown_tx: None,
    };

    // 1. Session A saves a broadcast key: broadcast:status:1
    let put_payload = serde_json::json!({
        "action": "put",
        "session_id": "sess_a",
        "key": "broadcast:status:1",
        "value": "active"
    });
    let _ = handle_manage_stm(&state, put_payload).await?;

    // 2. Session B retrieves the broadcast key: broadcast:status
    let get_payload = serde_json::json!({
        "action": "get",
        "session_id": "sess_b",
        "key": "broadcast:status"
    });
    let result = handle_manage_stm(&state, get_payload.clone()).await?;
    let val = result["content"][0]["text"].as_str().unwrap();
    assert_eq!(val, "active");

    // Also verify listing all keys for session B retrieves broadcast:status
    let get_all_payload = serde_json::json!({
        "action": "get",
        "session_id": "sess_b"
    });
    let all_res = handle_manage_stm(&state, get_all_payload).await?;
    let all_text = all_res["content"][0]["text"].as_str().unwrap();
    assert!(all_text.contains("broadcast:status"), "Should list broadcast key: {}", all_text);

    // 3. Wait for TTL (1 second) to expire
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    let result_expired = handle_manage_stm(&state, get_payload).await?;
    let val_exp = result_expired["content"][0]["text"].as_str().unwrap();
    assert!(val_exp.contains("not found"), "Should expire and return not found: {}", val_exp);

    Ok(())
}

#[tokio::test]
async fn test_vault_clean() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().join("db");
    
    // We need a real git repo for testing git branch pruning
    let repo_dir = temp_dir.path().join("repo");
    std::fs::create_dir_all(&repo_dir)?;
    
    let run = |cmd: &str, args: &[&str]| {
        std::process::Command::new(cmd)
            .args(args)
            .current_dir(&repo_dir)
            .status()
            .unwrap();
    };
    
    run("git", &["init"]);
    run("git", &["config", "user.name", "Test User"]);
    run("git", &["config", "user.email", "test@example.com"]);
    
    // Create initial commit
    std::fs::write(repo_dir.join("file.txt"), "hello")?;
    run("git", &["add", "file.txt"]);
    run("git", &["commit", "-m", "initial"]);

    // Create a stale branch (older than 30 days)
    run("git", &["branch", "htr_branch_stale"]);
    // Force committer date of branch commit to be 31 days ago by amending it on detached head, but wait, 
    // it's easier to just commit with custom committer dates:
    run("git", &["checkout", "htr_branch_stale"]);
    std::fs::write(repo_dir.join("file.txt"), "stale commit")?;
    run("git", &["add", "file.txt"]);
    std::process::Command::new("git")
        .args(&["commit", "-m", "stale branch commit"])
        .env("GIT_COMMITTER_DATE", "2026-06-01T12:00:00Z")
        .env("GIT_AUTHOR_DATE", "2026-06-01T12:00:00Z")
        .current_dir(&repo_dir)
        .status()?;
        
    // Create a fresh branch
    run("git", &["checkout", "main"]);
    run("git", &["branch", "htr_branch_fresh"]);
    run("git", &["checkout", "htr_branch_fresh"]);
    std::fs::write(repo_dir.join("file.txt"), "fresh commit")?;
    run("git", &["add", "file.txt"]);
    run("git", &["commit", "-m", "fresh branch commit"]);
    
    // Return to main branch so we can delete branches
    run("git", &["checkout", "main"]);

    // Initialize backend
    let backend = SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy())).await?;
    backend.init().await?;

    let vault_root = repo_dir.join("vault");
    std::fs::create_dir_all(&vault_root)?;
    let store = Arc::new(MarkdownStore::new(vault_root)?);
    let ignore_list = Arc::new(WatchIgnoreList::new());

    let state = ApiState {
        backend: Arc::new(backend),
        auth_token: "test".to_string(),
        store,
        ignore_list,
        dream_tx: None,
        shutdown_tx: None,
    };

    // Create a stale session (>30 days old) and a fresh session (<30 days old)
    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>().unwrap();
    
    surreal_backend.db.query("
        UPSERT type::record('short_term_memory', ['stale_sess', 'key']) CONTENT {
            session_id: 'stale_sess',
            key: 'key',
            value: 'val',
            updated_at: time::now() - 31d
        };
        UPSERT type::record('short_term_memory', ['fresh_sess', 'key']) CONTENT {
            session_id: 'fresh_sess',
            key: 'key',
            value: 'val',
            updated_at: time::now() - 1d
        };
    ").await?.check()?;

    // Override workspace root setting dynamically
    mythrax_core::store::set_workspace_root(repo_dir.clone());

    // 1. Clean Dry Run
    let dry_run_payload = serde_json::json!({
        "action": "clean",
        "dry_run": true,
        "confirm": false
    });
    let dry_res = handle_manage_vault(&state, dry_run_payload).await?;
    let dry_text = dry_res["content"][0]["text"].as_str().unwrap();
    
    assert!(dry_text.contains("Dry-Run Summary:"), "Should indicate dry-run: {}", dry_text);
    assert!(dry_text.contains("stale_sess"), "Should list stale session: {}", dry_text);
    assert!(dry_text.contains("htr_branch_stale"), "Should list stale branch: {}", dry_text);
    assert!(!dry_text.contains("fresh_sess"), "Should not list fresh session: {}", dry_text);
    assert!(!dry_text.contains("htr_branch_fresh"), "Should not list fresh branch: {}", dry_text);

    // Verify dry run did not delete anything
    let stm_stale = state.backend.get_stm("stale_sess", None).await?;
    assert!(!stm_stale.is_empty());
    let branches_output = std::process::Command::new("git")
        .args(&["branch"])
        .current_dir(&repo_dir)
        .output()?;
    let branches_str = String::from_utf8_lossy(&branches_output.stdout);
    assert!(branches_str.contains("htr_branch_stale"));

    // 2. Clean Confirm
    let clean_payload = serde_json::json!({
        "action": "clean",
        "dry_run": false,
        "confirm": true
    });
    let clean_res = handle_manage_vault(&state, clean_payload).await?;
    let clean_text = clean_res["content"][0]["text"].as_str().unwrap();
    
    assert!(clean_text.contains("Cleanup Completed"), "Should indicate completion: {}", clean_text);

    // Verify stale session and stale branch are deleted
    let stm_stale_after = state.backend.get_stm("stale_sess", None).await?;
    assert!(stm_stale_after.is_empty(), "Stale session STM should be cleared");
    let stm_fresh_after = state.backend.get_stm("fresh_sess", None).await?;
    assert!(!stm_fresh_after.is_empty(), "Fresh session STM should remain");

    let branches_after = std::process::Command::new("git")
        .args(&["branch"])
        .current_dir(&repo_dir)
        .output()?;
    let branches_str_after = String::from_utf8_lossy(&branches_after.stdout);
    assert!(!branches_str_after.contains("htr_branch_stale"), "Stale branch should be deleted");
    assert!(branches_str_after.contains("htr_branch_fresh"), "Fresh branch should remain");

    mythrax_core::store::clear_workspace_root();
    Ok(())
}
