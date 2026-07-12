use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::{HandoffSave, ForgedSectionBatch, ForgedConcept, ForgedRule};

use std::sync::Mutex;
static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_stm_db_operations() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let tmp = tempdir()?;
    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;
    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
    }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Test save_stm, get_stm, clear_stm
    backend.save_stm("sess_1", "key_a", "val_a").await?;
    backend.save_stm("sess_1", "key_b", "val_b").await?;
    backend.save_stm("sess_2", "key_a", "val_c").await?;

    // Get specific key
    let map = backend.get_stm("sess_1", Some("key_a")).await?;
    assert_eq!(map.len(), 1);
    assert_eq!(map.get("key_a").unwrap(), "val_a");

    // Get all keys
    let map_all = backend.get_stm("sess_1", None).await?;
    assert_eq!(map_all.len(), 2);
    assert_eq!(map_all.get("key_a").unwrap(), "val_a");
    assert_eq!(map_all.get("key_b").unwrap(), "val_b");

    // Clear session
    backend.clear_stm("sess_1").await?;
    let map_cleared = backend.get_stm("sess_1", None).await?;
    assert!(map_cleared.is_empty());

    // Sess 2 should still exist
    let map2 = backend.get_stm("sess_2", None).await?;
    assert_eq!(map2.len(), 1);
    assert_eq!(map2.get("key_a").unwrap(), "val_c");

    Ok(())
}

#[tokio::test]
async fn test_stm_mcp_and_file_sync() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("episodes"))?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;

    // Mock workspace root for .handoffs/
    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;
    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
    }

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let store = std::sync::Arc::new(mythrax_core::store::MarkdownStore::new(&vault_root)?);
    let mcp_server = mythrax_core::mcp::McpServer::new_local(backend.clone(), store);

    // 1. Put short term memory via MCP
    let mut params = serde_json::json!({
        "session_id": "sess_x",
        "key": "secret_data",
        "value": "bearer my-secret-token"
    });
    if let Some(obj) = params.as_object_mut() {
        obj.insert("action".to_string(), serde_json::Value::String("put_short_term".to_string()));
    }
    
    mcp_server.handle_request("tools/call", serde_json::json!({
        "name": "write",
        "arguments": params
    })).await?;

    // Verify it is saved in SurrealDB
    let db_val = backend.get_stm("sess_x", Some("secret_data")).await?;
    assert_eq!(db_val.get("secret_data").unwrap(), "bearer my-secret-token");

    // Verify it is written to disk
    let stm_file_path = workspace_root.join(".handoffs").join("stm_sess_x.json");
    assert!(stm_file_path.exists());
    
    let file_content = fs::read_to_string(&stm_file_path)?;
    // The secret should be filtered by SecretFilter
    assert!(!file_content.contains("my-secret-token"));

    // 2. Get short term memory via MCP
    let mut get_args = serde_json::json!({
        "session_id": "sess_x",
        "key": "secret_data"
    });
    if let Some(obj) = get_args.as_object_mut() {
        obj.insert("action".to_string(), serde_json::Value::String("get_short_term".to_string()));
    }
    let get_resp = mcp_server.handle_request("tools/call", serde_json::json!({
        "name": "read",
        "arguments": get_args
    })).await?;
    let text = get_resp["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("bearer my-secret-token"));

    // 3. Clear short term memory via MCP
    let mut clear_args = serde_json::json!({
        "session_id": "sess_x"
    });
    if let Some(obj) = clear_args.as_object_mut() {
        obj.insert("action".to_string(), serde_json::Value::String("clear_short_term".to_string()));
    }
    mcp_server.handle_request("tools/call", serde_json::json!({
        "name": "write",
        "arguments": clear_args
    })).await?;

    // Verify DB is cleared
    let db_val_cleared = backend.get_stm("sess_x", None).await?;
    assert!(db_val_cleared.is_empty());

    // Verify file is deleted
    assert!(!stm_file_path.exists());

    Ok(())
}

