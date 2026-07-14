use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::{WikiNode, EpisodeSave};
use mythrax_core::cognitive::synthesis::DreamCoordinator;
use mythrax_core::store::MarkdownStore;

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
    backend.save_profile_key("compactor.enable_cross_scope_graduation", "true").await?;
    coordinator.run_dream(&backend, &store, None, None).await?;

    // Verify a general scope WisdomRule has been created in DB
    let all_rules = backend.get_all_wisdom_rules().await?;
    let graduated_rule = all_rules.iter()
        .find(|r| r.scope == "general" && r.generator_name == "ScopeGraduator")
        .expect("Graduated WisdomRule should exist");

    println!("GRADUATED RULE: {:?}", graduated_rule);
    assert_eq!(graduated_rule.target_pattern, "test_graduated_pattern");
    assert_eq!(graduated_rule.tier, mythrax_core::contracts::Tier::Project); // Because wiki nodes are dynamic, not all procedural

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
