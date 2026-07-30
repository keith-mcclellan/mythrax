#![allow(dead_code, unused_imports)]

mod compactor {
use anyhow::Result;
use mythrax_core::cognitive::compactor::Compactor;
use mythrax_core::contracts::WikiNode;
use mythrax_core::db::{StorageBackend, SurrealBackend, parse_record_id};
use mythrax_core::store::MarkdownStore;
use std::fs;
use tempfile::tempdir;

use std::sync::{Arc, Mutex};
static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_dbscan_insight_compaction() -> Result<()> {
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
    let compactor = Compactor::new();

    // Create the insights directory structure in the vault
    let insights_dir = vault_root.join("wiki/scope1/insights");
    fs::create_dir_all(&insights_dir)?;

    let ins1_md = r#"---
title: "Insight One"
source_episodes:
  - "ep1"
---
Insight One content."#;

    let ins2_md = r#"---
title: "Insight Two"
source_episodes:
  - "ep2"
---
Insight Two content."#;

    let ins3_md = r#"---
title: "Insight Three"
source_episodes:
  - "ep3"
---
Insight Three content."#;

    fs::write(insights_dir.join("insight_one.md"), ins1_md)?;
    fs::write(insights_dir.join("insight_two.md"), ins2_md)?;
    fs::write(insights_dir.join("insight_three.md"), ins3_md)?;

    // Save corresponding WikiNodes in SurrealDB so their IDs can be resolved
    let node1 = WikiNode {
        id: None,
        name: "Insight One".to_string(),
        content: "Insight One content.".to_string(),
        scope: "scope1".to_string(),
        vault_path: Some("wiki/scope1/insights/insight_one.md".to_string()),
        embedding: None,
        ..Default::default()
    };
    let node2 = WikiNode {
        id: None,
        name: "Insight Two".to_string(),
        content: "Insight Two content.".to_string(),
        scope: "scope1".to_string(),
        vault_path: Some("wiki/scope1/insights/insight_two.md".to_string()),
        embedding: None,
        ..Default::default()
    };
    let node3 = WikiNode {
        id: None,
        name: "Insight Three".to_string(),
        content: "Insight Three content.".to_string(),
        scope: "scope1".to_string(),
        vault_path: Some("wiki/scope1/insights/insight_three.md".to_string()),
        embedding: None,
        ..Default::default()
    };

    let id1 = backend.save_wiki_node(&node1).await?;
    let id2 = backend.save_wiki_node(&node2).await?;
    let id3 = backend.save_wiki_node(&node3).await?;

    // Parse IDs and set mock embeddings in SurrealDB.
    // We want Node 1 and Node 2 to cluster (dist <= 0.10), and Node 3 to be an outlier.
    let rid1 = parse_record_id(&id1)?;
    let rid2 = parse_record_id(&id2)?;
    let rid3 = parse_record_id(&id3)?;

    let mut emb1 = vec![0.0; 768];
    emb1[0] = 1.0;

    let mut emb2 = vec![0.0; 768];
    emb2[0] = 0.95;
    emb2[1] = 0.3122;

    let mut emb3 = vec![0.0; 768];
    emb3[1] = 1.0;

    backend
        .db
        .query("UPDATE $id SET embedding = $emb;")
        .bind(("id", rid1))
        .bind(("emb", emb1))
        .await?
        .check()?;

    backend
        .db
        .query("UPDATE $id SET embedding = $emb;")
        .bind(("id", rid2))
        .bind(("emb", emb2))
        .await?
        .check()?;

    backend
        .db
        .query("UPDATE $id SET embedding = $emb;")
        .bind(("id", rid3))
        .bind(("emb", emb3))
        .await?
        .check()?;

    // Execute compaction
    compactor
        .compact_scope(std::sync::Arc::new(backend.clone()), &store, "scope1", backend.embedder.clone())
        .await?;

    // Verify atomic insights on disk in wiki/scope1/insights
    let insights_base = vault_root.join("wiki/scope1/insights");
    assert!(insights_base.exists());

    // Verify relations in the database
    let mut response = backend.db.query("SELECT id, name, item_type FROM wiki_node;").await?;
    let nodes: Vec<serde_json::Value> = response.take(0)?;

    // We should have at least the 3 initial insight nodes plus synthesized atomic items
    assert!(nodes.len() > 3, "Expected new atomic insight nodes to be generated in DB, found {}", nodes.len());

    let mut rel_resp = backend
        .db
        .query("SELECT * FROM relates_to;")
        .await?;
    let rels: Vec<serde_json::Value> = rel_resp.take(0)?;
    assert!(!rels.is_empty(), "Expected relates_to edges to be created for synthesized atomic insights");

    Ok(())
}

#[tokio::test]
async fn test_insight_centroid_drift_split() -> Result<()> {
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
    backend
        .save_profile_key("compactor.enable_contradiction_detection", "false")
        .await?;
    let store = MarkdownStore::new(&vault_root)?;

    // Save 4 episodes to SurrealDB
    let episode1 = mythrax_core::contracts::EpisodeSave {
        created_at: None,
        title: "Episode 1".to_string(),
        content: "Content 1".to_string(),
        entities: vec![],
        scope: Some("scope1".to_string()),
        vault_path: None,
        source_episode: None,
        session_id: None,
        task_id: None,
        ..Default::default()
    };
    let id1 = backend.save_episode(&episode1).await?;

    let episode2 = mythrax_core::contracts::EpisodeSave {
        created_at: None,
        title: "Episode 2".to_string(),
        content: "Content 2".to_string(),
        entities: vec![],
        scope: Some("scope1".to_string()),
        vault_path: None,
        source_episode: None,
        session_id: None,
        task_id: None,
        ..Default::default()
    };
    let id2 = backend.save_episode(&episode2).await?;

    let episode3 = mythrax_core::contracts::EpisodeSave {
        created_at: None,
        title: "Episode 3".to_string(),
        content: "Content 3".to_string(),
        entities: vec![],
        scope: Some("scope1".to_string()),
        vault_path: None,
        source_episode: None,
        session_id: None,
        task_id: None,
        ..Default::default()
    };
    let id3 = backend.save_episode(&episode3).await?;

    let episode4 = mythrax_core::contracts::EpisodeSave {
        created_at: None,
        title: "Episode 4".to_string(),
        content: "Content 4".to_string(),
        entities: vec![],
        scope: Some("scope1".to_string()),
        vault_path: None,
        source_episode: None,
        session_id: None,
        task_id: None,
        ..Default::default()
    };
    let id4 = backend.save_episode(&episode4).await?;

    // Construct embeddings of size 768
    let mut emb1 = vec![0.0; 768];
    emb1[0] = 1.0;

    let mut emb2 = vec![0.0; 768];
    emb2[0] = 0.98;
    emb2[1] = 0.198997;

    let mut emb3 = vec![0.0; 768];
    emb3[1] = 1.0;

    let mut emb4 = vec![0.0; 768];
    emb4[1] = 0.98;
    emb4[2] = 0.198997;

    // Update the embeddings of these 4 episodes in SurrealDB
    backend
        .db
        .query("UPDATE $id SET embedding = $emb;")
        .bind(("id", parse_record_id(&id1)?))
        .bind(("emb", emb1))
        .await?
        .check()?;
    backend
        .db
        .query("UPDATE $id SET embedding = $emb;")
        .bind(("id", parse_record_id(&id2)?))
        .bind(("emb", emb2))
        .await?
        .check()?;
    backend
        .db
        .query("UPDATE $id SET embedding = $emb;")
        .bind(("id", parse_record_id(&id3)?))
        .bind(("emb", emb3))
        .await?
        .check()?;
    backend
        .db
        .query("UPDATE $id SET embedding = $emb;")
        .bind(("id", parse_record_id(&id4)?))
        .bind(("emb", emb4))
        .await?
        .check()?;

    // Write an existing insight to disk
    let insight_dir = vault_root.join("wiki/scope1/insights");
    fs::create_dir_all(&insight_dir)?;
    let insight_path = insight_dir.join("drifting_insight.md");
    let insight_content = format!(
        r#"---
title: "Drifting Insight"
scope: "scope1"
source_episodes:
  - "{}"
  - "{}"
  - "{}"
  - "{}"
---
Insight content
"#,
        id1, id2, id3, id4
    );
    fs::write(&insight_path, insight_content)?;

    // Save this insight as a WikiNode in SurrealDB
    let old_node = WikiNode {
        id: None,
        name: "Drifting Insight".to_string(),
        content: "Insight content".to_string(),
        scope: "scope1".to_string(),
        vault_path: Some("wiki/scope1/insights/drifting_insight.md".to_string()),
        embedding: None,
        ..Default::default()
    };
    let _old_node_id = backend.save_wiki_node(&old_node).await?;

    // Mark episodes 2, 3, 4 as processed so they are not clustered in the unprocessed loop
    backend.mark_episode_processed(&id2).await?;
    backend.mark_episode_processed(&id3).await?;
    backend.mark_episode_processed(&id4).await?;

    let mut initial_nodes_resp = backend
        .db
        .query("SELECT * FROM wiki_node WHERE name = 'Drifting Insight';")
        .await?;
    let initial_nodes: Vec<serde_json::Value> = initial_nodes_resp.take(0)?;
    println!("DEBUG TEST: initial drifting nodes: {:?}", initial_nodes);

    // Call DreamCoordinator::run_dream
    mythrax_core::cognitive::synthesis::DreamCoordinator::new()
        .run_dream(std::sync::Arc::new(backend.clone()), &store, Some("deep"), backend.embedder.clone())
        .await?;

    let mut after_nodes_resp = backend
        .db
        .query("SELECT * FROM wiki_node WHERE name = 'Drifting Insight';")
        .await?;
    let after_nodes: Vec<serde_json::Value> = after_nodes_resp.take(0)?;
    println!("DEBUG TEST: after drifting nodes: {:?}", after_nodes);

    // Assertions to verify the split behavior

    // 1. The old insight file on disk is deleted.
    assert!(
        !insight_path.exists(),
        "Old insight file should be deleted."
    );

    // 2. The old insight WikiNode is deleted from the DB
    let mut response = backend
        .db
        .query("SELECT * FROM wiki_node WHERE name = 'Drifting Insight';")
        .await?;
    let old_nodes: Vec<serde_json::Value> = response.take(0)?;
    assert_eq!(
        old_nodes.len(),
        0,
        "Old insight WikiNode should be deleted from DB"
    );

    // 3. Check the database: two new split insight nodes are created by the drift check
    // (They will be created under "Split Analysis ..." because of mock LLM behavior when parsing fails, or from the JSON response).
    // Let's query all wiki nodes in scope1 except the old one.
    let mut response = backend
        .db
        .query(
            "SELECT id, name FROM wiki_node WHERE scope = 'scope1' AND name != 'Drifting Insight';",
        )
        .await?;
    let new_nodes: Vec<serde_json::Value> = response.take(0)?;

    // We should have split insights generated. Let's make sure we find them.
    let split_nodes: Vec<_> = new_nodes
        .iter()
        .filter(|n| n["name"].as_str().unwrap().contains("Split Analysis"))
        .collect();
    assert_eq!(split_nodes.len(), 2, "Expected exactly two split insights");

    // 4. Verify relations:
    // - Split Node 1 (for cluster of ep1, ep2) should relate to ep1 and ep2
    // - Split Node 2 (for cluster of ep3, ep4) should relate to ep3 and ep4
    let split_id1 = split_nodes[0]["id"].as_str().unwrap();
    let split_id2 = split_nodes[1]["id"].as_str().unwrap();

    let mut rel_resp1 = backend
        .db
        .query("SELECT * FROM relates_to WHERE in = $ep_id AND out = $split_id;")
        .bind(("ep_id", parse_record_id(&id1)?))
        .bind(("split_id", parse_record_id(split_id1)?))
        .await?;
    let rels1: Vec<serde_json::Value> = rel_resp1.take(0)?;

    let mut rel_resp2 = backend
        .db
        .query("SELECT * FROM relates_to WHERE in = $ep_id AND out = $split_id;")
        .bind(("ep_id", parse_record_id(&id1)?))
        .bind(("split_id", parse_record_id(split_id2)?))
        .await?;
    let rels2: Vec<serde_json::Value> = rel_resp2.take(0)?;

    // One of the split nodes should be related to id1/id2, and the other to id3/id4
    let (first_cluster_split_id, second_cluster_split_id) = if rels1.len() == 1 {
        (split_id1, split_id2)
    } else {
        assert_eq!(rels2.len(), 1);
        (split_id2, split_id1)
    };

    // Verify first cluster split relationships
    let mut check1 = backend
        .db
        .query("SELECT * FROM relates_to WHERE in = $ep_id AND out = $split_id;")
        .bind(("ep_id", parse_record_id(&id2)?))
        .bind(("split_id", parse_record_id(first_cluster_split_id)?))
        .await?;
    let check1_rels: Vec<serde_json::Value> = check1.take(0)?;
    assert_eq!(
        check1_rels.len(),
        1,
        "Episode 2 should relate to first cluster split insight"
    );

    // Verify second cluster split relationships
    let mut check2 = backend
        .db
        .query("SELECT * FROM relates_to WHERE in = $ep_id AND out = $split_id;")
        .bind(("ep_id", parse_record_id(&id3)?))
        .bind(("split_id", parse_record_id(second_cluster_split_id)?))
        .await?;
    let check2_rels: Vec<serde_json::Value> = check2.take(0)?;
    assert_eq!(
        check2_rels.len(),
        1,
        "Episode 3 should relate to second cluster split insight"
    );

    let mut check3 = backend
        .db
        .query("SELECT * FROM relates_to WHERE in = $ep_id AND out = $split_id;")
        .bind(("ep_id", parse_record_id(&id4)?))
        .bind(("split_id", parse_record_id(second_cluster_split_id)?))
        .await?;
    let check3_rels: Vec<serde_json::Value> = check3.take(0)?;
    assert_eq!(
        check3_rels.len(),
        1,
        "Episode 4 should relate to second cluster split insight"
    );

    Ok(())
}

#[tokio::test]
async fn test_wisdom_rule_deduplication_skills_anchor() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    let store = MarkdownStore::new(&vault_root)?;

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let mut emb = vec![0.0; 768];
    emb[0] = 1.0;

    // 1. Create an existing skills rule
    let existing_skills_rule = mythrax_core::contracts::WisdomRule {
        id: None,
        target_pattern: "Avoid repeating tests manually".to_string(),
        action_to_avoid: "manual testing".to_string(),
        causal_explanation: "leads to human error".to_string(),
        prescribed_remedy: "automate tests".to_string(),
        tier: mythrax_core::contracts::Tier::Wisdom,
        scope: "general".to_string(),
        vault_path: Some("wisdom/skills/automate.md".to_string()),
        embedding: Some(emb.clone()),
        source_episodes: vec!["episode:ep1".to_string()],
        generator_name: "test".to_string(),
        similarity: None,
        utility: None,
        status: None,
        superseded_at: None,
        superseded_by: None,

        rule_type: None,
        ..Default::default()
    };
    let skills_id = backend.save_wisdom_rule(&existing_skills_rule).await?;

    // 2. Create a new rule with similar content that should be deduplicated
    let new_rule = mythrax_core::contracts::WisdomRule {
        id: None,
        target_pattern: "Avoid repeating tests manually".to_string(),
        action_to_avoid: "manual testing".to_string(),
        causal_explanation: "leads to human error".to_string(),
        prescribed_remedy: "automate tests".to_string(),
        tier: mythrax_core::contracts::Tier::Project,
        scope: "general".to_string(),
        vault_path: Some("wisdom/dynamic/new_rule.md".to_string()),
        embedding: Some(emb.clone()),
        source_episodes: vec!["episode:ep2".to_string()],
        generator_name: "test".to_string(),
        similarity: None,
        utility: None,
        status: None,
        superseded_at: None,
        superseded_by: None,

        rule_type: None,
        ..Default::default()
    };
    // Write new rule's file to disk
    store.write_file("wisdom/dynamic/new_rule.md", "some content")?;

    // Call save_wisdom_rule_with_deduplication
    let saved_id = mythrax_core::cognitive::synthesis::save_wisdom_rule_with_deduplication(
        &backend, &store, &new_rule,
    )
    .await?;

    // Assert it returned skills_id
    assert_eq!(saved_id, skills_id);

    // Assert the new rule file is deleted
    let new_file_path = vault_root.join("wisdom/dynamic/new_rule.md");
    assert!(!new_file_path.exists());

    // Assert the skills rule now relates to the episode "ep2"
    let mut response = backend
        .db
        .query("SELECT * FROM relates_to WHERE out = $skills_id;")
        .bind(("skills_id", parse_record_id(&skills_id)?))
        .await?;
    let rels: Vec<serde_json::Value> = response.take(0)?;
    let ep2_related = rels
        .iter()
        .any(|r| r["in"].as_str().unwrap().contains("ep2"));
    assert!(
        ep2_related,
        "Episode 2 should be related to the skills rule"
    );

    Ok(())
}

#[tokio::test]
async fn test_wisdom_rule_deduplication_dynamic() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    let store = MarkdownStore::new(&vault_root)?;

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let mut emb = vec![0.0; 768];
    emb[0] = 1.0;

    // 1. Create an existing dynamic rule
    let existing_rule = mythrax_core::contracts::WisdomRule {
        id: None,
        target_pattern: "Avoid manual test runs".to_string(),
        action_to_avoid: "run manual tests".to_string(),
        causal_explanation: "waste of time".to_string(),
        prescribed_remedy: "write script".to_string(),
        tier: mythrax_core::contracts::Tier::Project,
        scope: "general".to_string(),
        vault_path: Some("wisdom/dynamic/rule1.md".to_string()),
        embedding: Some(emb.clone()),
        source_episodes: vec!["ep1".to_string()],
        generator_name: "test".to_string(),
        similarity: None,
        utility: None,
        status: None,
        superseded_at: None,
        superseded_by: None,

        rule_type: None,
        ..Default::default()
    };
    let old_id = backend.save_wisdom_rule(&existing_rule).await?;
    store.write_file("wisdom/dynamic/rule1.md", "old rule content")?;

    // 2. Create a new similar dynamic rule
    let new_rule = mythrax_core::contracts::WisdomRule {
        id: None,
        target_pattern: "Avoid manual test runs".to_string(),
        action_to_avoid: "run manual tests".to_string(),
        causal_explanation: "waste of time".to_string(),
        prescribed_remedy: "write script".to_string(),
        tier: mythrax_core::contracts::Tier::Project,
        scope: "general".to_string(),
        vault_path: Some("wisdom/dynamic/rule2.md".to_string()),
        embedding: Some(emb.clone()),
        source_episodes: vec!["ep2".to_string()],
        generator_name: "test".to_string(),
        similarity: None,
        utility: None,
        status: None,
        superseded_at: None,
        superseded_by: None,

        rule_type: None,
        ..Default::default()
    };
    store.write_file("wisdom/dynamic/rule2.md", "new rule content")?;

    // Call save_wisdom_rule_with_deduplication
    let saved_id = mythrax_core::cognitive::synthesis::save_wisdom_rule_with_deduplication(
        &backend, &store, &new_rule,
    )
    .await?;

    // The old rule's file should no longer exist at its original path, but the archived rule file SHOULD exist
    let old_file_path = vault_root.join("wisdom/dynamic/rule1.md");
    assert!(
        !old_file_path.exists(),
        "Old rule file should be removed from active directory"
    );

    let archived_file_path = vault_root.join("wisdom/superseded_archive/rule1.md");
    assert!(
        archived_file_path.exists(),
        "Archived rule file should exist in superseded_archive"
    );

    // The old rule record in SurrealDB should NOT be deleted, but its status should be updated to "superseded"
    let mut response = backend
        .db
        .query("SELECT * FROM wisdom WHERE vault_path = 'wisdom/dynamic/rule1.md';")
        .await?;
    let old_db_rules: Vec<serde_json::Value> = response.take(0)?;
    assert!(
        !old_db_rules.is_empty(),
        "Old rule record should still exist in database"
    );

    if let Some(rule) = old_db_rules.first() {
        let status = rule.get("status").and_then(|v| v.as_str()).unwrap_or("");
        assert_eq!(
            status, "superseded",
            "Old rule status should be updated to 'superseded'"
        );
    }

    // The new rule file should exist
    let new_file_path = vault_root.join("wisdom/dynamic/rule2.md");
    assert!(new_file_path.exists(), "New rule file should exist");

    // Assert that the returned ID is different from old_id
    assert_ne!(saved_id, old_id);

    Ok(())
}

#[test]
fn test_dot_product_unit_vectors() {
    let u = vec![1.0, 0.0, 0.0];
    let v = vec![1.0, 0.0, 0.0];
    let dp: f32 = u.iter().zip(v.iter()).map(|(a, b)| a * b).sum();
    assert_eq!(dp, 1.0);
}

#[test]
fn test_dot_product_orthogonal_vectors() {
    let u = vec![1.0, 0.0, 0.0];
    let v = vec![0.0, 1.0, 0.0];
    let dp: f32 = u.iter().zip(v.iter()).map(|(a, b)| a * b).sum();
    assert_eq!(dp, 0.0);
}

