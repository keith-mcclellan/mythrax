use std::fs;
use std::sync::Arc;
use std::sync::Mutex;
use anyhow::Result;
use tempfile::tempdir;

use mythrax_core::db::{SurrealBackend, StorageBackend, parse_record_id};
use mythrax_core::contracts::{EpisodeSave, WisdomRule};
use mythrax_core::cognitive::compactor::Compactor;
use mythrax_core::store::MarkdownStore;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_federated_promotion_and_auto_push() -> Result<()> {
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

    // Initialize git repository in workspace so that git commands succeed
    let git_init_status = std::process::Command::new("git")
        .arg("init")
        .current_dir(&workspace_root)
        .status()?;
    assert!(git_init_status.success());

    // Configure git user so commit succeeds
    let _ = std::process::Command::new("git")
        .args(&["config", "user.name", "Test User"])
        .current_dir(&workspace_root)
        .status();
    let _ = std::process::Command::new("git")
        .args(&["config", "user.email", "test@example.com"])
        .current_dir(&workspace_root)
        .status();

    unsafe {
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
        std::env::set_var("MYTHRAX_VAULT_ROOT", vault_root.to_str().unwrap());
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    let store = MarkdownStore::new(&vault_root)?;

    // Seed a rule file in the vault
    let rule_vault_path = "wisdom/rule_1.md";
    let rule_content = r#"---
target_pattern: "AntiPatternX"
action_to_avoid: "Avoiding X"
causal_explanation: "Causes Y"
prescribed_remedy: "Use Z"
tier: "dynamic"
scope: "project-x"
utility: 50.0
generator_name: "test"
---
Rule body"#;
    store.write_file(rule_vault_path, rule_content)?;

    let rule = WisdomRule {
        id: None,
        target_pattern: "AntiPatternX".to_string(),
        action_to_avoid: "Avoiding X".to_string(),
        causal_explanation: "Causes Y".to_string(),
        prescribed_remedy: "Use Z".to_string(),
        tier: "dynamic".to_string(),
        scope: "project-x".to_string(),
        vault_path: Some(rule_vault_path.to_string()),
        embedding: None,
        source_episodes: vec![],
        generator_name: "test".to_string(),
        similarity: None,
        utility: Some(50.0),
    };

    // Save wisdom rule (should trigger T1 federated promotion)
    let rule_id = backend.save_wisdom_rule(&rule).await?;
    assert!(rule_id.starts_with("wisdom:"));

    // Give the background thread a moment to run the git commands
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Verify the file was promoted to .mythrax-shared/wisdom/proposed/
    let shared_proposed_dir = workspace_root.join(".mythrax-shared").join("wisdom").join("proposed");
    assert!(shared_proposed_dir.exists());
    let promoted_file = shared_proposed_dir.join("rule_1.md");
    assert!(promoted_file.exists());

    let promoted_content = fs::read_to_string(&promoted_file)?;
    assert!(promoted_content.contains("target_pattern: \"AntiPatternX\""));

    Ok(())
}

#[tokio::test]
async fn test_concatenated_conflict_resolution() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let tmp = tempdir()?;
    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;

    unsafe {
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
    }

    let shared_dir = workspace_root.join(".mythrax-shared");
    let proposed_dir = shared_dir.join("wisdom").join("proposed");
    fs::create_dir_all(&proposed_dir)?;

    // Write two conflicting rules having the same target_pattern
    let rule1_md = r#"---
target_pattern: "DuplicatePattern"
action_to_avoid: "Action A"
causal_explanation: "Expl A"
prescribed_remedy: "Remedy A"
tier: "dynamic"
scope: "scope-a"
utility: 40.0
generator_name: "manual"
---
Body"#;

    let rule2_md = r#"---
target_pattern: "DuplicatePattern"
action_to_avoid: "Action B"
causal_explanation: "Expl B"
prescribed_remedy: "Remedy B"
tier: "skills"
scope: "scope-b"
utility: 60.0
generator_name: "manual"
---
Body"#;

    fs::write(proposed_dir.join("rule_a.md"), rule1_md)?;
    fs::write(proposed_dir.join("rule_b.md"), rule2_md)?;

    // Execute the merge-vault CLI action directly in-memory
    mythrax_core::cli::handle_merge_vault().await.unwrap();

    // Verify that original conflicting rules are moved to .mythrax-shared/wisdom/conflict_archive/
    let conflict_archive = shared_dir.join("wisdom").join("conflict_archive");
    assert!(conflict_archive.exists());
    assert!(conflict_archive.join("rule_a.md").exists());
    assert!(conflict_archive.join("rule_b.md").exists());

    // Verify that the merged rule is in proposed/
    let merged_rule_file = proposed_dir.join("duplicatepattern-merged.md");
    assert!(merged_rule_file.exists());

    let merged_content = fs::read_to_string(&merged_rule_file)?;
    assert!(merged_content.contains("DuplicatePattern"));
    assert!(merged_content.contains("Action A"));
    assert!(merged_content.contains("Action B"));
    assert!(merged_content.contains("Expl A"));
    assert!(merged_content.contains("Expl B"));
    assert!(merged_content.contains("Remedy A"));
    assert!(merged_content.contains("Remedy B"));
    assert!(merged_content.contains("> [!WARNING]"));
    assert!(merged_content.contains("tier: skills")); // Max tier was skills
    assert!(merged_content.contains("utility: 60.0")); // Max utility was 60.0

    Ok(())
}

