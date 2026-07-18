use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use mythrax_core::db::{SurrealBackend, StorageBackend, parse_record_id};
use mythrax_core::contracts::WikiNode;
use mythrax_core::cognitive::compactor::Compactor;
use mythrax_core::store::MarkdownStore;

use std::sync::Mutex;
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

    backend.db.query("UPDATE $id SET embedding = $emb;")
        .bind(("id", rid1))
        .bind(("emb", emb1))
        .await?.check()?;

    backend.db.query("UPDATE $id SET embedding = $emb;")
        .bind(("id", rid2))
        .bind(("emb", emb2))
        .await?.check()?;

    backend.db.query("UPDATE $id SET embedding = $emb;")
        .bind(("id", rid3))
        .bind(("emb", emb3))
        .await?.check()?;

    // Execute compaction
    compactor.compact_scope(&backend, &store, "scope1", backend.embedder.clone()).await?;

    // Verify compactions on disk
    let compaction_dir = vault_root.join("wiki/scope1/compactions");
    assert!(compaction_dir.exists());

    let entries = fs::read_dir(&compaction_dir)?;
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".md") {
            let content = fs::read_to_string(entry.path())?;
            files.push((name, content));
        }
    }

    assert_eq!(files.len(), 2, "Expected exactly two compaction files");

    let mut found_cluster = false;
    let mut found_misc = false;

    println!("ACTUAL FILES IN DIR: {:?}", files);
    for (name, content) in &files {
        let name_lower = name.to_lowercase();
        if content.contains("cluster_id: 0") {
            found_cluster = true;
            assert!(name_lower.contains("insight_one") || name_lower.contains("insight_two") || name_lower.contains("insight_three"), "Cluster compaction filename should contain slug of first insight: {}", name);
        } else if content.contains("cluster_id: \"miscellaneous\"") {
            found_misc = true;
            assert!(name_lower.contains("miscellaneous"), "Miscellaneous compaction filename should contain miscellaneous");
        }
    }

    assert!(found_cluster, "Clustered compaction not generated");
    assert!(found_misc, "Miscellaneous compaction not generated");

    // Verify relations in the database
    let mut response = backend.db.query("SELECT id, name FROM wiki_node;").await?;
    let nodes: Vec<serde_json::Value> = response.take(0)?;

    // We should have 5 wiki nodes total (3 insights + 2 compactions)
    assert_eq!(nodes.len(), 5);

    let cluster_compaction_node = nodes.iter()
        .find(|n| n["name"].as_str().unwrap().contains("Cluster 0"))
        .expect("Cluster 0 node not found");
    let misc_compaction_node = nodes.iter()
        .find(|n| n["name"].as_str().unwrap().contains("Miscellaneous"))
        .expect("Miscellaneous compaction node not found");

    let cluster_node_id = cluster_compaction_node["id"].as_str().unwrap();
    let misc_node_id = misc_compaction_node["id"].as_str().unwrap();

    let mut rel_resp1 = backend.db.query("SELECT * FROM relates_to WHERE in = $ins_id AND out = $comp_id;")
        .bind(("ins_id", parse_record_id(&id1)?))
        .bind(("comp_id", parse_record_id(cluster_node_id)?))
        .await?;
    let rels1: Vec<serde_json::Value> = rel_resp1.take(0)?;
    assert_eq!(rels1.len(), 1, "Relation between Insight One and Cluster Compaction missing");

    let mut rel_resp2 = backend.db.query("SELECT * FROM relates_to WHERE in = $ins_id AND out = $comp_id;")
        .bind(("ins_id", parse_record_id(&id2)?))
        .bind(("comp_id", parse_record_id(cluster_node_id)?))
        .await?;
    let rels2: Vec<serde_json::Value> = rel_resp2.take(0)?;
    assert_eq!(rels2.len(), 1, "Relation between Insight Two and Cluster Compaction missing");

    let mut rel_resp3 = backend.db.query("SELECT * FROM relates_to WHERE in = $ins_id AND out = $comp_id;")
        .bind(("ins_id", parse_record_id(&id3)?))
        .bind(("comp_id", parse_record_id(misc_node_id)?))
        .await?;
    let rels3: Vec<serde_json::Value> = rel_resp3.take(0)?;
    assert_eq!(rels3.len(), 1, "Relation between Insight Three and Miscellaneous Compaction missing");

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
    backend.db.query("UPDATE $id SET embedding = $emb;")
        .bind(("id", parse_record_id(&id1)?))
        .bind(("emb", emb1))
        .await?.check()?;
    backend.db.query("UPDATE $id SET embedding = $emb;")
        .bind(("id", parse_record_id(&id2)?))
        .bind(("emb", emb2))
        .await?.check()?;
    backend.db.query("UPDATE $id SET embedding = $emb;")
        .bind(("id", parse_record_id(&id3)?))
        .bind(("emb", emb3))
        .await?.check()?;
    backend.db.query("UPDATE $id SET embedding = $emb;")
        .bind(("id", parse_record_id(&id4)?))
        .bind(("emb", emb4))
        .await?.check()?;

    // Write an existing insight to disk
    let insight_dir = vault_root.join("wiki/scope1/insights");
    fs::create_dir_all(&insight_dir)?;
    let insight_path = insight_dir.join("drifting_insight.md");
    let insight_content = format!(r#"---
title: "Drifting Insight"
scope: "scope1"
source_episodes:
  - "{}"
  - "{}"
  - "{}"
  - "{}"
---
Insight content
"#, id1, id2, id3, id4);
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

    let mut initial_nodes_resp = backend.db.query("SELECT * FROM wiki_node WHERE name = 'Drifting Insight';").await?;
    let initial_nodes: Vec<serde_json::Value> = initial_nodes_resp.take(0)?;
    println!("DEBUG TEST: initial drifting nodes: {:?}", initial_nodes);

    // Call DreamCoordinator::run_dream
    mythrax_core::cognitive::synthesis::DreamCoordinator::new()
        .run_dream(&backend, &store, Some("deep"), backend.embedder.clone())
        .await?;

    let mut after_nodes_resp = backend.db.query("SELECT * FROM wiki_node WHERE name = 'Drifting Insight';").await?;
    let after_nodes: Vec<serde_json::Value> = after_nodes_resp.take(0)?;
    println!("DEBUG TEST: after drifting nodes: {:?}", after_nodes);

    // Assertions to verify the split behavior

    // 1. The old insight file on disk is deleted.
    assert!(!insight_path.exists(), "Old insight file should be deleted.");

    // 2. The old insight WikiNode is deleted from the DB
    let mut response = backend.db.query("SELECT * FROM wiki_node WHERE name = 'Drifting Insight';").await?;
    let old_nodes: Vec<serde_json::Value> = response.take(0)?;
    assert_eq!(old_nodes.len(), 0, "Old insight WikiNode should be deleted from DB");

    // 3. Check the database: two new split insight nodes are created by the drift check
    // (They will be created under "Split Analysis ..." because of mock LLM behavior when parsing fails, or from the JSON response).
    // Let's query all wiki nodes in scope1 except the old one.
    let mut response = backend.db.query("SELECT id, name FROM wiki_node WHERE scope = 'scope1' AND name != 'Drifting Insight';").await?;
    let new_nodes: Vec<serde_json::Value> = response.take(0)?;
    
    // We should have split insights generated. Let's make sure we find them.
    let split_nodes: Vec<_> = new_nodes.iter()
        .filter(|n| n["name"].as_str().unwrap().contains("Split Analysis"))
        .collect();
    assert_eq!(split_nodes.len(), 2, "Expected exactly two split insights");

    // 4. Verify relations:
    // - Split Node 1 (for cluster of ep1, ep2) should relate to ep1 and ep2
    // - Split Node 2 (for cluster of ep3, ep4) should relate to ep3 and ep4
    let split_id1 = split_nodes[0]["id"].as_str().unwrap();
    let split_id2 = split_nodes[1]["id"].as_str().unwrap();

    let mut rel_resp1 = backend.db.query("SELECT * FROM relates_to WHERE in = $ep_id AND out = $split_id;")
        .bind(("ep_id", parse_record_id(&id1)?))
        .bind(("split_id", parse_record_id(split_id1)?))
        .await?;
    let rels1: Vec<serde_json::Value> = rel_resp1.take(0)?;

    let mut rel_resp2 = backend.db.query("SELECT * FROM relates_to WHERE in = $ep_id AND out = $split_id;")
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
    let mut check1 = backend.db.query("SELECT * FROM relates_to WHERE in = $ep_id AND out = $split_id;")
        .bind(("ep_id", parse_record_id(&id2)?))
        .bind(("split_id", parse_record_id(first_cluster_split_id)?))
        .await?;
    let check1_rels: Vec<serde_json::Value> = check1.take(0)?;
    assert_eq!(check1_rels.len(), 1, "Episode 2 should relate to first cluster split insight");

    // Verify second cluster split relationships
    let mut check2 = backend.db.query("SELECT * FROM relates_to WHERE in = $ep_id AND out = $split_id;")
        .bind(("ep_id", parse_record_id(&id3)?))
        .bind(("split_id", parse_record_id(second_cluster_split_id)?))
        .await?;
    let check2_rels: Vec<serde_json::Value> = check2.take(0)?;
    assert_eq!(check2_rels.len(), 1, "Episode 3 should relate to second cluster split insight");

    let mut check3 = backend.db.query("SELECT * FROM relates_to WHERE in = $ep_id AND out = $split_id;")
        .bind(("ep_id", parse_record_id(&id4)?))
        .bind(("split_id", parse_record_id(second_cluster_split_id)?))
        .await?;
    let check3_rels: Vec<serde_json::Value> = check3.take(0)?;
    assert_eq!(check3_rels.len(), 1, "Episode 4 should relate to second cluster split insight");

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
        &backend,
        &store,
        &new_rule,
    ).await?;

    // Assert it returned skills_id
    assert_eq!(saved_id, skills_id);

    // Assert the new rule file is deleted
    let new_file_path = vault_root.join("wisdom/dynamic/new_rule.md");
    assert!(!new_file_path.exists());

    // Assert the skills rule now relates to the episode "ep2"
    let mut response = backend.db.query("SELECT * FROM relates_to WHERE out = $skills_id;").bind(("skills_id", parse_record_id(&skills_id)?)).await?;
    let rels: Vec<serde_json::Value> = response.take(0)?;
    let ep2_related = rels.iter().any(|r| r["in"].as_str().unwrap().contains("ep2"));
    assert!(ep2_related, "Episode 2 should be related to the skills rule");

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
        &backend,
        &store,
        &new_rule,
    ).await?;

    // The old rule's file should no longer exist at its original path, but the archived rule file SHOULD exist
    let old_file_path = vault_root.join("wisdom/dynamic/rule1.md");
    assert!(!old_file_path.exists(), "Old rule file should be removed from active directory");

    let archived_file_path = vault_root.join("wisdom/superseded_archive/rule1.md");
    assert!(archived_file_path.exists(), "Archived rule file should exist in superseded_archive");

    // The old rule record in SurrealDB should NOT be deleted, but its status should be updated to "superseded"
    let mut response = backend.db.query("SELECT * FROM wisdom WHERE vault_path = 'wisdom/dynamic/rule1.md';").await?;
    let old_db_rules: Vec<serde_json::Value> = response.take(0)?;
    assert!(!old_db_rules.is_empty(), "Old rule record should still exist in database");
    
    if let Some(rule) = old_db_rules.first() {
        let status = rule.get("status").and_then(|v| v.as_str()).unwrap_or("");
        assert_eq!(status, "superseded", "Old rule status should be updated to 'superseded'");
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
    let start1 = chrono::DateTime::parse_from_rfc3339("2026-07-08T00:00:00Z")?.with_timezone(&chrono::Utc);
    let end1 = chrono::DateTime::parse_from_rfc3339("2026-07-10T00:00:00Z")?.with_timezone(&chrono::Utc);
    let start2 = chrono::DateTime::parse_from_rfc3339("2026-07-14T00:00:00Z")?.with_timezone(&chrono::Utc);
    let end2 = chrono::DateTime::parse_from_rfc3339("2026-07-16T00:00:00Z")?.with_timezone(&chrono::Utc);

    backend.db.query("UPDATE $id SET temporal_range_start = $start, temporal_range_end = $end;")
        .bind(("id", parse_record_id(&ep_id1)?))
        .bind(("start", start1))
        .bind(("end", end1))
        .await?.check()?;

    backend.db.query("UPDATE $id SET temporal_range_start = $start, temporal_range_end = $end;")
        .bind(("id", parse_record_id(&ep_id2)?))
        .bind(("start", start2))
        .bind(("end", end2))
        .await?.check()?;

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

    backend.db.query("UPDATE $id SET embedding = $emb;")
        .bind(("id", rid1))
        .bind(("emb", emb1))
        .await?.check()?;

    backend.db.query("UPDATE $id SET embedding = $emb;")
        .bind(("id", rid2))
        .bind(("emb", emb2))
        .await?.check()?;

    backend.db.query("UPDATE $id SET temporal_range_start = $start, temporal_range_end = $end;")
        .bind(("id", parse_record_id(&id1)?))
        .bind(("start", start1))
        .bind(("end", end1))
        .await?.check()?;

    backend.db.query("UPDATE $id SET temporal_range_start = $start, temporal_range_end = $end;")
        .bind(("id", parse_record_id(&id2)?))
        .bind(("start", start2))
        .bind(("end", end2))
        .await?.check()?;

    // Execute compaction
    compactor.compact_scope(&backend, &store, "scope2", backend.embedder.clone()).await?;
    

    let compaction_dir = vault_root.join("wiki/scope2/compactions");
    assert!(compaction_dir.exists());

    let all_nodes = backend.get_all_wiki_nodes().await?;
    let compacted_nodes: Vec<WikiNode> = all_nodes.into_iter()
        .filter(|n| n.scope == "scope2" && n.name.contains("Cluster"))
        .collect();
    assert_eq!(compacted_nodes.len(), 1, "Expected exactly one cluster compaction node");
    let comp_node = &compacted_nodes[0];
    let comp_id = comp_node.id.as_ref().unwrap();

    assert_eq!(comp_node.temporal_range_start, Some(start1));
    assert_eq!(comp_node.temporal_range_end, Some(end2));

    let mut rel_resp1 = backend.db.query("SELECT * FROM relates_to WHERE in = $comp_id AND out = $ep_id AND relation = 'derived_from';")
        .bind(("comp_id", parse_record_id(comp_id)?))
        .bind(("ep_id", parse_record_id(&ep_id1)?))
        .await?;
    let rels1: Vec<serde_json::Value> = rel_resp1.take(0)?;
    assert_eq!(rels1.len(), 1, "Edge from compaction to Episode 1 with derived_from relation is missing");

    let mut rel_resp2 = backend.db.query("SELECT * FROM relates_to WHERE in = $comp_id AND out = $ep_id AND relation = 'derived_from';")
        .bind(("comp_id", parse_record_id(comp_id)?))
        .bind(("ep_id", parse_record_id(&ep_id2)?))
        .await?;
    let rels2: Vec<serde_json::Value> = rel_resp2.take(0)?;
    assert_eq!(rels2.len(), 1, "Edge from compaction to Episode 2 with derived_from relation is missing");

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
    backend.db.query("UPDATE $id SET updated_at = $past;")
        .bind(("id", rid.clone()))
        .bind(("past", past_time))
        .await?.check()?;

    // Execute compaction
    compactor.compact_scope(&backend, &store, "scope1", None).await?;

    // Verify:
    // 1. The physical file was moved to {vault_root}/archive/low_confidence_node.md
    let expected_archive_path = vault_root.join("archive/low_confidence_node.md");
    assert!(expected_archive_path.exists(), "File should be moved to archive directory");
    assert!(!md_path.exists(), "Original file should be deleted");

    // 2. The record was deleted from SurrealDB
    let mut response = backend.db.query("SELECT * FROM wiki_node WHERE id = $id;")
        .bind(("id", rid))
        .await?;
    let nodes: Vec<serde_json::Value> = response.take(0)?;
    assert!(nodes.is_empty(), "WikiNode record should be deleted from the database");

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
    backend.db.query("UPDATE relates_to SET weight = 0.105 WHERE in = $in AND out = $out;")
        .bind(("in", rid_a.clone()))
        .bind(("out", rid_b.clone()))
        .await?.check()?;

    backend.db.query("UPDATE relates_to SET weight = 0.5 WHERE in = $in AND out = $out;")
        .bind(("in", rid_a.clone()))
        .bind(("out", rid_c.clone()))
        .await?.check()?;

    // Execute compaction
    compactor.compact_scope(&backend, &store, "scope1", None).await?;

    // Verify:
    // 1. Edge 1 (Node A -> Node B) is deleted because weight 0.0945 < 0.1
    let mut check_ab = backend.db.query("SELECT weight FROM relates_to WHERE in = $in AND out = $out;")
        .bind(("in", rid_a.clone()))
        .bind(("out", rid_b.clone()))
        .await?;
    let ab_edges: Vec<serde_json::Value> = check_ab.take(0)?;
    assert!(ab_edges.is_empty(), "Edge A->B should be pruned");

    // 2. Edge 2 (Node A -> Node C) still exists with decayed weight 0.45
    let mut check_ac = backend.db.query("SELECT weight FROM relates_to WHERE in = $in AND out = $out;")
        .bind(("in", rid_a.clone()))
        .bind(("out", rid_c.clone()))
        .await?;
    let ac_edges: Vec<serde_json::Value> = check_ac.take(0)?;
    assert_eq!(ac_edges.len(), 1, "Edge A->C should not be pruned");
    let weight: f64 = ac_edges[0]["weight"].as_f64().unwrap();
    assert!((weight - 0.45).abs() < 1e-5, "Edge A->C weight should decay to 0.45, got {}", weight);

    Ok(())
}

