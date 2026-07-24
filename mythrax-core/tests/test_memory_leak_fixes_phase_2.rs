use anyhow::Result;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::schema::INIT_SCHEMA;

#[tokio::test]
async fn test_phase2_paginated_queries() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.db.query(INIT_SCHEMA).await?.check()?;

    for i in 0..15 {
        let ep = EpisodeSave {
            title: format!("Test {}", i),
            content: format!("Content {}", i),
            scope: Some("general".to_string()),
            vault_path: Some(format!("path{}", i)),
            ..Default::default()
        };
        backend.save_episode(&ep).await?;
    }

    let paginated_episodes = backend.get_episodes_paginated(10, 5).await?;
    assert_eq!(paginated_episodes.len(), 10);
    
    Ok(())
}

#[tokio::test]
async fn test_phase2_idf_index_updates() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.db.query(INIT_SCHEMA).await?.check()?;

    let ep1 = EpisodeSave {
        title: "Test IDF".to_string(),
        content: "apple banana apple".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let ep2 = EpisodeSave {
        title: "Test IDF 2".to_string(),
        content: "banana cherry".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };

    let id1 = backend.save_episode(&ep1).await?;
    let _id2 = backend.save_episode(&ep2).await?;

    let sql = "SELECT * FROM idf_index;";
    let mut response = backend.db.query(sql).await?;
    let all: Vec<serde_json::Value> = response.take(0)?;
    println!("ALL IDF INDEX: {:?}", all);

    async fn get_df(backend: &SurrealBackend, term: &str) -> Result<i64> {
        let sql = "SELECT VALUE document_frequency FROM idf_index WHERE term = $term AND scope = 'general';";
        let mut response = backend.db.query(sql).bind(("term", term)).await?;
        let res: Option<i64> = response.take(0)?;
        Ok(res.unwrap_or(0))
    }

    // 'apple' in ep1 only -> df = 1
    assert_eq!(get_df(&backend, "appl").await?, 1);
    // 'banana' in ep1 and ep2 -> df = 2
    assert_eq!(get_df(&backend, "banana").await?, 2);
    // 'cherry' in ep2 only -> df = 1
    assert_eq!(get_df(&backend, "cherri").await?, 1);

    // Now delete ep1
    backend.delete_episode(&id1).await?;

    // 'apple' should be 0
    assert_eq!(get_df(&backend, "appl").await?, 0);
    // 'banana' should be 1
    assert_eq!(get_df(&backend, "banana").await?, 1);
    // 'cherry' should be 1
    assert_eq!(get_df(&backend, "cherri").await?, 1);

    Ok(())
}

#[tokio::test]
async fn test_phase2_backfill() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.db.query(INIT_SCHEMA).await?.check()?;

    let ep1 = EpisodeSave {
        title: "Test IDF".to_string(),
        content: "apple banana apple".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    
    backend.save_episode(&ep1).await?;
    backend.db.query("DELETE FROM idf_index;").await?.check()?;
    
    backend.backfill_idf_index_db().await?;

    let sql = "SELECT VALUE document_frequency FROM idf_index WHERE term = 'appl' AND scope = 'general';";
    let mut response = backend.db.query(sql).await?;
    let res: Option<i64> = response.take(0)?;
    assert_eq!(res, Some(1));
    
    Ok(())
}
