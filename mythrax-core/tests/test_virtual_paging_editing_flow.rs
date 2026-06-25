use anyhow::Result;
use tempfile::tempdir;
use std::fs;
use std::sync::Arc;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::cognitive::paging::{page_code_block, intercept_and_restore_symbols};

#[tokio::test]
async fn test_virtual_skeleton_paging_and_editing_flow() -> Result<()> {
    // 1. Initialize Backend and Store
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    let tmp = tempdir()?;
    let file_path = tmp.path().join("my_module.rs");

    // 2. Create a clean, fully-populated source file on disk
    let raw_code = r#"
pub fn run_calculation(x: i32) -> i32 {
    let mut sum = 0;
    for i in 0..x {
        sum += i * 2;
    }
    sum
}

pub fn display_result(val: i32) {
    println!("Calculated value: {}", val);
}
"#;
    fs::write(&file_path, raw_code)?;

    // 3. Generate Virtual Skeleton (Simulates MCP view_file route)
    // We pass the clean content to page_code_block. It archives symbol bodies in SurrealDB
    // and returns a virtual skeleton containing placeholders.
    let virtual_skeleton = page_code_block(&backend, raw_code, "rs").await?;

    // Verify that the skeleton contains placeholders
    assert!(virtual_skeleton.contains("[Paged Symbol: Reference page_fn_run_calculation]"));
    assert!(virtual_skeleton.contains("[Paged Symbol: Reference page_fn_display_result]"));

    // Verify that the physical file on disk is 100% clean and untouched!
    let disk_content = fs::read_to_string(&file_path)?;
    assert_eq!(disk_content, raw_code, "Physical file on disk must remain unmodified and fully populated (Virtual Paging)");

    // 4. Simulate Paging-Aware Editing (Simulates replace_file_content MCP tool)
    // The agent attempts to edit the file. Since the agent only saw the virtual skeleton,
    // the target block specified by the agent contains the placeholder!
    let agent_target = r#"[Paged Symbol: Reference page_fn_run_calculation]"#;
    let agent_replacement = r#"pub fn run_calculation(x: i32) -> i32 { x * 10 }"#;

    // The paging-aware editor resolves the placeholder and reconstructs the clean target in memory:
    // a. Scan for placeholders in agent_target
    // b. Fetch original body of run_calculation from symbol_archive in SurrealDB
    // c. Reconstruct clean target in memory
    // d. Find and replace in the clean disk file
    
    // Let's implement the editor logic in the test to establish the contract:
    let mut clean_target = agent_target.to_string();
    if clean_target.contains("page_fn_run_calculation") {
        // Query symbol_archive for the original body
        let mut response = backend.db.query("SELECT VALUE content FROM type::record('symbol_archive', 'page_fn_run_calculation');")
            .await?;
        let original_body: Option<String> = response.take(0)?;
        assert!(original_body.is_some(), "Original body must be stored in symbol_archive");
        
        let placeholder = "[Paged Symbol: Reference page_fn_run_calculation]";
        clean_target = clean_target.replace(placeholder, &original_body.unwrap());
    }

    // Now, find and replace the reconstructed clean target inside the physical disk content
    let mut updated_disk_content = disk_content.clone();
    assert!(updated_disk_content.contains(&clean_target), "Reconstructed clean target must match the physical disk content");
    updated_disk_content = updated_disk_content.replace(&clean_target, agent_replacement);

    // Write the clean, updated content to disk (physical file remains clean and compiles!)
    fs::write(&file_path, &updated_disk_content)?;

    // Verify the disk file has the new implementation and contains absolutely no placeholders
    let final_disk_content = fs::read_to_string(&file_path)?;
    assert!(final_disk_content.contains("x * 10"));
    assert!(!final_disk_content.contains("[Paged"));

    Ok(())
}
