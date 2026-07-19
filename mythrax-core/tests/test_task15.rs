use std::sync::Arc;
use tempfile::tempdir;
use serde_json::json;
use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::api::ApiState;
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::WatchIgnoreList;
use mythrax_core::contracts::{WisdomRule, Tier};
use mythrax_core::mcp_routes::manage_handlers::handle_pre_invocation_hook;

fn setup_env_vars() {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
        std::env::set_var("MYTHRAX_PRE_INVOCATION_TOKEN_BUDGET", "3000");
    }
}

async fn create_test_state(temp_dir: &tempfile::TempDir) -> anyhow::Result<ApiState> {
    let db_path = temp_dir.path().join("db");
    let backend = SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy()), mythrax_core::db::BackendConfig { check_daemon: false, embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)), llm: Some(mythrax_core::llm::LLMClient::new_mock()) }).await?;
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
async fn test_policy_section_rendered_first() -> anyhow::Result<()> {
    setup_env_vars();
    let temp_dir = tempdir()?;
    let state = create_test_state(&temp_dir).await?;
    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>().unwrap();

    let rule = WisdomRule {
        id: Some("wisdom:policy1".to_string()),
        target_pattern: "UNIQUE_POLICY_XYZ".to_string(),
        action_to_avoid: "Avoid it".to_string(),
        causal_explanation: "Failed".to_string(),
        prescribed_remedy: "Do it".to_string(),
        tier: Tier::Project,
        scope: "general".to_string(),
        vault_path: None,
        embedding: None,
        source_episodes: vec![],
        generator_name: "Test".to_string(),
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
    state.backend.save_wisdom_rule(&rule).await?;

    let sql = "INSERT INTO episode { title: 'UNIQUE_ADVISORY_ABC', content: 'Advisory content', scope: 'general', node_type: 'experience' };";
    surreal_backend.db.query(sql).await?;

    let payload = json!({
        "session_id": "test_session_15",
        "action": "pre_invocation"
    });

    let res = handle_pre_invocation_hook(&state, payload).await?;
    let content = res["content"][0]["text"].as_str().unwrap();

    assert!(content.contains("🚫 Policy"));
    assert!(content.contains("UNIQUE_POLICY_XYZ"));
    assert!(content.contains("💡 Advisory"));
    assert!(content.contains("UNIQUE_ADVISORY_ABC"));

    let policy_idx = content.find("UNIQUE_POLICY_XYZ").unwrap();
    let advisory_idx = content.find("UNIQUE_ADVISORY_ABC").unwrap();
    assert!(policy_idx < advisory_idx, "Policy must be rendered before Advisory");
    Ok(())
}

#[tokio::test]
async fn test_policy_uses_caution_format() -> anyhow::Result<()> {
    setup_env_vars();
    let temp_dir = tempdir()?;
    let state = create_test_state(&temp_dir).await?;

    let rule = WisdomRule {
        id: Some("wisdom:policy2".to_string()),
        target_pattern: "POLICY_FORMAT_TEST".to_string(),
        action_to_avoid: "Avoid it".to_string(),
        causal_explanation: "Failed".to_string(),
        prescribed_remedy: "Do it".to_string(),
        tier: Tier::Project,
        scope: "general".to_string(),
        vault_path: None,
        embedding: None,
        source_episodes: vec![],
        generator_name: "Test".to_string(),
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
    state.backend.save_wisdom_rule(&rule).await?;

    let payload = json!({
        "session_id": "test_session_15",
        "action": "pre_invocation"
    });

    let res = handle_pre_invocation_hook(&state, payload).await?;
    let content = res["content"][0]["text"].as_str().unwrap();

    assert!(content.contains("> [!CAUTION]"));
    assert!(content.contains("POLICY_FORMAT_TEST"));
    Ok(())
}

#[tokio::test]
async fn test_advisory_uses_tip_format() -> anyhow::Result<()> {
    setup_env_vars();
    let temp_dir = tempdir()?;
    let state = create_test_state(&temp_dir).await?;
    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>().unwrap();

    let sql = "INSERT INTO episode { title: 'ADVISORY_FORMAT_TEST', content: 'Advisory content', scope: 'general', node_type: 'experience' };";
    surreal_backend.db.query(sql).await?;

    let payload = json!({
        "session_id": "test_session_15",
        "action": "pre_invocation"
    });

    let res = handle_pre_invocation_hook(&state, payload).await?;
    let content = res["content"][0]["text"].as_str().unwrap();

    assert!(content.contains("> [!TIP]"));
    assert!(content.contains("ADVISORY_FORMAT_TEST"));
    Ok(())
}

#[tokio::test]
async fn test_policy_never_truncated() -> anyhow::Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
        std::env::set_var("MYTHRAX_PRE_INVOCATION_TOKEN_BUDGET", "500");
    }
    
    let temp_dir = tempdir()?;
    let state = create_test_state(&temp_dir).await?;
    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>().unwrap();

    for i in 1..=3 {
        let rule = WisdomRule {
            id: Some(format!("wisdom:policy_trunc_{}", i)),
            target_pattern: format!("POLICY_TRUNC_TEST_{}", i),
            action_to_avoid: "Avoid it".to_string(),
            causal_explanation: "Failed".to_string(),
            prescribed_remedy: "Do it".to_string(),
            tier: Tier::Project,
            scope: "general".to_string(),
            vault_path: None,
            embedding: None,
            source_episodes: vec![],
            generator_name: "Test".to_string(),
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
        state.backend.save_wisdom_rule(&rule).await?;
    }

    for i in 1..=10 {
        let sql = format!("INSERT INTO episode {{ title: 'ADVISORY_TRUNC_TEST_{}', content: '{}', scope: 'general', node_type: 'experience' }};", i, "Some long content to trigger truncation. ".repeat(50));
        surreal_backend.db.query(&sql).await?;
    }

    let payload = json!({
        "session_id": "test_session_15",
        "action": "pre_invocation"
    });

    let res = handle_pre_invocation_hook(&state, payload).await?;
    let content = res["content"][0]["text"].as_str().unwrap();

    assert!(content.contains("POLICY_TRUNC_TEST_1"));
    assert!(content.contains("POLICY_TRUNC_TEST_2"));
    assert!(content.contains("POLICY_TRUNC_TEST_3"));
    
    let has_all_advisory = (1..=10).all(|i| content.contains(&format!("ADVISORY_TRUNC_TEST_{}", i)));
    assert!(!has_all_advisory, "Advisory section must be truncated under token pressure");
    
    Ok(())
}
