use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::store::MarkdownStore;
use mythrax_core::cognitive::arbor::{ArborCoordinator, ArborLlmClient};

#[tokio::test]
async fn test_obsidian_compatibility_linking() -> Result<()> {
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("episodes"))?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    let store = MarkdownStore::new(&vault_root)?;

    // 1. Create a mock episode on disk and save in SurrealDB
    let ep_vault_path = "episodes/antigravity_test_ep.md";
    let ep_save = EpisodeSave {
        created_at: None,
        title: "antigravity_test_ep".to_string(),
        content: "This is some test transcript content.".to_string(),
        entities: vec![],
        scope: Some("test-scope".to_string()),
        vault_path: Some(ep_vault_path.to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        ..Default::default()
    };
    let ep_id = backend.save_episode(&ep_save).await?;
    // Write physical file to disk
    store.write_file(ep_vault_path, "# antigravity_test_ep\nThis is some test transcript content.")?;

    // 2. Mock a compaction (synthesis split analysis) or incremental merge
    // Let's create an insight relative path
    let insight_relative_path = "wiki/test-scope/insights/test_insight_123.md";
    
    // Build source episode links (simulating synthesis.rs logic)
    let mut source_ep_links = Vec::new();
    let mem_nodes = backend.get_memory_nodes(&[ep_id.clone()]).await?;
    for ep in mem_nodes.episodes {
        if let Some(ref path) = ep.vault_path {
            let target = path.strip_suffix(".md").unwrap_or(path);
            source_ep_links.push(format!("- [[{}|{}]]", target, ep.title));
        }
    }
    
    let source_ep_section = if !source_ep_links.is_empty() {
        format!("\n\n## Source Episodes\n{}", source_ep_links.join("\n"))
    } else {
        String::new()
    };

    let insight_content = format!(
        "---\ntitle: \"Test Insight\"\nscope: \"test-scope\"\n---\n\nInsight summary body.{}",
        source_ep_section
    );
    
    store.write_file(insight_relative_path, &insight_content)?;

    // Call store.append_link_to_file on the episode path to link back to the insight
    store.append_link_to_file(ep_vault_path, "Insights & Summaries", insight_relative_path, "Test Insight")?;

    // Assertions for Insight -> Episode
    let read_insight = fs::read_to_string(vault_root.join(insight_relative_path))?;
    assert!(read_insight.contains("## Source Episodes"));
    assert!(read_insight.contains("- [[episodes/antigravity_test_ep|antigravity_test_ep]]"));

    // Assertions for Episode -> Insight backlink
    let read_episode = fs::read_to_string(vault_root.join(ep_vault_path))?;
    assert!(read_episode.contains("## Insights & Summaries"));
    assert!(read_episode.contains("- [[wiki/test-scope/insights/test_insight_123|Test Insight]]"));

    // 3. Test dynamic wisdom rule bidirectional links
    let rule_path = "wisdom/dynamic/test_pattern_abc.md";
    let mut rule_source_ep_links = Vec::new();
    if let Ok(nodes) = backend.get_memory_nodes(&[ep_id.clone()]).await {
        for ep in nodes.episodes {
            if let Some(ref path) = ep.vault_path {
                let target = path.strip_suffix(".md").unwrap_or(path);
                rule_source_ep_links.push(format!("- [[{}|{}]]", target, ep.title));
            }
        }
    }
    let rule_source_ep_section = if !rule_source_ep_links.is_empty() {
        format!("\n\n## Source Episodes\n{}", rule_source_ep_links.join("\n"))
    } else {
        String::new()
    };

    let rule_md = format!(
        "---\ntarget_pattern: \"test_pattern\"\ntier: \"dynamic\"\n---\n\nWisdom body.{}",
        rule_source_ep_section
    );
    store.write_file(rule_path, &rule_md)?;
    
    // Link back from episode to wisdom rule
    store.append_link_to_file(ep_vault_path, "Derived Wisdom Rules", rule_path, "Wisdom: test_pattern")?;

    // Assertions for Wisdom -> Episode
    let read_wisdom = fs::read_to_string(vault_root.join(rule_path))?;
    assert!(read_wisdom.contains("## Source Episodes"));
    assert!(read_wisdom.contains("- [[episodes/antigravity_test_ep|antigravity_test_ep]]"));

    // Assertions for Episode -> Wisdom backlink
    let read_episode_after_wisdom = fs::read_to_string(vault_root.join(ep_vault_path))?;
    assert!(read_episode_after_wisdom.contains("## Derived Wisdom Rules"));
    assert!(read_episode_after_wisdom.contains("- [[wisdom/dynamic/test_pattern_abc|Wisdom: test_pattern]]"));

    Ok(())
}

#[derive(Clone)]
pub struct SimpleMockLLMClient;
impl ArborLlmClient for SimpleMockLLMClient {
    async fn propose_hypotheses(
        &self,
        _db: &dyn mythrax_core::db::StorageBackend,
        _parent_id: &str,
        _parent_hypothesis: &str,
        _target_files: &[(String, String)],
        _constraints: &[String],
    ) -> Result<String> {
        Ok(r#"[
            {
                "node_id": "CHILD_NODE",
                "hypothesis": "Test Child",
                "score": 90.0,
                "code_changes": {}
            }
        ]"#.to_string())
    }

    async fn evaluate_run(&self, _db: &dyn mythrax_core::db::StorageBackend, _run_logs: &str) -> Result<String> {
        Ok(r#"{"success": true, "score": 95.0, "insight": "Worked"}"#.to_string())
    }

    async fn abstract_insights(&self, _db: &dyn mythrax_core::db::StorageBackend, _parent_insight: Option<&str>, _child_insight: &str) -> Result<String> {
        Ok("insight".to_string())
    }
}

#[tokio::test]
async fn test_arbor_navigation_formatting() -> Result<()> {
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    let repo_path = tmp.path().join("repo");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(&repo_path)?;

    let db = SurrealBackend::new_in_memory().await?;
    db.init().await?;

    let coordinator = ArborCoordinator::new(
        db.db.clone(),
        vault_root.clone(),
        repo_path.clone(),
        SimpleMockLLMClient,
        "test-scope".to_string(),
        "pytest".to_string(),
        vec![],
    ).await;

    // Initialize root node
    coordinator.init_root("Root hypothesis".to_string(), None).await?;

    let root_path = vault_root.join("wiki/test-scope/hypothesis_tree/ROOT.md");
    assert!(root_path.exists());
    let root_md = fs::read_to_string(&root_path)?;
    assert!(root_md.contains("## Navigation"));
    assert!(root_md.contains("- **Parent**: None"));
    assert!(root_md.contains("- **Children**: None"));

    // Trigger ideation to create a child
    coordinator.trigger_ideation("ROOT").await?;

    let child_path = vault_root.join("wiki/test-scope/hypothesis_tree/CHILD_NODE.md");
    assert!(child_path.exists());
    let child_md = fs::read_to_string(&child_path)?;
    assert!(child_md.contains("## Navigation"));
    assert!(child_md.contains("- **Parent**: [[wiki/test-scope/hypothesis_tree/ROOT|ROOT]]"));

    // Verify parent ROOT.md updated to link to child
    let root_md_after = fs::read_to_string(&root_path)?;
    assert!(root_md_after.contains("- **Children**:"));
    assert!(root_md_after.contains("- [[wiki/test-scope/hypothesis_tree/CHILD_NODE|CHILD_NODE]]"));

    Ok(())
}
