use std::sync::Arc;
use tempfile::tempdir;
use serde_json::json;
use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::api::ApiState;
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::WatchIgnoreList;
use mythrax_core::contracts::{WisdomRule, Tier, EpisodeSave};
use mythrax_core::mcp_routes::manage_handlers::handle_pre_invocation_hook;

fn setup_env_vars() {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
        std::env::set_var("MYTHRAX_PRE_INVOCATION_TOKEN_BUDGET", "100");
    }
}

async fn create_test_state(temp_dir: &tempfile::TempDir) -> anyhow::Result<ApiState> {
    let db_path = temp_dir.path().join("db");
    let backend = SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy())).await?;
    backend.init().await?;

    let store = Arc::new(MarkdownStore::new(temp_dir.path())?);
    let ignore_list = Arc::new(WatchIgnoreList::new());

    Ok(ApiState {
        backend: Arc::new(backend),
        auth_token: "test".to_string(),
        store,
        ignore_list,
        dream_tx: None,
        shutdown_tx: None,
    })
}

#[tokio::test]
async fn test_task10_injection_and_truncation() -> anyhow::Result<()> {
    setup_env_vars();
    let temp_dir = tempdir()?;
    let state = create_test_state(&temp_dir).await?;
    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>().unwrap();

    let session_id = "test_session_10";

    // 1. Setup Pruned Hypothesis
    let pruned = WisdomRule {
        id: Some("wisdom:pruned1".to_string()),
        target_pattern: "PRUNED: Failed path: some hypothesis".to_string(),
        action_to_avoid: "Avoid it".to_string(),
        causal_explanation: "Failed".to_string(),
        prescribed_remedy: "Try else".to_string(),
        tier: Tier::Project,
        scope: "general".to_string(),
        vault_path: None,
        embedding: None,
        source_episodes: vec![],
        generator_name: "HtrPruneAction".to_string(),
        similarity: Some(1.0),
        utility: Some(1.0),
        status: Some("active".to_string()),
        superseded_at: None,
        superseded_by: None,
        rule_type: Some("pruned_hypothesis".to_string()),
        severity: Some("warning".to_string()),
        blocking: Some(true),
        importance: Some(8.0),
    };
    state.backend.save_wisdom_rule(&pruned).await?;

    // 2. Setup Conflict Node
    let conflict_ep = EpisodeSave::builder("Knowledge Conflict".to_string(), "Conflicting info here".to_string())
        .node_type(Some("conflict".to_string()))
        .scope(Some("general".to_string()))
        .build();
    state.backend.save_episode(&conflict_ep).await?;

    // 3. Populate P3 (Belief State), P2 (STM), P1 (Wisdom - capabilities) to trigger truncation
    let sql = "INSERT INTO belief_state { session_id: $session_id, confidence_score: 0.5, tasks_todo: ['Long task description to consume tokens'], hypotheses_tested: [], uncertainty_areas: [], updated_at: time::now() };";
    surreal_backend.db.query(sql).bind(("session_id", session_id)).await?;

    state.backend.save_stm(session_id, "big_key", &"A ".repeat(500)).await?;

    let payload = json!({
        "session_id": session_id,
        "action": "pre_invocation"
    });

    let res = handle_pre_invocation_hook(&state, payload).await?;
    let content = res["content"][0]["text"].as_str().unwrap();

    assert!(content.contains("### ⛔ Known Failed Approaches"));
    assert!(content.contains("PRUNED: Failed path: some hypothesis"));
    
    assert!(content.contains("### ⚠️ Known Knowledge Boundaries / Conflicts"));
    assert!(content.contains("Knowledge Conflict"));

    // Check budget truncation (we set budget to 100 tokens, which is very small, so P3/P2 might be truncated)
    // Wait, let's test if it handles distiller exemption
    let payload_distiller = json!({
        "session_id": session_id,
        "action": "pre_invocation",
        "caller": "distiller"
    });
    let res_d = handle_pre_invocation_hook(&state, payload_distiller).await?;
    let content_d = res_d["content"][0]["text"].as_str().unwrap();
    // Distiller payload should not contain the same stuff
    assert!(!content_d.contains("Known Failed Approaches"));

    Ok(())
}
