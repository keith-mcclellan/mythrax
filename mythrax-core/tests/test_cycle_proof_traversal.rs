use anyhow::Result;
use mythrax_core::db::{SurrealBackend, StorageBackend};

#[tokio::test]
async fn test_cycle_proof_traversal_circular() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create a circular relation: A -> relates_to -> B -> relates_to -> A
    backend.relate_nodes("wiki_node:node_a", "wiki_node:node_b", None, None, None).await?;
    backend.relate_nodes("wiki_node:node_b", "wiki_node:node_a", None, None, None).await?;

    // Perform query_symbolic
    let results = backend.query_symbolic("wiki_node:node_a", None, Some(5)).await?;

// Verify it doesn't loop infinitely and contains node_b
    assert!(results.contains(&"wiki_node:node_b".to_string()));
    assert_eq!(results.len(), 1); // Only node_b is visited and returned (excluding start node)

    Ok(())
}

#[tokio::test]
async fn test_query_symbolic_scored_confidences() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create wiki nodes first so they exist:
    let node_contract = mythrax_core::contracts::WikiNode {
        id: Some("wiki_node:node_a".to_string()),
        name: "Node A".to_string(),
        content: "Content A".to_string(),
        scope: "general".to_string(),
        vault_path: None,
        embedding: None,
    };
    backend.save_wiki_node(&node_contract).await?;
    let node_contract = mythrax_core::contracts::WikiNode {
        id: Some("wiki_node:node_b".to_string()),
        name: "Node B".to_string(),
        content: "Content B".to_string(),
        scope: "general".to_string(),
        vault_path: None,
        embedding: None,
    };
    backend.save_wiki_node(&node_contract).await?;
    let node_contract = mythrax_core::contracts::WikiNode {
        id: Some("wiki_node:node_c".to_string()),
        name: "Node C".to_string(),
        content: "Content C".to_string(),
        scope: "general".to_string(),
        vault_path: None,
        embedding: None,
    };
    backend.save_wiki_node(&node_contract).await?;

    // Chain path:
    backend.relate_nodes("wiki_node:node_a", "wiki_node:node_b", None, None, Some(0.8)).await?;
    backend.relate_nodes("wiki_node:node_b", "wiki_node:node_c", None, None, Some(0.5)).await?;
    
    // Shortcut path (initially weaker but wait, shortcut is direct):
    backend.relate_nodes("wiki_node:node_a", "wiki_node:node_c", None, None, Some(0.5)).await?;

    let results = backend.query_symbolic_scored("wiki_node:node_a", None, Some(3), None).await?;
    
    let hit_c = results.iter().find(|h| h.node_id == "wiki_node:node_c").unwrap();
    // Chain path: 0.8 * 0.5 = 0.4. Shortcut path: 0.5. We should retain max (0.5).
    assert_eq!(hit_c.path_confidence, 0.5);

    Ok(())
}

#[tokio::test]
async fn test_query_symbolic_scored_temporal_filtering() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create wiki nodes
    for name in &["node_a", "node_b", "node_c"] {
        let node_contract = mythrax_core::contracts::WikiNode {
            id: Some(format!("wiki_node:{}", name)),
            name: name.to_string(),
            content: "content".to_string(),
            scope: "general".to_string(),
            vault_path: None,
            embedding: None,
        };
        backend.save_wiki_node(&node_contract).await?;
    }

    // A -[valid at Utc::now()]-> B
    // A -[valid ONLY in future]-> C
    let now = chrono::Utc::now();
    let future = now + chrono::Duration::days(1);
    
    let rel_ab = "RELATE wiki_node:node_a->relates_to->wiki_node:node_b SET confidence = 1.0, valid_from = $from, valid_to = $to;";
    backend.db.query(rel_ab)
        .bind(("from", now - chrono::Duration::days(1)))
        .bind(("to", now + chrono::Duration::days(5)))
        .await?.check()?;

    let rel_ac = "RELATE wiki_node:node_a->relates_to->wiki_node:node_c SET confidence = 1.0, valid_from = $from;";
    backend.db.query(rel_ac)
        .bind(("from", future))
        .await?.check()?;

    // Query as of now: node_b should be returned, but NOT node_c
    let hits_now = backend.query_symbolic_scored("wiki_node:node_a", None, Some(3), Some(now)).await?;
    let ids_now: Vec<String> = hits_now.into_iter().map(|h| h.node_id).collect();
    assert!(ids_now.contains(&"wiki_node:node_b".to_string()));
    assert!(!ids_now.contains(&"wiki_node:node_c".to_string()));

    // Query as of future: both should be returned
    let hits_future = backend.query_symbolic_scored("wiki_node:node_a", None, Some(3), Some(future + chrono::Duration::hours(1))).await?;
    let ids_future: Vec<String> = hits_future.into_iter().map(|h| h.node_id).collect();
    assert!(ids_future.contains(&"wiki_node:node_b".to_string()));
    assert!(ids_future.contains(&"wiki_node:node_c".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_resolve_query_anchors_knn_multi() -> anyhow::Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Seed 3 entities with 768-dimensional embeddings to satisfy HNSW constraints
    let mut emb1 = vec![0.0f32; 768];
    emb1[0] = 0.1;
    let mut emb2 = vec![0.0f32; 768];
    emb2[0] = 0.1; emb2[1] = 0.01;
    let mut emb3 = vec![0.0f32; 768];
    emb3[0] = 0.1; emb3[1] = 0.02;

    backend.db.query("CREATE entity SET name = 'Entity 1', entity_type = 'person', summary = 'Entity summary', labels = ['label1'], embedding = $emb;")
        .bind(("emb", emb1))
        .await?.check()?;
    backend.db.query("CREATE entity SET name = 'Entity 2', entity_type = 'person', summary = 'Entity summary', labels = ['label1'], embedding = $emb;")
        .bind(("emb", emb2))
        .await?.check()?;
    backend.db.query("CREATE entity SET name = 'Entity 3', entity_type = 'person', summary = 'Entity summary', labels = ['label1'], embedding = $emb;")
        .bind(("emb", emb3))
        .await?.check()?;

    let query_emb = {
        let mut qe = vec![0.0f32; 768];
        qe[0] = 0.1;
        qe
    };
    // Call resolve_query_anchors
    let anchors = backend.resolve_query_anchors("some query with no exact matches", Some(&query_emb)).await;

    // It should return more than 1 anchor (proves k > 1)
    assert!(anchors.len() > 1, "Should return more than 1 anchor, got {}", anchors.len());
    assert!(anchors.len() <= 5, "Should be capped at 5 anchors");

    Ok(())
}
