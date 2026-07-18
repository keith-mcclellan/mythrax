use mythrax_core::cognitive::synthesis::graduate_wisdom;
use mythrax_core::contracts::{Tier, WikiNode};
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_cross_scope_graduation_similarity() {
    let tmp = tempdir().unwrap();
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root).unwrap();
    fs::create_dir_all(vault_root.join("wiki")).unwrap();
    let store = MarkdownStore::new(&vault_root).unwrap();

    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    let emb1 = vec![0.1; 768];
    let emb2 = vec![0.1; 768];

    let node1 = WikiNode {
        id: None,
        name: "Direction 1".into(),
        content: "We should avoid using raw pointers.".into(),
        scope: "project_a".into(),
        vault_path: Some("wiki/project_a/directions/dir1.md".into()),
        embedding: Some(emb1),
        node_type: Some("direction".into()),
        ..Default::default()
    };

    let node2 = WikiNode {
        id: None,
        name: "Direction 2".into(),
        content: "Do not use raw pointers in the project.".into(),
        scope: "project_b".into(),
        vault_path: Some("wiki/project_b/directions/dir2.md".into()),
        embedding: Some(emb2),
        node_type: Some("direction".into()),
        ..Default::default()
    };

    backend.save_wiki_node(&node1).await.map_err(|e| { println!("Err saving node1: {:?}", e); e }).unwrap();
    backend.save_wiki_node(&node2).await.map_err(|e| { println!("Err saving node2: {:?}", e); e }).unwrap();

    // No conflict nodes
    graduate_wisdom(&backend, &store).await.unwrap();

    let rules = backend.get_all_wisdom_rules().await.unwrap();
    assert_eq!(rules.len(), 1, "Should graduate one wisdom rule");
    assert_eq!(rules[0].tier, Tier::Wisdom);
    assert!(rules[0].rule_type == Some("system_constraint".into()) || rules[0].rule_type == Some("procedural_heuristic".into()));
}

#[tokio::test]
async fn test_graduation_blocked_by_conflict() {
    let tmp = tempdir().unwrap();
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root).unwrap();
    fs::create_dir_all(vault_root.join("wiki")).unwrap();
    let store = MarkdownStore::new(&vault_root).unwrap();

    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    let emb1 = vec![0.5; 768];
    let emb2 = vec![0.5; 768];

    let node1 = WikiNode {
        id: None,
        name: "Direction 1".into(),
        content: "Always use abstract factories.".into(),
        scope: "project_a".into(),
        vault_path: Some("wiki/project_a/directions/dir3.md".into()),
        embedding: Some(emb1),
        node_type: Some("direction".into()),
        ..Default::default()
    };

    let node2 = WikiNode {
        id: None,
        name: "Direction 2".into(),
        content: "Use abstract factories for everything.".into(),
        scope: "project_b".into(),
        vault_path: Some("wiki/project_b/directions/dir4.md".into()),
        embedding: Some(emb2),
        node_type: Some("direction".into()),
        ..Default::default()
    };

    let conflict_node = WikiNode {
        id: None,
        name: "Conflict".into(),
        content: "Abstract factories cause too much indirection here.".into(),
        scope: "project_b".into(),
        vault_path: Some("wiki/project_b/conflicts/conf1.md".into()),
        node_type: Some("conflict".into()),
        ..Default::default()
    };

    backend.save_wiki_node(&node1).await.map_err(|e| { println!("Err saving node1: {:?}", e); e }).unwrap();
    let id_2 = backend.save_wiki_node(&node2).await.map_err(|e| { println!("Err saving node2: {:?}", e); e }).unwrap();
    let id_conflict = backend.save_wiki_node(&conflict_node).await.map_err(|e| { println!("Err saving conflict: {:?}", e); e }).unwrap();

    // Relate conflict node to node2
    backend.relate_nodes(&id_2, &id_conflict, None, None, None).await.unwrap();

    graduate_wisdom(&backend, &store).await.unwrap();

    let rules = backend.get_all_wisdom_rules().await.unwrap();
    assert_eq!(rules.len(), 0, "Graduation should be blocked by conflict node");
}
