use std::fs;
use std::sync::Arc;
use anyhow::Result;
use tempfile::tempdir;
use mythrax_core::db::{SurrealBackend, StorageBackend};
// Removed unused imports
use mythrax_core::cognitive::paging;
use mythrax_core::cognitive::compactor::Compactor;

use std::sync::Mutex;
static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_dual_durability_journaling() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    
    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;

    // Set env var for workspace root
    unsafe {
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
    }

    // Create a mock task.md
    let task_md_content = "- [ ] Task 1\n- [ ] Task 2";
    fs::write(workspace_root.join("task.md"), task_md_content)?;

    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    // Call save_stm to seed some STM data
    backend.save_stm("test-session", "_active_anchors", "[\"Anchor 1\"]").await?;

    // Execute journal_state
    backend.journal_state(&vault_root, Some("test-session")).await?;

    // Verify it saved in SurrealDB session_state table
    let mut resp = backend.db.query("SELECT * FROM type::record('session_state', 'test-session');").await?;
    let state_opt: Option<serde_json::Value> = resp.take(0)?;
    assert!(state_opt.is_some());
    let state = state_opt.unwrap();
    assert_eq!(state["task_checklist"].as_str().unwrap(), task_md_content);
    assert_eq!(state["active_stm"]["_active_anchors"].as_str().unwrap(), "[\"Anchor 1\"]");

    // Verify it wrote the backup file
    let journal_path = vault_root.join(".mythrax/session_journal.json");
    assert!(journal_path.exists());
    let backup_content = fs::read_to_string(journal_path)?;
    let backup_json: serde_json::Value = serde_json::from_str(&backup_content)?;
    assert_eq!(backup_json["task_checklist"].as_str().unwrap(), task_md_content);
    assert_eq!(backup_json["active_stm"]["_active_anchors"].as_str().unwrap(), "[\"Anchor 1\"]");

    Ok(())
}

#[tokio::test]
async fn test_symbol_extraction_and_paging_and_restoration() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // 1. Rust symbol extraction
    let rust_code = r#"
pub struct BackendStruct {
    field: String,
}

impl BackendStruct {
    pub fn new() -> Self {
        Self { field: String::new() }
    }
}
"#;
    let rust_symbols = paging::extract_symbols(rust_code, "rs");
    assert_eq!(rust_symbols.len(), 3);
    assert_eq!(rust_symbols[0].name, "BackendStruct");
    assert_eq!(rust_symbols[0].kind, "struct");
    assert_eq!(rust_symbols[1].name, "BackendStruct");
    assert_eq!(rust_symbols[1].kind, "impl");
    assert_eq!(rust_symbols[2].name, "new");
    assert_eq!(rust_symbols[2].kind, "fn");

    // TypeScript symbol extraction
    let ts_code = r#"
export class MyClass {
    constructor() {}
}

export interface MyInterface {
    id: string;
}
"#;
    let ts_symbols = paging::extract_symbols(ts_code, "ts");
    assert_eq!(ts_symbols.len(), 2);
    assert_eq!(ts_symbols[0].name, "MyClass");
    assert_eq!(ts_symbols[0].kind, "class");
    assert_eq!(ts_symbols[1].name, "MyInterface");
    assert_eq!(ts_symbols[1].kind, "interface");

    // Python symbol extraction
    let py_code = r#"
class MyPyClass:
    def __init__(self):
        pass

def my_func():
    pass
"#;
    let py_symbols = paging::extract_symbols(py_code, "py");
    assert_eq!(py_symbols.len(), 3);
    assert_eq!(py_symbols[0].name, "MyPyClass");
    assert_eq!(py_symbols[0].kind, "class");
    assert_eq!(py_symbols[1].name, "__init__");
    assert_eq!(py_symbols[1].kind, "def");
    assert_eq!(py_symbols[2].name, "my_func");
    assert_eq!(py_symbols[2].kind, "def");

    // 2. Page code block and write to DB
    let paged = paging::page_code_block(&backend, rust_code, "rs").await?;
    assert!(paged.contains("page_struct_backendstruct"));
    assert!(paged.contains("[Paged Symbol: Reference page_struct_backendstruct]"));
    assert!(paged.contains("=== Symbol Page Map ==="));

    // Verify archived in SurrealDB symbol_archive
    let mut resp = backend.db.query("SELECT * FROM type::record('symbol_archive', 'page_struct_backendstruct');").await?;
    let sym_opt: Option<serde_json::Value> = resp.take(0)?;
    assert!(sym_opt.is_some());
    let sym_val = sym_opt.unwrap();
    assert_eq!(sym_val["symbol_name"].as_str().unwrap(), "BackendStruct");

    // 3. Test transparent symbol restoration/swapping
    let restored = paging::intercept_and_restore_symbols(&backend, &paged).await;
    assert!(!restored.contains("[Paged Symbol: Reference page_struct_backendstruct]"));
    assert!(restored.contains("pub struct BackendStruct"));

    Ok(())
}

