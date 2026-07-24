use anyhow::Result;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};

#[tokio::test]
async fn test_pipeline_cluster_crud_and_cleanup() -> Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create dummy episodes
    let mut ep1 = EpisodeSave::default();
    ep1.title = "Ep 1".to_string();
    ep1.content = "Content 1".to_string();
    let id1 = backend.save_episode(&ep1).await?;

    let mut ep2 = EpisodeSave::default();
    ep2.title = "Ep 2".to_string();
    ep2.content = "Content 2".to_string();
    let id2 = backend.save_episode(&ep2).await?;

    let mut ep3 = EpisodeSave::default();
    ep3.title = "Ep 3".to_string();
    ep3.content = "Content 3".to_string();
    let id3 = backend.save_episode(&ep3).await?;

    let run_id = "test_run_123";

    // Insert cluster assignments
    backend.save_cluster_assignment(run_id, 1, &id1, Some("general")).await?;
    backend.save_cluster_assignment(run_id, 1, &id2, Some("general")).await?;
    backend.save_cluster_assignment(run_id, 2, &id3, Some("general")).await?;

    // Query cluster members paginated
    let members_c1 = backend.get_cluster_members_paginated(run_id, 1, 50, 0).await?;
    assert_eq!(members_c1.len(), 2);

    let members_c2 = backend.get_cluster_members_paginated(run_id, 2, 50, 0).await?;
    assert_eq!(members_c2.len(), 1);

    // Delete pipeline run
    backend.delete_pipeline_run(run_id).await?;

    let members_after = backend.get_cluster_members_paginated(run_id, 1, 50, 0).await?;
    assert_eq!(members_after.len(), 0);

    Ok(())
}
