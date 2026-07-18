use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::WikiNode;
use mythrax_core::cognitive::synthesis::DreamCoordinator;
use mythrax_core::store::MarkdownStore;

use std::sync::Mutex;
static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_contradiction_detection_resolution() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;
    fs::create_dir_all(vault_root.join("episodes"))?;

    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;
    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    let store = MarkdownStore::new(&vault_root)?;
    let coordinator = DreamCoordinator::new();

    // Create an existing wiki node
    let existing_node = WikiNode {
        id: None,
        name: "Existing DB Choice".to_string(),
        content: "We should use Postgres for the database.".to_string(),
        scope: "test_scope".to_string(),
        vault_path: Some("wiki/test_scope/insights/db_choice.md".to_string()),
        embedding: Some(vec![1.0; 768]),
        ..Default::default()
    };
    let existing_id = backend.save_wiki_node(&existing_node).await?;
    store.write_file("wiki/test_scope/insights/db_choice.md", "---\ntitle: \"Existing DB Choice\"\nscope: \"test_scope\"\n---\n\nWe should use Postgres for the database.")?;

    // Create a new wiki node that contradicts it
    let new_node = WikiNode {
        id: None,
        name: "New DB Choice".to_string(),
        content: "We should use SurrealDB for the database.".to_string(),
        scope: "test_scope".to_string(),
        vault_path: Some("wiki/test_scope/insights/new_db_choice.md".to_string()),
        embedding: Some(vec![1.0; 768]),
        ..Default::default()
    };

    // Run contradiction resolution save
    let result_id = coordinator.save_wiki_node_with_contradiction_resolution(&backend, &store, &new_node, None).await?;

    // Assert that the returned ID is the existing node's ID
    assert_eq!(result_id, existing_id);

    // Fetch the existing node from DB and assert content is updated to mock resolution
    let all_nodes = backend.get_all_wiki_nodes().await?;
    let updated_node = all_nodes.iter().find(|n| n.id.as_ref() == Some(&existing_id)).expect("Existing node should exist");
    assert_eq!(updated_node.content, "We should use SurrealDB for the database because Postgres was deprecated.");

    // Assert that the physical file of the existing node is updated with resolution
    let file_content = fs::read_to_string(vault_root.join("wiki/test_scope/insights/db_choice.md"))?;
    assert!(file_content.contains("We should use SurrealDB for the database because Postgres was deprecated."));
    assert!(file_content.contains("title: \"Existing DB Choice\""));

    // Assert that the new node's vault path does NOT exist (skipped writing)
    assert!(!vault_root.join("wiki/test_scope/insights/new_db_choice.md").exists());

    Ok(())
}
