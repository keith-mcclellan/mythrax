use anyhow::Result;
use mythrax_core::api::ApiState;
use mythrax_core::cognitive::compactor::Compactor;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::mcp_routes::call_mcp_tool;
use mythrax_core::store::MarkdownStore;
use serde_json::json;
use std::fs;
use std::sync::Mutex;
use surrealdb_types::SurrealValue;
use tempfile::tempdir;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_chat_history_dynamic_sliding_window() -> Result<()> {
    let _guard = match TEST_MUTEX.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };

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
    // Force initialize the schema locally in the test to ensure all tables exist
    backend
        .db
        .query(mythrax_core::db::schema::INIT_SCHEMA)
        .await?
        .check()?;
    backend.init().await?;

    let store = std::sync::Arc::new(MarkdownStore::new(&vault_root)?);

    let state = ApiState {
        backend: backend.clone(),
        auth_token: "secret".to_string(),
        store,
        ignore_list: std::sync::Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new()),
        dream_tx: None,
    };

    let session_id = "test-session-123";

    // 1. Verify user query logging
    let hook_args = json!({
        "action": "pre_invocation",
        "session_id": session_id,
        "query": "Hello, how do I optimize the pipeline?",
        "workspace_path": workspace_root.to_str().unwrap()
    });

    let _hook_res = call_mcp_tool(&state, "manage", hook_args).await?;

    // Verify that the query was logged
    let mut db_resp = backend
        .db
        .query("SELECT * FROM chat_history WHERE session_id = $session_id;")
        .bind(("session_id", session_id))
        .await?;

    #[derive(serde::Deserialize, Debug, SurrealValue)]
    struct ChatMessageRaw {
        role: String,
        content: String,
    }
    let messages: Vec<ChatMessageRaw> = db_resp.take(0)?;
    assert!(
        !messages.is_empty(),
        "User query should be logged in chat_history"
    );
    assert_eq!(messages[0].role, "user");
    assert_eq!(
        messages[0].content,
        "Hello, how do I optimize the pipeline?"
    );

    // 2. Verify assistant response logging after tool execution
    let _tool_res = call_mcp_tool(
        &state,
        "read",
        json!({
            "session_id": session_id,
            "action": "root"
        }),
    )
    .await?;

    // Verify assistant response is logged
    let mut db_resp2 = backend
        .db
        .query(
            "SELECT * FROM chat_history WHERE session_id = $session_id ORDER BY created_at DESC;",
        )
        .bind(("session_id", session_id))
        .await?;
    let messages2: Vec<ChatMessageRaw> = db_resp2.take(0)?;
    assert!(
        messages2.len() >= 2,
        "Assistant response should be logged after tool execution"
    );
    assert_eq!(messages2[0].role, "assistant");

    // 3. Verify dynamic sliding window token scaling
    let long_text =
        "This is a very long sentence that contains many tokens and will exceed budget. "
            .repeat(20); // ~260 tokens
    for i in 0..10 {
        let role = if i % 2 == 0 { "user" } else { "assistant" };
        let _ = backend.db.query("INSERT INTO chat_history { session_id: $session_id, role: $role, content: $content, created_at: time::now() };")
            .bind(("session_id", session_id))
            .bind(("role", role))
            .bind(("content", long_text.clone()))
            .await?;
    }

    // Call hook again
    let hook_res2 = call_mcp_tool(
        &state,
        "manage",
        json!({
            "action": "pre_invocation",
            "session_id": session_id,
            "query": "current status",
            "workspace_path": workspace_root.to_str().unwrap()
        }),
    )
    .await?;

    let hook_text = hook_res2
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.get(0))
        .and_then(|obj| obj.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    assert!(hook_text.contains("### 💬 Conversational Turn History"));
    let turn_count =
        hook_text.matches("- **User**").count() + hook_text.matches("- **Assistant**").count();
    assert!(
        turn_count < 10,
        "Conversational history should be dynamically scaled down to fit within the 2048 token budget"
    );

    // 4. Verify compaction pruning (> 100 turns)
    for _ in 0..120 {
        let _ = backend.db.query("INSERT INTO chat_history { session_id: $session_id, role: 'user', content: 'brief turn', created_at: time::now() };")
            .bind(("session_id", session_id))
            .await?;
    }

    // Execute compaction
    let compactor = Compactor::new();
    compactor
        .compact_scope(
            &*state.backend,
            &state.store,
            "general",
            backend.embedder.clone(),
        )
        .await?;

    // Count remaining messages for this session
    let mut db_resp3 = backend
        .db
        .query("SELECT * FROM chat_history WHERE session_id = $session_id;")
        .bind(("session_id", session_id))
        .await?;
    let messages3: Vec<ChatMessageRaw> = db_resp3.take(0)?;
    assert_eq!(
        messages3.len(),
        100,
        "Compactor should prune chat_history to exactly 100 turns per session"
    );

    Ok(())
}