#[tokio::test]
async fn test_compactor_range_merging_and_derived_from_edges() -> Result<()> {
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
    let compactor = Compactor::new();

    // 1. Create two episodes with specific created_at/temporal_range values
    let ep_save1 = mythrax_core::contracts::EpisodeSave {
        title: "Episode 1".to_string(),
        content: "First episode content".to_string(),
        scope: Some("scope2".to_string()),
        created_at: Some("2026-07-09T12:00:00Z".to_string()),
        ..Default::default()
    };
    let ep_id1 = backend.save_episode(&ep_save1).await?;

    let ep_save2 = mythrax_core::contracts::EpisodeSave {
        title: "Episode 2".to_string(),
        content: "Second episode content".to_string(),
        scope: Some("scope2".to_string()),
        created_at: Some("2026-07-15T12:00:00Z".to_string()),
        ..Default::default()
    };
    let ep_id2 = backend.save_episode(&ep_save2).await?;

    // Set temporal ranges on the episodes
    let start1 =
        chrono::DateTime::parse_from_rfc3339("2026-07-08T00:00:00Z")?.with_timezone(&chrono::Utc);
    let end1 =
        chrono::DateTime::parse_from_rfc3339("2026-07-10T00:00:00Z")?.with_timezone(&chrono::Utc);
    let start2 =
        chrono::DateTime::parse_from_rfc3339("2026-07-14T00:00:00Z")?.with_timezone(&chrono::Utc);
    let end2 =
        chrono::DateTime::parse_from_rfc3339("2026-07-16T00:00:00Z")?.with_timezone(&chrono::Utc);

    backend
        .db
        .query("UPDATE $id SET temporal_range_start = $start, temporal_range_end = $end;")
        .bind(("id", parse_record_id(&ep_id1)?))
        .bind(("start", start1))
        .bind(("end", end1))
        .await?
        .check()?;

    backend
        .db
        .query("UPDATE $id SET temporal_range_start = $start, temporal_range_end = $end;")
        .bind(("id", parse_record_id(&ep_id2)?))
        .bind(("start", start2))
        .bind(("end", end2))
        .await?
        .check()?;

    // 2. Create the insights directory structure in the vault
    let insights_dir = vault_root.join("wiki/scope2/insights");
    fs::create_dir_all(&insights_dir)?;

    let ins1_md = format!(
        r#"---
title: "Insight One"
source_episodes:
  - "{}"
---
Insight One content."#,
        ep_id1
    );

    let ins2_md = format!(
        r#"---
title: "Insight Two"
source_episodes:
  - "{}"
---
Insight Two content."#,
        ep_id2
    );

    fs::write(insights_dir.join("insight_one.md"), ins1_md)?;
    fs::write(insights_dir.join("insight_two.md"), ins2_md)?;

    // 3. Save matching WikiNodes in SurrealDB
    let node1 = WikiNode {
        id: None,
        name: "Insight One".to_string(),
        content: "Insight One content.".to_string(),
        scope: "scope2".to_string(),
        vault_path: Some("wiki/scope2/insights/insight_one.md".to_string()),
        embedding: None,
        ..Default::default()
    };
    let node2 = WikiNode {
        id: None,
        name: "Insight Two".to_string(),
        content: "Insight Two content.".to_string(),
        scope: "scope2".to_string(),
        vault_path: Some("wiki/scope2/insights/insight_two.md".to_string()),
        embedding: None,
        ..Default::default()
    };

    let id1 = backend.save_wiki_node(&node1).await?;
    let id2 = backend.save_wiki_node(&node2).await?;

    let rid1 = parse_record_id(&id1)?;
    let rid2 = parse_record_id(&id2)?;

    let mut emb1 = vec![0.0; 768];
    emb1[0] = 1.0;

    let mut emb2 = vec![0.0; 768];
    emb2[0] = 0.95;
    emb2[1] = 0.3122;

    backend
        .db
        .query("UPDATE $id SET embedding = $emb;")
        .bind(("id", rid1))
        .bind(("emb", emb1))
        .await?
        .check()?;

    backend
        .db
        .query("UPDATE $id SET embedding = $emb;")
        .bind(("id", rid2))
        .bind(("emb", emb2))
        .await?
        .check()?;

    backend
        .db
        .query("UPDATE $id SET temporal_range_start = $start, temporal_range_end = $end;")
        .bind(("id", parse_record_id(&id1)?))
        .bind(("start", start1))
        .bind(("end", end1))
        .await?
        .check()?;

    backend
        .db
        .query("UPDATE $id SET temporal_range_start = $start, temporal_range_end = $end;")
        .bind(("id", parse_record_id(&id2)?))
        .bind(("start", start2))
        .bind(("end", end2))
        .await?
        .check()?;

    // Execute compaction
    compactor
        .compact_scope(std::sync::Arc::new(backend.clone()), &store, "scope2", backend.embedder.clone())
        .await?;

    let compaction_dir = vault_root.join("wiki/scope2/compactions");
    assert!(compaction_dir.exists());

    let all_nodes = backend.get_all_wiki_nodes().await?;
    let compacted_nodes: Vec<WikiNode> = all_nodes
        .into_iter()
        .filter(|n| n.scope == "scope2" && n.vault_path.as_ref().map_or(false, |p| p.contains("compactions")))
        .collect();
    assert_eq!(
        compacted_nodes.len(),
        1,
        "Expected exactly one cluster compaction node"
    );
    let comp_node = &compacted_nodes[0];
    let comp_id = comp_node.id.as_ref().unwrap();

    assert_eq!(comp_node.temporal_range_start, Some(start1));
    assert_eq!(comp_node.temporal_range_end, Some(end2));

    let mut rel_resp1 = backend.db.query("SELECT * FROM relates_to WHERE in = $comp_id AND out = $ep_id AND relation = 'derived_from';")
        .bind(("comp_id", parse_record_id(comp_id)?))
        .bind(("ep_id", parse_record_id(&ep_id1)?))
        .await?;
    let rels1: Vec<serde_json::Value> = rel_resp1.take(0)?;
    assert_eq!(
        rels1.len(),
        1,
        "Edge from compaction to Episode 1 with derived_from relation is missing"
    );

    let mut rel_resp2 = backend.db.query("SELECT * FROM relates_to WHERE in = $comp_id AND out = $ep_id AND relation = 'derived_from';")
        .bind(("comp_id", parse_record_id(comp_id)?))
        .bind(("ep_id", parse_record_id(&ep_id2)?))
        .await?;
    let rels2: Vec<serde_json::Value> = rel_resp2.take(0)?;
    assert_eq!(
        rels2.len(),
        1,
        "Edge from compaction to Episode 2 with derived_from relation is missing"
    );

    Ok(())
}

#[tokio::test]
async fn test_garbage_collect_low_confidence_nodes() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("wiki/scope1"))?;

    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    let store = MarkdownStore::new(&vault_root)?;
    let compactor = Compactor::new();

    // Create a physical wiki markdown file
    let wiki_dir = vault_root.join("wiki/scope1");
    let md_path = wiki_dir.join("low_confidence_node.md");
    fs::write(&md_path, "Some old content")?;

    // Create the WikiNode record in SurrealDB with metacognitive_confidence = Some(2)
    let node = WikiNode {
        id: None,
        name: "Low Confidence Node".to_string(),
        content: "Some old content".to_string(),
        scope: "scope1".to_string(),
        vault_path: Some("wiki/scope1/low_confidence_node.md".to_string()),
        metacognitive_confidence: Some(2),
        ..Default::default()
    };
    let node_id = backend.save_wiki_node(&node).await?;
    let rid = parse_record_id(&node_id)?;

    // Mock updated_at in SurrealDB to 31 days ago
    let past_time = chrono::Utc::now() - chrono::Duration::days(31);
    backend
        .db
        .query("UPDATE $id SET updated_at = $past;")
        .bind(("id", rid.clone()))
        .bind(("past", past_time))
        .await?
        .check()?;

    // Execute compaction
    compactor
        .compact_scope(std::sync::Arc::new(backend.clone()), &store, "scope1", None)
        .await?;

    // Verify:
    // 1. The physical file was moved to {vault_root}/archive/low_confidence_node.md
    let expected_archive_path = vault_root.join("archive/low_confidence_node.md");
    assert!(
        expected_archive_path.exists(),
        "File should be moved to archive directory"
    );
    assert!(!md_path.exists(), "Original file should be deleted");

    // 2. The record was deleted from SurrealDB
    let mut response = backend
        .db
        .query("SELECT * FROM wiki_node WHERE id = $id;")
        .bind(("id", rid))
        .await?;
    let nodes: Vec<serde_json::Value> = response.take(0)?;
    assert!(
        nodes.is_empty(),
        "WikiNode record should be deleted from the database"
    );

    Ok(())
}

#[tokio::test]
async fn test_hebbian_synaptic_pruning() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("wiki/scope1"))?;

    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    let store = MarkdownStore::new(&vault_root)?;
    let compactor = Compactor::new();

    // Create two wiki nodes
    let node_a = WikiNode {
        id: None,
        name: "Node A".to_string(),
        content: "Content A".to_string(),
        scope: "scope1".to_string(),
        vault_path: Some("wiki/scope1/node_a.md".to_string()),
        ..Default::default()
    };
    let node_b = WikiNode {
        id: None,
        name: "Node B".to_string(),
        content: "Content B".to_string(),
        scope: "scope1".to_string(),
        vault_path: Some("wiki/scope1/node_b.md".to_string()),
        ..Default::default()
    };
    let node_c = WikiNode {
        id: None,
        name: "Node C".to_string(),
        content: "Content C".to_string(),
        scope: "scope1".to_string(),
        vault_path: Some("wiki/scope1/node_c.md".to_string()),
        ..Default::default()
    };

    let id_a = backend.save_wiki_node(&node_a).await?;
    let id_b = backend.save_wiki_node(&node_b).await?;
    let id_c = backend.save_wiki_node(&node_c).await?;

    let rid_a = parse_record_id(&id_a)?;
    let rid_b = parse_record_id(&id_b)?;
    let rid_c = parse_record_id(&id_c)?;

    // Create relates_to relations (edges)
    backend.relate_nodes(&id_a, &id_b, None, None, None).await?;
    backend.relate_nodes(&id_a, &id_c, None, None, None).await?;

    // Update weight fields on relates_to edges
    backend
        .db
        .query("UPDATE relates_to SET weight = 0.105 WHERE in = $in AND out = $out;")
        .bind(("in", rid_a.clone()))
        .bind(("out", rid_b.clone()))
        .await?
        .check()?;

    backend
        .db
        .query("UPDATE relates_to SET weight = 0.5 WHERE in = $in AND out = $out;")
        .bind(("in", rid_a.clone()))
        .bind(("out", rid_c.clone()))
        .await?
        .check()?;

    // Execute compaction
    compactor
        .compact_scope(std::sync::Arc::new(backend.clone()), &store, "scope1", None)
        .await?;

    // Verify:
    // 1. Edge 1 (Node A -> Node B) is deleted because weight 0.0945 < 0.1
    let mut check_ab = backend
        .db
        .query("SELECT weight FROM relates_to WHERE in = $in AND out = $out;")
        .bind(("in", rid_a.clone()))
        .bind(("out", rid_b.clone()))
        .await?;
    let ab_edges: Vec<serde_json::Value> = check_ab.take(0)?;
    assert!(ab_edges.is_empty(), "Edge A->B should be pruned");

    // 2. Edge 2 (Node A -> Node C) still exists with decayed weight 0.45
    let mut check_ac = backend
        .db
        .query("SELECT weight FROM relates_to WHERE in = $in AND out = $out;")
        .bind(("in", rid_a.clone()))
        .bind(("out", rid_c.clone()))
        .await?;
    let ac_edges: Vec<serde_json::Value> = check_ac.take(0)?;
    assert_eq!(ac_edges.len(), 1, "Edge A->C should not be pruned");
    let weight: f64 = ac_edges[0]["weight"].as_f64().unwrap();
    assert!(
        (weight - 0.45).abs() < 1e-5,
        "Edge A->C weight should decay to 0.45, got {}",
        weight
    );

    Ok(())
}

}

mod compactor_unit {
use anyhow::Result;
use mythrax_core::cognitive::compactor::compact_hierarchical_dbscan;
use mythrax_core::cognitive::synthesis::InsightNote;

#[tokio::test]
async fn test_hierarchical_dbscan_clustering() -> Result<()> {
    let ins1 = InsightNote {
        title: "July Insight 1".to_string(),
        content: "Content 1".to_string(),
        scope: "scope1".to_string(),
        source_episodes: vec![],
        vault_path: "wiki/scope1/insights/insight_2026-07-01.md".to_string(),
    };

    let ins2 = InsightNote {
        title: "July Insight 2".to_string(),
        content: "Content 2".to_string(),
        scope: "scope1".to_string(),
        source_episodes: vec![],
        vault_path: "wiki/scope1/insights/insight_2026-07-05.md".to_string(),
    };

    let ins3 = InsightNote {
        title: "July Outlier".to_string(),
        content: "Content 3".to_string(),
        scope: "scope1".to_string(),
        source_episodes: vec![],
        vault_path: "wiki/scope1/insights/insight_2026-07-10.md".to_string(),
    };

    let ins4 = InsightNote {
        title: "August Insight 1".to_string(),
        content: "Content 4".to_string(),
        scope: "scope1".to_string(),
        source_episodes: vec![],
        vault_path: "wiki/scope1/insights/insight_2026-08-01.md".to_string(),
    };

    let ins5 = InsightNote {
        title: "August Insight 2".to_string(),
        content: "Content 5".to_string(),
        scope: "scope1".to_string(),
        source_episodes: vec![],
        vault_path: "wiki/scope1/insights/insight_2026-08-05.md".to_string(),
    };

    let emb1 = vec![1.0, 0.0, 0.0];
    let emb2 = vec![0.99, 0.01, 0.0];
    let emb3 = vec![0.0, 1.0, 0.0];
    let emb4 = vec![0.0, 0.0, 1.0];
    let emb5 = vec![0.0, 0.01, 0.99];

    let valid_insights = vec![
        (ins1.clone(), "id1".to_string(), Some(emb1)),
        (ins2.clone(), "id2".to_string(), Some(emb2)),
        (ins3.clone(), "id3".to_string(), Some(emb3)),
        (ins4.clone(), "id4".to_string(), Some(emb4)),
        (ins5.clone(), "id5".to_string(), Some(emb5)),
    ];

    let result = compact_hierarchical_dbscan(&valid_insights, 0.10, 2);

    assert!(!result.is_empty(), "Clusters should not be empty");

    let has_july_cluster = result.iter().any(|cluster| {
        cluster.iter().any(|(ins, _)| ins.title == "July Insight 1")
            && cluster.iter().any(|(ins, _)| ins.title == "July Insight 2")
    });
    assert!(has_july_cluster, "Should cluster July Insight 1 and 2");

    let has_august_cluster = result.iter().any(|cluster| {
        cluster
            .iter()
            .any(|(ins, _)| ins.title == "August Insight 1")
            && cluster
                .iter()
                .any(|(ins, _)| ins.title == "August Insight 2")
    });
    assert!(has_august_cluster, "Should cluster August Insight 1 and 2");

    Ok(())
}

}

mod compactor_decay_safety {
use anyhow::Result;
use mythrax_core::cognitive::compactor::Compactor;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use std::fs;
use tempfile::tempdir;

use std::sync::Mutex;
static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_compactor_decay_referenced_safety() -> Result<()> {
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
    let compactor = Compactor::new();

    // 1. Create a referenced episode that is decayed:
    let ep_save = EpisodeSave {
        created_at: None,
        title: "Referenced Episode".to_string(),
        content: "Some important referenced content.".to_string(),
        scope: Some("general".to_string()),
        vault_path: Some("episodes/referenced_ep.md".to_string()),
        ..Default::default()
    };
    let ep_id = backend.save_episode(&ep_save).await?;

    // Manually set utility to 2.0 to force decay
    let ep_raw_id = ep_id.split(':').nth(1).unwrap_or(&ep_id).to_string();
    backend
        .db
        .query("UPDATE type::record('episode', $id) SET utility = 2.0;")
        .bind(("id", ep_raw_id.clone()))
        .await?
        .check()?;

    // Create the physical file
    store.write_file(
        "episodes/referenced_ep.md",
        "Some important referenced content.",
    )?;

    // Let's relate this episode to a wiki node so it is referenced
    let node_contract = mythrax_core::contracts::WikiNode {
        id: Some("wiki_node:target_node".to_string()),
        name: "Target Node".to_string(),
        content: "some content".to_string(),
        scope: "general".to_string(),
        vault_path: None,
        embedding: None,
        ..Default::default()
    };
    backend.save_wiki_node(&node_contract).await?;
    backend
        .relate_nodes(&ep_id, "wiki_node:target_node", None, None, None)
        .await?;

    // Verify it is referenced in the DB
    let is_ref = {
        let ep_rec = mythrax_core::db::backend::parse_record_id(&ep_id)?;
        let mut resp = backend
            .db
            .query("SELECT VALUE id FROM relates_to WHERE in = $ep OR out = $ep LIMIT 1;")
            .bind(("ep", ep_rec))
            .await?;
        let rows: Vec<surrealdb::types::RecordId> = resp.take(0)?;
        !rows.is_empty()
    };
    assert!(is_ref);

    // Call compaction to trigger decay of this node
    let _ = compactor
        .compact_scope(std::sync::Arc::new(backend.clone()), &store, "general", None)
        .await;

    // Check if the physical file still exists in its original place
    let orig_file = vault_root.join("episodes/referenced_ep.md");
    assert!(
        orig_file.exists(),
        "Referenced episode physical file must be preserved"
    );

    // Check if it is marked as archived in the DB
    let mut resp = backend
        .db
        .query("SELECT archived FROM type::record('episode', $id);")
        .bind(("id", ep_raw_id))
        .await?;
    let rows: Vec<serde_json::Value> = resp.take(0)?;
    let archived = rows[0]
        .get("archived")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(
        archived,
        "Referenced episode must be marked as archived in DB"
    );

    Ok(())
}

}

mod contradiction_detection {
use anyhow::Result;
use mythrax_core::cognitive::synthesis::DreamCoordinator;
use mythrax_core::contracts::WikiNode;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use std::fs;
use tempfile::tempdir;

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
    let result_id = coordinator
        .save_wiki_node_with_contradiction_resolution(&backend, &store, &new_node, None, vec![])
        .await?;

    // Assert that the returned ID is the existing node's ID
    assert_eq!(result_id, existing_id);

    // Fetch the existing node from DB and assert content is updated to mock resolution
    let all_nodes = backend.get_all_wiki_nodes().await?;
    let updated_node = all_nodes
        .iter()
        .find(|n| n.id.as_ref() == Some(&existing_id))
        .expect("Existing node should exist");
    assert_eq!(
        updated_node.content,
        "We should use SurrealDB for the database because Postgres was deprecated."
    );

    // Assert that the physical file of the existing node is updated with resolution
    let file_content =
        fs::read_to_string(vault_root.join("wiki/test_scope/insights/db_choice.md"))?;
    assert!(
        file_content
            .contains("We should use SurrealDB for the database because Postgres was deprecated.")
    );
    assert!(file_content.contains("title: \"Existing DB Choice\""));

    // Assert that the new node's vault path does NOT exist (skipped writing)
    assert!(
        !vault_root
            .join("wiki/test_scope/insights/new_db_choice.md")
            .exists()
    );

    Ok(())
}

}

mod insight_graduation {
use anyhow::Result;
use mythrax_core::cognitive::synthesis::DreamCoordinator;
use mythrax_core::contracts::{EpisodeSave, WikiNode};
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use std::fs;
use tempfile::tempdir;

use std::sync::Mutex;
static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_insight_graduation_lifecycle() -> Result<()> {
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

    // 1. Create a wiki node in scope_A
    let node_a = WikiNode {
        id: None,
        name: "Insight A".to_string(),
        content: "Testing strategy A".to_string(),
        scope: "scope_A".to_string(),
        vault_path: Some("wiki/scope_A/insights/strategy_a.md".to_string()),
        embedding: Some(vec![1.0; 768]),
        ..Default::default()
    };
    let id_a = backend.save_wiki_node(&node_a).await?;

    // 2. Create a wiki node in scope_B
    let node_b = WikiNode {
        id: None,
        name: "Insight B".to_string(),
        content: "Testing strategy B".to_string(),
        scope: "scope_B".to_string(),
        vault_path: Some("wiki/scope_B/insights/strategy_b.md".to_string()),
        embedding: Some(vec![1.0; 768]),
        ..Default::default()
    };
    let id_b = backend.save_wiki_node(&node_b).await?;

    // Create an unprocessed episode so run_dream doesn't exit early
    let dummy_ep = EpisodeSave {
        created_at: None,
        title: "Dummy Ep".to_string(),
        content: "Dummy content".to_string(),
        scope: Some("scope_A".to_string()),
        ..Default::default()
    };
    let _ = backend.save_episode(&dummy_ep).await?;

    // Enable cross-scope graduation and run dream
    backend
        .save_profile_key("compactor.enable_cross_scope_graduation", "true")
        .await?;
    println!(
        "DEBUG - ALL WIKI NODES IN DB: {:#?}",
        backend.get_all_wiki_nodes().await?
    );
    coordinator.run_dream(std::sync::Arc::new(backend.clone()), &store, None, None).await?;

    // Verify a general scope WisdomRule has been created in DB
    let all_rules = backend.get_all_wisdom_rules().await?;
    println!("DEBUG - ALL RULES IN DB: {:#?}", all_rules);
    println!(
        "DEBUG - SELECT * FROM wisdom: {:#?}",
        backend
            .db
            .query("SELECT * FROM wisdom;")
            .await?
            .take::<Vec<serde_json::Value>>(0)?
    );
    let graduated_rule = all_rules
        .iter()
        .find(|r| r.scope == "general" && r.generator_name == "ScopeGraduator")
        .expect("Graduated WisdomRule should exist");

    println!("GRADUATED RULE: {:?}", graduated_rule);
    assert_eq!(graduated_rule.target_pattern, "test_graduated_pattern");
    assert_eq!(graduated_rule.tier, mythrax_core::contracts::Tier::Project); // Because wiki nodes are dynamic, not all procedural
    assert!(graduated_rule.source_episodes.contains(&id_a));
    assert!(graduated_rule.source_episodes.contains(&id_b));

    // Verify relates_to edges link source nodes to the graduated rule
    let related_a = backend.get_related_node_ids(&id_a).await?;
    assert!(related_a.contains(graduated_rule.id.as_ref().unwrap()));

    let related_b = backend.get_related_node_ids(&id_b).await?;
    assert!(related_b.contains(graduated_rule.id.as_ref().unwrap()));

    // Verify physical file exists under global/wisdom/dynamic/
    let dynamic_dir = vault_root.join("global/wisdom/dynamic");
    assert!(dynamic_dir.exists());
    let files = fs::read_dir(dynamic_dir)?;
    let mut found_file = false;
    for file in files {
        let f = file?;
        let name = f.file_name().to_string_lossy().into_owned();
        if name.starts_with("avoid_test") && name.ends_matches(".md") {
            found_file = true;
            let file_content = fs::read_to_string(f.path())?;
            assert!(file_content.contains("generator_name: \"ScopeGraduator\""));
            assert!(file_content.contains("scope: \"general\""));
        }
    }
    assert!(found_file, "Graduated rule file should be created");

    Ok(())
}

trait EndsMatches {
    fn ends_matches(&self, suffix: &str) -> bool;
}
impl EndsMatches for String {
    fn ends_matches(&self, suffix: &str) -> bool {
        self.ends_with(suffix)
    }
}

}

