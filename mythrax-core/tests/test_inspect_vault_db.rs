use anyhow::Result;
use mythrax_core::cognitive::synthesis::DreamCoordinator;
use mythrax_core::db::{BackendConfig, StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::operations::sync_vault_to_db;
use std::sync::Arc;

#[tokio::test]
#[ignore]
async fn test_inspect_db() -> Result<()> {
    let db_path = "/Users/keith/.mythrax/db";
    let vault_root = std::path::PathBuf::from("/Users/keith/mythrax-vault");

    println!("Connecting DB at {}", db_path);
    let surreal_backend = Arc::new(
        SurrealBackend::new(
            &format!("surrealkv://{}", db_path),
            BackendConfig::default(),
        )
        .await?,
    );
    surreal_backend.init().await?;
    let backend: Arc<dyn StorageBackend> = surreal_backend.clone();

    let store = Arc::new(MarkdownStore::new(&vault_root)?);

    println!("Syncing vault to DB...");
    let synced = sync_vault_to_db(&backend, &store).await?;
    println!("Synced {} files from vault to DB.", synced);

    let all_eps = backend.get_all_episodes().await?;
    let unprocessed = backend.get_unprocessed_episodes().await?;
    println!("Total Episodes in DB: {}", all_eps.len());
    println!("Unprocessed Episodes in DB BEFORE DREAM: {}", unprocessed.len());

    println!("Running DreamCoordinator (mode: deep)...");
    let dc = DreamCoordinator::new();
    let _ = dc.run_dream(&*backend, &store, Some("deep"), None).await;

    let pending_tasks = surreal_backend.get_pending_cognitive_tasks().await?;
    println!("Pending Cognitive Tasks AFTER DREAM: {}", pending_tasks.len());

    Ok(())
}
