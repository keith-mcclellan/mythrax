use mythrax_core::mcp_routes::handle_pre_invocation_hook;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::llm::{DynamicModelBroker, ModelTier};
use mythrax_core::api::ApiState;
use tempfile::tempdir;
use std::sync::Arc;

#[tokio::test]
async fn test_soft_thresholding_and_hook_injection() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("db");
    
    // Initialize SurrealDB with KV store
    let backend = SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy()), mythrax_core::db::BackendConfig { check_daemon: false, embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)), llm: Some(mythrax_core::llm::LLMClient::new_mock()) }).await.unwrap();
    backend.init().await.unwrap();

    // Create a "borderline" episode (low confidence/score)
    let episode = mythrax_core::contracts::EpisodeSave {
        created_at: None,
        title: "Borderline Note".to_string(),
        content: "Soft threshold test content".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/borderline.md".to_string()),
        source_episode: None,
        session_id: Some("test_session".to_string()),
        task_id: None,
        ..Default::default()
    };
    backend.save_episode(&episode).await.unwrap();

    // Initialize Model Broker
    let models_dir = if std::env::var("MYTHRAX_TEST_MOCK").is_ok() {
        temp_dir.path().to_path_buf()
    } else {
        let home = std::env::var("HOME").unwrap();
        std::path::PathBuf::from(home).join(".mythrax/models")
    };
    let broker = DynamicModelBroker::new(models_dir).await.unwrap();
    let broker = Arc::new(broker);
    let _ = mythrax_core::llm::DYNAMIC_MODEL_BROKER.set(broker.clone());
    // Preload embedding model and acquire a Tier2 LLM to simulate active state
    broker.preload_embedding_model("mlx-community/nomic-embed-text-v1.5-mlx").await.unwrap();
    if std::env::var("MYTHRAX_TEST_MOCK").is_err() {
        broker.update_config_model("mlx-community/Qwen2.5-0.5B-Instruct-4bit").await.unwrap();
    }
    let _ = broker.acquire_llm(ModelTier::Tier2).await.unwrap();

    // Construct ApiState with necessary dependencies
    let state = ApiState {
        backend: Arc::new(backend),
        auth_token: "secret-token".to_string(),
        store: Arc::new(mythrax_core::store::MarkdownStore::new(temp_dir.path().to_path_buf()).unwrap()),
        ignore_list: Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new()),
        dream_tx: None,
        shutdown_tx: None,
    };

    // Prepare payload for pre-invocation hook
    let payload = serde_json::json!({
        "session_id": "test_session",
        "query": "threshold test",
        "workspace_path": temp_dir.path().to_string_lossy()
    });

    // Execute the hook
    let result = handle_pre_invocation_hook(&state, payload).await.unwrap();
    
    // Extract content from the result (assuming JSON structure with 'content' array)
    let text_content = result["content"][0]["text"].as_str().unwrap();

    // Assertions
    // 1. The "borderline" candidate (soft thresholded) must be preserved in the response
    assert!(
        text_content.contains("Soft threshold test content"), 
        "Borderline candidate must be preserved and ranked"
    );
    
    // 2. The hook must inject local model status into the response
    assert!(
        text_content.contains("### 🤖 Local Inference & Model Broker Status"), 
        "Hook must inject local model status"
    );
}
