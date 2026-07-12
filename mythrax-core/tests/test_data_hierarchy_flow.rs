#[cfg(feature = "mlx")]
use mythrax_core::db::{SurrealBackend, StorageBackend};
#[cfg(feature = "mlx")]
use mythrax_core::llm::{DynamicModelBroker, DYNAMIC_MODEL_BROKER};
#[cfg(feature = "mlx")]
use mythrax_core::contracts::{EpisodeSave, WikiNode, WisdomRule, Entity};
#[cfg(feature = "mlx")]
use std::sync::Arc;

#[tokio::test]
#[cfg(feature = "mlx")]
async fn test_data_hierarchy_flow_ingest_and_retrieve() {
    let home = std::env::var("HOME").unwrap();
    let models_dir = std::path::PathBuf::from(home).join(".mythrax/models");
    
    // Initialize the dynamic model broker
    let broker = DynamicModelBroker::new(models_dir).await.unwrap();
    let broker_arc = Arc::new(broker);
    let _ = DYNAMIC_MODEL_BROKER.set(broker_arc.clone());

    // Preload embedding model first so that the files are downloaded to the cache
    broker_arc.preload_embedding_model("mlx-community/nomic-embed-text-v1.5-mlx").await.unwrap();
    assert!(broker_arc.is_embedding_model_loaded());

    // Initialize SurrealDB in memory AFTER the embedder is present
    let backend = SurrealBackend::new("mem://").await.unwrap();
    backend.init().await.unwrap();

    // 1. Episode Ingestion and Retrieval
    let ep_save = EpisodeSave {
        created_at: None,
        title: "Test Ingestion System Flow".to_string(),
        content: "We are testing the complete data hierarchy from episodes to wisdom rules. This is the raw execution context.".to_string(),
        entities: vec![Entity {
            name: "test_entity".to_string(),
            entity_type: "concept".to_string(),
            summary: "A test concept".to_string(),
            labels: vec!["test".to_string()],
            scope: Some("general".to_string()),
            vault_path: None,
            embedding: None,
        }],
        scope: Some("general".to_string()),
        vault_path: Some("vault/episode_1.md".to_string()),
        source_episode: None,
        session_id: Some("session_123".to_string()),
        task_id: Some("task_123".to_string()),
        discovery_tokens: Some(10),
        facts: Some(vec!["Fact 1: Systems are working".to_string()]),
        concepts: Some(vec!["test_entity".to_string()]),
        files_read: Some(vec!["src/main.rs".to_string()]),
        files_modified: Some(vec![]),
    
        confidence: None,
        ..Default::default()
    };

    let ep_id = backend.save_episode(&ep_save).await.unwrap();
    assert!(!ep_id.is_empty(), "Saved episode ID must not be empty");

    // Retrieve via search matching general
    let search_res = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "execution context",
        Some("general"),
        false,
        5,
        0,
        0.1,
        None,
        false,
        true,
        false,
        None,
        true,
        None,
    )).await.unwrap();
    assert!(!search_res.results.is_empty(), "Must retrieve the ingested episode");
    assert_eq!(search_res.results[0].title, "Test Ingestion System Flow");

    // 2. RAPTOR Summary Node Generation & Retrieval
    let raptor_node = WikiNode {
        id: None,
        name: "Raptor Summary: Test Ingestion System Flow".to_string(),
        content: "Summary of raw execution context for testing data hierarchy flow.".to_string(),
        scope: "general".to_string(),
        vault_path: Some("wiki/archive/raptor_summary_test.md".to_string()),
        embedding: None,
    };

    let raptor_id = backend.save_wiki_node(&raptor_node).await.unwrap();
    assert!(!raptor_id.is_empty(), "Saved Raptor summary ID must not be empty");

    // Retrieve Raptor Summary
    let search_raptor = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "Raptor Summary",
        Some("general"),
        false,
        5,
        0,
        0.1,
        None,
        false,
        false,
        false,
        None,
        true,
        None,
    )).await.unwrap();
    assert!(!search_raptor.results.is_empty(), "Must retrieve the Raptor summary node");
    assert!(search_raptor.results[0].title.contains("Raptor Summary"));

    // 3. Insight Synthesis (WikiNode)
    let insight_node = WikiNode {
        id: None,
        name: "Insight: System Hierarchical Flow".to_string(),
        content: "Compactions compile raw trace episodes into synthesized permanent wiki nodes to build long term memory.".to_string(),
        scope: "general".to_string(),
        vault_path: Some("wiki/insight_hierarchical.md".to_string()),
        embedding: None,
    };

    let insight_id = backend.save_wiki_node(&insight_node).await.unwrap();
    assert!(!insight_id.is_empty(), "Saved insight node ID must not be empty");

    // Retrieve Insight Node
    let search_insight = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "permanent wiki nodes",
        Some("general"),
        false,
        5,
        0,
        0.1,
        None,
        false,
        false,
        false,
        None,
        true,
        None,
    )).await.unwrap();
    assert!(!search_insight.results.is_empty(), "Must retrieve the synthesized insight node");
    assert_eq!(search_insight.results[0].title, "Insight: System Hierarchical Flow");

    // 4. Wisdom Extraction (WisdomRule)
    let wisdom_rule = WisdomRule {
        id: None,
        target_pattern: "unscaped query parameters in db".to_string(),
        action_to_avoid: "injecting raw strings into WHERE clauses".to_string(),
        causal_explanation: "triggers SQL/SurrealQL injection or schema corruption".to_string(),
        prescribed_remedy: "always use query parameters via bind bindings".to_string(),
        tier: mythrax_core::contracts::Tier::Wisdom,
        scope: "general".to_string(),
        vault_path: Some("wisdom/permanent/rule_query_params.md".to_string()),
        embedding: None,
        source_episodes: vec![ep_id.clone()],
        generator_name: "TestHarness".to_string(),
        similarity: None,
        utility: Some(1.0),
        status: Some("active".to_string()),
        superseded_at: None,
        superseded_by: None,
    
        rule_type: None,};

    let wisdom_id = backend.save_wisdom_rule(&wisdom_rule).await.unwrap();
    assert!(!wisdom_id.is_empty(), "Saved wisdom rule ID must not be empty");

    // Retrieve Wisdom Rule
    let search_wisdom = backend.get_wisdom("WHERE clauses", None, 5, 0, 0.1).await.unwrap();
    assert!(!search_wisdom.results.is_empty(), "Must retrieve the WisdomRule");
    assert_eq!(search_wisdom.results[0].action_to_avoid, "injecting raw strings into WHERE clauses");

    // Relate episode to wisdom rule to link the hierarchy
    backend.relate_nodes(&ep_id, &wisdom_id, None, None, None).await.unwrap();

    // 5. MCP coding agent request flow routing check
    let temp_store_dir = tempfile::tempdir().expect("Failed to create temp store dir");
    let api_state = mythrax_core::api::ApiState {
        backend: Arc::new(backend),
        auth_token: "test_token".to_string(),
        store: Arc::new(mythrax_core::store::MarkdownStore::new(temp_store_dir.path().to_path_buf()).unwrap()),
        ignore_list: Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new()),
        dream_tx: None,
        shutdown_tx: None,
    };

    // Invoke complete_code_task which routes through the mlx-lm HTTP server at
    // :8080. Uses the production Qwen3.6-35B-A3B MoE model running on Metal GPU.
    let args = serde_json::json!({
        "action": "complete_task",
        "prompt": "How should we pass query parameters to SurrealDB WHERE clauses?",
        "system_instruction": "Use retrieved context.",
        "model": "mlx-community/Qwen3.6-35B-A3B-4bit"
    });

    let mcp_res = mythrax_core::mcp_routes::call_mcp_tool(&api_state, "agent", args).await;
    if let Err(ref e) = mcp_res {
        eprintln!("MCP TOOL ERROR: {:?}", e);
    }
    assert!(mcp_res.is_ok(), "MCP tool complete_code_task call must succeed");
    let val = mcp_res.unwrap();
    let text = val["content"][0]["text"].as_str().unwrap();
    assert!(!text.is_empty(), "Response must not be empty");
}
