#[cfg(feature = "mlx")]
use mythrax_core::db::{SurrealBackend, StorageBackend};
#[cfg(feature = "mlx")]
use mythrax_core::llm::{DynamicModelBroker, LLMClient, DYNAMIC_MODEL_BROKER};
#[cfg(feature = "mlx")]
use mythrax_core::store::MarkdownStore;
#[cfg(feature = "mlx")]
use tempfile::tempdir;
#[cfg(feature = "mlx")]
use std::sync::Arc;
#[cfg(feature = "mlx")]
use std::path::Path;
#[cfg(feature = "mlx")]
use std::env;
#[cfg(feature = "mlx")]
use std::fs::File;
#[cfg(feature = "mlx")]
use std::io::Write;

#[tokio::test]
#[cfg(feature = "mlx")]
async fn test_external_and_in_process_hybrid_routing() {
    let home = env::var("HOME").unwrap();
    let model_dir = Path::new(&home).join(".mythrax/models");
    let _temp_dir = tempdir().expect("Failed to create temp dir");

    // Force mock off for this test to verify real HTTP and in-process routing
    unsafe {
        env::set_var("MYTHRAX_MOCK_LLM", "false");
    }

    // Initialize SurrealDB in memory
    let backend = SurrealBackend::new("mem://", mythrax_core::db::BackendConfig { check_daemon: false, embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)), llm: Some(mythrax_core::llm::LLMClient::new_mock()) }).await.unwrap();
    backend.init().await.unwrap();

    // Initialize the dynamic model broker
    let broker = DynamicModelBroker::new(model_dir.clone()).await.unwrap();
    let broker_arc = Arc::new(broker);
    let _ = DYNAMIC_MODEL_BROKER.set(broker_arc.clone());

    // 1. Direct Model Request: 0.5B Model (must run in-process)
    let client = LLMClient::new_mock();
    let response_0_5b = client.completion_explicit(
        &backend,
        "local",
        "gemini",
        "mlx-community/Qwen2.5-0.5B-Instruct-4bit",
        Some("Be extremely concise, output exactly one word: 'hello'."),
        "Greet me.",
        false,
    ).await;
    assert!(response_0_5b.is_ok(), "Direct 0.5B in-process completion failed: {:?}", response_0_5b.err());
    let text_0_5b = response_0_5b.unwrap();
    println!("DEBUG ROUTING TEST: 0.5B (in-process) Response: {}", text_0_5b);
    assert!(!text_0_5b.is_empty());

    // 2. Direct Model Request: 35B Model (must route to external mlx-lm HTTP server)
    let response_35b = client.completion_explicit(
        &backend,
        "local",
        "gemini",
        "mlx-community/Qwen3.6-35B-A3B-4bit",
        Some("Be extremely concise, output exactly one word: 'apple'."),
        "Name a fruit.",
        false,
    ).await;
    assert!(response_35b.is_ok(), "Direct 35B external completion failed: {:?}", response_35b.err());
    let text_35b = response_35b.unwrap();
    println!("DEBUG ROUTING TEST: 35B (external HTTP) Response: {}", text_35b);
    assert!(!text_35b.is_empty());
}

#[tokio::test]
#[cfg(feature = "mlx")]
async fn test_dreaming_routing_to_external_model() -> anyhow::Result<()> {
    let home = env::var("HOME").unwrap();
    let model_dir = Path::new(&home).join(".mythrax/models");
    let trans_dir = tempdir()?;
    let workspace_path = trans_dir.path().join("workspace");
    std::fs::create_dir_all(&workspace_path)?;

    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_path.to_str().unwrap());
        std::env::set_var("MYTHRAX_MOCK_LLM", "false");
    }

    // Initialize the dynamic model broker (since dreaming needs nomic embeddings in-process)
    let broker = DynamicModelBroker::new(model_dir.clone()).await.unwrap();
    let broker_arc = Arc::new(broker);
    // Preload embedding model first so that the files are loaded/cached
    broker_arc.preload_embedding_model("mlx-community/nomic-embed-text-v1.5-mlx").await.unwrap();
    let _ = DYNAMIC_MODEL_BROKER.set(broker_arc.clone());

    // Initialize SurrealDB in memory
    let backend = SurrealBackend::new("mem://", mythrax_core::db::BackendConfig { check_daemon: false, embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)), llm: Some(mythrax_core::llm::LLMClient::new_mock()) }).await.unwrap();
    backend.init().await.unwrap();

    let vault_dir = tempdir()?;
    let store = MarkdownStore::new(vault_dir.path())?;

    // Create the transcript directory & file
    let transcript_path = trans_dir.path().join("transcript.jsonl");
    let transcript_path_str = transcript_path.to_string_lossy().to_string();

    let mut trans_file = File::create(&transcript_path)?;
    writeln!(trans_file, r#"{{"role": "user", "content": "Hello compactor, analyze this test session"}}"#)?;
    writeln!(trans_file, r#"{{"role": "tool", "content": "Session is active and verification token is EXTERNAL_DREAM_VERIFICATION_TOKEN"}}"#)?;
    drop(trans_file);

    // Register the transcript path in STM
    backend.save_stm("sess_external_dream", "_transcript_path", &transcript_path_str).await?;
    backend.save_stm("sess_external_dream", "_last_activity", "some activity").await?;

    // Force aging of STM records to satisfy >10m idleness check
    let surreal_backend = backend.as_any().downcast_ref::<SurrealBackend>()
        .expect("Failed to downcast to SurrealBackend");
    surreal_backend.db
        .query("UPDATE short_term_memory SET updated_at = time::now() - 11m WHERE session_id = 'sess_external_dream';")
        .await?
        .check()?;

    // Run the compactor dreaming sweep unmocked (will route LLM calls to 35B model on mlx-lm HTTP server)
    let coordinator = mythrax_core::cognitive::synthesis::DreamCoordinator::new();
    coordinator.run_dream(&backend, &store, Some("incremental"), None).await?;

    // Verify the new turns are mined into the database
    let search_res = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "EXTERNAL_DREAM_VERIFICATION_TOKEN",
        Some("general"),
        false,
        5,
        0,
        0.0,
        None,
        false,
        true,
        false,
        None,
        true,
        None,
    )).await?;
    assert!(search_res.total_matches > 0, "Mined episode containing verification token should be retrievable");

    // The key _last_swept_at is stashed in STM
    let stm_map = backend.get_stm("sess_external_dream", Some("_last_swept_at")).await?;
    let first_swept = stm_map.get("_last_swept_at").cloned()
        .expect("_last_swept_at should be stashed in STM");
    assert!(!first_swept.is_empty(), "_last_swept_at should have a timestamp value");

    Ok(())
}