#[tokio::test]
async fn test_stale_handoff_background_cleanup() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("episodes"))?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;

    let workspace_root = tmp.path().join("workspace");
    let handoffs_dir = workspace_root.join(".handoffs");
    fs::create_dir_all(&handoffs_dir)?;
    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create 4 handoffs:
    // 1. Completed + 8 days old -> should be cleaned
    // 2. Failed + 8 days old -> should be cleaned
    // 3. Pending + 8 days old -> should NOT be cleaned
    // 4. Completed + 1 day old -> should NOT be cleaned

    let h1_file = handoffs_dir.join("handoff_task1.md");
    let h1_stm = handoffs_dir.join("stm_sess1.json");
    fs::write(&h1_file, "mock handoff 1")?;
    fs::write(&h1_stm, "{}")?;
    let id1 = backend.save_handoff(&HandoffSave {
        parent_conversation_id: "sess1".to_string(),
        subagent_conversation_id: "sub1".to_string(),
        summary: "handoff 1".to_string(),
        handoff_file_path: h1_file.to_string_lossy().to_string(),
        scope: None,
        include_tool_execution: None,
    }).await?;
    backend.save_stm("sess1", "k", "v").await?;

    let h2_file = handoffs_dir.join("handoff_task2.md");
    let h2_stm = handoffs_dir.join("stm_sess2.json");
    fs::write(&h2_file, "mock handoff 2")?;
    fs::write(&h2_stm, "{}")?;
    let id2 = backend.save_handoff(&HandoffSave {
        parent_conversation_id: "sess2".to_string(),
        subagent_conversation_id: "sub2".to_string(),
        summary: "handoff 2".to_string(),
        handoff_file_path: h2_file.to_string_lossy().to_string(),
        scope: None,
        include_tool_execution: None,
    }).await?;
    backend.save_stm("sess2", "k", "v").await?;

    let h3_file = handoffs_dir.join("handoff_task3.md");
    let h3_stm = handoffs_dir.join("stm_sess3.json");
    fs::write(&h3_file, "mock handoff 3")?;
    fs::write(&h3_stm, "{}")?;
    let id3 = backend.save_handoff(&HandoffSave {
        parent_conversation_id: "sess3".to_string(),
        subagent_conversation_id: "sub3".to_string(),
        summary: "handoff 3".to_string(),
        handoff_file_path: h3_file.to_string_lossy().to_string(),
        scope: None,
        include_tool_execution: None,
    }).await?;
    backend.save_stm("sess3", "k", "v").await?;

    let h4_file = handoffs_dir.join("handoff_task4.md");
    let h4_stm = handoffs_dir.join("stm_sess4.json");
    fs::write(&h4_file, "mock handoff 4")?;
    fs::write(&h4_stm, "{}")?;
    let id4 = backend.save_handoff(&HandoffSave {
        parent_conversation_id: "sess4".to_string(),
        subagent_conversation_id: "sub4".to_string(),
        summary: "handoff 4".to_string(),
        handoff_file_path: h4_file.to_string_lossy().to_string(),
        scope: None,
        include_tool_execution: None,
    }).await?;
    backend.save_stm("sess4", "k", "v").await?;

    // Update their status and created_at manually via SurrealDB query
    let rec1 = mythrax_core::db::parse_record_id(&id1)?;
    let rec2 = mythrax_core::db::parse_record_id(&id2)?;
    let rec3 = mythrax_core::db::parse_record_id(&id3)?;
    let rec4 = mythrax_core::db::parse_record_id(&id4)?;

    backend.db.query("
        UPDATE $r1 SET status = 'COMPLETED', created_at = time::now() - 8d;
        UPDATE $r2 SET status = 'FAILED', created_at = time::now() - 8d;
        UPDATE $r3 SET status = 'PENDING', created_at = time::now() - 8d;
        UPDATE $r4 SET status = 'COMPLETED', created_at = time::now() - 1d;
    ")
    .bind(("r1", rec1))
    .bind(("r2", rec2))
    .bind(("r3", rec3))
    .bind(("r4", rec4))
    .await?.check()?;

    // Perform cleanup with 7 days threshold (matches 8d age in test setup)
    backend.delete_stale_handoffs(7).await?;

    // Assert stale files are deleted
    assert!(!h1_file.exists());
    assert!(!h1_stm.exists());
    assert!(!h2_file.exists());
    assert!(!h2_stm.exists());

    // Assert non-stale/pending files still exist
    assert!(h3_file.exists());
    assert!(h3_stm.exists());
    assert!(h4_file.exists());
    assert!(h4_stm.exists());

    // Assert DB entries
    // H1 and H2 should be deleted from DB
    let h1_in_db: Option<serde_json::Value> = backend.db.select(("handoff", mythrax_core::db::backend::record_key_to_string(&mythrax_core::db::parse_record_id(&id1)?.key))).await?;
    assert!(h1_in_db.is_none());
    let h2_in_db: Option<serde_json::Value> = backend.db.select(("handoff", mythrax_core::db::backend::record_key_to_string(&mythrax_core::db::parse_record_id(&id2)?.key))).await?;
    assert!(h2_in_db.is_none());

    // H3 and H4 should still exist in DB
    let h3_in_db: Option<serde_json::Value> = backend.db.select(("handoff", mythrax_core::db::backend::record_key_to_string(&mythrax_core::db::parse_record_id(&id3)?.key))).await?;
    assert!(h3_in_db.is_some());
    let h4_in_db: Option<serde_json::Value> = backend.db.select(("handoff", mythrax_core::db::backend::record_key_to_string(&mythrax_core::db::parse_record_id(&id4)?.key))).await?;
    assert!(h4_in_db.is_some());

    // Stale STM entries in DB should be deleted
    let stm1 = backend.get_stm("sess1", None).await?;
    assert!(stm1.is_empty());
    let stm2 = backend.get_stm("sess2", None).await?;
    assert!(stm2.is_empty());

    // Non-stale STM entries in DB should still exist
    let stm3 = backend.get_stm("sess3", None).await?;
    assert!(!stm3.is_empty());
    let stm4 = backend.get_stm("sess4", None).await?;
    assert!(!stm4.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_save_forged_section_lifecycle() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("episodes"))?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;

    unsafe {
        std::env::remove_var("MYTHRAX_WORKSPACE_ROOT");
        std::env::set_var("MYTHRAX_VAULT_ROOT", vault_root.to_str().unwrap());
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create a batch
    let batch = ForgedSectionBatch {
        doc_title: "My System Playbook!".to_string(),
        scope: "production".to_string(),
        chunk_index: 0,
        chunk_text: "We should avoid hardcoding API keys in our deployment scripts. For example, api_key: 'sk-123' must be prevented.".to_string(),
        concepts: vec![
            ForgedConcept {
                name: "API Secret Management".to_string(),
                content: "Centralized environment secrets storage.".to_string(),
            }
        ],
        rules: vec![
            ForgedRule {
                target_pattern: "Avoid Hardcoded API Keys".to_string(),
                action_to_avoid: "hardcoding api_key = 'sk-...'".to_string(),
                causal_explanation: "This leaks credentials to source control.".to_string(),
                prescribed_remedy: "Use environment variables or vault references instead.".to_string(),
            }
        ],
    };

    // Save batch
    backend.save_forged_section(&batch).await?;

    // 1. Verify files are written to disk with SecretFilter sanitization
    let doc_slug = "my_system_playbook";

    // Chunk file
    let chunk_path = vault_root.join(format!("episodes/forge/{}/chunk_0.md", doc_slug));
    assert!(chunk_path.exists());
    let chunk_content = fs::read_to_string(&chunk_path)?;
    assert!(chunk_content.contains("title: \"My System Playbook! - Chunk 0\""));
    assert!(chunk_content.contains("scope: \"production\""));
    assert!(chunk_content.contains("source: \"forge\""));
    assert!(chunk_content.contains("api_key: \"[REDACTED]\"")); // Check secret cleaning!
    assert!(!chunk_content.contains("sk-123"));

    // Concept file
    let wiki_dir = vault_root.join(format!("wiki/forge/{}", doc_slug));
    assert!(wiki_dir.exists());
    let wiki_files: Vec<_> = fs::read_dir(&wiki_dir)?
        .map(|r| r.unwrap().path())
        .collect();
    assert_eq!(wiki_files.len(), 1);
    let wiki_path = &wiki_files[0];
    let wiki_name = wiki_path.file_name().unwrap().to_str().unwrap();
    assert!(wiki_name.starts_with("concept_api_secret_management_"));
    let wiki_content = fs::read_to_string(wiki_path)?;
    assert!(wiki_content.contains("name: \"API Secret Management\""));
    assert!(wiki_content.contains("Centralized environment secrets storage."));

    // Wisdom file
    let wisdom_dir = vault_root.join(format!("wisdom/forge/{}", doc_slug));
    assert!(wisdom_dir.exists());
    let wisdom_files: Vec<_> = fs::read_dir(&wisdom_dir)?
        .map(|r| r.unwrap().path())
        .collect();
    assert_eq!(wisdom_files.len(), 1);
    let wisdom_path = &wisdom_files[0];
    let wisdom_name = wisdom_path.file_name().unwrap().to_str().unwrap();
    assert!(wisdom_name.starts_with("rule_avoid_hardcoded_api_keys_"));
    let wisdom_content = fs::read_to_string(wisdom_path)?;
    assert!(wisdom_content.contains("target_pattern: \"Avoid Hardcoded API Keys\""));
    assert!(wisdom_content.contains("tier: \"forge\""));
    assert!(wisdom_content.contains("Use environment variables or vault references instead."));

    // 2. Verify database records are inserted and relations exist
    // Fetch episode
    let mut ep_resp = backend.db.query("SELECT * FROM episode WHERE source = 'forge' LIMIT 1;").await?;
    let episodes: Vec<serde_json::Value> = ep_resp.take(0)?;
    assert_eq!(episodes.len(), 1);
    let ep = &episodes[0];
    assert_eq!(ep["title"].as_str().unwrap(), "My System Playbook! - Chunk 0");
    assert!(ep["content"].as_str().unwrap().contains("api_key: \"[REDACTED]\""));

    // Fetch wiki node
    let mut wiki_resp = backend.db.query("SELECT * FROM wiki_node WHERE name = 'API Secret Management' LIMIT 1;").await?;
    let wiki_nodes: Vec<serde_json::Value> = wiki_resp.take(0)?;
    assert_eq!(wiki_nodes.len(), 1);

    // Fetch wisdom
    let mut wisdom_resp = backend.db.query("SELECT * FROM wisdom WHERE target_pattern = 'Avoid Hardcoded API Keys' LIMIT 1;").await?;
    let wisdom_rules: Vec<serde_json::Value> = wisdom_resp.take(0)?;
    assert_eq!(wisdom_rules.len(), 1);
    assert_eq!(wisdom_rules[0]["tier"].as_str().unwrap(), "forge");

    // Verify relations: Playbook (WisdomRule) -> relates_to -> Concept (WikiNode) -> relates_to -> Chunk (Episode)
    let ep_id = ep["id"].as_str().unwrap();
    let wiki_id = wiki_nodes[0]["id"].as_str().unwrap();
    let wisdom_id = wisdom_rules[0]["id"].as_str().unwrap();

    let mut rel_resp1 = backend.db.query("SELECT * FROM relates_to WHERE in = $wiki_id AND out = $ep_id;")
        .bind(("ep_id", mythrax_core::db::parse_record_id(ep_id)?))
        .bind(("wiki_id", mythrax_core::db::parse_record_id(wiki_id)?))
        .await?;
    let rels1: Vec<serde_json::Value> = rel_resp1.take(0)?;
    assert_eq!(rels1.len(), 1);

    let mut rel_resp2 = backend.db.query("SELECT * FROM relates_to WHERE in = $wisdom_id AND out = $wiki_id;")
        .bind(("wisdom_id", mythrax_core::db::parse_record_id(wisdom_id)?))
        .bind(("wiki_id", mythrax_core::db::parse_record_id(wiki_id)?))
        .await?;
    let rels2: Vec<serde_json::Value> = rel_resp2.take(0)?;
    assert_eq!(rels2.len(), 1);

    // Verify metrics records are created
    let mut met_resp = backend.db.query("SELECT * FROM metrics;").await?;
    let metrics_records: Vec<serde_json::Value> = met_resp.take(0)?;
    assert!(metrics_records.len() >= 2);

    Ok(())
}

#[tokio::test]
async fn test_save_forged_section_rollback() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;

    unsafe {
        std::env::remove_var("MYTHRAX_WORKSPACE_ROOT");
        std::env::set_var("MYTHRAX_VAULT_ROOT", vault_root.to_str().unwrap());
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create a batch
    let batch = ForgedSectionBatch {
        doc_title: "Rollback Doc".to_string(),
        scope: "production".to_string(),
        chunk_index: 0,
        chunk_text: "Some chunk text".to_string(),
        concepts: vec![
            ForgedConcept {
                name: "Rollback Concept".to_string(),
                content: "Rollback content".to_string(),
            }
        ],
        rules: vec![
            ForgedRule {
                target_pattern: "Rollback Rule".to_string(),
                action_to_avoid: "avoid".to_string(),
                causal_explanation: "why".to_string(),
                prescribed_remedy: "remedy".to_string(),
            }
        ],
    };

    // Break SurrealDB so the transaction fails
    backend.db.query("REMOVE TABLE wiki_node;").await?.check()?;

    // Call save_forged_section - it should return Err
    let res = backend.save_forged_section(&batch).await;
    assert!(res.is_err());

    // Verify no files are left in the vault
    let chunk_file = vault_root.join("episodes/forge/rollback_doc/chunk_0.md");
    assert!(!chunk_file.exists());

    let wiki_dir = vault_root.join("wiki/forge/rollback_doc");
    if wiki_dir.exists() {
        let entries: Vec<_> = fs::read_dir(wiki_dir)?.collect();
        assert!(entries.is_empty());
    }

    let wisdom_dir = vault_root.join("wisdom/forge/rollback_doc");
    if wisdom_dir.exists() {
        let entries: Vec<_> = fs::read_dir(wisdom_dir)?.collect();
        assert!(entries.is_empty());
    }

    Ok(())
}

#[tokio::test]
async fn test_mcp_forge_tools() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("episodes"))?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;

    unsafe {
        std::env::remove_var("MYTHRAX_WORKSPACE_ROOT");
        std::env::set_var("MYTHRAX_VAULT_ROOT", vault_root.to_str().unwrap());
    }

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let store = std::sync::Arc::new(mythrax_core::store::MarkdownStore::new(&vault_root)?);
    let mcp_server = mythrax_core::mcp::McpServer::new_local(backend.clone(), store);

    // 1. Call get_forge_instructions
    let inst_resp = mcp_server.handle_request("tools/call", serde_json::json!({
        "name": "get_forge_instructions",
        "arguments": {}
    })).await?;

    let inst_text = inst_resp["content"][0]["text"].as_str().unwrap();
    assert!(inst_text.contains("Wisdom Rules Extraction"));
    assert!(inst_text.contains("Concept Wiki Nodes Extraction"));

    // 2. Call save_forged_assets
    let batch = serde_json::json!({
        "doc_title": "MCP Forge Doc",
        "scope": "development",
        "chunk_index": 1,
        "chunk_text": "Grounding chunk content.",
        "concepts": [
            {
                "name": "MCP Concept",
                "content": "MCP concept definition."
            }
        ],
        "rules": [
            {
                "target_pattern": "MCP Rule",
                "action_to_avoid": "avoiding mcp",
                "causal_explanation": "explanation",
                "prescribed_remedy": "remedy"
            }
        ]
    });

    let mut write_args = batch.clone();
    if let Some(obj) = write_args.as_object_mut() {
        obj.insert("action".to_string(), serde_json::Value::String("save_forged_assets".to_string()));
    }

    let save_resp = mcp_server.handle_request("tools/call", serde_json::json!({
        "name": "write",
        "arguments": write_args
    })).await?;

    let save_text = save_resp["content"][0]["text"].as_str().unwrap();
    assert!(save_text.contains("Successfully saved forged assets"));

    // Verify files on disk
    let chunk_path = vault_root.join("episodes/forge/mcp_forge_doc/chunk_1.md");
    assert!(chunk_path.exists());

    // Verify DB entry
    let mut ep_resp = backend.db.query("SELECT * FROM episode WHERE source = 'forge' LIMIT 1;").await?;
    let episodes: Vec<serde_json::Value> = ep_resp.take(0)?;
    assert_eq!(episodes.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_api_save_forged_assets() -> Result<()> {
    use axum::http::Request;
    use tower::util::ServiceExt;
    use mythrax_core::api::{ApiState, create_router};
    use mythrax_core::vault::watcher::WatchIgnoreList;

    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("episodes"))?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;

    unsafe {
        std::env::remove_var("MYTHRAX_WORKSPACE_ROOT");
        std::env::set_var("MYTHRAX_VAULT_ROOT", vault_root.to_str().unwrap());
    }

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let store = std::sync::Arc::new(mythrax_core::store::MarkdownStore::new(&vault_root)?);
    let ignore_list = std::sync::Arc::new(WatchIgnoreList::new());

    let state = std::sync::Arc::new(ApiState {
        backend: backend.clone(),
        auth_token: "secret-api-token".to_string(),
        store,
        ignore_list,
        dream_tx: None,
        shutdown_tx: None,
    });

    let app = create_router(state);

    let batch = serde_json::json!({
        "doc_title": "API Forge Doc",
        "scope": "production",
        "chunk_index": 2,
        "chunk_text": "API grounding chunk content.",
        "concepts": [
            {
                "name": "API Concept",
                "content": "API concept definition."
            }
        ],
        "rules": [
            {
                "target_pattern": "API Rule",
                "action_to_avoid": "avoiding api",
                "causal_explanation": "explanation",
                "prescribed_remedy": "remedy"
            }
        ]
    });

    // 1. Test Unauthorized
    let response = app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/forge/save")
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(serde_json::to_vec(&batch).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);

    // 2. Test Success (Authorized)
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/forge/save")
                .header("X-Mythrax-Token", "secret-api-token")
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(serde_json::to_vec(&batch).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);

    // Verify files on disk
    let chunk_path = vault_root.join("episodes/forge/api_forge_doc/chunk_2.md");
    assert!(chunk_path.exists());

    // Verify DB entry
    let mut ep_resp = backend.db.query("SELECT * FROM episode WHERE source = 'forge' AND title = 'API Forge Doc - Chunk 2' LIMIT 1;").await?;
    let episodes: Vec<serde_json::Value> = ep_resp.take(0)?;
    assert_eq!(episodes.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_stm_continuous_pruning() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    let handoffs_dir = vault_root.join(".handoffs");
    fs::create_dir_all(&handoffs_dir)?;

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create a 4-day-old handoff file and stm file
    let old_handoff_file = handoffs_dir.join("old_handoff.md");
    let old_stm_file = handoffs_dir.join("stm_old_sess.json");
    fs::write(&old_handoff_file, "old handoff content")?;
    fs::write(&old_stm_file, "{}")?;
    
    // Set modification time of old stm file to 4 days ago using std::fs::File::set_modified
    let file = fs::OpenOptions::new().write(true).open(&old_stm_file)?;
    file.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(4 * 24 * 3600))?;
    drop(file);

    // Create a fresh stm file (2 hours old)
    let fresh_stm_file = handoffs_dir.join("stm_fresh_sess.json");
    fs::write(&fresh_stm_file, "{}")?;
    let file = fs::OpenOptions::new().write(true).open(&fresh_stm_file)?;
    file.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 3600))?;
    drop(file);

    // Insert an STM record into SurrealDB and set updated_at to 4 days ago
    backend.save_stm("old_sess", "k1", "v1").await?;
    backend.db.query("UPDATE type::record('short_term_memory', [$session_id, $key]) SET updated_at = time::now() - 4d;")
        .bind(("session_id", "old_sess"))
        .bind(("key", "k1"))
        .await?.check()?;

    // Insert a fresh STM record (2 hours old)
    backend.save_stm("fresh_sess", "k2", "v2").await?;

    // Set environment variable to customize pruning days to 3 (so 4d old records get pruned)
    unsafe {
        std::env::set_var("MYTHRAX_STM_PRUNING_DAYS", "3");
    }

    // Run pruning
    let prune_result = backend.prune_stale_memories(&vault_root).await;

    // Clean up environment variable
    unsafe {
        std::env::remove_var("MYTHRAX_STM_PRUNING_DAYS");
    }

    prune_result?;

    // Assertions
    assert!(!old_stm_file.exists(), "Old STM file should be pruned");
    assert!(fresh_stm_file.exists(), "Fresh STM file should be preserved");

    // Check DB
    let old_stm_map = backend.get_stm("old_sess", None).await?;
    assert!(old_stm_map.is_empty(), "Old STM record in DB should be pruned");

    let fresh_stm_map = backend.get_stm("fresh_sess", None).await?;
    assert_eq!(fresh_stm_map.get("k2").unwrap(), "v2", "Fresh STM record in DB should be preserved");

    Ok(())
}