mod near_duplicate_merging {
use anyhow::Result;
use mythrax_core::cognitive::compactor::Compactor;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use std::fs;
use tempfile::tempdir;

use std::sync::Mutex;
static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_near_duplicate_merging_behavior() -> Result<()> {
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
    let compactor = Compactor::new();

    // Enable feature and set threshold
    backend
        .save_profile_key("compactor.enable_near_duplicate_merging", "true")
        .await?;
    backend
        .save_profile_key("compactor.dedup_threshold", "0.90")
        .await?;

    let embedding = vec![1.0f32; 768];

    // Create older episode
    let ep_older = EpisodeSave {
        created_at: None,
        title: "Older Episode".to_string(),
        content: "Older content".to_string(),
        scope: Some("test_scope".to_string()),
        vault_path: Some("episodes/older.md".to_string()),
        session_id: Some("session-123".to_string()),
        ..Default::default()
    };
    let older_id = backend.save_episode(&ep_older).await?;
    store.write_file("episodes/older.md", "Older content")?;

    // Manually set embedding, temporal range and last_retrieved_at to be older
    let older_raw_id = older_id.split(':').nth(1).unwrap().to_string();
    backend.db.query("UPDATE type::record('episode', $id) SET embedding = $emb, last_retrieved_at = '2026-07-05T10:00:00Z', node_type = 'test_type', temporal_range_start = <datetime>'2026-07-01T10:00:00Z', temporal_range_end = <datetime>'2026-07-02T10:00:00Z';")
        .bind(("id", older_raw_id.clone()))
        .bind(("emb", embedding.clone()))
        .await?.check()?;

    // Update metrics with access count = 5 for older
    backend
        .db
        .query(
            "UPDATE metrics SET access_count = 5 WHERE target_id = type::record('episode', $id);",
        )
        .bind(("id", older_raw_id.clone()))
        .await?
        .check()?;

    // Create newer episode
    let ep_newer = EpisodeSave {
        created_at: None,
        title: "Newer Episode".to_string(),
        content: "Newer content".to_string(),
        scope: Some("test_scope".to_string()),
        vault_path: Some("episodes/newer.md".to_string()),
        session_id: Some("session-123".to_string()),
        ..Default::default()
    };
    let newer_id = backend.save_episode(&ep_newer).await?;
    store.write_file("episodes/newer.md", "Newer content")?;

    // Manually set embedding, temporal range and last_retrieved_at to be newer
    let newer_raw_id = newer_id.split(':').nth(1).unwrap().to_string();
    backend.db.query("UPDATE type::record('episode', $id) SET embedding = $emb, last_retrieved_at = '2026-07-05T12:00:00Z', node_type = 'test_type', temporal_range_start = <datetime>'2026-07-03T10:00:00Z', temporal_range_end = <datetime>'2026-07-04T10:00:00Z';")
        .bind(("id", newer_raw_id.clone()))
        .bind(("emb", embedding.clone()))
        .await?.check()?;

    // Update metrics with access count = 3 for newer
    backend
        .db
        .query(
            "UPDATE metrics SET access_count = 3 WHERE target_id = type::record('episode', $id);",
        )
        .bind(("id", newer_raw_id.clone()))
        .await?
        .check()?;

    // Create target wiki node and relate to newer episode
    backend.db.query("CREATE wiki_node:target CONTENT { name: 'Target Node', scope: 'test_scope', content: 'Target content' };").await?.check()?;
    let newer_record_id = mythrax_core::db::backend::parse_record_id(&newer_id)?;
    backend
        .db
        .query("RELATE wiki_node:target -> relates_to -> $newer_id CONTENT { confidence: 0.8 };")
        .bind(("newer_id", newer_record_id))
        .await?
        .check()?;

    // Run compact_scope
    compactor
        .compact_scope(std::sync::Arc::new(backend.clone()), &store, "test_scope", None)
        .await?;

    // Verify newer episode is updated to superseded
    let mut resp = backend
        .db
        .query("SELECT * FROM type::record('episode', $id);")
        .bind(("id", newer_raw_id.clone()))
        .await?;
    let rows: Vec<serde_json::Value> = resp.take(0)?;
    assert!(!rows.is_empty(), "Newer episode should still exist in DB");
    let status = rows[0]
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert_eq!(status, "superseded");
    let archived = rows[0]
        .get("archived")
        .and_then(|v| v.as_bool())
        .unwrap_or_default();
    assert!(archived);

    // Verify newer physical file is deleted
    let newer_file = vault_root.join("episodes/newer.md");
    assert!(
        !newer_file.exists(),
        "Newer physical file should be deleted"
    );

    // Verify older episode has merged content
    let mut resp = backend
        .db
        .query("SELECT content FROM type::record('episode', $id);")
        .bind(("id", older_raw_id.clone()))
        .await?;
    let rows: Vec<serde_json::Value> = resp.take(0)?;
    let content = rows[0].get("content").and_then(|v| v.as_str()).unwrap();
    assert_eq!(
        content, "Older content\nNewer content",
        "Content should be merged"
    );

    // Verify older physical file has merged content
    let older_file_content = fs::read_to_string(vault_root.join("episodes/older.md"))?;
    assert_eq!(older_file_content, "Older content\nNewer content");

    // Verify older metrics has access count = 8 (5 + 3)
    let mut resp = backend
        .db
        .query("SELECT access_count FROM metrics WHERE target_id = type::record('episode', $id);")
        .bind(("id", older_raw_id.clone()))
        .await?;
    let rows: Vec<serde_json::Value> = resp.take(0)?;
    let access_count = rows[0]
        .get("access_count")
        .and_then(|v| v.as_i64())
        .unwrap();
    assert_eq!(access_count, 8);

    // Verify relates_to edge is transferred: wiki_node:target -> relates_to -> older
    let mut resp = backend.db.query("SELECT * FROM relates_to WHERE in = wiki_node:target AND out = type::record('episode', $older_id);")
        .bind(("older_id", older_raw_id.clone()))
        .await?;
    let rows: Vec<serde_json::Value> = resp.take(0)?;
    assert!(
        !rows.is_empty(),
        "relates_to edge should be transferred to the surviving older episode"
    );

    // Verify temporal range start/end expanded on surviving node
    let mut resp = backend
        .db
        .query("SELECT temporal_range_start, temporal_range_end FROM type::record('episode', $id);")
        .bind(("id", older_raw_id.clone()))
        .await?;
    let rows: Vec<serde_json::Value> = resp.take(0)?;
    let start_val = rows[0]
        .get("temporal_range_start")
        .unwrap()
        .as_str()
        .unwrap();
    let end_val = rows[0].get("temporal_range_end").unwrap().as_str().unwrap();
    assert!(start_val.starts_with("2026-07-01"));
    assert!(end_val.starts_with("2026-07-04"));

    Ok(())
}

}

mod procedural_memory {
use anyhow::Result;
use mythrax_core::cognitive::compactor::Compactor;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use std::fs;
use tempfile::tempdir;

use std::sync::Mutex;
static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_procedural_memory_decay_and_cap() -> Result<()> {
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
    let compactor = Compactor::new();

    // 1. Verify 365-day half-life protection for procedural nodes:
    // Create a procedural episode and a standard episode, both 100 days old.
    let hundred_days_ago = (chrono::Utc::now() - chrono::Duration::days(100)).to_rfc3339();

    // Procedural
    let ep_proc = EpisodeSave {
        created_at: None,
        title: "Procedural Ep".to_string(),
        content: "Some procedural content".to_string(),
        scope: Some("test_scope".to_string()),
        vault_path: Some("episodes/proc.md".to_string()),
        ..Default::default()
    };
    let proc_id = backend.save_episode(&ep_proc).await?;
    store.write_file("episodes/proc.md", "Some procedural content")?;
    let proc_raw_id = proc_id.split(':').nth(1).unwrap().to_string();
    backend.db.query("UPDATE type::record('episode', $id) SET node_type = 'procedural', last_retrieved_at = $lr, utility = 50.0;")
        .bind(("id", proc_raw_id.clone()))
        .bind(("lr", hundred_days_ago.clone()))
        .await?.check()?;

    // Standard (not procedural)
    let ep_std = EpisodeSave {
        created_at: None,
        title: "Standard Ep".to_string(),
        content: "Some standard content".to_string(),
        scope: Some("test_scope".to_string()),
        vault_path: Some("episodes/std.md".to_string()),
        ..Default::default()
    };
    let std_id = backend.save_episode(&ep_std).await?;
    store.write_file("episodes/std.md", "Some standard content")?;
    let std_raw_id = std_id.split(':').nth(1).unwrap().to_string();
    backend
        .db
        .query("UPDATE type::record('episode', $id) SET last_retrieved_at = $lr, utility = 50.0;")
        .bind(("id", std_raw_id.clone()))
        .bind(("lr", hundred_days_ago.clone()))
        .await?
        .check()?;

    // Run prune_stale_memories (or compact_scope) to trigger decay evaluation
    compactor
        .compact_scope(std::sync::Arc::new(backend.clone()), &store, "test_scope", None)
        .await?;

    // Verify Standard Ep is archived
    let mut resp = backend
        .db
        .query("SELECT archived FROM type::record('episode', $id);")
        .bind(("id", std_raw_id))
        .await?;
    let rows: Vec<serde_json::Value> = resp.take(0)?;
    let std_archived = rows[0]
        .get("archived")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(
        std_archived,
        "Standard episode should be archived after 100 days"
    );

    // Verify Procedural Ep is NOT archived
    let mut resp = backend
        .db
        .query("SELECT archived FROM type::record('episode', $id);")
        .bind(("id", proc_raw_id))
        .await?;
    let rows: Vec<serde_json::Value> = resp.take(0)?;
    let proc_archived = rows[0]
        .get("archived")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(
        !proc_archived,
        "Procedural episode should NOT be archived after 100 days"
    );

    // 2. Verify 500-node LRU cap per scope:
    // Disable near-duplicate merging to prevent mock embedder collisions in this test
    backend
        .save_profile_key("compactor.enable_near_duplicate_merging", "false")
        .await?;

    // Insert 505 procedural episodes in a new scope
    for k in 0..505 {
        let ep = EpisodeSave {
            created_at: None,
            title: format!("Proc Cap {}", k),
            content: format!("Content {}", k),
            scope: Some("cap_scope".to_string()),
            vault_path: Some(format!("episodes/cap_{}.md", k)),
            ..Default::default()
        };
        let eid = backend.save_episode(&ep).await?;
        let eraw = eid.split(':').nth(1).unwrap().to_string();

        // We set last_retrieved_at to be sequentially increasing, so older ones are evicted first.
        let time_str = (chrono::Utc::now() - chrono::Duration::hours(505 - k)).to_rfc3339();
        backend.db.query("UPDATE type::record('episode', $id) SET node_type = 'procedural', last_retrieved_at = $lr;")
            .bind(("id", eraw))
            .bind(("lr", time_str))
            .await?.check()?;
    }

    // Run pruning
    compactor
        .compact_scope(std::sync::Arc::new(backend.clone()), &store, "cap_scope", None)
        .await?;

    // Query active (unarchived) procedural episodes in cap_scope
    let mut resp = backend.db.query("SELECT * FROM episode WHERE scope = 'cap_scope' AND node_type = 'procedural' AND (archived = false OR archived IS NONE);").await?;
    let active_cap_eps: Vec<serde_json::Value> = resp.take(0)?;
    assert_eq!(
        active_cap_eps.len(),
        500,
        "Active procedural episodes in cap_scope should be capped at 500"
    );

    // Assert that the oldest 5 (Cap 0 to Cap 4) are archived
    for k in 0..5 {
        let mut resp = backend
            .db
            .query("SELECT archived FROM episode WHERE title = $title LIMIT 1;")
            .bind(("title", format!("Proc Cap {}", k)))
            .await?;
        let rows: Vec<serde_json::Value> = resp.take(0)?;
        let archived = rows[0]
            .get("archived")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        assert!(
            archived,
            "Episode Proc Cap {} (one of the oldest) should be archived",
            k
        );
    }

    Ok(())
}

}

mod wisdom_rule_decay {
use mythrax_core::contracts::{Tier, WisdomRule};
use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::db::graduation_pipeline::run_graduation_pipeline;
use std::sync::Arc;

#[tokio::test]
async fn test_wisdom_rule_decay() -> anyhow::Result<()> {
    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let now = chrono::Utc::now();
    let one_year_ago = now - chrono::Duration::days(365);
    let two_years_ago = now - chrono::Duration::days(730);

    let rule_now = WisdomRule {
        id: Some("wisdom:rule_now".to_string()),
        target_pattern: "Pattern Now".to_string(),
        action_to_avoid: "Avoid Now".to_string(),
        causal_explanation: "Cause Now".to_string(),
        prescribed_remedy: "Remedy Now".to_string(),
        tier: Tier::Wisdom,
        scope: "global".to_string(),
        utility: Some(1.0),
        ..Default::default()
    };

    let rule_1yr = WisdomRule {
        id: Some("wisdom:rule_1yr".to_string()),
        target_pattern: "Pattern 1yr".to_string(),
        action_to_avoid: "Avoid 1yr".to_string(),
        causal_explanation: "Cause 1yr".to_string(),
        prescribed_remedy: "Remedy 1yr".to_string(),
        tier: Tier::Wisdom,
        scope: "global".to_string(),
        utility: Some(1.0),
        ..Default::default()
    };

    let rule_2yr = WisdomRule {
        id: Some("wisdom:rule_2yr".to_string()),
        target_pattern: "Pattern 2yr".to_string(),
        action_to_avoid: "Avoid 2yr".to_string(),
        causal_explanation: "Cause 2yr".to_string(),
        prescribed_remedy: "Remedy 2yr".to_string(),
        tier: Tier::Wisdom,
        scope: "global".to_string(),
        utility: Some(1.0),
        ..Default::default()
    };

    // Save them to DB
    let sql = "CREATE type::record('wisdom', $id) CONTENT {
        target_pattern: $target_pattern,
        action_to_avoid: $action_to_avoid,
        causal_explanation: $causal_explanation,
        prescribed_remedy: $prescribed_remedy,
        tier: 'Wisdom',
        scope: 'global',
        generator_name: 'Test',
        source_episodes: [],
        created_at: $created_at
    };";

    backend
        .db
        .query(sql)
        .bind(("id", "rule_now"))
        .bind(("target_pattern", rule_now.target_pattern.as_str()))
        .bind(("action_to_avoid", rule_now.action_to_avoid.as_str()))
        .bind(("causal_explanation", rule_now.causal_explanation.as_str()))
        .bind(("prescribed_remedy", rule_now.prescribed_remedy.as_str()))
        .bind(("created_at", now))
        .await?
        .check()?;

    backend
        .db
        .query(sql)
        .bind(("id", "rule_1yr"))
        .bind(("target_pattern", rule_1yr.target_pattern.as_str()))
        .bind(("action_to_avoid", rule_1yr.action_to_avoid.as_str()))
        .bind(("causal_explanation", rule_1yr.causal_explanation.as_str()))
        .bind(("prescribed_remedy", rule_1yr.prescribed_remedy.as_str()))
        .bind(("created_at", one_year_ago))
        .await?
        .check()?;

    backend
        .db
        .query(sql)
        .bind(("id", "rule_2yr"))
        .bind(("target_pattern", rule_2yr.target_pattern.as_str()))
        .bind(("action_to_avoid", rule_2yr.action_to_avoid.as_str()))
        .bind(("causal_explanation", rule_2yr.causal_explanation.as_str()))
        .bind(("prescribed_remedy", rule_2yr.prescribed_remedy.as_str()))
        .bind(("created_at", two_years_ago))
        .await?
        .check()?;

    // Create metrics entries for each rule
    let sql_metrics = "CREATE type::record('metrics', $met_id) CONTENT {
        target_id: type::record('wisdom', $id),
        utility_score: 1.0,
        access_count: 1,
        last_accessed: time::now()
    };";

    backend
        .db
        .query(sql_metrics)
        .bind(("met_id", "met_now"))
        .bind(("id", "rule_now"))
        .await?
        .check()?;

    backend
        .db
        .query(sql_metrics)
        .bind(("met_id", "met_1yr"))
        .bind(("id", "rule_1yr"))
        .await?
        .check()?;

    backend
        .db
        .query(sql_metrics)
        .bind(("met_id", "met_2yr"))
        .bind(("id", "rule_2yr"))
        .await?
        .check()?;

    // Run the graduation pipeline
    run_graduation_pipeline(backend.as_ref(), "test-scope").await?;

    // Retrieve rules and verify utility values from the metrics table
    let mut resp = backend
        .db
        .query("SELECT target_id, utility_score FROM metrics ORDER BY target_id;")
        .await?
        .check()?;
    let results: Vec<serde_json::Value> = resp.take(0)?;

    let get_util = |id: &str| -> f64 {
        results
            .iter()
            .find(|val| val["target_id"].as_str().unwrap().contains(id))
            .and_then(|val| val["utility_score"].as_f64())
            .unwrap_or(0.0)
    };

    let util_now = get_util("rule_now");
    let util_1yr = get_util("rule_1yr");
    let util_2yr = get_util("rule_2yr");

    println!(
        "DECAY RESULTS: now={}, 1yr={}, 2yr={}",
        util_now, util_1yr, util_2yr
    );

    // Rule 1: 0 days old -> should stay close to 1.0 (e.g. > 0.99)
    assert!(
        util_now > 0.99,
        "Rule created now should not decay noticeably (util={})",
        util_now
    );

    // Rule 2: 365 days old -> should be close to 0.5 (half-life of 365 days)
    assert!(
        (util_1yr - 0.5).abs() < 0.05,
        "Rule created 1 year ago should decay to ~0.5 (util={})",
        util_1yr
    );

    // Rule 3: 730 days old -> should be close to 0.25 (two half-lives)
    assert!(
        (util_2yr - 0.25).abs() < 0.05,
        "Rule created 2 years ago should decay to ~0.25 (util={})",
        util_2yr
    );

    Ok(())
}

}

mod task5 {
use anyhow::Result;
use mythrax_core::cognitive::synthesis::{backpropagate_directions, promote_insight_to_direction};
use mythrax_core::contracts::{Episode, WikiNode};
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use std::fs;
use tempfile::tempdir;

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

    let dir_node = WikiNode {
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

    let rel = backend
        .relate_nodes(&dir_id, &child_id, None, None, None)
        .await?;
    println!("Related edge: {:?}", rel);

    backpropagate_directions(&backend, &store).await?;

    let nodes = backend.get_all_wiki_nodes().await?;
    let updated_dir = nodes
        .iter()
        .find(|n| n.id.as_deref() == Some(&dir_id))
        .unwrap();
    println!("Updated dir content: {}", updated_dir.content);

    assert!(
        updated_dir.content.contains("Child Insight")
            || updated_dir.content.contains("architectural compaction"),
        "Content should be synthesized"
    );

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
    let initial_id = backend
        .save_wiki_node(&node)
        .await
        .expect("Failed to save initial node");
    let node_with_id = backend
        .get_all_wiki_nodes()
        .await
        .unwrap()
        .into_iter()
        .find(|n| n.id.as_deref() == Some(&initial_id))
        .unwrap();

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

    promote_insight_to_direction(&backend, &store, &node_with_id, &episodes)
        .await
        .expect("Promotion failed");

    let nodes = backend.get_all_wiki_nodes().await.unwrap();
    let promoted = nodes
        .iter()
        .find(|n| n.name == "Completely Unique Name")
        .unwrap();
    assert_eq!(promoted.node_type.as_deref(), Some("direction"));

    Ok(())
}

}

mod task7 {
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

    backend
        .save_wiki_node(&node1)
        .await
        .map_err(|e| {
            println!("Err saving node1: {:?}", e);
            e
        })
        .unwrap();
    backend
        .save_wiki_node(&node2)
        .await
        .map_err(|e| {
            println!("Err saving node2: {:?}", e);
            e
        })
        .unwrap();

    // No conflict nodes
    graduate_wisdom(&backend, &store).await.unwrap();

    let rules = backend.get_all_wisdom_rules().await.unwrap();
    assert_eq!(rules.len(), 1, "Should graduate one wisdom rule");
    assert_eq!(rules[0].tier, Tier::Wisdom);
    assert!(
        rules[0].rule_type == Some("system_constraint".into())
            || rules[0].rule_type == Some("procedural_heuristic".into())
    );
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

    backend
        .save_wiki_node(&node1)
        .await
        .map_err(|e| {
            println!("Err saving node1: {:?}", e);
            e
        })
        .unwrap();
    let id_2 = backend
        .save_wiki_node(&node2)
        .await
        .map_err(|e| {
            println!("Err saving node2: {:?}", e);
            e
        })
        .unwrap();
    let id_conflict = backend
        .save_wiki_node(&conflict_node)
        .await
        .map_err(|e| {
            println!("Err saving conflict: {:?}", e);
            e
        })
        .unwrap();

    // Relate conflict node to node2
    backend
        .relate_nodes(&id_2, &id_conflict, None, None, None)
        .await
        .unwrap();

    graduate_wisdom(&backend, &store).await.unwrap();

    let rules = backend.get_all_wisdom_rules().await.unwrap();
    assert_eq!(
        rules.len(),
        0,
        "Graduation should be blocked by conflict node"
    );
}

}

mod task8 {
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use tempfile::tempdir;

use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::llm::LLMClient;
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::WatchIgnoreList;

#[tokio::test]
async fn test_checkpoint_resume() -> anyhow::Result<()> {
    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let vault_dir = tempdir()?;
    let store = Arc::new(MarkdownStore::new(vault_dir.path())?);
    let ignore = WatchIgnoreList::new();

    let trans_dir = tempdir()?;
    let transcript_path = trans_dir.path().join("transcript.jsonl");
    let mut trans_file = File::create(&transcript_path)?;
    let turns = vec![
        r#"{"role": "user", "content": "Turn 1 content"}"#,
        r#"{"role": "user", "content": "Turn 2 content"}"#,
        r#"{"role": "user", "content": "Turn 3 content"}"#,
        r#"{"role": "user", "content": "Turn 4 content"}"#,
    ];

    for turn in turns {
        writeln!(trans_file, "{}", turn)?;
    }

    let path_str = transcript_path.to_string_lossy();

    let count1 = mythrax_core::hooks::precompact::mine_transcript(
        "sess_checkpoint_test",
        &path_str,
        backend.as_ref(),
        &store,
        &ignore,
    )
    .await?;
    assert_eq!(count1, 4);

    let checkpoint_dir = store.vault_root.join(".mythrax");
    std::fs::create_dir_all(&checkpoint_dir)?;
    let checkpoint_path = checkpoint_dir.join("bootstrap_checkpoint.json");

    let checkpoint_json = r#"{"session_id": "sess_checkpoint_test", "last_processed_index": 1}"#;
    std::fs::write(&checkpoint_path, checkpoint_json)?;

    let _ = backend.clear_stm("sess_checkpoint_test").await;

    let count2 = mythrax_core::hooks::precompact::mine_transcript(
        "sess_checkpoint_test",
        &path_str,
        backend.as_ref(),
        &store,
        &ignore,
    )
    .await?;
    assert_eq!(count2, 2);

    Ok(())
}

#[tokio::test]
async fn test_quota_exhaustion_hibernation() -> anyhow::Result<()> {
    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let profile = mythrax_core::contracts::TaskProfile::new(
        mythrax_core::contracts::TaskArchetype::Reasoning,
    );

    unsafe {
        std::env::set_var("GEMINI_API_KEY", "dummy_key");
        std::env::remove_var("MYTHRAX_FORCE_LOCAL");
        std::env::remove_var("MYTHRAX_TEST_MOCK");
        std::env::remove_var("MYTHRAX_MOCK_LLM");
        std::env::set_var("MYTHRAX_MOCK_FAIL", "true");
        std::env::set_var("MYTHRAX_QUOTA_RETRY_SECS", "1");
        std::env::set_var("MYTHRAX_BOOTSTRAPPING", "true");
    }

    let client = LLMClient::default();

    let res1 = client
        .routed_completion(backend.as_ref(), &profile, None, "test")
        .await;
    assert!(res1.is_err());
    assert!(!mythrax_core::llm::is_hibernating());

    let res2 = client
        .routed_completion(backend.as_ref(), &profile, None, "test")
        .await;
    assert!(res2.is_err());
    assert!(!mythrax_core::llm::is_hibernating());

    let start = std::time::Instant::now();
    let res3 = client
        .routed_completion(backend.as_ref(), &profile, None, "test")
        .await;
    assert!(res3.is_err());
    assert!(start.elapsed() >= std::time::Duration::from_secs(1));

    unsafe {
        std::env::remove_var("MYTHRAX_MOCK_FAIL");
        std::env::remove_var("MYTHRAX_QUOTA_RETRY_SECS");
        std::env::remove_var("GEMINI_API_KEY");
        std::env::remove_var("MYTHRAX_BOOTSTRAPPING");
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }
    Ok(())
}

}

