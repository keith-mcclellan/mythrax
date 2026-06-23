use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::HandoffSave;

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
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
    }

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let store = std::sync::Arc::new(mythrax_core::store::MarkdownStore::new(&vault_root)?);
    let mcp_server = mythrax_core::mcp::McpServer::new(backend.clone(), store);

    // 1. Put short term memory via MCP
    let params = serde_json::json!({
        "session_id": "sess_x",
        "key": "secret_data",
        "value": "bearer my-secret-token"
    });
    
    mcp_server.handle_request("tools/call", serde_json::json!({
        "name": "put_short_term",
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
    let get_args = serde_json::json!({
        "session_id": "sess_x",
        "key": "secret_data"
    });
    let get_resp = mcp_server.handle_request("tools/call", serde_json::json!({
        "name": "get_short_term",
        "arguments": get_args
    })).await?;
    let text = get_resp["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("bearer my-secret-token"));

    // 3. Clear short term memory via MCP
    let clear_args = serde_json::json!({
        "session_id": "sess_x"
    });
    mcp_server.handle_request("tools/call", serde_json::json!({
        "name": "clear_short_term",
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

    // Perform cleanup
    backend.delete_stale_handoffs().await?;

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
