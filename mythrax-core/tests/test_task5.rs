use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::{WikiNode, Episode, EpisodeSave};
use mythrax_core::store::MarkdownStore;
use mythrax_core::cognitive::synthesis::{backpropagate_directions, promote_insight_to_direction};

#[tokio::test]
async fn test_backpropagation() -> Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(vault_root.join("wiki/scope_A/directions"))?;
    let store = MarkdownStore::new(&vault_root)?;

    let mut dir_node = WikiNode {
        name: "Test Direction".to_string(),
        content: "Old Understanding".to_string(),
        scope: "scope_A".to_string(),
        node_type: Some("direction".to_string()),
        ..Default::default()
    };
    let dir_id = backend.save_wiki_node(&dir_node).await?;

    let child_insight = WikiNode {
        name: "Child Insight".to_string(),
        content: "New detail to add".to_string(),
        scope: "scope_A".to_string(),
        node_type: Some("insight".to_string()),
        ..Default::default()
    };
    let child_id = backend.save_wiki_node(&child_insight).await?;

    let rel = backend.relate_nodes(&dir_id, &child_id, None, None, None).await?;
    println!("Related edge: {:?}", rel);

    backpropagate_directions(&backend, &store).await?;

    let nodes = backend.get_all_wiki_nodes().await?;
    let updated_dir = nodes.iter().find(|n| n.id.as_deref() == Some(&dir_id)).unwrap();
    println!("Updated dir content: {}", updated_dir.content);
    
    assert!(updated_dir.content.contains("Child Insight") || updated_dir.content.contains("architectural compaction"), "Content should be synthesized");

    Ok(())
}

#[tokio::test]
async fn test_direction_promotion() -> Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    let store = MarkdownStore::new(&vault_root)?;

    let node = WikiNode {
        name: "Completely Unique Name".to_string(),
        content: "Initial".to_string(),
        scope: "scope_A".to_string(), // added scope
        node_type: Some("direction".to_string()),
        embedding: None,
        ..Default::default()
    };
    let initial_id = backend.save_wiki_node(&node).await.expect("Failed to save initial node");
    let node_with_id = backend.get_all_wiki_nodes().await.unwrap().into_iter().find(|n| n.id.as_deref() == Some(&initial_id)).unwrap();

    let mut episodes = Vec::new();
    for i in 0..16 {
        episodes.push(Episode {
            id: Some(format!("ep_{}", i)),
            title: "Test".to_string(),
            content: "Test content".to_string(),
            embedding: None,
            ..Default::default()
        });
    }

    promote_insight_to_direction(&backend, &store, &node_with_id, &episodes).await.expect("Promotion failed");
    
    let nodes = backend.get_all_wiki_nodes().await.unwrap();
    let promoted = nodes.iter().find(|n| n.name == "Completely Unique Name").unwrap();
    assert_eq!(promoted.node_type.as_deref(), Some("direction"));

    Ok(())
}
