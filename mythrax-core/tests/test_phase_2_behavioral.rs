use std::fs;
use std::sync::Arc;
use anyhow::Result;
use tempfile::tempdir;
use mythrax_core::db::{SurrealBackend, StorageBackend, parse_record_id};
use mythrax_core::contracts::{EpisodeSave, WikiNode};
use mythrax_core::cognitive::compactor::Compactor;
use mythrax_core::cognitive::synthesis::DreamCoordinator;
use mythrax_core::store::MarkdownStore;
use mythrax_core::mcp::McpServer;

use std::sync::Mutex;
static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_zero_touch_correction_and_critic_extraction() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;
    fs::create_dir_all(vault_root.join("wisdom/dynamic"))?;
    fs::create_dir_all(vault_root.join("episodes"))?;

    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;

    // We must set the environment variables
    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
        std::env::set_var("MYTHRAX_ACTIVE_SCOPE", "test-project");
    }

    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let store = Arc::new(MarkdownStore::new(&vault_root)?);

    // Instantiate McpServer to use its call_tool / handle_request
    let server = McpServer::new_local(backend.clone(), store.clone());

    // 1. Trigger zero-touch correction by saving an episode containing a correction indicator
    let save_args = serde_json::json!({
        "action": "save",
        "title": "Correction Episode",
        "content": "Wait, that was a mistake! You forgot to run the tests first.",
        "scope": "test-project"
    });

    let result = server.call_tool("write", save_args).await?;
    assert!(result.to_string().contains("Episode saved successfully"));

    // Call run_llm_critic directly to diagnose any errors synchronously
    mythrax_core::mcp::run_llm_critic(backend.clone(), store.clone(), "Wait, that was a mistake! You forgot to run the tests first.".to_string(), Some("test-project".to_string())).await?;

    // Verify wisdom rule was written to dynamic wisdom directory
    let wisdom_dynamic_dir = vault_root.join("wisdom/dynamic");
    let entries = fs::read_dir(&wisdom_dynamic_dir)?;
    let mut files = Vec::new();
    for entry in entries.flatten() {
        files.push(entry.file_name());
    }
    assert!(!files.is_empty(), "LLM Critic should have saved a wisdom rule file under wisdom/dynamic/");

    // Verify registered in SurrealDB with utility = 50.0 and active project scope
    let all_rules = backend.get_all_wisdom_rules().await?;
    assert!(!all_rules.is_empty(), "Wisdom rule should be registered in the database");
    let rule = &all_rules[0];
    assert_eq!(rule.utility, Some(50.0));
    assert_eq!(rule.scope, "test-project");

    Ok(())
}

#[tokio::test]
async fn test_aesthetic_vs_procedural_synthesis() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;
    fs::create_dir_all(vault_root.join("wisdom/dynamic"))?;
    fs::create_dir_all(vault_root.join("global/wisdom/permanent"))?;
    fs::create_dir_all(vault_root.join("episodes"))?;

    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let store = Arc::new(MarkdownStore::new(&vault_root)?);

    // Seed some episodes so DreamCoordinator has something to process
    let ep1 = mythrax_core::contracts::Episode {
        id: None,
        title: "Test CSS layout".to_string(),
        content: "We designed a dark theme layout with a glowing shadow.".to_string(),
        source: None,
        scope: Some("project-x".to_string()),
        vault_path: Some("episodes/ep1.md".to_string()),
        embedding: Some(vec![0.1; 768]), // Mock embedding
        processed_in_dream: Some(false),
        source_episode: None,
        last_retrieved_at: None,
        utility: None,
        node_type: Some("procedural".to_string()),
        ..Default::default()
    };
    let ep2 = mythrax_core::contracts::Episode {
        id: None,
        title: "Refactored CSS layout".to_string(),
        content: "Fixed alignment and shadow sizing in dark theme layout.".to_string(),
        source: None,
        scope: Some("project-y".to_string()),
        vault_path: Some("episodes/ep2.md".to_string()),
        embedding: Some(vec![0.11; 768]), // Close embedding to form a cluster
        processed_in_dream: Some(false),
        source_episode: None,
        last_retrieved_at: None,
        utility: None,
        node_type: Some("procedural".to_string()),
        ..Default::default()
    };

    backend.save_episode(&EpisodeSave {
        title: ep1.title.clone(),
        content: ep1.content.clone(),
        entities: vec![],
        scope: ep1.scope.clone(),
        vault_path: ep1.vault_path.clone(),
        node_type: ep1.node_type.clone(),
        source_episode: None,
        session_id: None,
        task_id: None,
        ..Default::default()
    }).await?;

    backend.save_episode(&EpisodeSave {
        title: ep2.title.clone(),
        content: ep2.content.clone(),
        entities: vec![],
        scope: ep2.scope.clone(),
        vault_path: ep2.vault_path.clone(),
        node_type: ep2.node_type.clone(),
        source_episode: None,
        session_id: None,
        task_id: None,
        ..Default::default()
    }).await?;

    // Seed embeddings in DB (since save_episode might not generate mock ones)
    let db_eps = backend.get_all_episodes().await?;
    for ep in db_eps {
        let ep_id = ep.id.unwrap();
        backend.db.query("UPDATE $id SET embedding = $emb;")
            .bind(("id", parse_record_id(&ep_id)?))
            .bind(("emb", vec![0.1f32; 768]))
            .await?.check()?;
    }

    let coordinator = DreamCoordinator::new();
    coordinator.run_dream(&*backend, &store, Some("deep"), backend.embedder.clone()).await?;

    // The mock LLM when prompt contains "Wisdom" will return a procedural rule.
    // Procedural rules should be promoted to Global permanent wisdom and indexed with scope = "general", tier = "permanent".

    let global_permanent_dir = vault_root.join("global/wisdom/permanent");
    let entries = fs::read_dir(&global_permanent_dir)?;
    let mut files = Vec::new();
    for entry in entries.flatten() {
        files.push(entry.file_name());
    }
    assert!(!files.is_empty(), "DreamCoordinator should have promoted procedural rule to global permanent wisdom");

    let all_rules = backend.get_all_wisdom_rules().await?;
    assert!(!all_rules.is_empty());
    let promoted_rule = all_rules.iter().find(|r| r.tier == "permanent").unwrap();
    assert_eq!(promoted_rule.scope, "general");

    Ok(())
}