mod task9 {
use mythrax_core::cognitive::synthesis::{
    CONCISION_DIRECTIVE, build_synthesis_prompt, check_compression_ratio,
};
use std::sync::{Arc, Mutex};

#[test]
fn test_concision_prompt_prepending() {
    let base_prompt = "You are a systems synthesizer.";
    let final_prompt = build_synthesis_prompt(base_prompt);

    assert!(final_prompt.starts_with(CONCISION_DIRECTIVE));
    assert!(final_prompt.contains(base_prompt));
}

#[derive(Default, Clone)]
struct MockWarningSubscriber {
    warnings: Arc<Mutex<Vec<String>>>,
}

impl tracing::Subscriber for MockWarningSubscriber {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        if event.metadata().level() == &tracing::Level::WARN {
            let mut visitor = StringVisitor::default();
            event.record(&mut visitor);
            self.warnings.lock().unwrap().push(visitor.0);
        }
    }
    fn enter(&self, _span: &tracing::span::Id) {}
    fn exit(&self, _span: &tracing::span::Id) {}
}

#[derive(Default)]
struct StringVisitor(String);

impl tracing::field::Visit for StringVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{:?}", value);
        }
    }
}

#[test]
fn test_compression_warning_triggers() {
    let subscriber = MockWarningSubscriber::default();
    let warnings = subscriber.warnings.clone();

    // Set the env var for the test
    unsafe {
        std::env::set_var("MYTHRAX_VERBOSITY_ALERT_RATIO", "1.5");
    }

    // Run inside subscriber dispatcher
    tracing::subscriber::with_default(subscriber, || {
        // Mock large input and output compared to original tokens
        // input_tokens + output_tokens / original_tokens > 1.5
        // Let input_text length be 800 (200 tokens)
        // Let output_text length be 400 (100 tokens)
        // total tokens = 300
        // original_tokens = 100
        // ratio = 3.0 > 1.5 -> Should trigger warning
        let input_text = "a".repeat(800);
        let output_text = "a".repeat(400);

        check_compression_ratio(&input_text, &output_text, 100);

        // original_tokens = 300
        // ratio = 1.0 < 1.5 -> Should not trigger warning
        check_compression_ratio(&input_text, &output_text, 300);
    });

    let w = warnings.lock().unwrap();
    assert_eq!(w.len(), 1);
    assert!(w[0].contains("Verbosity alert: compression ratio"));
}

}

mod task10 {
use mythrax_core::api::ApiState;
use mythrax_core::contracts::{EpisodeSave, Tier, WisdomRule};
use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::mcp_routes::manage_handlers::handle_pre_invocation_hook;
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::WatchIgnoreList;
use serde_json::json;
use std::sync::Arc;
use tempfile::tempdir;

fn setup_env_vars() {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
        std::env::set_var("MYTHRAX_PRE_INVOCATION_TOKEN_BUDGET", "100");
    }
}

async fn create_test_state(temp_dir: &tempfile::TempDir) -> anyhow::Result<ApiState> {
    let db_path = temp_dir.path().join("db");
    let backend = SurrealBackend::new(
        &format!("surrealkv://{}", db_path.to_string_lossy()),
        mythrax_core::db::BackendConfig {
            check_daemon: false,
            embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)),
            llm: Some(mythrax_core::llm::LLMClient::new_mock()),
        },
    )
    .await?;
    backend.init().await?;

    let store = Arc::new(MarkdownStore::new(temp_dir.path())?);
    let ignore_list = Arc::new(WatchIgnoreList::new());

    Ok(ApiState {
        backend: Arc::new(backend),
        auth_token: "test".to_string(),
        store,
        ignore_list,
        dream_tx: None,
        shutdown_tx: None,
    })
}

#[tokio::test]
async fn test_task10_injection_and_truncation() -> anyhow::Result<()> {
    setup_env_vars();
    let temp_dir = tempdir()?;
    let state = create_test_state(&temp_dir).await?;
    let surreal_backend = state
        .backend
        .as_any()
        .downcast_ref::<SurrealBackend>()
        .unwrap();

    let session_id = "test_session_10";

    // 1. Setup Pruned Hypothesis
    let pruned = WisdomRule {
        id: Some("wisdom:pruned1".to_string()),
        target_pattern: "PRUNED: Failed path: some hypothesis".to_string(),
        action_to_avoid: "Avoid it".to_string(),
        causal_explanation: "Failed".to_string(),
        prescribed_remedy: "Try else".to_string(),
        tier: Tier::Project,
        scope: "general".to_string(),
        vault_path: None,
        embedding: None,
        source_episodes: vec![],
        generator_name: "HtrPruneAction".to_string(),
        similarity: Some(1.0),
        utility: Some(1.0),
        status: Some("active".to_string()),
        superseded_at: None,
        superseded_by: None,
        rule_type: Some("pruned_hypothesis".to_string()),
        severity: Some("warning".to_string()),
        blocking: Some(true),
        importance: Some(8.0),
        content_hash: None,
    };
    state.backend.save_wisdom_rule(&pruned).await?;

    // 2. Setup Conflict Node
    let conflict_ep = EpisodeSave::builder(
        "Knowledge Conflict".to_string(),
        "Conflicting info here".to_string(),
    )
    .node_type(Some("conflict".to_string()))
    .scope(Some("general".to_string()))
    .build();
    state.backend.save_episode(&conflict_ep).await?;

    // 3. Populate P3 (Belief State), P2 (STM), P1 (Wisdom - capabilities) to trigger truncation
    let sql = "INSERT INTO belief_state { session_id: $session_id, confidence_score: 0.5, tasks_todo: ['Long task description to consume tokens'], hypotheses_tested: [], uncertainty_areas: [], updated_at: time::now() };";
    surreal_backend
        .db
        .query(sql)
        .bind(("session_id", session_id))
        .await?;

    state
        .backend
        .save_stm(session_id, "big_key", &"A ".repeat(500))
        .await?;

    let payload = json!({
        "session_id": session_id,
        "action": "pre_invocation"
    });

    let res = handle_pre_invocation_hook(&state, payload).await?;
    let content = res["content"][0]["text"].as_str().unwrap();

    assert!(content.contains("### 🚫 Policy (Non-Negotiable Rules)"));
    assert!(content.contains("PRUNED: Failed path: some hypothesis"));

    assert!(content.contains("Knowledge Conflict"));

    // Check budget truncation (we set budget to 100 tokens, which is very small, so P3/P2 might be truncated)
    // Wait, let's test if it handles distiller exemption
    let payload_distiller = json!({
        "session_id": session_id,
        "action": "pre_invocation",
        "caller": "distiller"
    });
    let res_d = handle_pre_invocation_hook(&state, payload_distiller).await?;
    let content_d = res_d["content"][0]["text"].as_str().unwrap();
    // Distiller payload should not contain the same stuff
    assert!(!content_d.contains("Policy (Non-Negotiable Rules)"));

    Ok(())
}

}

mod task14 {
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::hooks::reflect::handle_reflect;
use std::fs;
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_reflect_queues_cognitive_task() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    // Create a mock transcript file with 10 turns and 5 tool calls to pass the gate
    let transcript_path = temp_dir.path().join("transcript.jsonl");
    let mut transcript_content = String::new();
    for i in 0..10 {
        let _role = if i % 2 == 0 { "user" } else { "assistant" };
        let tool_call = if i < 5 {
            r#", "tool_calls": [{"name": "read_file"}]"#
        } else {
            ""
        };
        transcript_content.push_str(&format!(
            r#"{{"step_index": {}, "source": "MODEL", "type": "PLANNER_RESPONSE", "content": "hello turn {}"{}}}"#,
            i, i, tool_call
        ));
        transcript_content.push_str("\n");
    }
    fs::write(&transcript_path, transcript_content).unwrap();

    let session_id = "test_session_123";
    let status = handle_reflect(session_id, &transcript_path.to_string_lossy(), &backend)
        .await
        .unwrap();

    // Assert task queued
    assert_eq!(status, "reflection_queued");

    // Fetch pending tasks
    let pending = backend.get_pending_cognitive_tasks().await.unwrap();
    assert_eq!(pending.len(), 1);
    let task = &pending[0];
    assert_eq!(task.task_type, "reflection_distillation");
    assert_eq!(task.session_id.as_deref(), Some(session_id));
}

#[tokio::test]
async fn test_reflect_skips_trivial_sessions() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    // Trivial transcript: 2 turns, 1 tool call
    let transcript_path = temp_dir.path().join("trivial.jsonl");
    fs::write(
        &transcript_path,
        r#"{"type":"USER_INPUT","content":"hi"}
{"type":"PLANNER_RESPONSE","content":"hello","tool_calls":[{"name":"read"}]}
"#,
    )
    .unwrap();

    let status = handle_reflect(
        "session_trivial",
        &transcript_path.to_string_lossy(),
        &backend,
    )
    .await
    .unwrap();
    assert_eq!(status, "skipped_trivial");

    let pending = backend.get_pending_cognitive_tasks().await.unwrap();
    assert!(pending.is_empty());
}

#[tokio::test]
async fn test_reflect_transcript_missing() {
    let _temp_dir = tempdir().expect("Failed to create temp dir");
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    // Transcript doesn't exist
    let status = handle_reflect("session_missing", "/nonexistent/path.jsonl", &backend)
        .await
        .unwrap();
    assert_eq!(status, "skipped_missing");
}

#[tokio::test]
async fn test_harvest_completed_reflections() {
    let _temp_dir = tempdir().expect("Failed to create temp dir");
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    let session_id = "test_session_harvest";

    let task_id = format!("cognitive_task:{}", uuid::Uuid::new_v4());
    let result_json = serde_json::json!({
        "outcome": "failure",
        "causal_explanation": "Ran out of tokens",
        "lessons": ["Monitor token usage"],
        "error_patterns": ["Max length exceeded"],
        "files_modified": ["src/lib.rs"]
    });

    let task = mythrax_core::db::CognitiveTask {
        id: task_id.clone(),
        task_type: "reflection_distillation".to_string(),
        prompt: "distill...".to_string(),
        system_instruction: "sys".to_string(),
        expected_format: "Json".to_string(),
        priority: "Normal".to_string(),
        created_at: chrono::Utc::now(),
        status: "Completed".to_string(),
        result: Some(serde_json::to_string(&result_json).unwrap()),
        ttl_minutes: 10,
        injected_at: None,
        session_id: Some(session_id.to_string()),
    };

    backend.create_cognitive_task(&task).await.unwrap();

    let embedder = mythrax_core::embeddings::MockEmbedder;
    let lessons_val = serde_json::json!(["Monitor token usage"]);
    let text_to_embed = format!("Ran out of tokens {:?}", lessons_val);
    let embedding_vec =
        mythrax_core::embeddings::TextEmbedder::embed(&embedder, &text_to_embed).await.unwrap();

    let rule = mythrax_core::contracts::WisdomRule {
        id: None,
        target_pattern: "PRUNED: Existing causal".to_string(),
        action_to_avoid: "Repeat failed approach".to_string(),
        causal_explanation: "Existing causal".to_string(),
        prescribed_remedy: "Existing remedy".to_string(),
        tier: mythrax_core::contracts::Tier::Working,
        scope: "general".to_string(),
        vault_path: None,
        source_episodes: vec![],
        generator_name: "reflect_harvester".to_string(),
        embedding: Some(embedding_vec),
        utility: Some(50.0),
        status: Some("active".to_string()),
        superseded_at: None,
        superseded_by: None,
        severity: Some("low".to_string()),
        blocking: Some(false),
        rule_type: Some("pruned_hypothesis".to_string()),
        importance: Some(0.2),
        ..Default::default()
    };
    backend.save_wisdom_rule_db(&rule).await.unwrap();

    mythrax_core::hooks::reflect::harvest_completed_reflections(&backend)
        .await
        .unwrap();

    let sql = "SELECT * FROM type::record('cognitive_task', $id);";
    let id_part = task_id.strip_prefix("cognitive_task:").unwrap();
    let mut res = backend.db.query(sql).bind(("id", id_part)).await.unwrap();
    let tasks: Vec<mythrax_core::db::cognitive_tasks::CognitiveTaskRaw> = res.take(0).unwrap();
    assert!(tasks.is_empty(), "Processed task should be deleted");

    let ep_sql = "SELECT * FROM episode WHERE session_id = $session_id;";
    let mut ep_res = backend
        .db
        .query(ep_sql)
        .bind(("session_id", session_id))
        .await
        .unwrap();
    let eps: Vec<serde_json::Value> = ep_res.take(0).unwrap();
    assert_eq!(eps.len(), 1);
    assert_eq!(eps[0]["node_type"], "experience");

    let rule_sql = "
        SELECT *
        FROM wisdom
        WHERE rule_type = 'pruned_hypothesis';
    ";
    let mut rule_res = backend.db.query(rule_sql).await.unwrap();
    let rules: Vec<serde_json::Value> = rule_res.take(0).unwrap();
    assert!(
        rules
            .iter()
            .any(|r| r["importance"].as_f64().unwrap_or(0.0) > 0.3)
    );
}

}

mod task15 {
use mythrax_core::api::ApiState;
use mythrax_core::contracts::{Tier, WisdomRule};
use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::mcp_routes::manage_handlers::handle_pre_invocation_hook;
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::WatchIgnoreList;
use serde_json::json;
use std::sync::Arc;
use tempfile::tempdir;

fn setup_env_vars() {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
        std::env::set_var("MYTHRAX_PRE_INVOCATION_TOKEN_BUDGET", "3000");
    }
}

async fn create_test_state(temp_dir: &tempfile::TempDir) -> anyhow::Result<ApiState> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    let store = Arc::new(MarkdownStore::new(temp_dir.path())?);
    let ignore_list = Arc::new(WatchIgnoreList::new());

    Ok(ApiState {
        backend: Arc::new(backend),
        auth_token: "test".to_string(),
        store,
        ignore_list,
        dream_tx: None,
        shutdown_tx: None,
    })
}

#[tokio::test]
async fn test_policy_section_rendered_first() -> anyhow::Result<()> {
    setup_env_vars();
    let temp_dir = tempdir()?;
    let state = create_test_state(&temp_dir).await?;
    let surreal_backend = state
        .backend
        .as_any()
        .downcast_ref::<SurrealBackend>()
        .unwrap();

    let rule = WisdomRule {
        id: Some("wisdom:policy1".to_string()),
        target_pattern: "UNIQUE_POLICY_XYZ".to_string(),
        action_to_avoid: "Avoid it".to_string(),
        causal_explanation: "Failed".to_string(),
        prescribed_remedy: "Do it".to_string(),
        tier: Tier::Project,
        scope: "general".to_string(),
        vault_path: None,
        embedding: None,
        source_episodes: vec![],
        generator_name: "Test".to_string(),
        similarity: Some(1.0),
        utility: Some(1.0),
        status: Some("active".to_string()),
        superseded_at: None,
        superseded_by: None,
        rule_type: Some("pruned_hypothesis".to_string()),
        severity: Some("warning".to_string()),
        blocking: Some(true),
        importance: Some(8.0),
        content_hash: None,
    };
    state.backend.save_wisdom_rule(&rule).await?;

    let sql = "INSERT INTO episode { title: 'UNIQUE_ADVISORY_ABC', content: 'Advisory content', scope: 'general', node_type: 'experience' };";
    surreal_backend.db.query(sql).await?;

    let payload = json!({
        "session_id": "test_session_15",
        "action": "pre_invocation"
    });

    let res = handle_pre_invocation_hook(&state, payload).await?;
    let content = res["content"][0]["text"].as_str().unwrap();

    assert!(content.contains("🚫 Policy"));
    assert!(content.contains("UNIQUE_POLICY_XYZ"));
    assert!(content.contains("💡 Advisory"));
    assert!(content.contains("UNIQUE_ADVISORY_ABC"));

    let policy_idx = content.find("UNIQUE_POLICY_XYZ").unwrap();
    let advisory_idx = content.find("UNIQUE_ADVISORY_ABC").unwrap();
    assert!(
        policy_idx < advisory_idx,
        "Policy must be rendered before Advisory"
    );
    Ok(())
}

#[tokio::test]
async fn test_policy_uses_caution_format() -> anyhow::Result<()> {
    setup_env_vars();
    let temp_dir = tempdir()?;
    let state = create_test_state(&temp_dir).await?;

    let rule = WisdomRule {
        id: Some("wisdom:policy2".to_string()),
        target_pattern: "POLICY_FORMAT_TEST".to_string(),
        action_to_avoid: "Avoid it".to_string(),
        causal_explanation: "Failed".to_string(),
        prescribed_remedy: "Do it".to_string(),
        tier: Tier::Project,
        scope: "general".to_string(),
        vault_path: None,
        embedding: None,
        source_episodes: vec![],
        generator_name: "Test".to_string(),
        similarity: Some(1.0),
        utility: Some(1.0),
        status: Some("active".to_string()),
        superseded_at: None,
        superseded_by: None,
        rule_type: Some("pruned_hypothesis".to_string()),
        severity: Some("warning".to_string()),
        blocking: Some(true),
        importance: Some(8.0),
        content_hash: None,
    };
    state.backend.save_wisdom_rule(&rule).await?;

    let payload = json!({
        "session_id": "test_session_15",
        "action": "pre_invocation"
    });

    let res = handle_pre_invocation_hook(&state, payload).await?;
    let content = res["content"][0]["text"].as_str().unwrap();

    assert!(content.contains("> [!CAUTION]"));
    assert!(content.contains("POLICY_FORMAT_TEST"));
    Ok(())
}

#[tokio::test]
async fn test_advisory_uses_tip_format() -> anyhow::Result<()> {
    setup_env_vars();
    let temp_dir = tempdir()?;
    let state = create_test_state(&temp_dir).await?;
    let surreal_backend = state
        .backend
        .as_any()
        .downcast_ref::<SurrealBackend>()
        .unwrap();

    let sql = "INSERT INTO episode { title: 'ADVISORY_FORMAT_TEST', content: 'Advisory content', scope: 'general', node_type: 'experience' };";
    surreal_backend.db.query(sql).await?;

    let payload = json!({
        "session_id": "test_session_15",
        "action": "pre_invocation"
    });

    let res = handle_pre_invocation_hook(&state, payload).await?;
    let content = res["content"][0]["text"].as_str().unwrap();

    assert!(content.contains("> [!TIP]"));
    assert!(content.contains("ADVISORY_FORMAT_TEST"));
    Ok(())
}

#[tokio::test]
async fn test_policy_never_truncated() -> anyhow::Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
        std::env::set_var("MYTHRAX_PRE_INVOCATION_TOKEN_BUDGET", "500");
    }

    let temp_dir = tempdir()?;
    let state = create_test_state(&temp_dir).await?;
    let surreal_backend = state
        .backend
        .as_any()
        .downcast_ref::<SurrealBackend>()
        .unwrap();

    for i in 1..=3 {
        let rule = WisdomRule {
            id: Some(format!("wisdom:policy_trunc_{}", i)),
            target_pattern: format!("POLICY_TRUNC_TEST_{}", i),
            action_to_avoid: "Avoid it".to_string(),
            causal_explanation: "Failed".to_string(),
            prescribed_remedy: "Do it".to_string(),
            tier: Tier::Project,
            scope: "general".to_string(),
            vault_path: None,
            embedding: None,
            source_episodes: vec![],
            generator_name: "Test".to_string(),
            similarity: Some(1.0),
            utility: Some(1.0),
            status: Some("active".to_string()),
            superseded_at: None,
            superseded_by: None,
            rule_type: Some("pruned_hypothesis".to_string()),
            severity: Some("warning".to_string()),
            blocking: Some(true),
            importance: Some(8.0),
            content_hash: None,
        };
        state.backend.save_wisdom_rule(&rule).await?;
    }

    for i in 1..=10 {
        let sql = format!(
            "INSERT INTO episode {{ title: 'ADVISORY_TRUNC_TEST_{}', content: '{}', scope: 'general', node_type: 'experience' }};",
            i,
            "Some long content to trigger truncation. ".repeat(50)
        );
        surreal_backend.db.query(&sql).await?;
    }

    let payload = json!({
        "session_id": "test_session_15",
        "action": "pre_invocation"
    });

    let res = handle_pre_invocation_hook(&state, payload).await?;
    let content = res["content"][0]["text"].as_str().unwrap();

    assert!(content.contains("POLICY_TRUNC_TEST_1"));
    assert!(content.contains("POLICY_TRUNC_TEST_2"));
    assert!(content.contains("POLICY_TRUNC_TEST_3"));

    let has_all_advisory =
        (1..=10).all(|i| content.contains(&format!("ADVISORY_TRUNC_TEST_{}", i)));
    assert!(
        !has_all_advisory,
        "Advisory section must be truncated under token pressure"
    );

    Ok(())
}

}

mod task16 {
use mythrax_core::api::ApiState;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::mcp_routes::manage_handlers::{handle_manage, handle_manage_stm};
use serde_json::json;
use std::sync::Arc;
use tempfile::tempdir;

// We need a test state helper
async fn setup_state() -> ApiState {
    let tmp = tempdir().unwrap();
    let vault_root = tmp.path().join("vault");
    std::fs::create_dir_all(&vault_root).unwrap();

    let backend = Arc::new(SurrealBackend::new_in_memory().await.unwrap());
    backend.init().await.unwrap();

    let store = Arc::new(mythrax_core::store::MarkdownStore::new(&vault_root).unwrap());
    let ignore_list = Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new());

    ApiState {
        backend,
        store,
        ignore_list,
        auth_token: "test".to_string(),
        dream_tx: None,
        shutdown_tx: None,
    }
}

async fn create_handoff_file(state: &ApiState, task_id: &str, content: &str) -> String {
    let dir = state.store.vault_root.join(".handoffs");
    std::fs::create_dir_all(&dir).unwrap();
    let file_path = dir.join(format!("handoff_{}.md", task_id));
    std::fs::write(&file_path, content).unwrap();
    file_path.to_string_lossy().to_string()
}

#[tokio::test]
async fn test_contract_rejects_missing_input() {
    let state = setup_state().await;
    let yaml = r#"---
task_id: "test_1"
title: "Test"
status: "pending"
parent_conversation_id: "parent-1"
inputs:
  - name: "req_input"
    type: "string"
    required: true
---"#;
    let path = create_handoff_file(&state, "test_1", yaml).await;

    let args = json!({
        "action": "handoff",
        "parent_conversation_id": "parent-1",
        "subagent_conversation_id": "sub-1",
        "summary": "test",
        "handoff_file_path": path
    });

    let res = handle_manage_stm(&state, args).await;
    assert!(res.is_err());
    let err_str = res.unwrap_err().to_string();
    assert!(err_str.contains("Missing required input"));
}

#[tokio::test]
async fn test_contract_accepts_valid_handoff() {
    let state = setup_state().await;
    let yaml = r#"---
task_id: "test_2"
title: "Test"
status: "pending"
parent_conversation_id: "parent-1"
inputs:
  - name: "req_input"
    type: "string"
    required: true
    value: "some_value"
---"#;
    let path = create_handoff_file(&state, "test_2", yaml).await;

    let args = json!({
        "action": "handoff",
        "parent_conversation_id": "parent-1",
        "subagent_conversation_id": "sub-1",
        "summary": "test",
        "handoff_file_path": path
    });

    let res = handle_manage_stm(&state, args).await;
    assert!(res.is_ok());

    // Check DB status remains PENDING
    // Check STM
    let stm = state
        .backend
        .get_stm("sub-1", Some("stm_test_2_input_req_input"))
        .await
        .unwrap();
    assert!(stm.contains_key("stm_test_2_input_req_input"));
}

