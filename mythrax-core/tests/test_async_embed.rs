use anyhow::Result;
use mythrax_core::embeddings::{LocalEmbedder, MockEmbedder, TextEmbedder};

#[tokio::test]
async fn test_async_embed_identical_results() -> Result<()> {
    // 1. Test MockEmbedder
    let mock = MockEmbedder;

    let text = "Quantum computing leverages superposition and entanglement.";
    let res = mock.embed(text).await?;
    assert_eq!(res.len(), 768);

    let batch = vec![
        "First test document for async embedding.".to_string(),
        "Second test document with different length and structure.".to_string(),
        "Third document for batch verification.".to_string(),
    ];
    let batch_res = mock.embed_batch(&batch).await?;
    assert_eq!(batch_res.len(), 3);

    // 2. Test LocalEmbedder if model files exist
    if let Ok(local) = LocalEmbedder::new() {
        let text_local = "Mythrax sidecar intelligence companion.";
        let local_res = local.embed(text_local).await?;
        assert_eq!(local_res.len(), 768);

        let batch_local = vec![
            "Alpha document.".to_string(),
            "Beta document.".to_string(),
        ];
        let batch_local_res = local.embed_batch(&batch_local).await?;
        assert_eq!(batch_local_res.len(), 2);

        let sub_batch_res = local.embed_sub_batch(&batch_local).await?;
        assert_eq!(sub_batch_res.len(), 2);
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_async_embed_semaphore_nonblocking() -> Result<()> {
    let mock = MockEmbedder;
    let sem = mythrax_core::llm::metal_embedding_semaphore();

    // Acquire the single permit synchronously/asynchronously to block embed
    let permit = sem.acquire().await.unwrap();

    // Spawn a task invoking embed which will wait non-blockingly on the semaphore
    let embed_handle = tokio::spawn(async move {
        mock.embed("Non-blocking semaphore test string").await
    });

    // Spawn a lightweight task to ensure worker threads are NOT blocked by sleeping
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        tokio::task::yield_now().await;
        let _ = tx.send(true);
    });

    // The lightweight task should complete quickly within timeout
    let lightweight_done = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        rx,
    )
    .await;
    assert!(
        lightweight_done.is_ok(),
        "Tokio runtime worker thread was blocked while embed waited on semaphore"
    );
    assert_eq!(lightweight_done.unwrap(), Ok(true));

    // Release permit so embed can finish
    drop(permit);

    let embed_result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        embed_handle,
    )
    .await;
    assert!(
        embed_result.is_ok(),
        "embed failed to proceed after semaphore permit was released"
    );
    assert!(embed_result.unwrap().unwrap().is_ok());

    Ok(())
}
