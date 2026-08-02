#![allow(dead_code, unused_imports)]

use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::backend::SurrealBackend;
use mythrax_core::db::StorageBackend;

#[tokio::test]
async fn test_implicit_precompact_and_distiller_heartbeat_hooks() {
    let backend = SurrealBackend::new(
        "mem://",
        mythrax_core::db::BackendConfig {
            check_daemon: false,
            embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)),
            llm: Some(mythrax_core::llm::LLMClient::new_mock()),
        },
    )
    .await
    .unwrap();
    backend.init().await.unwrap();

    let session_id = "test_implicit_hook_session";
    let now_unix = chrono::Utc::now().timestamp();

    // 1. Assert initial STM is empty
    let stm_before = backend.get_stm(session_id, None).await.unwrap();
    assert!(stm_before.get("_distiller_heartbeat").is_none());

    // 2. Simulate pre_invocation hook execution (distiller caller)
    let _ = backend
        .save_stm(session_id, "_distiller_heartbeat", &now_unix.to_string())
        .await;

    // 3. Save episode to simulate workload
    let save = EpisodeSave::builder(
        "Implicit Test Episode".to_string(),
        "Testing background hook invocation".to_string(),
    )
    .scope(Some("mythrax".to_string()))
    .session_id(Some(session_id.to_string()))
    .build();

    let ep_id = backend.save_episode(&save).await.unwrap();
    assert!(ep_id.starts_with("episode:"));

    // 4. Assert stashed heartbeat and episode persistence
    let stm_after = backend.get_stm(session_id, None).await.unwrap();
    assert_eq!(
        stm_after.get("_distiller_heartbeat").unwrap(),
        &now_unix.to_string()
    );
}
