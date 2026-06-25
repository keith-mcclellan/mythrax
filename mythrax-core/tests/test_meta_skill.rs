use anyhow::Result;
use tempfile::tempdir;
use std::fs;
use std::env;
use std::sync::Mutex;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::store::MarkdownStore;
use mythrax_core::contracts::{WisdomRule, WikiNode};
use mythrax_core::cognitive::meta_skill::MetaSkillSynthesizer;

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
        tier: "dynamic".to_string(),
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
    };
    backend.save_wisdom_rule(&rule).await?;

    let node = WikiNode {
        id: None,
        name: "Test Document".to_string(),
        content: "Detailed design specifications".to_string(),
        scope: "test-scope".to_string(),
        vault_path: None,
        embedding: Some(vec![0.1; 768]),
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

    // Set HOME to tmp so scan_all_skills looks there for global config if not found
    let original_home = env::var("HOME").ok();
    unsafe { env::set_var("HOME", tmp.path()); }

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
    unsafe { env::set_var("HOME", tmp.path()); }

    let store = MarkdownStore::new(&vault_root)?;

    // Create one meta skill and one custom skill
    let skills_dir = vault_root.join("../.agents/skills");
    let sk1_dir = skills_dir.join("meta-git-commit");
    let sk2_dir = skills_dir.join("custom-git-pull");
    fs::create_dir_all(&sk1_dir)?;
    fs::create_dir_all(&sk2_dir)?;

    let sk1_content = "---\nname: meta-git-commit\ndescription: git commit instructions\ngenerator_name: MetaSkillSynthesizer\n---\nbody";
    // Custom skill (no generator_name)
    let sk2_content = "---\nname: custom-git-pull\ndescription: git pull manual instructions\n---\nbody";

    fs::write(sk1_dir.join("SKILL.md"), sk1_content)?;
    fs::write(sk2_dir.join("SKILL.md"), sk2_content)?;

    let synthesizer = MetaSkillSynthesizer::new();
    let merged_name = synthesizer.merge_skills(
        &backend,
        &store,
        &["meta-git-commit".to_string(), "custom-git-pull".to_string()],
        "git-workflow"
    ).await?;

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
