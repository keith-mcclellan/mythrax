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
