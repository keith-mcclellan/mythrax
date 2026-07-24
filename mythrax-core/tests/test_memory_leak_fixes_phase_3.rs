use anyhow::Result;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::backend::SurrealBackend;
use mythrax_core::db::schema::INIT_SCHEMA;

#[tokio::test]
async fn test_phase3_content_hash_deduplication() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.db.query(INIT_SCHEMA).await?.check()?;

    let ep1 = EpisodeSave {
        title: "Test Dedupe".to_string(),
        content: "this is some exact text".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    
    // Save first time, should create new ID
    let id1 = backend.save_episode_db(&ep1).await?;
    
    // Save second time with same exact content, should return identical ID
    let id2 = backend.save_episode_db(&ep1).await?;
    
    assert_eq!(id1, id2, "Content hash deduplication should return the same ID");

    Ok(())
}

#[tokio::test]
async fn test_phase3_content_hash_backfill() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.db.query(INIT_SCHEMA).await?.check()?;

    let id = uuid::Uuid::new_v4().to_string();
    let insert_sql = format!("INSERT INTO episode {{ id: '{}', title: 'Legacy', content: 'Legacy content without hash' }};", id);
    backend.db.query(&insert_sql).await?.check()?;

    // Backfill should compute hashes for this legacy episode
    backend.backfill_content_hashes_db().await?;

    let check_sql = format!("SELECT VALUE content_hash FROM type::record('episode', '{}');", id);
    let mut res = backend.db.query(&check_sql).await?;
    let hash: Option<String> = res.take(0)?;
    assert!(hash.is_some(), "Backfill should populate content_hash");

    Ok(())
}

#[tokio::test]
async fn test_phase3_get_wisdom_tier_trait() -> Result<()> {
    use mythrax_core::contracts::{WisdomRule, Tier};
    use mythrax_core::db::backend::StorageBackend;

    let backend = SurrealBackend::new_in_memory().await?;
    backend.db.query(INIT_SCHEMA).await?.check()?;

    let rule = WisdomRule {
        target_pattern: "Test Pattern".to_string(),
        action_to_avoid: "Test Avoid".to_string(),
        causal_explanation: "Test Why".to_string(),
        prescribed_remedy: "Test Remedy".to_string(),
        tier: Tier::Wisdom,
        scope: "general".to_string(),
        ..Default::default()
    };
    let id = backend.save_wisdom_rule(&rule).await?;

    let tier_res = backend.get_wisdom_tier(&id).await?;
    assert_eq!(tier_res, Some(Tier::Wisdom), "StorageBackend::get_wisdom_tier should return correct tier");

    let fake_tier = backend.get_wisdom_tier("episode:nonexistent").await?;
    assert_eq!(fake_tier, None, "StorageBackend::get_wisdom_tier for non-wisdom ID should return None");

    Ok(())
}

#[tokio::test]
async fn test_phase3_zero_row_backfill_loop_safety() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.db.query(INIT_SCHEMA).await?.check()?;

    // Backfilling empty database should exit without spinning
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        backend.backfill_content_hashes_db(),
    )
    .await??;

    // Insert an episode with content_hash already set
    let id = uuid::Uuid::new_v4().to_string();
    let insert_sql = format!(
        "INSERT INTO episode {{ id: '{}', title: 'Already Hashed', content: 'Some content', content_hash: 'abc' }};",
        id
    );
    backend.db.query(&insert_sql).await?.check()?;

    // Backfilling again should inspect 0 rows and terminate safely
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        backend.backfill_content_hashes_db(),
    )
    .await??;

    Ok(())
}
