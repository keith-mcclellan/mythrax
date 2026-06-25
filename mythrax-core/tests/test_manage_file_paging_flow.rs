use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use serde_json::json;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::store::MarkdownStore;
use mythrax_core::api::ApiState;
use mythrax_core::mcp_routes::call_mcp_tool;

#[tokio::test]
async fn test_manage_file_paging_flow() -> Result<()> {
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;
    fs::create_dir_all(vault_root.join("episodes"))?;

    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;
    
    let file_path = workspace_root.join("src_lib.rs"); // Use .rs extension to trigger virtual paging

    // Create initial file content
    let initial_content = r#"
pub fn run_calc(x: i32) -> i32 {
    let mut sum = 0;
    for i in 0..x {
        sum += i * 2;
    }
    sum
}

pub fn display_val(val: i32) {
    println!("value: {}", val);
}
"#;
    fs::write(&file_path, initial_content)?;

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.db.query(mythrax_core::db::schema::INIT_SCHEMA).await?.check()?;
    backend.init().await?;

    let store = std::sync::Arc::new(MarkdownStore::new(&vault_root)?);

    let state = ApiState {
        backend: backend.clone(),
        auth_token: "secret".to_string(),
        store,
        ignore_list: std::sync::Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new()),
        dream_tx: None,
    };

    // 1. Test "view" (Virtual Paging)
    let view_res = call_mcp_tool(&state, "manage_file", json!({
        "action": "view",
        "path": file_path.to_str().unwrap()
    })).await?;

    let view_text = view_res.get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.get(0))
        .and_then(|obj| obj.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    assert!(view_text.contains("[Paged Symbol: Reference page_fn_run_calc]"), "Should contain run_calc placeholder");
    assert!(view_text.contains("[Paged Symbol: Reference page_fn_display_val]"), "Should contain display_val placeholder");

    // File on disk must remain untouched
    let disk_content = fs::read_to_string(&file_path)?;
    assert_eq!(disk_content, initial_content);

    // 2. Test "replace" (Paging-Aware Contiguous Edit)
    let replace_res = call_mcp_tool(&state, "manage_file", json!({
        "action": "replace",
        "path": file_path.to_str().unwrap(),
        "target_content": "[Paged Symbol: Reference page_fn_run_calc]",
        "replacement_content": "pub fn run_calc(x: i32) -> i32 { x * 10 }"
    })).await?;

    // File on disk should be updated and contain no placeholders
    let disk_content2 = fs::read_to_string(&file_path)?;
    assert!(disk_content2.contains("pub fn run_calc(x: i32) -> i32 { x * 10 }"));
    assert!(!disk_content2.contains("[Paged Symbol:"));

    // 3. Test "multi_replace" (Multi-Block Edit)
    // First, let's re-view to generate placeholders on the new content
    let view_res2 = call_mcp_tool(&state, "manage_file", json!({
        "action": "view",
        "path": file_path.to_str().unwrap()
    })).await?;

    let view_text2 = view_res2.get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.get(0))
        .and_then(|obj| obj.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    assert!(view_text2.contains("[Paged Symbol: Reference page_fn_run_calc]"));
    assert!(view_text2.contains("[Paged Symbol: Reference page_fn_display_val]"));

    let _multi_replace_res = call_mcp_tool(&state, "manage_file", json!({
        "action": "multi_replace",
        "path": file_path.to_str().unwrap(),
        "chunks": [
            {
                "target_content": "[Paged Symbol: Reference page_fn_run_calc]",
                "replacement_content": "pub fn run_calc(x: i32) -> i32 { x * 100 }"
            },
            {
                "target_content": "[Paged Symbol: Reference page_fn_display_val]",
                "replacement_content": "pub fn display_val(val: i32) { println!(\"final: {}\", val); }"
            }
        ]
    })).await?;

    let disk_content3 = fs::read_to_string(&file_path)?;
    assert!(disk_content3.contains("pub fn run_calc(x: i32) -> i32 { x * 100 }"));
    assert!(disk_content3.contains("pub fn display_val(val: i32) { println!(\"final: {}\", val); }"));
    assert!(!disk_content3.contains("[Paged Symbol:"));

    Ok(())
}