#[tokio::test]
async fn test_biological_episode_decay_and_reinforcement() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Seed an episode
    let ep = EpisodeSave {
        title: "Decay Test Episode".to_string(),
        content: "Decay test content.".to_string(),
        entities: vec![],
        scope: Some("decay-test".to_string()),
        vault_path: Some("episodes/decay_ep.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
    };

    let ep_id = backend.save_episode(&ep).await?;

    // 1. Initial utility should be 50.0
    let sql = "SELECT utility, last_retrieved_at FROM episode WHERE id = $id;";
    let mut response = backend.db.query(sql).bind(("id", parse_record_id(&ep_id)?)).await?;
    let records: Vec<serde_json::Value> = response.take(0)?;
    assert_eq!(records.len(), 1);
    let initial_utility = records[0]["utility"].as_f64().unwrap();
    assert_eq!(initial_utility, 50.0);

    // 2. Artificially back-date last_retrieved_at to 10 days ago to trigger decay
    let ten_days_ago = (chrono::Utc::now() - chrono::Duration::days(10)).to_rfc3339();
    let update_sql = "UPDATE $id MERGE { last_retrieved_at: $time };";
    let _ = backend.db.query(update_sql)
        .bind(("id", parse_record_id(&ep_id)?))
        .bind(("time", ten_days_ago))
        .await?;

    // 3. Run a search. This will calculate decay on-the-fly and return it
    let search_res = backend.search("Decay", Some("decay-test"), false, 10, 0, 0.0, None, false, true, false).await?;
    assert_eq!(search_res.results.len(), 1);
    let returned_utility = search_res.results[0].utility;
    // Decay: 50.0 * e^(-0.05 * 10) = 50.0 * e^(-0.5) = 50.0 * 0.6065 = 30.32
    assert!(returned_utility < 40.0);
    assert!(returned_utility > 25.0);

    // 4. Verify reinforcement resets it to 50.0
    // Give the search's background write-back thread a moment to finish to avoid a race condition
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
    backend.reinforce_episode(&ep_id).await?;
    let mut response2 = backend.db.query(sql).bind(("id", parse_record_id(&ep_id)?)).await?;
    let records2: Vec<serde_json::Value> = response2.take(0)?;
    let reinforced_utility = records2[0]["utility"].as_f64().unwrap();
    assert_eq!(reinforced_utility, 50.0);

    Ok(())
}