#[tokio::test]
async fn test_pre_invocation_hook_flow() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("episodes"))?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;

    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;
    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
    }

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let store = std::sync::Arc::new(mythrax_core::store::MarkdownStore::new(&vault_root)?);
    let mcp_server = mythrax_core::mcp::McpServer::new_local(backend.clone(), store);

    // 1. Create a handoff
    let handoff = HandoffSave {
        parent_conversation_id: "parent_123".to_string(),
        subagent_conversation_id: "subagent_456".to_string(),
        summary: "Build a new hook feature".to_string(),
        handoff_file_path: "handoff_test.md".to_string(),
        scope: Some("general".to_string()),
        include_tool_execution: None,
    };
    backend.save_handoff(&handoff).await?;

    // 2. Insert the wisdom rule in the database so it can be hydrated
    let rule = mythrax_core::contracts::WisdomRule {
        id: Some("wisdom:rule_abc".to_string()),
        target_pattern: "Test Pattern".to_string(),
        action_to_avoid: "Avoiding test".to_string(),
        causal_explanation: "Causal details".to_string(),
        prescribed_remedy: "Remedy details".to_string(),
        tier: "dynamic".to_string(),
        scope: "general".to_string(),
        vault_path: Some("wisdom/rule_abc.md".to_string()),
        embedding: None,
        source_episodes: vec![],
        generator_name: "test".to_string(),
        similarity: None,
        utility: Some(50.0),
        status: None,
        superseded_at: None,
        superseded_by: None,
        rule_type: None,
    };
    let saved_id = backend.save_wisdom_rule(&rule).await?;

    // 3. Add distilled context nodes to STM
    backend.save_stm("subagent_456", "distilled_context_nodes", &format!("[\"{}\"]", saved_id)).await?;

    // 4. Call pre_invocation_hook via MCP consolidated manage tool
    let args = serde_json::json!({
        "action": "pre_invocation",
        "session_id": "subagent_456",
        "query": "test query"
    });
    let resp = mcp_server.handle_request("tools/call", serde_json::json!({
        "name": "manage",
        "arguments": args
    })).await?;

    let text = resp["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Handoff Metadata"), "Expected Handoff Metadata in: {}", text);
    assert!(text.contains("Test Pattern"), "Expected Test Pattern in: {}", text);
    assert!(text.contains("Avoiding test"), "Expected Avoiding test in: {}", text);

    // 5. Test root agent path (when no handoff active)
    let args_root = serde_json::json!({
        "action": "pre_invocation",
        "session_id": "root_session_789",
        "query": "test query",
        "workspace_path": workspace_root.to_str().unwrap()
    });
    let resp_root = mcp_server.handle_request("tools/call", serde_json::json!({
        "name": "manage",
        "arguments": args_root
    })).await?;
    let text_root = resp_root["content"][0]["text"].as_str().unwrap();
    assert!(text_root.contains("Retrieved Semantic Context"), "Expected Retrieved Semantic Context in: {}", text_root);
    assert!(text_root.contains("Pinned Deep-Search Instruction"), "Expected Pinned Deep-Search Instruction in: {}", text_root);

    Ok(())
}