#[tokio::test]
async fn test_checkpointing_daemon_and_delta_compaction() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;

    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;

    // Set env var
    unsafe {
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    // Create a Cargo.toml to trigger Rust project detection
    fs::write(workspace_root.join("Cargo.toml"), "[package]")?;

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Seed config for LLM mock client
    backend.db.query("UPSERT config:settings CONTENT { active_provider: 'local', model: 'mock', cloud_provider: 'mock' };").await?;

    // Create a checkpoint
    let response = backend.db.query("
        UPSERT checkpoint_node:ch1 CONTENT {
            project_type: 'rust',
            exit_code: 0,
            compiler_errors: '',
            git_diff: 'diff --git a/src/lib.rs b/src/lib.rs\n+pub fn new() {}',
            timestamp: time::now()
        };
    ").await?;
    response.check()?;

    let response2 = backend.db.query("
        UPSERT checkpoint_node:ch2 CONTENT {
            project_type: 'rust',
            exit_code: 0,
            compiler_errors: '',
            git_diff: 'diff --git a/src/main.rs b/src/main.rs\n+fn main() {}',
            timestamp: time::now() - 1h
        };
    ").await?;
    response2.check()?;

    // Verify checkpoints returned by get_checkpoints
    let checkpoints = backend.get_checkpoints().await?;
    assert_eq!(checkpoints.len(), 2);

    // Run delta compaction
    let compactor = Compactor::new();
    let delta = compactor.delta_compact_checkpoints(&backend).await?;
    assert!(!delta.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_auto_trigger_paging_in_compaction() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() { Ok(guard) => guard, Err(p) => p.into_inner() };
    let tmp = tempdir()?;
    
    let workspace_root = tmp.path().join("workspace");
    let vault_root = tmp.path().join("vault");
    std::fs::create_dir_all(&workspace_root)?;
    std::fs::create_dir_all(&vault_root)?;

    unsafe {
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    // Write the source file that must NOT be paged
    let main_rs_path = workspace_root.join("main.rs");
    let original_content = r#"pub struct TestPaging {
    val: i32,
}

pub fn run_test() {}
"#;
    std::fs::write(&main_rs_path, original_content)?;

    // Initialize backend
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Seed LLM config
    backend.db.query("UPSERT config:settings CONTENT { active_provider: 'local', model: 'mock', cloud_provider: 'mock' };").await?;

    // Initialize store
    let store = mythrax_core::store::MarkdownStore::new(&vault_root)?;

    // Create a mock insight to trigger compaction
    let insight_dir = vault_root.join("wiki/test_scope/insights");
    std::fs::create_dir_all(&insight_dir)?;
    let insight_path = insight_dir.join("mock_insight.md");
    let insight_content = r#"---
title: Mock Insight
source_episodes: []
---
This is a test insight that references `page_fn_test_fn`.
"#;
    std::fs::write(&insight_path, insight_content)?;

    // Run compactor
    let compactor = Compactor::new();
    compactor.compact_scope(&backend, &store, "test_scope", backend.embedder.clone()).await?;

    // Assert that the workspace source file was NOT modified on disk
    let current_content = std::fs::read_to_string(&main_rs_path)?;
    assert_eq!(current_content, original_content, "Source file should not be modified by compaction");

    // Find the compaction summary file
    let compaction_dir = vault_root.join("wiki/test_scope/compactions");
    let mut found_compaction_file = None;
    
    if let Ok(entries) = std::fs::read_dir(&compaction_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "md") {
                let content = std::fs::read_to_string(&path)?;
                if content.contains("[Paged Symbol: Reference page_fn_test_fn]") {
                    found_compaction_file = Some(path);
                    break;
                }
            }
        }
    }

    assert!(found_compaction_file.is_some(), "Compaction summary file containing paged symbol reference not found");

    // Query SurrealDB symbol_archive
    let mut resp = backend.db.query("SELECT * FROM type::record('symbol_archive', 'page_fn_test_fn');").await?;
    let sym_opt: Option<serde_json::Value> = resp.take(0)?;
    assert!(sym_opt.is_some(), "Symbol archive entry for page_fn_test_fn should exist");

    // Retrieve all wiki nodes and restore paged content
    let nodes = backend.get_all_wiki_nodes().await?;
    
    let mut restored_found = false;
    for node in nodes {
        if node.content.contains("[Paged Symbol: Reference page_fn_test_fn]") {
            let restored_content = paging::intercept_and_restore_symbols(&backend, &node.content).await;
            
            // Assert that the restored content contains the original function definition
            assert!(restored_content.contains("pub fn test_fn() {}"), "Restored content should contain the original function");
            
            // Assert that the paged placeholder is removed
            assert!(!restored_content.contains("[Paged Symbol:"), "Restored content should not contain paged placeholders");
            
            restored_found = true;
            break;
        }
    }

    assert!(restored_found, "A wiki node with paged symbol reference was not found or restored correctly");

    Ok(())
}
