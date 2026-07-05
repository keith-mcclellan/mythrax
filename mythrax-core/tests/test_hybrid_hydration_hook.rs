use anyhow::Result;
use mythrax_core::api::ApiState;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::mcp_routes::call_mcp_tool;
use mythrax_core::store::MarkdownStore;
use serde_json::json;

#[tokio::test]
async fn test_hybrid_hydration_hook_behavior() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    let temp_dir = tempfile::tempdir()?;
    let store = MarkdownStore::new(temp_dir.path())?;

    let state = ApiState {
        backend: std::sync::Arc::new(backend),
        auth_token: "secret-api-token".to_string(),
        store: std::sync::Arc::new(store),
        ignore_list: std::sync::Arc::new(Default::default()),
        dream_tx: None,
    };

    // 1. Create a BeliefState in SurrealDB
    let session_id = "test-session-123";
    let _ = state
        .backend
        .as_any()
        .downcast_ref::<SurrealBackend>()
        .unwrap()
        .db
        .query(
            "
        UPSERT type::record('belief_state', $session_id) CONTENT {
            session_id: $session_id,
            tasks_todo: ['task1'],
            hypotheses_tested: ['hyp1'],
            confidence_score: 0.75,
            uncertainty_areas: ['unc1'],
            updated_at: '2026-06-25T00:00:00Z'
        };
    ",
        )
        .bind(("session_id", session_id))
        .await?;

    // 2. Insert handoff to trigger the search path
    let handoff = mythrax_core::contracts::HandoffSave {
        parent_conversation_id: "parent".to_string(),
        subagent_conversation_id: session_id.to_string(),
        summary: "test summary".to_string(),
        handoff_file_path: "handoff.md".to_string(),
        scope: Some("general".to_string()),
    };
    state.backend.save_handoff(&handoff).await?;

    // Call pre_invocation_hook via consolidated manage tool
    let args = json!({
        "action": "pre_invocation",
        "session_id": session_id,
        "query": "test query",
        "workspace_path": temp_dir.path().to_str().unwrap()
    });

    let response = call_mcp_tool(&state, "manage", args).await?;

    let text = response["content"][0]["text"].as_str().unwrap();

    // Verify BeliefState is prepended nicely
    assert!(text.contains("POMDP Belief State"));
    assert!(text.contains("0.75"));
    assert!(text.contains("task1"));

    Ok(())
}
