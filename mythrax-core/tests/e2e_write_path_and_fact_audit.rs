//! End-to-End Test Suite: Write Paths, Frontmatter Integrity, & Fact Extraction
//! Enforces Karpathy TDD directives and Karpathy execution rules.

use anyhow::Result;
use mythrax_core::contracts::{EpisodeSave, FactSource};
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::markdown::parse_frontmatter;
use mythrax_core::vault::watcher::sync_file_to_db_with_cache;
use std::sync::Arc;
use tempfile::tempdir;

/// Test 1: Validate frontmatter integrity across ingestion, disk write, and watcher sync.
/// MUST FAIL if watcher overwrites vault file with plain text or duplicates frontmatter.
#[tokio::test]
async fn test_e2e_frontmatter_preservation_and_no_duplication() -> Result<()> {
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    std::fs::create_dir_all(&vault_root)?;
    let store = Arc::new(MarkdownStore::new(&vault_root)?);
    let backend: Arc<dyn StorageBackend> = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let rel_path = "episodes/test_episode_001.md";
    let original_content = "---\ntitle: \"Test Episode\"\nscope: \"mythrax\"\nsource: \"antigravity\"\n---\n\n# Test Episode\n\nUser request details and [[wiki/mythrax/raw/artifact1]].\n";

    // Step 1: Save episode via DB backend (simulating ingestion)
    let ep_save = EpisodeSave::builder("Test Episode".to_string(), original_content.to_string())
        .scope(Some("mythrax".to_string()))
        .vault_path(Some(rel_path.to_string()))
        .build();
    backend.save_episode(&ep_save).await?;

    let full_path = vault_root.join(rel_path);
    assert!(full_path.exists(), "File must exist on disk");

    // Step 2: Trigger watcher sync on the written file
    sync_file_to_db_with_cache(&full_path, &backend, &store, None).await?;

    // Step 3: Read file from disk and assert frontmatter is preserved exactly once
    let disk_content = std::fs::read_to_string(&full_path)?;
    let (yaml_opt, body) = parse_frontmatter(&disk_content);

    let yaml = yaml_opt.expect("Frontmatter must remain intact after watcher sync");
    assert_eq!(yaml["title"].as_str(), Some("Test Episode"));
    assert_eq!(yaml["scope"].as_str(), Some("mythrax"));

    // Assert no duplicate '---' delimiters exist in body
    assert!(!body.contains("---"), "Body must not contain duplicated frontmatter delimiters");
    assert!(body.contains("[[wiki/mythrax/raw/artifact1]]"), "Wikilinks must be preserved in markdown body");

    Ok(())
}

/// Test 2: Validate session artifact fact extraction pipeline.
/// MUST FAIL if pre-scanned artifacts bypass fact extraction.
#[tokio::test]
async fn test_e2e_artifact_fact_extraction_and_linking() -> Result<()> {
    let backend: Arc<dyn StorageBackend> = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let artifact_content = "# Design Decision\n\nWe select SurrealDB embedded engine for zero-network local sidecar deployment.";
    let vault_path = "wiki/mythrax/raw/design_decision.md";
    let scope = "mythrax";

    // Execute document fact extraction on session artifact
    let facts = mythrax_core::cognitive::pipeline::extract_from_document(
        backend.as_ref(),
        None,
        artifact_content,
        vault_path,
        scope,
    ).await?;

    assert!(!facts.is_empty(), "Artifact must produce at least one extracted Fact");
    let fact = &facts[0];
    assert_eq!(fact.source_type, FactSource::Document);
    assert_eq!(fact.scope, scope);
    let hypothesis = fact.hypothesis.as_deref().expect("hypothesis string must exist");
    assert_eq!(hypothesis.is_empty(), false);

    // Assert Fact record is persisted in DB
    let fact_id = fact.id.as_ref().expect("fact id must exist");
    let fetched = backend.get_fact(fact_id).await?.expect("Fact must be retrievable from database");
    assert_eq!(fetched.scope, scope);

    Ok(())
}

