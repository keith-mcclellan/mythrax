use anyhow::Result;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use std::sync::Arc;

static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tokio::test]
async fn test_concurrent_embedding_execution() -> Result<()> {
    let _guard = match TEST_MUTEX.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    let backend = Arc::new(backend);

    let mut handles = Vec::new();

    for i in 0..10 {
        let backend_clone = Arc::clone(&backend);
        let handle = tokio::spawn(async move {
            let _ = backend_clone.embed(&format!("test embedding {}", i)).await;
        });
        handles.push(handle);
    }

    for (i, handle) in handles.into_iter().enumerate() {
        match handle.await {
            Ok(_) => {}
            Err(e) => panic!("Task {} panicked: {:?}", i, e),
        }
    }

    Ok(())
}