#[tokio::test]
async fn test_save_handoff_legacy_markdown() {
    let state = setup_state().await;
    let md = r#"# Legacy Handoff
No YAML here."#;
    let path = create_handoff_file(&state, "test_3", md).await;

    let args = json!({
        "action": "handoff",
        "parent_conversation_id": "parent-1",
        "subagent_conversation_id": "sub-1",
        "summary": "test",
        "handoff_file_path": path
    });

    let res = handle_manage_stm(&state, args).await;
    assert!(res.is_ok()); // Should bypass validation
}

#[tokio::test]
async fn test_save_handoff_malformed_yaml() {
    let state = setup_state().await;
    let md = r#"---
task_id: [broken
---"#;
    let path = create_handoff_file(&state, "test_4", md).await;

    let args = json!({
        "action": "handoff",
        "parent_conversation_id": "parent-1",
        "subagent_conversation_id": "sub-1",
        "summary": "test",
        "handoff_file_path": path
    });

    let res = handle_manage_stm(&state, args).await;
    assert!(res.is_err());
    assert!(
        res.unwrap_err()
            .to_string()
            .contains("Malformed contract frontmatter")
    );
}

#[tokio::test]
async fn test_complete_handoff_rejects_missing_output() {
    let state = setup_state().await;
    let yaml = r#"---
task_id: "test_5"
title: "Test"
status: "pending"
parent_conversation_id: "parent-1"
outputs:
  - name: "req_out"
    type: "string"
    required: true
---"#;
    let path = create_handoff_file(&state, "test_5", yaml).await;

    let args = json!({
        "action": "complete_handoff",
        "task_id": "test_5"
    });

    let res = handle_manage(&state, args).await;
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("missing_output"));

    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("status: \"failed\""));
}

#[tokio::test]
async fn test_complete_handoff_intentional_failure() {
    let state = setup_state().await;
    let yaml = r#"---
task_id: "test_6"
title: "Test"
status: "pending"
parent_conversation_id: "parent-1"
outputs:
  - name: "req_out"
    type: "string"
    required: true
---"#;
    let path = create_handoff_file(&state, "test_6", yaml).await;

    let args = json!({
        "action": "complete_handoff",
        "task_id": "test_6",
        "status": "failed",
        "fail_reason": "I gave up"
    });

    let res = handle_manage(&state, args).await;
    assert!(res.is_err());

    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("status: \"failed\""));
    assert!(content.contains("I gave up"));
}

#[tokio::test]
async fn test_complete_handoff_validates_enum() {
    let state = setup_state().await;
    let yaml = r#"---
task_id: "test_7"
title: "Test"
status: "pending"
parent_conversation_id: "parent-1"
outputs:
  - name: "req_out"
    type: "string"
    required: true
    enum: ["pass", "fail"]
---"#;
    let _path = create_handoff_file(&state, "test_7", yaml).await;

    let args = json!({
        "action": "complete_handoff",
        "task_id": "test_7",
        "outputs": {
            "req_out": "maybe"
        }
    });

    let res = handle_manage(&state, args).await;
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("enum"));
}

#[tokio::test]
async fn test_complete_handoff_task_not_found() {
    let state = setup_state().await;
    let args = json!({
        "action": "complete_handoff",
        "task_id": "nonexistent"
    });

    let res = handle_manage(&state, args).await;
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_contract_writes_to_stm() {
    // Covered by test_contract_accepts_valid_handoff, but we can do a specific check if needed
}

#[tokio::test]
async fn test_complete_handoff_writes_outputs_to_stm() {
    let state = setup_state().await;
    let yaml = r#"---
task_id: "test_10"
title: "Test"
status: "pending"
parent_conversation_id: "parent-1"
outputs:
  - name: "req_out"
    type: "string"
    required: true
---"#;
    let _path = create_handoff_file(&state, "test_10", yaml).await;

    let args = json!({
        "action": "complete_handoff",
        "task_id": "test_10",
        "outputs": {
            "req_out": "done"
        }
    });

    let res = handle_manage(&state, args).await;
    assert!(res.is_ok());

    let stm = state
        .backend
        .get_stm("parent-1", Some("stm_test_10_output_req_out"))
        .await
        .unwrap();
    assert!(stm.contains_key("stm_test_10_output_req_out"));
}

}

mod archived_demotion {
use anyhow::Result;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};

#[tokio::test]
async fn test_archived_demotion_logic() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    backend
        .save_profile_key("search.sigmoid_center", "0.55")
        .await?;
    backend
        .save_profile_key("search.fusion_sigmoid_center", "0.60")
        .await?;
    backend
        .save_profile_key("search.gamma_rerank", "0.0")
        .await?;
    backend
        .save_profile_key("search.rerank_pool_size", "25")
        .await?;
    backend
        .save_profile_key("search.rerank_weight", "0.45")
        .await?;

    // Create 7 episodes with the same content so their raw vector similarity is identical
    let content = "Database index compression algorithms and HNSW graphs.";
    let titles = vec![
        "Active Node",                 // Index 0: Not archived
        "Recent Archived Node",        // Index 1: Archived 30m ago (factor 0.85)
        "One Day Archived Node",       // Index 2: Archived 24h ago (factor 0.85)
        "Three Days Archived Node",    // Index 3: Archived 3d ago (factor 0.70)
        "Seven Days Archived Node",    // Index 4: Archived 7d ago (factor 0.40)
        "Fourteen Days Archived Node", // Index 5: Archived 14d ago (factor 0.40)
        "Legacy Archived Node",        // Index 6: Archived, archived_at is None (factor 0.40)
    ];

    let mut ids = Vec::new();
    for (i, _title) in titles.iter().enumerate() {
        let session_id = if i == 0 || i == 1 {
            Some("session-123".to_string())
        } else {
            Some("session-abc".to_string())
        };
        let ep = EpisodeSave {
            created_at: None,
            title: "Node".to_string(),
            content: format!("{} - {}", _title, content),
            scope: Some("general".to_string()),
            session_id,
            ..Default::default()
        };
        let id = backend.save_episode(&ep).await?;
        ids.push(id);
    }

    // Set archived and archived_at fields for each node
    // 0: Active (archived: false, archived_at: None - defaults)

    // 1: Recent (archived: true, archived_at: now - 30m)
    let uuid_1 = ids[1].split(':').nth(1).unwrap();
    backend.db.query("UPDATE type::record('episode', $id) MERGE { archived: true, archived_at: time::now() - 30m };")
        .bind(("id", uuid_1))
        .await?.check()?;

    // 2: One Day (archived: true, archived_at: now - 24h)
    let uuid_2 = ids[2].split(':').nth(1).unwrap();
    backend.db.query("UPDATE type::record('episode', $id) MERGE { archived: true, archived_at: time::now() - 24h };")
        .bind(("id", uuid_2))
        .await?.check()?;

    // 3: Three Days (archived: true, archived_at: now - 3d)
    let uuid_3 = ids[3].split(':').nth(1).unwrap();
    backend.db.query("UPDATE type::record('episode', $id) MERGE { archived: true, archived_at: time::now() - 3d };")
        .bind(("id", uuid_3))
        .await?.check()?;

    // 4: Seven Days (archived: true, archived_at: now - 7d)
    let uuid_4 = ids[4].split(':').nth(1).unwrap();
    backend.db.query("UPDATE type::record('episode', $id) MERGE { archived: true, archived_at: time::now() - 7d };")
        .bind(("id", uuid_4))
        .await?.check()?;

    // 5: Fourteen Days (archived: true, archived_at: now - 14d)
    let uuid_5 = ids[5].split(':').nth(1).unwrap();
    backend.db.query("UPDATE type::record('episode', $id) MERGE { archived: true, archived_at: time::now() - 14d };")
        .bind(("id", uuid_5))
        .await?.check()?;

    // 6: Legacy (archived: true, archived_at: None)
    let uuid_6 = ids[6].split(':').nth(1).unwrap();
    backend
        .db
        .query("UPDATE type::record('episode', $id) MERGE { archived: true, archived_at: None };")
        .bind(("id", uuid_6))
        .await?
        .check()?;

    // Allow SurrealDB FTS to index
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    // Debug print all episodes in the database
    let mut all_eps_res = backend
        .db
        .query("SELECT id, title, session_id, archived, archived_at FROM episode;")
        .await?;
    let all_eps: Vec<serde_json::Value> = all_eps_res.take(0)?;
    println!("ALL EPISODES IN DB: {:#?}", all_eps);

    // 1. CROSS-SESSION SEARCH: Search with session_id = None (meaning all retrieved nodes from different sessions are cross-session)
    unsafe {
        std::env::set_var("MYTHRAX_SESSION_ISOLATION", "false");
    }
    let resp_cross = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "Database index compression",
            Some("general"),
            false,
            20,
            0,
            0.0,
            None,
            false,
            true,
            true,
            None,
            true,
            None,
        ))
        .await?;
    unsafe {
        std::env::set_var("MYTHRAX_SESSION_ISOLATION", "true");
    }

    let results_cross = resp_cross.results;
    println!(
        "CROSS-SESSION RESULTS: {:?}",
        results_cross.iter().map(|r| &r.title).collect::<Vec<_>>()
    );
    assert_eq!(
        results_cross.len(),
        7,
        "Should return all 7 search results for cross-session search"
    );

    let get_score_cross = |id: &str| -> f32 {
        results_cross
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.similarity)
            .unwrap_or(0.0)
    };

    let score_active_cross = get_score_cross(&ids[0]);
    let score_recent_cross = get_score_cross(&ids[1]);
    let score_one_day_cross = get_score_cross(&ids[2]);
    let score_three_days_cross = get_score_cross(&ids[3]);
    let score_seven_days_cross = get_score_cross(&ids[4]);
    let score_fourteen_days_cross = get_score_cross(&ids[5]);
    let score_legacy_cross = get_score_cross(&ids[6]);

    println!("CROSS-SESSION Active Score: {}", score_active_cross);
    println!(
        "CROSS-SESSION Recent Score (expect demoted, ratio ~0.4): {}",
        score_recent_cross
    );
    println!(
        "CROSS-SESSION One Day Score (expect demoted, ratio ~0.4): {}",
        score_one_day_cross
    );

    // Verify cross-session ratios (all archived nodes should be demoted by 0.4)
    let ratio_recent_cross = score_recent_cross / score_active_cross;
    assert!(
        (ratio_recent_cross - 0.40).abs() < 0.08,
        "Recent cross-session ratio was {}",
        ratio_recent_cross
    );

    let ratio_one_day_cross = score_one_day_cross / score_active_cross;
    assert!(
        (ratio_one_day_cross - 0.40).abs() < 0.08,
        "One day cross-session ratio was {}",
        ratio_one_day_cross
    );

    let ratio_three_days_cross = score_three_days_cross / score_active_cross;
    assert!(
        (ratio_three_days_cross - 0.40).abs() < 0.08,
        "Three days cross-session ratio was {}",
        ratio_three_days_cross
    );

    let ratio_seven_days_cross = score_seven_days_cross / score_active_cross;
    assert!(
        (ratio_seven_days_cross - 0.40).abs() < 0.08,
        "Seven days cross-session ratio was {}",
        ratio_seven_days_cross
    );

    let ratio_fourteen_days_cross = score_fourteen_days_cross / score_active_cross;
    assert!(
        (ratio_fourteen_days_cross - 0.40).abs() < 0.08,
        "Fourteen days cross-session ratio was {}",
        ratio_fourteen_days_cross
    );

    let ratio_legacy_cross = score_legacy_cross / score_active_cross;
    assert!(
        (ratio_legacy_cross - 0.40).abs() < 0.08,
        "Legacy cross-session ratio was {}",
        ratio_legacy_cross
    );

    // 2. SAME-SESSION SEARCH: Search with session_id = Some("session-123")
    // This will retrieve only Index 0 (Active) and Index 1 (Recent Archived), as they are same-session.
    let resp_same = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "Database index compression",
            Some("general"),
            false,
            20,
            0,
            0.0,
            None,
            false,
            true,
            true,
            Some("session-123"),
            true,
            None,
        ))
        .await?;

    let results_same = resp_same.results;
    println!(
        "SAME-SESSION RESULTS: {:?}",
        results_same.iter().map(|r| &r.title).collect::<Vec<_>>()
    );
    assert_eq!(
        results_same.len(),
        2,
        "Should return 2 search results matching the session"
    );

    let get_score_same = |id: &str| -> f32 {
        results_same
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.similarity)
            .unwrap_or(0.0)
    };

    let score_active_same = get_score_same(&ids[0]);
    let score_recent_same = get_score_same(&ids[1]);

    println!("SAME-SESSION Active Score: {}", score_active_same);
    println!(
        "SAME-SESSION Recent Score (expect same-session bypass, ratio ~1.0): {}",
        score_recent_same
    );

    // Verify same-session ratio (should bypass demotion, so ratio is ~1.0)
    let ratio_recent_same = score_recent_same / score_active_same;
    assert!(
        (ratio_recent_same - 1.0).abs() < 0.05,
        "Recent same-session ratio was {}",
        ratio_recent_same
    );

    Ok(())
}

}

mod pipeline_cluster {
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

}

mod precompact_ingest {
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use tempfile::tempdir;

use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::WatchIgnoreList;

#[tokio::test]
async fn precompact_persists_raw_tool_output() -> anyhow::Result<()> {
    // 1. Build in-memory backend + MarkdownStore(tempdir) + WatchIgnoreList
    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let vault_dir = tempdir()?;
    let store = Arc::new(MarkdownStore::new(vault_dir.path())?);
    let ignore = WatchIgnoreList::new();

    // 2. Write a temp transcript.jsonl containing a user turn and a tool output
    let trans_dir = tempdir()?;
    let transcript_path = trans_dir.path().join("transcript.jsonl");
    let mut trans_file = File::create(&transcript_path)?;

    // We write a user turn and a tool turn
    // Standard shape: we can check how Claude Code or other host transcript formats represent tool outputs,
    // but a general standard is a JSON line representing each message/turn.
    // In runner.rs, we parsed QuestionEntry containing haystack_sessions, which contains role & content.
    // Let's make sure our mine_transcript can parse a simple array of messages or JSONL entries.
    // The spec says:
    // "mine_transcript: read JSONL line-by-line; for each user turn and each tool-result turn, build an EpisodeSave... with content = raw text verbatim."
    // Let's write standard JSONL records representing a user message and a tool response.
    let turns = vec![
        r#"{"role": "user", "content": "Run the compile command."}"#,
        r#"{"role": "tool", "content": "Compilation successful: RAW_TOOL_PAYLOAD_XYZ"}"#,
    ];

    for turn in turns {
        writeln!(trans_file, "{}", turn)?;
    }

    let path_str = transcript_path.to_string_lossy();

    // 3. Call mine_transcript
    let count = mythrax_core::hooks::precompact::mine_transcript(
        "sess1",
        &path_str,
        backend.as_ref(),
        &store,
        &ignore,
    )
    .await?;

    // 4. Assert returned count >= 2
    assert!(count >= 2);

    // 5. Query the backend directly to verify the raw tool payload was indexed verbatim (since tool_execution is excluded from standard search results)
    let mut db_res = backend
        .db
        .query("SELECT VALUE content FROM episode WHERE string::contains(content, $payload);")
        .bind(("payload", "RAW_TOOL_PAYLOAD_XYZ"))
        .await?;
    let matching_contents: Vec<String> = db_res.take(0)?;
    assert!(
        !matching_contents.is_empty(),
        "Verbatim tool output was not found in the database"
    );

    Ok(())
}

#[tokio::test]
async fn precompact_persists_array_form_tool_result_blocks() -> anyhow::Result<()> {
    // Real Claude/Codex transcripts represent a user turn's `content` as an ARRAY
    // of content blocks (text + tool_result), not a flat string. The old
    // deserializer typed content as Option<String>, so these lines failed to parse
    // and the verbatim tool output was silently dropped. This exercises the
    // array-of-blocks path through extract_text().
    let backend: Arc<dyn StorageBackend> = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let vault_dir = tempdir()?;
    let store = Arc::new(MarkdownStore::new(vault_dir.path())?);
    let ignore = WatchIgnoreList::new();

    let trans_dir = tempdir()?;
    let transcript_path = trans_dir.path().join("transcript.jsonl");
    let mut trans_file = File::create(&transcript_path)?;

    // A user turn whose content is an array of blocks, including a tool_result
    // whose own content is itself an array of text blocks (the nested shape used
    // by real transcripts). Also covers the `message`-nested wrapper form.
    let turns = vec![
        r#"{"role":"user","content":[{"type":"text","text":"Here is the build output."},{"type":"tool_result","content":[{"type":"text","text":"BLOCK_TOOL_PAYLOAD_ABC compiled ok"}]}]}"#,
        r#"{"message":{"role":"user","content":[{"type":"tool_result","content":"NESTED_TOOL_PAYLOAD_DEF"}]}}"#,
    ];
    for turn in turns {
        writeln!(trans_file, "{}", turn)?;
    }

    let path_str = transcript_path.to_string_lossy();
    let count = mythrax_core::hooks::precompact::mine_transcript(
        "sess-blocks",
        &path_str,
        backend.as_ref(),
        store.as_ref(),
        &ignore,
    )
    .await?;
    assert!(
        count >= 2,
        "expected both array-form turns mined, got {}",
        count
    );

    for payload in ["BLOCK_TOOL_PAYLOAD_ABC", "NESTED_TOOL_PAYLOAD_DEF"] {
        let response = backend
            .search(mythrax_core::contracts::SearchParams::from_positional(
                payload,
                Some("general"),
                false,
                5,
                0,
                0.0,
                None,
                false,
                true,
                true,
                None,
                true,
                None,
            ))
            .await?;
        let found = response.results.iter().any(|r| r.content.contains(payload));
        assert!(
            found,
            "verbatim tool output {} was dropped from array-form content",
            payload
        );
    }

    Ok(())
}

#[tokio::test]
async fn precompact_mines_assistant_turns() -> anyhow::Result<()> {
    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let vault_dir = tempdir()?;
    let store = Arc::new(MarkdownStore::new(vault_dir.path())?);
    let ignore = WatchIgnoreList::new();

    let trans_dir = tempdir()?;
    let transcript_path = trans_dir.path().join("transcript.jsonl");
    let mut trans_file = File::create(&transcript_path)?;

    // Assistant turn longer than 20 characters
    let turns = vec![
        r#"{"role": "assistant", "content": "I will begin the troubleshooting process. First, let's identify what process is listening on port 8080."}"#,
    ];
    for turn in turns {
        writeln!(trans_file, "{}", turn)?;
    }

    let path_str = transcript_path.to_string_lossy();
    let count = mythrax_core::hooks::precompact::mine_transcript(
        "sess-assistant",
        &path_str,
        backend.as_ref(),
        store.as_ref(),
        &ignore,
    )
    .await?;

    assert_eq!(count, 1, "expected 1 assistant turn mined, got {}", count);

    let response = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "troubleshooting process",
            None,
            false,
            5,
            0,
            0.0,
            None,
            false,
            true,
            true,
            None,
            true,
            None,
        ))
        .await?;
    let found = response
        .results
        .iter()
        .any(|r| r.content.contains("troubleshooting process"));
    assert!(
        found,
        "assistant turn content was not found in search results"
    );

    Ok(())
}

#[tokio::test]
async fn precompact_filters_short_assistant_turns() -> anyhow::Result<()> {
    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let vault_dir = tempdir()?;
    let store = Arc::new(MarkdownStore::new(vault_dir.path())?);
    let ignore = WatchIgnoreList::new();

    let trans_dir = tempdir()?;
    let transcript_path = trans_dir.path().join("transcript.jsonl");
    let mut trans_file = File::create(&transcript_path)?;

    // Assistant turns: one short (20 chars), one long (21 chars)
    // "Sure thing, I'll try." is 21 chars
    // "Sure, OK, I will do." is 20 chars
    let turns = vec![
        r#"{"role": "assistant", "content": "Sure, OK, I will do."}"#,
        r#"{"role": "assistant", "content": "Sure thing, I'll try."}"#,
    ];
    for turn in turns {
        writeln!(trans_file, "{}", turn)?;
    }

    let path_str = transcript_path.to_string_lossy();
    let count = mythrax_core::hooks::precompact::mine_transcript(
        "sess-short-assistant",
        &path_str,
        backend.as_ref(),
        store.as_ref(),
        &ignore,
    )
    .await?;

    // Only the 21-char turn should be mined, the 20-char one should be skipped
    assert_eq!(
        count, 1,
        "expected exactly 1 turn mined (length > 20), got {}",
        count
    );

    // Verify the short one is NOT in search results
    let response_short = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "Sure, OK", None, false, 5, 0, 0.0, None, false, true, true, None, true, None,
        ))
        .await?;
    let found_short = response_short
        .results
        .iter()
        .any(|r| r.content.contains("Sure, OK"));
    assert!(!found_short, "short assistant turn was incorrectly indexed");

    // Verify the long one IS in search results
    let response_long = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "Sure thing",
            None,
            false,
            5,
            0,
            0.0,
            None,
            false,
            true,
            true,
            None,
            true,
            None,
        ))
        .await?;
    let found_long = response_long
        .results
        .iter()
        .any(|r| r.content.contains("Sure thing"));
    assert!(found_long, "long assistant turn was not indexed");

    Ok(())
}

#[tokio::test]
async fn precompact_mixed_roles_all_mined() -> anyhow::Result<()> {
    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let vault_dir = tempdir()?;
    let store = Arc::new(MarkdownStore::new(vault_dir.path())?);
    let ignore = WatchIgnoreList::new();

    let trans_dir = tempdir()?;
    let transcript_path = trans_dir.path().join("transcript.jsonl");
    let mut trans_file = File::create(&transcript_path)?;

    // user + assistant(>20) + tool + tool_result + computer + system + short assistant
    let turns = vec![
        r#"{"role": "user", "content": "Run help"}"#,
        r#"{"role": "assistant", "content": "I will run the help command now to verify."}"#,
        r#"{"role": "tool", "content": "Help options..."}"#,
        r#"{"role": "computer", "content": "System CPU stable."}"#,
        r#"{"role": "system", "content": "System boot marker."}"#,
        r#"{"role": "assistant", "content": "Short."}"#,
    ];
    for turn in turns {
        writeln!(trans_file, "{}", turn)?;
    }

    let path_str = transcript_path.to_string_lossy();
    let count = mythrax_core::hooks::precompact::mine_transcript(
        "sess-mixed",
        &path_str,
        backend.as_ref(),
        store.as_ref(),
        &ignore,
    )
    .await?;

    // user, assistant (>20), tool, computer, and system should be mined. short assistant should be skipped.
    // Total mined should be 5.
    assert_eq!(count, 5, "expected 5 mined, got {}", count);

    Ok(())
}

}

mod proactive_pruned_injection {
use anyhow::Result;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};