#[tokio::test]
async fn test_attention_anchors_verbatim_carry() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wiki/scope1/insights"))?;
    fs::create_dir_all(vault_root.join("wiki/compaction"))?;
    fs::create_dir_all(vault_root.join("wiki/general"))?;
    fs::create_dir_all(vault_root.join(".handoffs"))?;

    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let store = Arc::new(MarkdownStore::new(&vault_root)?);
    let compactor = Compactor::new();

    // 1. Set up input texts containing attention anchor markers
    let ins_md = r#"---
title: "Anchor Insight"
scope: "scope1"
source_episodes:
  - "ep1"
---
This is standard content.
@attention-anchor Always use Vanilla CSS
[ANCHOR: Keep components focused]"#;

    fs::write(vault_root.join("wiki/scope1/insights/anchor_insight.md"), ins_md)?;

    // Save corresponding WikiNode
    let node = WikiNode {
        id: None,
        name: "Anchor Insight".to_string(),
        content: "This is standard content.\n@attention-anchor Always use Vanilla CSS\n[ANCHOR: Keep components focused]".to_string(),
        scope: "scope1".to_string(),
        vault_path: Some("wiki/scope1/insights/anchor_insight.md".to_string()),
        embedding: Some(vec![0.1; 768]),
    };
    backend.save_wiki_node(&node).await?;

    // 2. Set up STM active anchors under key `_active_anchors`
    let stm_data = serde_json::json!({
        "_active_anchors": [
            "Test TDD cycle first",
            "Do not suppress compiler warnings"
        ]
    });
    fs::write(vault_root.join(".handoffs/stm_test_session.json"), serde_json::to_string(&stm_data)?)?;

    // 3. Run compaction
    compactor.compact_scope(&*backend, &store, "scope1", backend.embedder.clone()).await?;

    // 4. Verify that anchors are carried verbatim in the compaction file and the content is cleaned of markers
    let compaction_dir = vault_root.join("wiki/compaction");
    let entries = fs::read_dir(&compaction_dir)?;
    let mut comp_file_content = String::new();
    for entry in entries.flatten() {
        let content = fs::read_to_string(entry.path())?;
        if content.contains("Miscellaneous") {
            comp_file_content = content;
            break;
        }
    }
    
    assert!(!comp_file_content.is_empty(), "Compaction file should be created");
    
    // Extracted anchors must be appended verbatim
    assert!(comp_file_content.contains("Always use Vanilla CSS"));
    assert!(comp_file_content.contains("Keep components focused"));
    // STM active anchors must be appended verbatim
    assert!(comp_file_content.contains("Test TDD cycle first"));
    assert!(comp_file_content.contains("Do not suppress compiler warnings"));

    Ok(())
}
