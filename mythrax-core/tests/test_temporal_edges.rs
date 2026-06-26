use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use chrono::{TimeZone, Utc};

#[tokio::test]
async fn as_of_returns_only_facts_valid_then() -> anyhow::Result<()> {
    unsafe { std::env::set_var("MYTHRAX_MOCK_LLM", "true"); }
    
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    
    // Ingest two mock nodes (episodes)
    let id_a = "episode:node_a".to_string();
    let id_b = "episode:node_b".to_string();
    
    // Create direct table records first to relate them
    let sql = "
        CREATE type::record('episode', 'node_a') CONTENT { title: 'A', content: 'A content' };
        CREATE type::record('episode', 'node_b') CONTENT { title: 'B', content: 'B content' };
    ";
    backend.db.query(sql).await?.check()?;
    
    // Relate A -> B valid from 2025-01-01 to 2025-06-01
    let t_from = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    let t_to = Utc.with_ymd_and_hms(2025, 6, 1, 23, 59, 59).unwrap();
    
    backend.relate_nodes(&id_a, &id_b, Some(t_from), Some(t_to), Some(1.0)).await?;
    
    // Query as of 2025-03-01 -> Edge should be present
    let t_mid = Utc.with_ymd_and_hms(2025, 3, 1, 12, 0, 0).unwrap();
    let edges_mid = backend.query_edges_as_of(&id_a, t_mid).await?;
    assert!(edges_mid.contains(&id_b), "Edge A->B should be valid on 2025-03-01");
    
    // Query as of 2025-09-01 -> Edge should be absent
    let t_late = Utc.with_ymd_and_hms(2025, 9, 1, 12, 0, 0).unwrap();
    let edges_late = backend.query_edges_as_of(&id_a, t_late).await?;
    assert!(!edges_late.contains(&id_b), "Edge A->B should not be valid on 2025-09-01");
    
    Ok(())
}

#[tokio::test]
async fn invalidate_closes_not_deletes() -> anyhow::Result<()> {
    unsafe { std::env::set_var("MYTHRAX_MOCK_LLM", "true"); }
    
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    
    let id_a = "episode:node_a".to_string();
    let id_b = "episode:node_b".to_string();
    
    let sql = "
        CREATE type::record('episode', 'node_a') CONTENT { title: 'A', content: 'A content' };
        CREATE type::record('episode', 'node_b') CONTENT { title: 'B', content: 'B content' };
    ";
    backend.db.query(sql).await?.check()?;
    
    // Relate A -> B open-ended (valid_to = None)
    let t_from = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    backend.relate_nodes(&id_a, &id_b, Some(t_from), None, Some(1.0)).await?;
    
    // Query as of 2025-03-01 -> Edge is present
    let t_mid = Utc.with_ymd_and_hms(2025, 3, 1, 12, 0, 0).unwrap();
    assert!(backend.query_edges_as_of(&id_a, t_mid).await?.contains(&id_b));
    
    // Invalidate as of 2025-06-01
    let t_end = Utc.with_ymd_and_hms(2025, 6, 1, 12, 0, 0).unwrap();
    backend.invalidate_edge(&id_a, &id_b, Some(t_end)).await?;
    
    // Query as of 2025-09-01 -> Edge is now absent
    let t_late = Utc.with_ymd_and_hms(2025, 9, 1, 12, 0, 0).unwrap();
    assert!(!backend.query_edges_as_of(&id_a, t_late).await?.contains(&id_b), "Edge should be invalid after invalidation time");
    
    // Query as of 2025-03-01 -> Edge is STILL present (history preserved!)
    assert!(backend.query_edges_as_of(&id_a, t_mid).await?.contains(&id_b), "Edge should still be valid historically");
    
    Ok(())
}

#[tokio::test]
async fn reject_inverted_interval() -> anyhow::Result<()> {
    unsafe { std::env::set_var("MYTHRAX_MOCK_LLM", "true"); }
    
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    
    let id_a = "episode:node_a".to_string();
    let id_b = "episode:node_b".to_string();
    
    let sql = "
        CREATE type::record('episode', 'node_a') CONTENT { title: 'A', content: 'A content' };
        CREATE type::record('episode', 'node_b') CONTENT { title: 'B', content: 'B content' };
    ";
    backend.db.query(sql).await?.check()?;
    
    let t_from = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
    let t_to = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(); // inverted!
    
    let res = backend.relate_nodes(&id_a, &id_b, Some(t_from), Some(t_to), Some(1.0)).await;
    assert!(res.is_err(), "Inverted validity interval must be rejected with an error");
    
    Ok(())
}