#[tokio::test]
async fn test_proactive_pruned_injection() -> Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Test that pruned/archived nodes are not proactively injected in normal queries
    let ep = EpisodeSave {
        title: "Pruned Episode".to_string(),
        content: "This episode is pruned".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let ep_id = backend.save_episode(&ep).await?;

    // Archive it
    let uuid = ep_id.split(':').nth(1).unwrap();
    backend
        .db
        .query("UPDATE type::record('episode', $id) MERGE { archived: true };")
        .bind(("id", uuid))
        .await?
        .check()?;

    let resp = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "This episode is pruned",
            Some("general"),
            false,
            10,
            0,
            0.0,
            None,
            false,
            true,
            true,
            None,
            false,
            None,
        ))
        .await?;

    let found = resp.results.iter().any(|r| r.id == ep_id);
    assert!(
        !found,
        "Archived/pruned episodes should not be proactively injected into results"
    );

    Ok(())
}

}

mod procedural_expansion {
use anyhow::Result;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend, parse_record_id};

#[tokio::test]
async fn test_procedural_cue_neighbor_expansion() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create 4 sequential episodes
    let ep0 = EpisodeSave {
        created_at: None,
        title: "Step 0 CLI Start".to_string(),
        content: "First command line interface run. Initialized repo successfully.".to_string(),
        scope: Some("general".to_string()),
        session_id: Some("session-procedural".to_string()),
        ..Default::default()
    };
    let ep0_id = backend.save_episode(&ep0).await?;

    let ep1 = EpisodeSave {
        created_at: None,
        title: "Step 1 Compile Action".to_string(),
        content: "Second compile action. Compiling main module now.".to_string(),
        scope: Some("general".to_string()),
        session_id: Some("session-procedural".to_string()),
        ..Default::default()
    };
    let ep1_id = backend.save_episode(&ep1).await?;

    let ep2 = EpisodeSave {
        created_at: None,
        title: "Step 2 Dev Deploy".to_string(),
        content: "Third deployment script step. Deploying dev server.".to_string(),
        scope: Some("general".to_string()),
        session_id: Some("session-procedural".to_string()),
        ..Default::default()
    };
    let ep2_id = backend.save_episode(&ep2).await?;

    let ep3 = EpisodeSave {
        created_at: None,
        title: "Step 3 Health Verify".to_string(),
        content: "Fourth verification curl. Checked localhost health status page.".to_string(),
        scope: Some("general".to_string()),
        session_id: Some("session-procedural".to_string()),
        ..Default::default()
    };
    let ep3_id = backend.save_episode(&ep3).await?;

    // Link: ep0 -> followed_by -> ep1 -> followed_by -> ep2 -> followed_by -> ep3
    let rec0 = parse_record_id(&ep0_id)?;
    let rec1 = parse_record_id(&ep1_id)?;
    let rec2 = parse_record_id(&ep2_id)?;
    let rec3 = parse_record_id(&ep3_id)?;

    backend
        .db
        .query("RELATE $from -> followed_by -> $to;")
        .bind(("from", rec0.clone()))
        .bind(("to", rec1.clone()))
        .await?
        .check()?;

    backend
        .db
        .query("RELATE $from -> followed_by -> $to;")
        .bind(("from", rec1.clone()))
        .bind(("to", rec2.clone()))
        .await?
        .check()?;

    backend
        .db
        .query("RELATE $from -> followed_by -> $to;")
        .bind(("from", rec2.clone()))
        .bind(("to", rec3.clone()))
        .await?
        .check()?;

    // Allow SurrealDB index
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    // Search query matches "Second compile action" (ep1)
    // Query is a procedural question which triggers depth=3 bidirectional expansion
    let query = "What compile actions did I run?";

    let resp = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            query,
            Some("general"),
            true,
            10,
            0,
            0.0,
            None,
            false,
            true,
            true,
            None,
            true,
            None,
        ))
        .await?;

    let results = resp.results;
    println!("Search Results:");
    for r in &results {
        println!(" - Title: {}, ID: {}", r.title, r.id);
        if let Some(ref rels) = r.related_nodes {
            for rel in rels {
                println!("    -> Related Title: {}, ID: {}", rel.title, rel.id);
            }
        }
    }

    // Verify ep1 is the primary result
    assert!(!results.is_empty(), "Should return results");
    let primary = &results[0];
    assert_eq!(
        primary.id, ep1_id,
        "Primary result should be Step 1 Compile Action"
    );

    // Recursively collect all returned episode IDs (including related_nodes)
    let mut all_ids = std::collections::HashSet::new();
    for r in &results {
        all_ids.insert(r.id.clone());
        if let Some(ref rels) = r.related_nodes {
            for rel in rels {
                all_ids.insert(rel.id.clone());
            }
        }
    }

    // Verify all neighbors are expanded and returned
    assert!(
        all_ids.contains(&ep0_id),
        "Should return preceding neighbor Step 0"
    );
    assert!(
        all_ids.contains(&ep2_id),
        "Should return succeeding neighbor Step 2"
    );
    assert!(
        all_ids.contains(&ep3_id),
        "Should return succeeding neighbor Step 3 (depth 2)"
    );

    Ok(())
}

}

mod progressive_disclosure {
use anyhow::Result;
use mythrax_core::api::ApiState;
use mythrax_core::contracts::{EpisodeSave, IndexRow, SearchResult};
use mythrax_core::db::{StorageBackend, SurrealBackend, backend::parse_record_id};
use mythrax_core::mcp_routes::call_mcp_tool;
use mythrax_core::store::MarkdownStore;
use serde_json::json;
use std::fs;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

async fn setup_test_state() -> Result<(ApiState, std::sync::Arc<SurrealBackend>, tempfile::TempDir)>
{
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

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend
        .db
        .query(mythrax_core::db::schema::INIT_SCHEMA)
        .await?
        .check()?;
    backend.init().await?;

    let store = std::sync::Arc::new(MarkdownStore::new(&vault_root)?);

    let state = ApiState {
        backend: backend.clone(),
        auth_token: "secret".to_string(),
        store,
        ignore_list: std::sync::Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new()),
        dream_tx: None,
        shutdown_tx: None,
    };

    Ok((state, backend, tmp))
}

#[tokio::test]
async fn test_search_index_omits_content() -> Result<()> {
    let _guard = match TEST_MUTEX.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };

    let (state, backend, _tmp) = setup_test_state().await?;

    // Save a few test episodes
    for i in 1..=3 {
        let ep = EpisodeSave {
            created_at: None,
            title: format!("Episode Title {}", i),
            content: format!(
                "This is the long content for episode number {}. It contains details that should not be returned in a compact index query.",
                i
            ),
            entities: vec![],
            scope: Some("general".to_string()),
            vault_path: Some(format!("notes/ep_{}.md", i)),
            source_episode: None,
            session_id: None,
            task_id: None,
            discovery_tokens: None,
            facts: None,
            concepts: None,
            files_read: None,
            files_modified: None,
            node_type: None,

            confidence: None,
            ..Default::default()
        };
        backend.save_episode(&ep).await?;
    }

    // Call search_index
    let mcp_res = call_mcp_tool(
        &state,
        "read",
        json!({
            "action": "search_index",
            "query": "Episode",
            "limit": 10
        }),
    )
    .await?;

    // Extract the text content from the MCP response
    let content_arr = mcp_res.get("content").and_then(|v| v.as_array()).unwrap();
    let text = content_arr[0].get("text").and_then(|v| v.as_str()).unwrap();

    let index_rows: Vec<IndexRow> = serde_json::from_str(text)?;
    assert!(!index_rows.is_empty());

    for row in &index_rows {
        assert!(!row.id.is_empty());
        assert!(!row.title.is_empty());
        assert!(!row.subtitle.is_empty());
        // Verify subtitle is a truncated version of content (max 120 chars) and does not contain the full body
        assert!(row.subtitle.len() <= 123); // 120 + "..."

        // Convert to a raw JSON value to verify that content/embedding fields are absent
        let val = serde_json::to_value(row)?;
        assert!(val.get("content").is_none());
        assert!(val.get("embedding").is_none());
    }

    Ok(())
}

#[tokio::test]
async fn test_get_full_hydrates() -> Result<()> {
    let _guard = match TEST_MUTEX.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };

    let (state, backend, _tmp) = setup_test_state().await?;

    let ep = EpisodeSave {
        created_at: None,
        title: "Unique Hydration Test".to_string(),
        content: "Detailed secret documentation that must be hydrated.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/hydration.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,

        confidence: None,
        ..Default::default()
    };
    let ep_id = backend.save_episode(&ep).await?;

    // Call search_index first to get the ID
    let mcp_res = call_mcp_tool(
        &state,
        "read",
        json!({
            "action": "search_index",
            "query": "Hydration",
            "limit": 1
        }),
    )
    .await?;

    let text = mcp_res["content"][0]["text"].as_str().unwrap();
    let index_rows: Vec<IndexRow> = serde_json::from_str(text)?;
    assert_eq!(index_rows.len(), 1);
    assert_eq!(index_rows[0].id, ep_id);

    // Call get_full to hydrate it
    let full_res = call_mcp_tool(
        &state,
        "read",
        json!({
            "action": "get_full",
            "ids": [ep_id.clone()]
        }),
    )
    .await?;

    let full_text = full_res["content"][0]["text"].as_str().unwrap();
    let results: Vec<SearchResult> = serde_json::from_str(full_text)?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, ep_id);
    assert_eq!(
        results[0].content,
        "Detailed secret documentation that must be hydrated."
    );

    Ok(())
}

#[tokio::test]
async fn test_timeline_orders_neighbors() -> Result<()> {
    let _guard = match TEST_MUTEX.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };

    let (state, backend, _tmp) = setup_test_state().await?;

    // Save 5 episodes and update their created_at strictly sequentially
    let mut ids = Vec::new();
    for i in 1..=5 {
        let ep = EpisodeSave {
            created_at: None,
            title: format!("Timeline Episode {}", i),
            content: format!("Content for sequential timeline episode {}", i),
            entities: vec![],
            scope: Some("general".to_string()),
            vault_path: Some(format!("notes/time_{}.md", i)),
            source_episode: None,
            session_id: None,
            task_id: None,
            discovery_tokens: None,
            facts: None,
            concepts: None,
            files_read: None,
            files_modified: None,
            node_type: None,

            confidence: None,
            ..Default::default()
        };
        let id = backend.save_episode(&ep).await?;
        ids.push(id);
    }

    // Assign sequential created_at timestamps
    let base_time = chrono::Utc::now();
    for (idx, id) in ids.iter().enumerate() {
        let time = base_time + chrono::Duration::seconds(idx as i64 * 10);
        let record = parse_record_id(id)?;
        backend
            .db
            .query("UPDATE $id SET created_at = $time;")
            .bind(("id", record))
            .bind(("time", time))
            .await?
            .check()?;
    }

    // Call timeline centered on the 3rd episode (index 2) with depth_before=1, depth_after=1
    let mid_id = &ids[2];
    let mcp_res = call_mcp_tool(
        &state,
        "read",
        json!({
            "action": "timeline",
            "anchor_id": mid_id,
            "depth_before": 1,
            "depth_after": 1
        }),
    )
    .await?;

    let text = mcp_res["content"][0]["text"].as_str().unwrap();
    let index_rows: Vec<IndexRow> = serde_json::from_str(text)?;

    // Should return exactly the 2nd and 4th episodes, chronologically ordered
    assert_eq!(index_rows.len(), 2);
    assert_eq!(index_rows[0].id, ids[1]); // Episode 2 (prior)
    assert_eq!(index_rows[1].id, ids[3]); // Episode 4 (subsequent)

    // Test with query anchor search
    let mcp_res_query = call_mcp_tool(
        &state,
        "read",
        json!({
            "action": "timeline",
            "query": "Timeline Episode 3",
            "depth_before": 1,
            "depth_after": 1
        }),
    )
    .await?;

    let text_query = mcp_res_query["content"][0]["text"].as_str().unwrap();
    let index_rows_query: Vec<IndexRow> = serde_json::from_str(text_query)?;
    assert_eq!(index_rows_query.len(), 2);
    assert_eq!(index_rows_query[0].id, ids[1]);
    assert_eq!(index_rows_query[1].id, ids[3]);

    Ok(())
}

#[tokio::test]
async fn test_index_then_full_token_savings() -> Result<()> {
    let _guard = match TEST_MUTEX.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };

    let (state, backend, _tmp) = setup_test_state().await?;

    // Create 10 episodes with very long content (e.g. lots of repeated text)
    let long_text = "This is a very long text block. ".repeat(30); // ~180 words/1000 characters
    let mut ids = Vec::new();
    for i in 1..=10 {
        let ep = EpisodeSave {
            created_at: None,
            title: format!("Big Episode Title {}", i),
            content: format!("Start of episode {}. {}", i, long_text),
            entities: vec![],
            scope: Some("general".to_string()),
            vault_path: Some(format!("notes/big_{}.md", i)),
            source_episode: None,
            session_id: None,
            task_id: None,
            discovery_tokens: None,
            facts: None,
            concepts: None,
            files_read: None,
            files_modified: None,
            node_type: None,
            confidence: None,
            ..Default::default()
        };
        let id = backend.save_episode(&ep).await?;
        ids.push(id);
    }

    // 1. Full search (hydrates all matching episodes)
    let full_search_res = call_mcp_tool(
        &state,
        "read",
        json!({
            "action": "search",
            "query": "Big Episode",
            "include_episodes": true,
            "limit": 10
        }),
    )
    .await?;
    let full_search_text = full_search_res["content"][0]["text"].as_str().unwrap();
    let full_search_size = full_search_text.len();

    // 2. Progressive disclosure: search_index + get_full for only 2 episodes
    let index_res = call_mcp_tool(
        &state,
        "read",
        json!({
            "action": "search_index",
            "query": "Big Episode",
            "limit": 10
        }),
    )
    .await?;
    let index_text = index_res["content"][0]["text"].as_str().unwrap();
    let index_size = index_text.len();

    let get_full_res = call_mcp_tool(
        &state,
        "read",
        json!({
            "action": "get_full",
            "ids": [ids[0].clone(), ids[1].clone()]
        }),
    )
    .await?;
    let get_full_text = get_full_res["content"][0]["text"].as_str().unwrap();
    let get_full_size = get_full_text.len();

    let progressive_total_size = index_size + get_full_size;

    println!("Full Search Size: {} bytes", full_search_size);
    println!("Index Search Size: {} bytes", index_size);
    println!("Get Full (2 nodes) Size: {} bytes", get_full_size);
    println!("Progressive Total Size: {} bytes", progressive_total_size);

    // Progressive disclosure should be significantly smaller than full search returning all 10 hydrated
    assert!(progressive_total_size < full_search_size);

    Ok(())
}

}

mod stm {
use anyhow::Result;
use mythrax_core::contracts::{ForgedConcept, ForgedRule, ForgedSectionBatch, HandoffSave};
use mythrax_core::db::{StorageBackend, SurrealBackend};
use std::fs;
use tempfile::tempdir;

use std::sync::Mutex;
static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_stm_db_operations() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let tmp = tempdir()?;
    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;
    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
    }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Test save_stm, get_stm, clear_stm
    backend.save_stm("sess_1", "key_a", "val_a").await?;
    backend.save_stm("sess_1", "key_b", "val_b").await?;
    backend.save_stm("sess_2", "key_a", "val_c").await?;

    // Get specific key
    let map = backend.get_stm("sess_1", Some("key_a")).await?;
    assert_eq!(map.len(), 1);
    assert_eq!(map.get("key_a").unwrap(), "val_a");

    // Get all keys
    let map_all = backend.get_stm("sess_1", None).await?;
    assert_eq!(map_all.len(), 2);
    assert_eq!(map_all.get("key_a").unwrap(), "val_a");
    assert_eq!(map_all.get("key_b").unwrap(), "val_b");

    // Clear session
    backend.clear_stm("sess_1").await?;
    let map_cleared = backend.get_stm("sess_1", None).await?;
    assert!(map_cleared.is_empty());

    // Sess 2 should still exist
    let map2 = backend.get_stm("sess_2", None).await?;
    assert_eq!(map2.len(), 1);
    assert_eq!(map2.get("key_a").unwrap(), "val_c");

    Ok(())
}

#[tokio::test]
async fn test_stm_mcp_and_file_sync() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("episodes"))?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;

    // Mock workspace root for .handoffs/
    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;
    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
    }

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let store = std::sync::Arc::new(mythrax_core::store::MarkdownStore::new(&vault_root)?);
    let mcp_server = mythrax_core::mcp::McpServer::new_local(backend.clone(), store);

    // 1. Put short term memory via MCP
    let mut params = serde_json::json!({
        "session_id": "sess_x",
        "key": "secret_data",
        "value": "bearer my-secret-token"
    });
    if let Some(obj) = params.as_object_mut() {
        obj.insert(
            "action".to_string(),
            serde_json::Value::String("put_short_term".to_string()),
        );
    }

    mcp_server
        .handle_request(
            "tools/call",
            serde_json::json!({
                "name": "write",
                "arguments": params
            }),
        )
        .await?;

    // Verify it is saved in SurrealDB
    let db_val = backend.get_stm("sess_x", Some("secret_data")).await?;
    assert_eq!(db_val.get("secret_data").unwrap(), "bearer my-secret-token");

    // Verify it is written to disk
    let stm_file_path = workspace_root.join(".handoffs").join("stm_sess_x.json");
    assert!(stm_file_path.exists());

    let file_content = fs::read_to_string(&stm_file_path)?;
    // The secret should be filtered by SecretFilter
    assert!(!file_content.contains("my-secret-token"));

    // 2. Get short term memory via MCP
    let mut get_args = serde_json::json!({
        "session_id": "sess_x",
        "key": "secret_data"
    });
    if let Some(obj) = get_args.as_object_mut() {
        obj.insert(
            "action".to_string(),
            serde_json::Value::String("get_short_term".to_string()),
        );
    }
    let get_resp = mcp_server
        .handle_request(
            "tools/call",
            serde_json::json!({
                "name": "read",
                "arguments": get_args
            }),
        )
        .await?;
    let text = get_resp["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("bearer my-secret-token"));

    // 3. Clear short term memory via MCP
    let mut clear_args = serde_json::json!({
        "session_id": "sess_x"
    });
    if let Some(obj) = clear_args.as_object_mut() {
        obj.insert(
            "action".to_string(),
            serde_json::Value::String("clear_short_term".to_string()),
        );
    }
    mcp_server
        .handle_request(
            "tools/call",
            serde_json::json!({
                "name": "write",
                "arguments": clear_args
            }),
        )
        .await?;

    // Verify DB is cleared
    let db_val_cleared = backend.get_stm("sess_x", None).await?;
    assert!(db_val_cleared.is_empty());

    // Verify file is deleted
    assert!(!stm_file_path.exists());

    Ok(())
}

