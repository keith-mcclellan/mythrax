#[cfg(feature = "mlx")]
use mythrax_core::db::{SurrealBackend, StorageBackend};
#[cfg(feature = "mlx")]
use mythrax_core::llm::{DynamicModelBroker, LLMClient, DYNAMIC_MODEL_BROKER};
#[cfg(feature = "mlx")]
use tempfile::tempdir;
#[cfg(feature = "mlx")]
use std::sync::Arc;
#[cfg(feature = "mlx")]
use std::path::Path;
#[cfg(feature = "mlx")]
use std::env;

#[tokio::test]
#[cfg(feature = "mlx")]
async fn test_completion_dynamic_server_loading() {
    let home = env::var("HOME").unwrap();
    let model_dir = Path::new(&home).join(".mythrax/models");
    
    // Initialize SurrealDB in memory
    let backend = SurrealBackend::new("mem://", mythrax_core::db::BackendConfig { check_daemon: false, embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)), llm: Some(mythrax_core::llm::LLMClient::new_mock()) }).await.unwrap();
    backend.init().await.unwrap();

    // Update LLM config to local provider with Qwen 0.5B (Tier 1) model
    let req = mythrax_core::contracts::LlmConfigRequest {
        provider: "local".to_string(),
        duration: None,
        model: Some("mlx-community/Qwen2.5-0.5B-Instruct-4bit".to_string()),
        cloud_provider: Some("gemini".to_string()),
        api_key: None,
        llm_post_inference_delay_ms: None,
    };
    backend.update_llm_config(&req).await.unwrap();

    // Initialize the dynamic model broker
    let broker = DynamicModelBroker::new(model_dir.clone()).await.unwrap();
    let broker_arc = Arc::new(broker);
    
    // Set the global static broker
    let _ = DYNAMIC_MODEL_BROKER.set(broker_arc.clone());

    println!("DEBUG TEST: Calling completion...");
    // Execute the completions call
    let client = LLMClient::new_mock();
    let response = client.completion(&backend, Some("You are a helpful assistant"), "Say Hello in one word").await;
    println!("DEBUG TEST: Completion response received: {:?}", response.is_ok());
    
    assert!(response.is_ok(), "Completion execution must succeed dynamically: {:?}", response.err());
    let text = response.unwrap();
    assert!(!text.is_empty(), "Generated response must not be empty");
    
    // Evict unused models to trigger drop and verify cleanup
    drop(client);
    broker_arc.evict_unused_models().await;
    
    // Weak reference upgrade must fail indicating the model was evicted/cleaned up
    let weak_ref = broker_arc.get_weak_llm_reference();
    assert!(weak_ref.and_then(|w| w.upgrade()).is_none(), "In-process model must be cleaned up and evicted upon drop");
}

#[tokio::test]
#[cfg(feature = "mlx")]
async fn test_complete_code_task_mcp_tool() {
    let home = env::var("HOME").unwrap();
    let model_dir = Path::new(&home).join(".mythrax/models");
    let temp_dir = tempdir().expect("Failed to create temp dir");
    
    // Initialize SurrealDB in memory
    let backend = SurrealBackend::new("mem://", mythrax_core::db::BackendConfig { check_daemon: false, embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)), llm: Some(mythrax_core::llm::LLMClient::new_mock()) }).await.unwrap();
    backend.init().await.unwrap();

    // Update LLM config to local provider
    let req = mythrax_core::contracts::LlmConfigRequest {
        provider: "local".to_string(),
        duration: None,
        model: Some("mlx-community/Qwen2.5-0.5B-Instruct-4bit".to_string()),
        cloud_provider: Some("gemini".to_string()),
        api_key: None,
        llm_post_inference_delay_ms: None,
    };
    backend.update_llm_config(&req).await.unwrap();

    // Initialize the dynamic model broker
    let broker = DynamicModelBroker::new(model_dir.clone()).await.unwrap();
    let broker_arc = Arc::new(broker);
    
    // Set the global static broker (ignore if already set in previous test)
    let _ = DYNAMIC_MODEL_BROKER.set(broker_arc.clone());

    // Create ApiState
    let api_state = mythrax_core::api::ApiState {
        backend: Arc::new(backend),
        auth_token: "test_token".to_string(),
        store: Arc::new(mythrax_core::store::MarkdownStore::new(temp_dir.path().to_path_buf()).unwrap()),
        ignore_list: Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new()),
        dream_tx: None,
        shutdown_tx: None,
    };

    // Invoke complete_code_task MCP tool via consolidated agent tool
    let args = serde_json::json!({
        "action": "complete_task",
        "prompt": "Write a rust function to add two numbers.",
        "system_instruction": "Be concise.",
        "model": "mlx-community/Qwen2.5-0.5B-Instruct-4bit"
    });

    let res = mythrax_core::mcp_routes::call_mcp_tool(&api_state, "agent", args).await;
    assert!(res.is_ok(), "MCP tool complete_code_task call must succeed: {:?}", res.err());
    
    let val = res.unwrap();
    let text = val["content"][0]["text"].as_str().unwrap();
    assert!(!text.is_empty(), "Generated tool response must not be empty");
}

#[tokio::test]
#[cfg(feature = "mlx")]
async fn test_tier3_completion_and_eviction() {
    let home = env::var("HOME").unwrap();
    let model_dir = Path::new(&home).join(".mythrax/models");
    
    // Initialize SurrealDB in memory
    let backend = SurrealBackend::new("mem://", mythrax_core::db::BackendConfig { check_daemon: false, embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)), llm: Some(mythrax_core::llm::LLMClient::new_mock()) }).await.unwrap();
    backend.init().await.unwrap();

    // Update LLM config to local provider with Tier 3 model
    let req = mythrax_core::contracts::LlmConfigRequest {
        provider: "local".to_string(),
        duration: None,
        model: Some("mlx-community/Qwen3.6-35B-A3B-4bit".to_string()),
        cloud_provider: Some("gemini".to_string()),
        api_key: None,
        llm_post_inference_delay_ms: None,
    };
    backend.update_llm_config(&req).await.unwrap();

    // Initialize the dynamic model broker
    let broker = DynamicModelBroker::new(model_dir.clone()).await.unwrap();
    let broker_arc = Arc::new(broker);
    
    // Set the global static broker
    let _ = DYNAMIC_MODEL_BROKER.set(broker_arc.clone());

    // Execute completion on Tier 3
    let client = LLMClient::new_mock();
    let response = client.completion(&backend, Some("You are a helpful assistant"), "Reason about 2+2").await;
    
    assert!(response.is_ok(), "Tier 3 completion execution must succeed dynamically: {:?}", response.err());
    let text = response.unwrap();
    assert!(!text.is_empty(), "Generated response must not be empty");
    
    // Evict unused models to trigger drop and verify cleanup
    drop(client);
    broker_arc.evict_unused_models().await;
    
    // Weak reference upgrade must fail indicating the model was evicted/cleaned up
    let weak_ref = broker_arc.get_weak_llm_reference();
    assert!(weak_ref.and_then(|w| w.upgrade()).is_none(), "Tier 3 model must be cleaned up and evicted upon drop");
}