#[tokio::test]
async fn test_cognitive_sleep_archiving() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("episodes"))?;
    fs::create_dir_all(vault_root.join("wiki"))?;

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    let store = MarkdownStore::new(&vault_root)?;
    let compactor = Compactor::new();

    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    // Seed a physical episode file and active SurrealDB record with utility < 5.0
    let ep_vault_path = "episodes/decayed_ep.md";
    let ep_content = "Decayed episode content.";
    store.write_file(ep_vault_path, ep_content)?;

    let ep = EpisodeSave {
        title: "Decayed Episode".to_string(),
        content: ep_content.to_string(),
        entities: vec![],
        scope: Some("archive-test".to_string()),
        vault_path: Some(ep_vault_path.to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
    };
    let ep_id = backend.save_episode(&ep).await?;

    // Force utility to 4.0 in the DB
    let update_sql = "UPDATE $id MERGE { utility: 4.0 };";
    let _ = backend.db.query(update_sql).bind(("id", parse_record_id(&ep_id)?)).await?;

    // Verify it exists in the active records
    let active_eps = backend.get_all_episodes().await?;
    assert_eq!(active_eps.len(), 1);

    // Run compaction/sleep cycle
    compactor.compact_scope(&backend, &store, "archive-test").await?;

    // 1. Verify active record is deleted from DB
    let active_eps_after = backend.get_all_episodes().await?;
    assert_eq!(active_eps_after.len(), 0);

    // 2. Verify physical file is moved to vault/archive/
    let old_file = vault_root.join(ep_vault_path);
    assert!(!old_file.exists());
    let archived_file = vault_root.join("vault/archive/decayed_ep.md");
    assert!(archived_file.exists());

    // 3. Verify high-level Raptor summary WikiNode is created in DB
    let wiki_nodes = backend.get_all_wiki_nodes().await?;
    assert_eq!(wiki_nodes.len(), 1);
    assert!(wiki_nodes[0].name.contains("Raptor Summary:"));

    Ok(())
}

#[tokio::test]
async fn test_auditor_calibration_and_citations() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;

    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }



    // Test Citations Footnotes in MCP
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    let store = MarkdownStore::new(&vault_root)?;
    let backend_arc = Arc::new(backend);
    let mcp = mythrax_core::mcp::McpServer::new(backend_arc.clone(), Arc::new(store));

    // Seed an episode
    let ep = EpisodeSave {
        title: "Cited Episode".to_string(),
        content: "Cited content.".to_string(),
        entities: vec![],
        scope: Some("citations-test".to_string()),
        vault_path: Some("episodes/cited.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
    };
    let ep_id = mcp.call_tool("save_episode", serde_json::to_value(&ep)?).await?;
    
    // Test Auditor Self-Healing Calibration directly in-memory against the seeded episode
    mythrax_core::cli::run_auditor(&backend_arc).await.unwrap();

    let ep_id_str = ep_id["content"][0]["text"].as_str().unwrap().split("episode:").last().unwrap().trim_matches('"');
    let full_ep_id = format!("episode:{}", ep_id_str);

    // Call search_memories with a session_id
    let search_args = serde_json::json!({
        "query": "Cited",
        "scope": "citations-test",
        "session_id": "session123",
        "include_episodes": true
    });
    let search_res = mcp.call_tool("search_memories", search_args).await?;
    assert!(search_res.is_object());

    // Verify citation ID is written to session STM
    let get_args = serde_json::json!({
        "session_id": "session123",
        "key": "_session_citations"
    });
    let get_res = mcp.call_tool("get_short_term", get_args).await?;
    let citations_text = get_res["content"][0]["text"].as_str().unwrap();
    assert!(citations_text.contains(&full_ep_id));

    // Call save_handoff to create a handoff task plan and verify citation footnote is automatically appended
    let handoff_file = vault_root.join("handoff_task.md");
    fs::write(&handoff_file, "# Task Plan\nThis is a task plan.")?;

    let handoff_args = serde_json::json!({
        "parent_conversation_id": "session123",
        "subagent_conversation_id": "subagent456",
        "summary": "citations handoff",
        "handoff_file_path": handoff_file.to_str().unwrap(),
        "scope": "citations-test"
    });
    let _ = mcp.call_tool("save_handoff", handoff_args).await?;

    // Verify Citations footnote block is appended to the handoff file
    let handoff_content = fs::read_to_string(&handoff_file)?;
    assert!(handoff_content.contains("### Citations"));
    assert!(handoff_content.contains("Cited Episode"));
    assert!(handoff_content.contains("episodes/cited.md"));

    Ok(())
}