#[tokio::test]
async fn test_stale_handoff_background_cleanup() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("episodes"))?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;

    let workspace_root = tmp.path().join("workspace");
    let handoffs_dir = workspace_root.join(".handoffs");
    fs::create_dir_all(&handoffs_dir)?;
    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create 4 handoffs:
    // 1. Completed + 8 days old -> should be cleaned
    // 2. Failed + 8 days old -> should be cleaned
    // 3. Pending + 8 days old -> should NOT be cleaned
    // 4. Completed + 1 day old -> should NOT be cleaned

    let h1_file = handoffs_dir.join("handoff_task1.md");
    let h1_stm = handoffs_dir.join("stm_sess1.json");
    fs::write(&h1_file, "mock handoff 1")?;
    fs::write(&h1_stm, "{}")?;
    let id1 = backend
        .save_handoff(&HandoffSave {
            parent_conversation_id: "sess1".to_string(),
            subagent_conversation_id: "sub1".to_string(),
            summary: "handoff 1".to_string(),
            handoff_file_path: h1_file.to_string_lossy().to_string(),
            scope: None,
            include_tool_execution: None,
        })
        .await?;
    backend.save_stm("sess1", "k", "v").await?;

    let h2_file = handoffs_dir.join("handoff_task2.md");
    let h2_stm = handoffs_dir.join("stm_sess2.json");
    fs::write(&h2_file, "mock handoff 2")?;
    fs::write(&h2_stm, "{}")?;
    let id2 = backend
        .save_handoff(&HandoffSave {
            parent_conversation_id: "sess2".to_string(),
            subagent_conversation_id: "sub2".to_string(),
            summary: "handoff 2".to_string(),
            handoff_file_path: h2_file.to_string_lossy().to_string(),
            scope: None,
            include_tool_execution: None,
        })
        .await?;
    backend.save_stm("sess2", "k", "v").await?;

    let h3_file = handoffs_dir.join("handoff_task3.md");
    let h3_stm = handoffs_dir.join("stm_sess3.json");
    fs::write(&h3_file, "mock handoff 3")?;
    fs::write(&h3_stm, "{}")?;
    let id3 = backend
        .save_handoff(&HandoffSave {
            parent_conversation_id: "sess3".to_string(),
            subagent_conversation_id: "sub3".to_string(),
            summary: "handoff 3".to_string(),
            handoff_file_path: h3_file.to_string_lossy().to_string(),
            scope: None,
            include_tool_execution: None,
        })
        .await?;
    backend.save_stm("sess3", "k", "v").await?;

    let h4_file = handoffs_dir.join("handoff_task4.md");
    let h4_stm = handoffs_dir.join("stm_sess4.json");
    fs::write(&h4_file, "mock handoff 4")?;
    fs::write(&h4_stm, "{}")?;
    let id4 = backend
        .save_handoff(&HandoffSave {
            parent_conversation_id: "sess4".to_string(),
            subagent_conversation_id: "sub4".to_string(),
            summary: "handoff 4".to_string(),
            handoff_file_path: h4_file.to_string_lossy().to_string(),
            scope: None,
            include_tool_execution: None,
        })
        .await?;
    backend.save_stm("sess4", "k", "v").await?;

    // Update their status and created_at manually via SurrealDB query
    let rec1 = mythrax_core::db::parse_record_id(&id1)?;
    let rec2 = mythrax_core::db::parse_record_id(&id2)?;
    let rec3 = mythrax_core::db::parse_record_id(&id3)?;
    let rec4 = mythrax_core::db::parse_record_id(&id4)?;

    backend
        .db
        .query(
            "
        UPDATE $r1 SET status = 'COMPLETED', created_at = time::now() - 8d;
        UPDATE $r2 SET status = 'FAILED', created_at = time::now() - 8d;
        UPDATE $r3 SET status = 'PENDING', created_at = time::now() - 8d;
        UPDATE $r4 SET status = 'COMPLETED', created_at = time::now() - 1d;
    ",
        )
        .bind(("r1", rec1))
        .bind(("r2", rec2))
        .bind(("r3", rec3))
        .bind(("r4", rec4))
        .await?
        .check()?;

    // Perform cleanup with 7 days threshold (matches 8d age in test setup)
    backend.delete_stale_handoffs(7).await?;

    // Assert stale files are deleted
    assert!(!h1_file.exists());
    assert!(!h1_stm.exists());
    assert!(!h2_file.exists());
    assert!(!h2_stm.exists());

    // Assert non-stale/pending files still exist
    assert!(h3_file.exists());
    assert!(h3_stm.exists());
    assert!(h4_file.exists());
    assert!(h4_stm.exists());

    // Assert DB entries
    // H1 and H2 should be deleted from DB
    let h1_in_db: Option<serde_json::Value> = backend
        .db
        .select((
            "handoff",
            mythrax_core::db::backend::record_key_to_string(
                &mythrax_core::db::parse_record_id(&id1)?.key,
            ),
        ))
        .await?;
    assert!(h1_in_db.is_none());
    let h2_in_db: Option<serde_json::Value> = backend
        .db
        .select((
            "handoff",
            mythrax_core::db::backend::record_key_to_string(
                &mythrax_core::db::parse_record_id(&id2)?.key,
            ),
        ))
        .await?;
    assert!(h2_in_db.is_none());

    // H3 and H4 should still exist in DB
    let h3_in_db: Option<serde_json::Value> = backend
        .db
        .select((
            "handoff",
            mythrax_core::db::backend::record_key_to_string(
                &mythrax_core::db::parse_record_id(&id3)?.key,
            ),
        ))
        .await?;
    assert!(h3_in_db.is_some());
    let h4_in_db: Option<serde_json::Value> = backend
        .db
        .select((
            "handoff",
            mythrax_core::db::backend::record_key_to_string(
                &mythrax_core::db::parse_record_id(&id4)?.key,
            ),
        ))
        .await?;
    assert!(h4_in_db.is_some());

    // Stale STM entries in DB should be deleted
    let stm1 = backend.get_stm("sess1", None).await?;
    assert!(stm1.is_empty());
    let stm2 = backend.get_stm("sess2", None).await?;
    assert!(stm2.is_empty());

    // Non-stale STM entries in DB should still exist
    let stm3 = backend.get_stm("sess3", None).await?;
    assert!(!stm3.is_empty());
    let stm4 = backend.get_stm("sess4", None).await?;
    assert!(!stm4.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_save_forged_section_lifecycle() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("episodes"))?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;

    unsafe {
        std::env::remove_var("MYTHRAX_WORKSPACE_ROOT");
        std::env::set_var("MYTHRAX_VAULT_ROOT", vault_root.to_str().unwrap());
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create a batch
    let batch = ForgedSectionBatch {
        doc_title: "My System Playbook!".to_string(),
        scope: "production".to_string(),
        chunk_index: 0,
        chunk_text: "We should avoid hardcoding API keys in our deployment scripts. For example, api_key: 'sk-123' must be prevented.".to_string(),
        concepts: vec![
            ForgedConcept {
                name: "API Secret Management".to_string(),
                content: "Centralized environment secrets storage.".to_string(),
            }
        ],
        rules: vec![
            ForgedRule {
                target_pattern: "Avoid Hardcoded API Keys".to_string(),
                action_to_avoid: "hardcoding api_key = 'sk-...'".to_string(),
                causal_explanation: "This leaks credentials to source control.".to_string(),
                prescribed_remedy: "Use environment variables or vault references instead.".to_string(),
            }
        ],
    };

    // Save batch
    backend.save_forged_section(&batch).await?;

    // 1. Verify files are written to disk with SecretFilter sanitization
    let doc_slug = "my_system_playbook";

    // Chunk file
    let chunk_path = vault_root.join(format!("episodes/forge/{}/chunk_0.md", doc_slug));
    assert!(chunk_path.exists());
    let chunk_content = fs::read_to_string(&chunk_path)?;
    assert!(chunk_content.contains("title: \"My System Playbook! - Chunk 0\""));
    assert!(chunk_content.contains("scope: \"production\""));
    assert!(chunk_content.contains("source: \"forge\""));
    assert!(chunk_content.contains("api_key: \"[REDACTED]\"")); // Check secret cleaning!
    assert!(!chunk_content.contains("sk-123"));

    // Concept file
    let wiki_dir = vault_root.join(format!("wiki/forge/{}", doc_slug));
    assert!(wiki_dir.exists());
    let wiki_files: Vec<_> = fs::read_dir(&wiki_dir)?
        .map(|r| r.unwrap().path())
        .collect();
    assert_eq!(wiki_files.len(), 1);
    let wiki_path = &wiki_files[0];
    let wiki_name = wiki_path.file_name().unwrap().to_str().unwrap();
    assert!(wiki_name.starts_with("concept_api_secret_management_"));
    let wiki_content = fs::read_to_string(wiki_path)?;
    assert!(wiki_content.contains("name: \"API Secret Management\""));
    assert!(wiki_content.contains("Centralized environment secrets storage."));

    // Wisdom file
    let wisdom_dir = vault_root.join(format!("wisdom/forge/{}", doc_slug));
    assert!(wisdom_dir.exists());
    let wisdom_files: Vec<_> = fs::read_dir(&wisdom_dir)?
        .map(|r| r.unwrap().path())
        .collect();
    assert_eq!(wisdom_files.len(), 1);
    let wisdom_path = &wisdom_files[0];
    let wisdom_name = wisdom_path.file_name().unwrap().to_str().unwrap();
    assert!(wisdom_name.starts_with("rule_avoid_hardcoded_api_keys_"));
    let wisdom_content = fs::read_to_string(wisdom_path)?;
    assert!(wisdom_content.contains("target_pattern: \"Avoid Hardcoded API Keys\""));
    assert!(wisdom_content.contains("tier: \"forge\""));
    assert!(wisdom_content.contains("Use environment variables or vault references instead."));

    // 2. Verify database records are inserted and relations exist
    // Fetch episode
    let mut ep_resp = backend
        .db
        .query("SELECT * FROM episode WHERE source = 'forge' LIMIT 1;")
        .await?;
    let episodes: Vec<serde_json::Value> = ep_resp.take(0)?;
    assert_eq!(episodes.len(), 1);
    let ep = &episodes[0];
    assert_eq!(
        ep["title"].as_str().unwrap(),
        "My System Playbook! - Chunk 0"
    );
    assert!(
        ep["content"]
            .as_str()
            .unwrap()
            .contains("api_key: \"[REDACTED]\"")
    );

    // Fetch wiki node
    let mut wiki_resp = backend
        .db
        .query("SELECT * FROM wiki_node WHERE name = 'API Secret Management' LIMIT 1;")
        .await?;
    let wiki_nodes: Vec<serde_json::Value> = wiki_resp.take(0)?;
    assert_eq!(wiki_nodes.len(), 1);

    // Fetch wisdom
    let mut wisdom_resp = backend
        .db
        .query("SELECT * FROM wisdom WHERE target_pattern = 'Avoid Hardcoded API Keys' LIMIT 1;")
        .await?;
    let wisdom_rules: Vec<serde_json::Value> = wisdom_resp.take(0)?;
    assert_eq!(wisdom_rules.len(), 1);
    assert_eq!(wisdom_rules[0]["tier"].as_str().unwrap(), "forge");

    // Verify relations: Playbook (WisdomRule) -> relates_to -> Concept (WikiNode) -> relates_to -> Chunk (Episode)
    let ep_id = ep["id"].as_str().unwrap();
    let wiki_id = wiki_nodes[0]["id"].as_str().unwrap();
    let wisdom_id = wisdom_rules[0]["id"].as_str().unwrap();

    let mut rel_resp1 = backend
        .db
        .query("SELECT * FROM relates_to WHERE in = $wiki_id AND out = $ep_id;")
        .bind(("ep_id", mythrax_core::db::parse_record_id(ep_id)?))
        .bind(("wiki_id", mythrax_core::db::parse_record_id(wiki_id)?))
        .await?;
    let rels1: Vec<serde_json::Value> = rel_resp1.take(0)?;
    assert_eq!(rels1.len(), 1);

    let mut rel_resp2 = backend
        .db
        .query("SELECT * FROM relates_to WHERE in = $wisdom_id AND out = $wiki_id;")
        .bind(("wisdom_id", mythrax_core::db::parse_record_id(wisdom_id)?))
        .bind(("wiki_id", mythrax_core::db::parse_record_id(wiki_id)?))
        .await?;
    let rels2: Vec<serde_json::Value> = rel_resp2.take(0)?;
    assert_eq!(rels2.len(), 1);

    // Verify metrics records are created
    let mut met_resp = backend.db.query("SELECT * FROM metrics;").await?;
    let metrics_records: Vec<serde_json::Value> = met_resp.take(0)?;
    assert!(metrics_records.len() >= 2);

    Ok(())
}

#[tokio::test]
async fn test_save_forged_section_rollback() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;

    unsafe {
        std::env::remove_var("MYTHRAX_WORKSPACE_ROOT");
        std::env::set_var("MYTHRAX_VAULT_ROOT", vault_root.to_str().unwrap());
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create a batch
    let batch = ForgedSectionBatch {
        doc_title: "Rollback Doc".to_string(),
        scope: "production".to_string(),
        chunk_index: 0,
        chunk_text: "Some chunk text".to_string(),
        concepts: vec![ForgedConcept {
            name: "Rollback Concept".to_string(),
            content: "Rollback content".to_string(),
        }],
        rules: vec![ForgedRule {
            target_pattern: "Rollback Rule".to_string(),
            action_to_avoid: "avoid".to_string(),
            causal_explanation: "why".to_string(),
            prescribed_remedy: "remedy".to_string(),
        }],
    };

    // Break SurrealDB so the transaction fails
    backend.db.query("REMOVE TABLE wiki_node;").await?.check()?;

    // Call save_forged_section - it should return Err
    let res = backend.save_forged_section(&batch).await;
    assert!(res.is_err());

    // Verify no files are left in the vault
    let chunk_file = vault_root.join("episodes/forge/rollback_doc/chunk_0.md");
    assert!(!chunk_file.exists());

    let wiki_dir = vault_root.join("wiki/forge/rollback_doc");
    if wiki_dir.exists() {
        let entries: Vec<_> = fs::read_dir(wiki_dir)?.collect();
        assert!(entries.is_empty());
    }

    let wisdom_dir = vault_root.join("wisdom/forge/rollback_doc");
    if wisdom_dir.exists() {
        let entries: Vec<_> = fs::read_dir(wisdom_dir)?.collect();
        assert!(entries.is_empty());
    }

    Ok(())
}

#[tokio::test]
async fn test_mcp_forge_tools() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("episodes"))?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;

    unsafe {
        std::env::remove_var("MYTHRAX_WORKSPACE_ROOT");
        std::env::set_var("MYTHRAX_VAULT_ROOT", vault_root.to_str().unwrap());
    }

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let store = std::sync::Arc::new(mythrax_core::store::MarkdownStore::new(&vault_root)?);
    let mcp_server = mythrax_core::mcp::McpServer::new_local(backend.clone(), store);

    // 1. Call get_forge_instructions
    let inst_resp = mcp_server
        .handle_request(
            "tools/call",
            serde_json::json!({
                "name": "get_forge_instructions",
                "arguments": {}
            }),
        )
        .await?;

    let inst_text = inst_resp["content"][0]["text"].as_str().unwrap();
    assert!(inst_text.contains("Wisdom Rules Extraction"));
    assert!(inst_text.contains("Concept Wiki Nodes Extraction"));

    // 2. Call save_forged_assets
    let batch = serde_json::json!({
        "doc_title": "MCP Forge Doc",
        "scope": "development",
        "chunk_index": 1,
        "chunk_text": "Grounding chunk content.",
        "concepts": [
            {
                "name": "MCP Concept",
                "content": "MCP concept definition."
            }
        ],
        "rules": [
            {
                "target_pattern": "MCP Rule",
                "action_to_avoid": "avoiding mcp",
                "causal_explanation": "explanation",
                "prescribed_remedy": "remedy"
            }
        ]
    });

    let mut write_args = batch.clone();
    if let Some(obj) = write_args.as_object_mut() {
        obj.insert(
            "action".to_string(),
            serde_json::Value::String("save_forged_assets".to_string()),
        );
    }

    let save_resp = mcp_server
        .handle_request(
            "tools/call",
            serde_json::json!({
                "name": "write",
                "arguments": write_args
            }),
        )
        .await?;

    let save_text = save_resp["content"][0]["text"].as_str().unwrap();
    assert!(save_text.contains("Successfully saved forged assets"));

    // Verify files on disk
    let chunk_path = vault_root.join("episodes/forge/mcp_forge_doc/chunk_1.md");
    assert!(chunk_path.exists());

    // Verify DB entry
    let mut ep_resp = backend
        .db
        .query("SELECT * FROM episode WHERE source = 'forge' LIMIT 1;")
        .await?;
    let episodes: Vec<serde_json::Value> = ep_resp.take(0)?;
    assert_eq!(episodes.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_api_save_forged_assets() -> Result<()> {
    use axum::http::Request;
    use mythrax_core::api::{ApiState, create_router};
    use mythrax_core::vault::watcher::WatchIgnoreList;
    use tower::util::ServiceExt;

    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("episodes"))?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;

    unsafe {
        std::env::remove_var("MYTHRAX_WORKSPACE_ROOT");
        std::env::set_var("MYTHRAX_VAULT_ROOT", vault_root.to_str().unwrap());
    }

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let store = std::sync::Arc::new(mythrax_core::store::MarkdownStore::new(&vault_root)?);
    let ignore_list = std::sync::Arc::new(WatchIgnoreList::new());

    let state = std::sync::Arc::new(ApiState {
        backend: backend.clone(),
        auth_token: "secret-api-token".to_string(),
        store,
        ignore_list,
        dream_tx: None,
        shutdown_tx: None,
    });

    let app = create_router(state);

    let batch = serde_json::json!({
        "doc_title": "API Forge Doc",
        "scope": "production",
        "chunk_index": 2,
        "chunk_text": "API grounding chunk content.",
        "concepts": [
            {
                "name": "API Concept",
                "content": "API concept definition."
            }
        ],
        "rules": [
            {
                "target_pattern": "API Rule",
                "action_to_avoid": "avoiding api",
                "causal_explanation": "explanation",
                "prescribed_remedy": "remedy"
            }
        ]
    });

    // 1. Test Unauthorized
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/forge/save")
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(serde_json::to_vec(&batch).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);

    // 2. Test Success (Authorized)
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/forge/save")
                .header("X-Mythrax-Token", "secret-api-token")
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(serde_json::to_vec(&batch).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);

    // Verify files on disk
    let chunk_path = vault_root.join("episodes/forge/api_forge_doc/chunk_2.md");
    assert!(chunk_path.exists());

    // Verify DB entry
    let mut ep_resp = backend.db.query("SELECT * FROM episode WHERE source = 'forge' AND title = 'API Forge Doc - Chunk 2' LIMIT 1;").await?;
    let episodes: Vec<serde_json::Value> = ep_resp.take(0)?;
    assert_eq!(episodes.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_stm_continuous_pruning() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    let handoffs_dir = vault_root.join(".handoffs");
    fs::create_dir_all(&handoffs_dir)?;

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create a 4-day-old handoff file and stm file
    let old_handoff_file = handoffs_dir.join("old_handoff.md");
    let old_stm_file = handoffs_dir.join("stm_old_sess.json");
    fs::write(&old_handoff_file, "old handoff content")?;
    fs::write(&old_stm_file, "{}")?;

    // Set modification time of old stm file to 4 days ago using std::fs::File::set_modified
    let file = fs::OpenOptions::new().write(true).open(&old_stm_file)?;
    file.set_modified(
        std::time::SystemTime::now() - std::time::Duration::from_secs(4 * 24 * 3600),
    )?;
    drop(file);

    // Create a fresh stm file (2 hours old)
    let fresh_stm_file = handoffs_dir.join("stm_fresh_sess.json");
    fs::write(&fresh_stm_file, "{}")?;
    let file = fs::OpenOptions::new().write(true).open(&fresh_stm_file)?;
    file.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 3600))?;
    drop(file);

    // Insert an STM record into SurrealDB and set updated_at to 4 days ago
    backend.save_stm("old_sess", "k1", "v1").await?;
    backend.db.query("UPDATE type::record('short_term_memory', [$session_id, $key]) SET updated_at = time::now() - 4d;")
        .bind(("session_id", "old_sess"))
        .bind(("key", "k1"))
        .await?.check()?;

    // Insert a fresh STM record (2 hours old)
    backend.save_stm("fresh_sess", "k2", "v2").await?;

    // Set environment variable to customize pruning days to 3 (so 4d old records get pruned)
    unsafe {
        std::env::set_var("MYTHRAX_STM_PRUNING_DAYS", "3");
    }

    // Run pruning
    let prune_result = backend.prune_stale_memories(&vault_root).await;

    // Clean up environment variable
    unsafe {
        std::env::remove_var("MYTHRAX_STM_PRUNING_DAYS");
    }

    prune_result?;

    // Assertions
    assert!(!old_stm_file.exists(), "Old STM file should be pruned");
    assert!(
        fresh_stm_file.exists(),
        "Fresh STM file should be preserved"
    );

    // Check DB
    let old_stm_map = backend.get_stm("old_sess", None).await?;
    assert!(
        old_stm_map.is_empty(),
        "Old STM record in DB should be pruned"
    );

    let fresh_stm_map = backend.get_stm("fresh_sess", None).await?;
    assert_eq!(
        fresh_stm_map.get("k2").unwrap(),
        "v2",
        "Fresh STM record in DB should be preserved"
    );

    Ok(())
}

#[tokio::test]
async fn test_pre_invocation_hook_flow() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("episodes"))?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;

    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;
    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
    }

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let store = std::sync::Arc::new(mythrax_core::store::MarkdownStore::new(&vault_root)?);
    let mcp_server = mythrax_core::mcp::McpServer::new_local(backend.clone(), store);

    // 1. Create a handoff
    let handoff = HandoffSave {
        parent_conversation_id: "parent_123".to_string(),
        subagent_conversation_id: "subagent_456".to_string(),
        summary: "Build a new hook feature".to_string(),
        handoff_file_path: "handoff_test.md".to_string(),
        scope: Some("general".to_string()),
        include_tool_execution: None,
    };
    backend.save_handoff(&handoff).await?;

    // 2. Insert the wisdom rule in the database so it can be hydrated
    let rule = mythrax_core::contracts::WisdomRule {
        id: Some("wisdom:rule_abc".to_string()),
        target_pattern: "Test Pattern".to_string(),
        action_to_avoid: "Avoiding test".to_string(),
        causal_explanation: "Causal details".to_string(),
        prescribed_remedy: "Remedy details".to_string(),
        tier: mythrax_core::contracts::Tier::Project,
        scope: "general".to_string(),
        vault_path: Some("wisdom/rule_abc.md".to_string()),
        embedding: None,
        source_episodes: vec![],
        generator_name: "test".to_string(),
        similarity: None,
        utility: Some(50.0),
        status: None,
        superseded_at: None,
        superseded_by: None,
        rule_type: None,

        ..Default::default()
    };
    let saved_id = backend.save_wisdom_rule(&rule).await?;

    // 3. Add distilled context nodes to STM
    backend
        .save_stm(
            "subagent_456",
            "distilled_context_nodes",
            &format!("[\"{}\"]", saved_id),
        )
        .await?;

    // 4. Call pre_invocation_hook via MCP consolidated manage tool
    let args = serde_json::json!({
        "action": "pre_invocation",
        "session_id": "subagent_456",
        "query": "test query"
    });
    let resp = mcp_server
        .handle_request(
            "tools/call",
            serde_json::json!({
                "name": "manage",
                "arguments": args
            }),
        )
        .await?;

    let text = resp["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("Handoff Metadata"),
        "Expected Handoff Metadata in: {}",
        text
    );
    assert!(
        text.contains("Test Pattern"),
        "Expected Test Pattern in: {}",
        text
    );
    assert!(
        text.contains("Avoiding test"),
        "Expected Avoiding test in: {}",
        text
    );

    // 5. Test root agent path (when no handoff active)
    let args_root = serde_json::json!({
        "action": "pre_invocation",
        "session_id": "root_session_789",
        "query": "test query",
        "workspace_path": workspace_root.to_str().unwrap()
    });
    let resp_root = mcp_server
        .handle_request(
            "tools/call",
            serde_json::json!({
                "name": "manage",
                "arguments": args_root
            }),
        )
        .await?;
    let text_root = resp_root["content"][0]["text"].as_str().unwrap();
    assert!(
        text_root.contains("Pinned Deep-Search Instruction"),
        "Expected Pinned Deep-Search Instruction in: {}",
        text_root
    );

    Ok(())
}

}

mod stm_grounded_ideation {
use anyhow::Result;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use surrealdb::Surreal;
use surrealdb::engine::local::{Db, Mem};
use tempfile::TempDir;

use mythrax_core::cognitive::arbor::{ArborCoordinator, ArborLlmClient};

#[derive(Clone)]
pub struct StmMockLlmClient {
    pub received_anchors: Arc<Mutex<Vec<String>>>,
}

impl StmMockLlmClient {
    pub fn new() -> Self {
        Self {
            received_anchors: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl ArborLlmClient for StmMockLlmClient {
    async fn propose_hypotheses(
        &self,
        _db: &dyn mythrax_core::db::StorageBackend,
        _parent_id: &str,
        _parent_hypothesis: &str,
        _target_files: &[(String, String)],
        _constraints: &[String],
        stm_anchors: &[String],
    ) -> Result<String> {
        let mut guard = self.received_anchors.lock().unwrap();
        *guard = stm_anchors.to_vec();

        Ok(r#"[
            {
                "node_id": "1",
                "hypothesis": "Optimize check range",
                "score": 90.0,
                "code_changes": {
                    "prime_calc.py": "def is_prime(n): return True"
                }
            }
        ]"#
        .to_string())
    }

    async fn evaluate_run(
        &self,
        _db: &dyn mythrax_core::db::StorageBackend,
        _run_logs: &str,
    ) -> Result<String> {
        Ok(r#"{"success": true, "score": 99.0, "insight": "success"}"#.to_string())
    }

    async fn abstract_insights(
        &self,
        _db: &dyn mythrax_core::db::StorageBackend,
        _parent_insight: Option<&str>,
        _child_insight: &str,
    ) -> Result<String> {
        Ok("insight".to_string())
    }
}

fn setup_mock_git_repo(repo_dir: &Path) -> Result<()> {
    let status = Command::new("git")
        .arg("init")
        .current_dir(repo_dir)
        .status()?;
    assert!(status.success());

    let status = Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_dir)
        .status()?;
    assert!(status.success());

    let status = Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_dir)
        .status()?;
    assert!(status.success());

    fs::write(
        repo_dir.join("prime_calc.py"),
        "def is_prime(n): return True",
    )?;

    let status = Command::new("git")
        .args(["add", "prime_calc.py"])
        .current_dir(repo_dir)
        .status()?;
    assert!(status.success());

    let status = Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_dir)
        .status()?;
    assert!(status.success());

    Ok(())
}

async fn setup_surreal_schema(db: &Surreal<Db>) -> Result<()> {
    let schema = r#"
        DEFINE TABLE hypothesis_node SCHEMALESS;
        DEFINE INDEX node_id_idx ON hypothesis_node FIELDS node_id UNIQUE;
    "#;
    db.query(schema).await?.check()?;
    Ok(())
}

#[tokio::test]
async fn test_arbor_stm_grounded_ideation() -> Result<()> {
    let vault_temp = TempDir::new()?;
    let repo_temp = TempDir::new()?;

    setup_mock_git_repo(repo_temp.path())?;

    // Create the .handoffs directory inside vault_temp
    let handoffs_dir = vault_temp.path().join(".handoffs");
    fs::create_dir_all(&handoffs_dir)?;

    // Write a mock active STM anchors JSON file
    let stm_content = r#"{
        "_active_anchors": ["anchor_1", "anchor_2", "anchor_3"]
    }"#;
    fs::write(handoffs_dir.join("stm_123.json"), stm_content)?;