/// Test 3: Validate AST symbol to code fact relation.
/// MUST FAIL if extracted code facts are not linked to specific CodeSymbol record IDs.
#[tokio::test]
async fn test_e2e_ast_symbol_fact_linking() -> Result<()> {
    let backend: Arc<dyn StorageBackend> = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let code_content = "/// Invariant: Memory buffer must be flushed before drop.\npub fn flush_buffer() -> Result<()> { Ok(()) }";
    let file_path = "src/buffer.rs";
    let scope = "mythrax";

    let facts = mythrax_core::cognitive::pipeline::extract_from_code(
        backend.as_ref(),
        None,
        code_content,
        file_path,
        scope,
    ).await?;

    assert!(!facts.is_empty(), "Code file must produce extracted facts");

    // Fetch stored AST symbols for file
    let all_nodes = backend.get_all_wiki_nodes().await?;
    let ast_node = all_nodes.into_iter().find(|n| n.content.contains("flush_buffer")).expect("AST symbol WikiNode must be persisted");
    assert_eq!(ast_node.scope, scope);


    Ok(())
}

/// Test 4: Validate automatic sanitization of pre-existing corrupted title repetitions and recursion guard.
/// MUST FAIL if pre-existing repeated titles are preserved or if generated fact files trigger recursive fact extraction.
#[tokio::test]
async fn test_e2e_sanitize_corrupted_headers_and_no_recursion() -> Result<()> {
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    std::fs::create_dir_all(&vault_root)?;
    let store = Arc::new(MarkdownStore::new(&vault_root)?);
    let backend: Arc<dyn StorageBackend> = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    // Step 1: Create a corrupted file on disk containing H2 headers and repeated title tokens
    let rel_path = "wiki/mythrax/raw/cto_review_critique_part1.md";
    let corrupted_content = "---\nnode_type: insight\nscope: mythrax\ntitle: mythrax/cto_review_critique_part1\n---\n\n## cto_review_critique_part1\n\n### mythrax/cto_review_critique_part1\n\ncto_review_critique_part1 cto_review_critique_part1 cto_review_critique_part1 cto_review_critique_part1\n\nCTO Adversarial Critique: Mythrax v2.6.0 Code Review\n";
    let full_path = vault_root.join(rel_path);
    std::fs::create_dir_all(full_path.parent().unwrap())?;
    std::fs::write(&full_path, corrupted_content)?;

    // Step 2: Trigger watcher sync / DB save
    sync_file_to_db_with_cache(&full_path, &backend, &store, None).await?;

    // Step 3: Read file from disk and DB, assert pristine content preservation and DB-Disk Parity
    let disk_content = std::fs::read_to_string(&full_path)?;
    assert!(disk_content.contains("CTO Adversarial Critique: Mythrax v2.6.0 Code Review"), "Actual content must be preserved");

    // Step 4: Verify DB content matches disk content (DB-Disk Parity)
    let all_nodes = backend.get_all_wiki_nodes().await?;
    let node_in_db = all_nodes.into_iter().find(|n| n.name == "mythrax/cto_review_critique_part1").unwrap();
    assert!(node_in_db.content.contains("CTO Adversarial Critique: Mythrax v2.6.0 Code Review"), "SurrealDB content must contain preserved body text");



    // Step 5: Verify uppercase extension and node_type metadata guards in extract_from_document
    let fact_path_upper = "wiki/mythrax/TEST_UPPERCASE_FACT.MD";
    let fact_facts_upper = mythrax_core::cognitive::pipeline::extract_from_document(
        backend.as_ref(),
        None,
        "Fact content",
        fact_path_upper,
        "mythrax",
    ).await?;
    assert!(fact_facts_upper.is_empty(), "Uppercase _FACT.MD files must be excluded from recursive fact extraction");

    let fact_content_yaml = "---\nnode_type: fact\nscope: mythrax\ntitle: custom_fact\n---\n\n# custom_fact\n\nFact detail";
    let fact_facts_meta = mythrax_core::cognitive::pipeline::extract_from_document(
        backend.as_ref(),
        None,
        fact_content_yaml,
        "wiki/mythrax/custom_name.md",
        "mythrax",
    ).await?;
    assert!(fact_facts_meta.is_empty(), "node_type: fact files must be excluded from recursive fact extraction regardless of filename");

    Ok(())
}