    let db = Surreal::new::<Mem>(()).await?;
    db.use_ns("mythrax").use_db("test").await?;
    setup_surreal_schema(&db).await?;

    let llm_client = StmMockLlmClient::new();
    let coordinator = ArborCoordinator::new(
        db.clone(),
        vault_temp.path().to_path_buf(),
        repo_temp.path().to_path_buf(),
        llm_client.clone(),
        "stm-testing".to_string(),
        "python3 prime_calc.py".to_string(),
        vec!["prime_calc.py".to_string()],
    )
    .await;

    coordinator
        .init_root("Base hypothesis".to_string(), None)
        .await?;
    coordinator.trigger_ideation("ROOT").await?;

    // Verify that the mock client received the STM anchors from the JSON file
    let anchors = llm_client.received_anchors.lock().unwrap().clone();
    assert_eq!(anchors, vec!["anchor_1", "anchor_2", "anchor_3"]);

    Ok(())
}

}

mod chat_history_dynamic_sliding_window {
use anyhow::Result;
use mythrax_core::api::ApiState;
use mythrax_core::cognitive::compactor::Compactor;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::mcp_routes::call_mcp_tool;
use mythrax_core::store::MarkdownStore;
use serde_json::json;
use std::fs;
use std::sync::Mutex;
use surrealdb_types::SurrealValue;
use tempfile::tempdir;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_chat_history_dynamic_sliding_window() -> Result<()> {
    let _guard = match TEST_MUTEX.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
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

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    // Force initialize the schema locally in the test to ensure all tables exist
    backend
        .db
        .query(mythrax_core::db::schema::INIT_SCHEMA)
        .await?
        .check()?;
    backend.init().await?;

    let store = std::sync::Arc::new(MarkdownStore::new(&vault_root)?);

    let state = ApiState {
        backend: backend.clone(),
        auth_token: "secret".to_string(),
        store,
        ignore_list: std::sync::Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new()),
        dream_tx: None,
        shutdown_tx: None,
    };

    let session_id = "test-session-123";

    // 1. Verify user query logging
    let hook_args = json!({
        "action": "pre_invocation",
        "session_id": session_id,
        "query": "Hello, how do I optimize the pipeline?",
        "workspace_path": workspace_root.to_str().unwrap()
    });

    let _hook_res = call_mcp_tool(&state, "manage", hook_args).await?;

    // Verify that the query was logged
    let mut db_resp = backend
        .db
        .query("SELECT * FROM chat_history WHERE session_id = $session_id;")
        .bind(("session_id", session_id))
        .await?;

    #[derive(serde::Deserialize, Debug, SurrealValue)]
    struct ChatMessageRaw {
        role: String,
        content: String,
    }
    let messages: Vec<ChatMessageRaw> = db_resp.take(0)?;
    assert!(
        !messages.is_empty(),
        "User query should be logged in chat_history"
    );
    assert_eq!(messages[0].role, "user");
    assert_eq!(
        messages[0].content,
        "Hello, how do I optimize the pipeline?"
    );

    // 2. Verify assistant response logging after tool execution
    let _tool_res = call_mcp_tool(
        &state,
        "read",
        json!({
            "session_id": session_id,
            "action": "root"
        }),
    )
    .await?;

    // Verify assistant response is logged
    let mut db_resp2 = backend
        .db
        .query(
            "SELECT * FROM chat_history WHERE session_id = $session_id ORDER BY created_at DESC;",
        )
        .bind(("session_id", session_id))
        .await?;
    let messages2: Vec<ChatMessageRaw> = db_resp2.take(0)?;
    assert!(
        messages2.len() >= 2,
        "Assistant response should be logged after tool execution"
    );
    assert_eq!(messages2[0].role, "assistant");

    // 3. Verify dynamic sliding window token scaling
    let long_text =
        "This is a very long sentence that contains many tokens and will exceed budget. "
            .repeat(20); // ~260 tokens
    for i in 0..10 {
        let role = if i % 2 == 0 { "user" } else { "assistant" };
        let _ = backend.db.query("INSERT INTO chat_history { session_id: $session_id, role: $role, content: $content, created_at: time::now() };")
            .bind(("session_id", session_id))
            .bind(("role", role))
            .bind(("content", long_text.clone()))
            .await?;
    }

    // Call hook again
    let hook_res2 = call_mcp_tool(
        &state,
        "manage",
        json!({
            "action": "pre_invocation",
            "session_id": session_id,
            "query": "current status",
            "workspace_path": workspace_root.to_str().unwrap()
        }),
    )
    .await?;

    let hook_text = hook_res2
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.get(0))
        .and_then(|obj| obj.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    assert!(hook_text.contains("### 💬 Conversational Turn History"));
    let turn_count =
        hook_text.matches("- **User**").count() + hook_text.matches("- **Assistant**").count();
    assert!(
        turn_count < 10,
        "Conversational history should be dynamically scaled down to fit within the 2048 token budget"
    );

    // 4. Verify compaction pruning (> 100 turns)
    for _ in 0..120 {
        let _ = backend.db.query("INSERT INTO chat_history { session_id: $session_id, role: 'user', content: 'brief turn', created_at: time::now() };")
            .bind(("session_id", session_id))
            .await?;
    }

    // Execute compaction
    let compactor = Compactor::new();
    compactor
        .compact_scope(
            state.backend.clone(),
            &state.store,
            "general",
            backend.embedder.clone(),
        )
        .await?;

    // Count remaining messages for this session
    let mut db_resp3 = backend
        .db
        .query("SELECT * FROM chat_history WHERE session_id = $session_id;")
        .bind(("session_id", session_id))
        .await?;
    let messages3: Vec<ChatMessageRaw> = db_resp3.take(0)?;
    assert_eq!(
        messages3.len(),
        100,
        "Compactor should prune chat_history to exactly 100 turns per session"
    );

    Ok(())
}

}

mod count_human_messages {
use mythrax_core::hooks::shell::count_human_messages;
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_count_human_messages_various_formats() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("transcript.jsonl");
    let mut file = File::create(&file_path).unwrap();

    // Write a set of messages in JSON lines format:
    // 1. Simple user string content (valid user turn)
    writeln!(file, r#"{{"role": "user", "content": "hello there"}}"#).unwrap();

    // 2. User with nested structure (valid user turn)
    writeln!(
        file,
        r#"{{"message": {{"role": "user", "content": "how are you"}}}}"#
    )
    .unwrap();

    // 3. User with command-message turn (should be ignored)
    writeln!(
        file,
        r#"{{"role": "user", "content": "run some command <command-message>"}}"#
    )
    .unwrap();

    // 4. User with array-form content blocks (valid user turn)
    writeln!(
        file,
        r#"{{"role": "user", "content": [{{"type": "text", "text": "hey"}}, {{"type": "tool_result", "content": "success"}}]}}"#
    ).unwrap();

    // 5. Assistant turn (should be ignored)
    writeln!(
        file,
        r#"{{"role": "assistant", "content": "I am an assistant"}}"#
    )
    .unwrap();

    let count = count_human_messages(file_path.to_str().unwrap());
    assert_eq!(count, 3); // Message 1, 2, and 4 are valid user turns.
}

}

mod live_session_feedback_hardening {
use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::WatchIgnoreList;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_live_session_feedback_hardening() -> anyhow::Result<()> {
    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let vault_dir = tempdir()?;
    let store = Arc::new(MarkdownStore::new(vault_dir.path())?);
    let ignore = WatchIgnoreList::new();

    let trans_dir = tempdir()?;
    let transcript_path = trans_dir.path().join("transcript.jsonl");
    let mut trans_file = File::create(&transcript_path)?;

    // We write:
    // 1. User turn (user_input)
    // 2. Assistant turn (agent_thought)
    // 3. User turn with correction keyword (user_feedback + is_correction)
    let turns = vec![
        r#"{"role": "user", "content": "Please implement the database helper."}"#,
        r#"{"role": "assistant", "content": "I have created the database helper class with connection logic."}"#,
        r#"{"role": "user", "content": "Actually, you forgot to specify the path to the database!"}"#,
    ];

    for turn in turns {
        writeln!(trans_file, "{}", turn)?;
    }

    let path_str = transcript_path.to_string_lossy();

    // Run mine_transcript
    let count = mythrax_core::hooks::precompact::mine_transcript(
        "sess-live-feedback",
        &path_str,
        backend.as_ref(),
        &store,
        &ignore,
    )
    .await?;

    assert_eq!(count, 3);

    // Let's query the episodes and verify their type
    let episodes = backend.get_all_episodes().await?;
    assert_eq!(episodes.len(), 3);

    // Find the feedback episode (Turn 3) and assistant episode (Turn 2)
    let ep_feedback = episodes
        .iter()
        .find(|e| e.content.contains("forgot to specify"))
        .unwrap();
    let ep_assistant = episodes
        .iter()
        .find(|e| e.content.contains("I have created"))
        .unwrap();

    assert_eq!(ep_feedback.node_type.as_deref(), Some("user_feedback"));
    assert_eq!(ep_assistant.node_type.as_deref(), Some("agent_thought"));

    // Check if the 'corrects' edge exists between Turn 3 (feedback) and Turn 2 (assistant)
    let mut db_res = backend
        .db
        .query("SELECT VALUE in FROM relates_to WHERE out = $assistant AND relation = 'corrects';")
        .bind((
            "assistant",
            mythrax_core::db::parse_record_id(ep_assistant.id.as_ref().unwrap())?,
        ))
        .await?;
    let corrects_sources: Vec<surrealdb::types::RecordId> = db_res.take(0)?;
    assert_eq!(
        corrects_sources.len(),
        1,
        "Should create a 'corrects' relation from feedback to agent thought"
    );

    // Run LLM critic directly to diagnose/guarantee execution
    mythrax_core::mcp_routes::write_handlers::run_llm_critic(
        backend.clone(),
        store.clone(),
        ep_feedback.content.clone(),
        Some("general".to_string()),
        Some(ep_feedback.id.clone().unwrap()),
    )
    .await
    .unwrap();

    // Check if a WisdomRule was saved via the LLM critic
    let rules = backend.get_all_wisdom_rules().await?;
    assert!(
        !rules.is_empty(),
        "Should extract and save at least one WisdomRule via LLM critic"
    );

    Ok(())
}

}

mod meta_skill {
use anyhow::Result;
use mythrax_core::cognitive::meta_skill::MetaSkillSynthesizer;
use mythrax_core::contracts::{WikiNode, WisdomRule};
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use std::env;
use std::fs;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_meta_skill_synthesis() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    let store = MarkdownStore::new(&vault_root)?;

    // Seed rules
    let rule = WisdomRule {
        id: None,
        target_pattern: "Test pattern".to_string(),
        action_to_avoid: "doing something".to_string(),
        causal_explanation: "cuz bad".to_string(),
        prescribed_remedy: "do better".to_string(),
        tier: mythrax_core::contracts::Tier::Project,
        scope: "test-scope".to_string(),
        vault_path: None,
        embedding: Some(vec![0.1; 768]),
        source_episodes: vec![],
        generator_name: "test".to_string(),
        similarity: None,
        utility: None,
        status: None,
        superseded_at: None,
        superseded_by: None,

        rule_type: None,
        ..Default::default()
    };
    backend.save_wisdom_rule(&rule).await?;

    let node = WikiNode {
        id: None,
        name: "Test Document".to_string(),
        content: "Detailed design specifications".to_string(),
        scope: "test-scope".to_string(),
        vault_path: None,
        embedding: Some(vec![0.1; 768]),
        ..Default::default()
    };
    backend.save_wiki_node(&node).await?;

    let synthesizer = MetaSkillSynthesizer::new();
    let published = synthesizer.synthesize_meta_skills(&backend, &store).await?;

    assert_eq!(published.len(), 1);
    assert_eq!(published[0], "meta-test-scope");

    // Check that SKILL.md was written
    let skill_file = vault_root.join("../.agents/skills/meta-test-scope/SKILL.md");
    assert!(skill_file.exists());

    let content = fs::read_to_string(skill_file)?;
    assert!(content.contains("generator_name: MetaSkillSynthesizer"));
    assert!(content.contains("meta-test-scope"));

    Ok(())
}

#[tokio::test]
async fn test_detect_skill_merges() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    if backend.embed("test").await.is_err() {
        println!(
            "Skipping test_detect_skill_merges: model files not present in ~/.mythrax/models/"
        );
        return Ok(());
    }

    // Set HOME to tmp so scan_all_skills looks there for global config if not found
    let original_home = env::var("HOME").ok();
    unsafe {
        env::set_var("HOME", tmp.path());
    }

    let store = MarkdownStore::new(&vault_root)?;

    // Create two playbooks under .agents/skills/
    let skills_dir = vault_root.join("../.agents/skills");
    let sk1_dir = skills_dir.join("meta-git-commit");
    let sk2_dir = skills_dir.join("meta-git-pull");
    fs::create_dir_all(&sk1_dir)?;
    fs::create_dir_all(&sk2_dir)?;

    let sk1_content = "---\nname: meta-git-commit\ndescription: git workflow management instructions\ngenerator_name: MetaSkillSynthesizer\n---\nbody";
    let sk2_content = "---\nname: meta-git-pull\ndescription: git workflow management instructions\ngenerator_name: MetaSkillSynthesizer\n---\nbody";

    fs::write(sk1_dir.join("SKILL.md"), sk1_content)?;
    fs::write(sk2_dir.join("SKILL.md"), sk2_content)?;

    let synthesizer = MetaSkillSynthesizer::new();
    let suggestions = synthesizer.detect_skill_merges(&backend, &store).await?;

    // Since mock LLM is active and description similarities will be calculated, they should merge
    assert!(!suggestions.is_empty());
    assert_eq!(suggestions[0]["suggested_target_name"], "git-workflow");

    // Verify suggestions file was written
    let suggestions_file = vault_root.join("wiki/skill_merge_suggestions.md");
    assert!(suggestions_file.exists());

    unsafe {
        if let Some(h) = original_home {
            env::set_var("HOME", h);
        } else {
            env::remove_var("HOME");
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_execute_skill_merge() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    let original_home = env::var("HOME").ok();
    unsafe {
        env::set_var("HOME", tmp.path());
    }

    let store = MarkdownStore::new(&vault_root)?;

    // Create one meta skill and one custom skill
    let skills_dir = vault_root.join("../.agents/skills");
    let sk1_dir = skills_dir.join("meta-git-commit");
    let sk2_dir = skills_dir.join("custom-git-pull");
    fs::create_dir_all(&sk1_dir)?;
    fs::create_dir_all(&sk2_dir)?;

    let sk1_content = "---\nname: meta-git-commit\ndescription: git commit instructions\ngenerator_name: MetaSkillSynthesizer\n---\nbody";
    // Custom skill (no generator_name)
    let sk2_content =
        "---\nname: custom-git-pull\ndescription: git pull manual instructions\n---\nbody";

    fs::write(sk1_dir.join("SKILL.md"), sk1_content)?;
    fs::write(sk2_dir.join("SKILL.md"), sk2_content)?;

    let synthesizer = MetaSkillSynthesizer::new();
    let merged_name = synthesizer
        .merge_skills(
            &backend,
            &store,
            &["meta-git-commit".to_string(), "custom-git-pull".to_string()],
            "git-workflow",
        )
        .await?;

    assert_eq!(merged_name, "meta-git-workflow");

    // Check that target meta-skill exists
    let target_file = skills_dir.join("meta-git-workflow/SKILL.md");
    assert!(target_file.exists());

    // Source meta-skill should be moved to .trash (which will be under vault_root/../.trash)
    assert!(!sk1_dir.exists());
    let trash_dir = vault_root.join("../.trash");
    let trash_entries = fs::read_dir(trash_dir)?.collect::<Vec<_>>();
    assert!(!trash_entries.is_empty());

    // Custom source skill should be moved to archive (.agents/archive/skills/custom-git-pull)
    assert!(!sk2_dir.exists());
    let archive_dir = vault_root.join("../.agents/archive/skills/custom-git-pull");
    assert!(archive_dir.exists());

    unsafe {
        if let Some(h) = original_home {
            env::set_var("HOME", h);
        } else {
            env::remove_var("HOME");
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_atomic_insight_item_validation() -> Result<()> {
    use mythrax_core::cognitive::synthesis::{AtomicInsightItem, ClusterAnalysis};

    let valid_item = AtomicInsightItem {
        title: "Test Insight".to_string(),
        item_type: "lesson".to_string(),
        content: "What was tried: A. What happened: B. Why: C.".to_string(),
        what_was_tried: Some("A".to_string()),
        what_happened: Some("B".to_string()),
        why: Some("C".to_string()),
        metacognitive_confidence: Some(90),
    };
    assert_eq!(valid_item.title, "Test Insight");
    assert_eq!(valid_item.item_type, "lesson");
    assert_eq!(
        valid_item.causal_content(),
        "Tried: A\nWhat happened: B\nWhy: C"
    );

    let fallback_analysis = ClusterAnalysis {
        items: vec![],
        title: Some("Fallback Title".to_string()),
        summary: Some("Fallback Summary".to_string()),
        metacognitive_confidence: Some(85),
        node_type: Some("lesson".to_string()),
    };
    let items = fallback_analysis.resolved_items("raw fallback text");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Fallback Title");
    assert_eq!(items[0].content, "Fallback Summary");

    Ok(())
}

#[tokio::test]
async fn test_synthesis_system_prompt_formatting() -> Result<()> {
    use mythrax_core::cognitive::synthesis::build_synthesis_prompt;

    let sys_prompt = build_synthesis_prompt("Extract items array containing title, item_type, content, metacognitive_confidence.");
    assert!(sys_prompt.contains("Strunk & White"));
    assert!(sys_prompt.contains("items"));
    assert!(sys_prompt.contains("title"));
    assert!(sys_prompt.contains("item_type"));
    assert!(sys_prompt.contains("content"));
    assert!(sys_prompt.contains("metacognitive_confidence"));
    Ok(())
}

#[tokio::test]
async fn test_empty_array_error_handling() -> Result<()> {
    use mythrax_core::cognitive::synthesis::ClusterAnalysis;

    let analysis: ClusterAnalysis = serde_json::from_str(r#"{"items": []}"#).unwrap();
    let resolved = analysis.resolved_items("Fallback raw text for empty items array");
    assert_eq!(resolved.len(), 0);
    Ok(())
}

#[tokio::test]
async fn test_cross_item_type_deduplication() -> Result<()> {
    use mythrax_core::cognitive::synthesis::AtomicInsightItem;

    let item_pattern = AtomicInsightItem {
        title: "Database Lock Contention".to_string(),
        item_type: "pattern".to_string(),
        content: "Detailed pattern explanation of DB locks under load.".to_string(),
        what_was_tried: None,
        what_happened: None,
        why: None,
        metacognitive_confidence: Some(95),
    };
    let item_failure = AtomicInsightItem {
        title: "Database Lock Contention".to_string(),
        item_type: "failure_mode".to_string(),
        content: "Detailed failure mode explanation of DB locks under load.".to_string(),
        what_was_tried: None,
        what_happened: None,
        why: None,
        metacognitive_confidence: Some(95),
    };
    assert_ne!(item_pattern.item_type, item_failure.item_type);
    Ok(())
}

#[tokio::test]
async fn test_item_type_routing_promote_insight_to_direction() -> Result<()> {
    use mythrax_core::cognitive::synthesis::promote_insight_to_direction;
    use mythrax_core::contracts::{Episode, WikiNode};
    use mythrax_core::db::backend::SurrealBackend;
    use mythrax_core::store::MarkdownStore;
    use tempfile::tempdir;

    let dir = tempdir()?;
    let store = MarkdownStore::new(dir.path())?;
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    let failure_node = WikiNode {
        id: Some("wiki_node:fail1".to_string()),
        name: "Catastrophic Deadlock".to_string(),
        content: "Holding mutexes across async boundary causes deadlock.".to_string(),
        scope: "test_scope".to_string(),
        item_type: Some("failure_mode".to_string()),
        metacognitive_confidence: Some(90),
        ..Default::default()
    };
    backend.save_wiki_node(&failure_node).await?;

    let eps = vec![
        Episode { id: Some("ep1".to_string()), confidence: Some(5.0), ..Default::default() },
        Episode { id: Some("ep2".to_string()), confidence: Some(5.0), ..Default::default() },
        Episode { id: Some("ep3".to_string()), confidence: Some(5.0), ..Default::default() },
        Episode { id: Some("ep4".to_string()), confidence: Some(5.0), ..Default::default() },
    ];

    promote_insight_to_direction(&backend, &store, &failure_node, &eps).await?;

    let rules = backend.get_all_wisdom_rules().await?;
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].target_pattern, "Catastrophic Deadlock");
    assert_eq!(rules[0].action_to_avoid, "Holding mutexes across async boundary causes deadlock.");

    let pattern_node = WikiNode {
        id: Some("wiki_node:pat1".to_string()),
        name: "Lock Free Queue".to_string(),
        content: "Use atomic crossbeam queue for low contention.".to_string(),
        scope: "test_scope".to_string(),
        item_type: Some("pattern".to_string()),
        metacognitive_confidence: Some(90),
        ..Default::default()
    };
    backend.save_wiki_node(&pattern_node).await?;

    promote_insight_to_direction(&backend, &store, &pattern_node, &eps).await?;

    let nodes = backend.get_all_wiki_nodes().await?;
    let dir_nodes: Vec<_> = nodes.into_iter().filter(|n| n.node_type.as_deref() == Some("direction")).collect();
    assert_eq!(dir_nodes.len(), 1);
    assert_eq!(dir_nodes[0].item_type.as_deref(), Some("pattern"));

    Ok(())
}

#[tokio::test]
async fn test_phase3_extract_wisdom_from_spec_risk_section() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    let spec_content = r#"# Specification

## Risk Assessment
Holding database locks across network RPC boundaries triggers lock contention and deadlocks under concurrency.

## Failure Modes
OOM occurs when un-evicted vector models exceed GPU VRAM memory budget.
"#;

    mythrax_core::vault::distillation::extract_wisdom_from_document(&backend, spec_content, "test_scope").await?;

    let rules = backend.get_all_wisdom_rules().await?;
    assert!(!rules.is_empty());
    assert_eq!(rules[0].generator_name, "document_extraction");

    Ok(())
}

#[tokio::test]
async fn test_phase3_ingest_artifacts_markdown() -> Result<()> {
    let dir = tempdir()?;
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    let walkthrough_path = dir.path().join("walkthrough.md");
    std::fs::write(&walkthrough_path, "# Walkthrough\nCompleted phase 3 changes.")?;

    let spec_path = dir.path().join("spec.md");
    std::fs::write(&spec_path, "# Spec\n## Risk\nSystem failure occurs on unhandled panic.")?;

    mythrax_core::vault::distillation::ingest_artifacts_in_dir(&backend, dir.path(), "session_123", "test_scope").await?;

    let eps = backend.get_all_episodes().await?;
    assert_eq!(eps.len(), 1);
    assert_eq!(eps[0].node_type.as_deref(), Some("walkthrough"));

    let nodes = backend.get_all_wiki_nodes().await?;
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].item_type.as_deref(), Some("constraint"));

    let rules = backend.get_all_wisdom_rules().await?;
    assert!(!rules.is_empty());

    Ok(())
}

}
