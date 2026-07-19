use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::api::ApiState;
use mythrax_core::mcp_routes::manage_handlers::{handle_manage_stm, handle_manage};
use serde_json::json;
use std::sync::Arc;
use tempfile::tempdir;

// We need a test state helper
async fn setup_state() -> ApiState {
    let tmp = tempdir().unwrap();
    let vault_root = tmp.path().join("vault");
    std::fs::create_dir_all(&vault_root).unwrap();
    
    let backend = Arc::new(SurrealBackend::new_in_memory().await.unwrap());
    backend.init().await.unwrap();
    
    let store = Arc::new(mythrax_core::store::MarkdownStore::new(&vault_root).unwrap());
    let ignore_list = Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new());
    
    ApiState {
        backend,
        store,
        ignore_list,
        auth_token: "test".to_string(),
        dream_tx: None,
        shutdown_tx: None,
    }
}

async fn create_handoff_file(state: &ApiState, task_id: &str, content: &str) -> String {
    let dir = state.store.vault_root.join(".handoffs");
    std::fs::create_dir_all(&dir).unwrap();
    let file_path = dir.join(format!("handoff_{}.md", task_id));
    std::fs::write(&file_path, content).unwrap();
    file_path.to_string_lossy().to_string()
}

#[tokio::test]
async fn test_contract_rejects_missing_input() {
    let state = setup_state().await;
    let yaml = r#"---
task_id: "test_1"
title: "Test"
status: "pending"
parent_conversation_id: "parent-1"
inputs:
  - name: "req_input"
    type: "string"
    required: true
---"#;
    let path = create_handoff_file(&state, "test_1", yaml).await;
    
    let args = json!({
        "action": "handoff",
        "parent_conversation_id": "parent-1",
        "subagent_conversation_id": "sub-1",
        "summary": "test",
        "handoff_file_path": path
    });
    
    let res = handle_manage_stm(&state, args).await;
    assert!(res.is_err());
    let err_str = res.unwrap_err().to_string();
    assert!(err_str.contains("Missing required input"));
}

#[tokio::test]
async fn test_contract_accepts_valid_handoff() {
    let state = setup_state().await;
    let yaml = r#"---
task_id: "test_2"
title: "Test"
status: "pending"
parent_conversation_id: "parent-1"
inputs:
  - name: "req_input"
    type: "string"
    required: true
    value: "some_value"
---"#;
    let path = create_handoff_file(&state, "test_2", yaml).await;
    
    let args = json!({
        "action": "handoff",
        "parent_conversation_id": "parent-1",
        "subagent_conversation_id": "sub-1",
        "summary": "test",
        "handoff_file_path": path
    });
    
    let res = handle_manage_stm(&state, args).await;
    assert!(res.is_ok());
    
    // Check DB status remains PENDING
    // Check STM
    let stm = state.backend.get_stm("sub-1", Some("stm_test_2_input_req_input")).await.unwrap();
    assert!(stm.contains_key("stm_test_2_input_req_input"));
}

#[tokio::test]
async fn test_save_handoff_legacy_markdown() {
    let state = setup_state().await;
    let md = r#"# Legacy Handoff
No YAML here."#;
    let path = create_handoff_file(&state, "test_3", md).await;
    
    let args = json!({
        "action": "handoff",
        "parent_conversation_id": "parent-1",
        "subagent_conversation_id": "sub-1",
        "summary": "test",
        "handoff_file_path": path
    });
    
    let res = handle_manage_stm(&state, args).await;
    assert!(res.is_ok()); // Should bypass validation
}

#[tokio::test]
async fn test_save_handoff_malformed_yaml() {
    let state = setup_state().await;
    let md = r#"---
task_id: [broken
---"#;
    let path = create_handoff_file(&state, "test_4", md).await;
    
    let args = json!({
        "action": "handoff",
        "parent_conversation_id": "parent-1",
        "subagent_conversation_id": "sub-1",
        "summary": "test",
        "handoff_file_path": path
    });
    
    let res = handle_manage_stm(&state, args).await;
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("Malformed contract frontmatter"));
}

#[tokio::test]
async fn test_complete_handoff_rejects_missing_output() {
    let state = setup_state().await;
    let yaml = r#"---
task_id: "test_5"
title: "Test"
status: "pending"
parent_conversation_id: "parent-1"
outputs:
  - name: "req_out"
    type: "string"
    required: true
---"#;
    let path = create_handoff_file(&state, "test_5", yaml).await;
    
    let args = json!({
        "action": "complete_handoff",
        "task_id": "test_5"
    });
    
    let res = handle_manage(&state, args).await;
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("missing_output"));
    
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("status: \"failed\""));
}

#[tokio::test]
async fn test_complete_handoff_intentional_failure() {
    let state = setup_state().await;
    let yaml = r#"---
task_id: "test_6"
title: "Test"
status: "pending"
parent_conversation_id: "parent-1"
outputs:
  - name: "req_out"
    type: "string"
    required: true
---"#;
    let path = create_handoff_file(&state, "test_6", yaml).await;
    
    let args = json!({
        "action": "complete_handoff",
        "task_id": "test_6",
        "status": "failed",
        "fail_reason": "I gave up"
    });
    
    let res = handle_manage(&state, args).await;
    assert!(res.is_err());
    
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("status: \"failed\""));
    assert!(content.contains("I gave up"));
}

#[tokio::test]
async fn test_complete_handoff_validates_enum() {
    let state = setup_state().await;
    let yaml = r#"---
task_id: "test_7"
title: "Test"
status: "pending"
parent_conversation_id: "parent-1"
outputs:
  - name: "req_out"
    type: "string"
    required: true
    enum: ["pass", "fail"]
---"#;
    let path = create_handoff_file(&state, "test_7", yaml).await;
    
    let args = json!({
        "action": "complete_handoff",
        "task_id": "test_7",
        "outputs": {
            "req_out": "maybe"
        }
    });
    
    let res = handle_manage(&state, args).await;
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("enum"));
}

#[tokio::test]
async fn test_complete_handoff_task_not_found() {
    let state = setup_state().await;
    let args = json!({
        "action": "complete_handoff",
        "task_id": "nonexistent"
    });
    
    let res = handle_manage(&state, args).await;
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_contract_writes_to_stm() {
    // Covered by test_contract_accepts_valid_handoff, but we can do a specific check if needed
}

#[tokio::test]
async fn test_complete_handoff_writes_outputs_to_stm() {
    let state = setup_state().await;
    let yaml = r#"---
task_id: "test_10"
title: "Test"
status: "pending"
parent_conversation_id: "parent-1"
outputs:
  - name: "req_out"
    type: "string"
    required: true
---"#;
    let path = create_handoff_file(&state, "test_10", yaml).await;
    
    let args = json!({
        "action": "complete_handoff",
        "task_id": "test_10",
        "outputs": {
            "req_out": "done"
        }
    });
    
    let res = handle_manage(&state, args).await;
    assert!(res.is_ok());
    
    let stm = state.backend.get_stm("parent-1", Some("stm_test_10_output_req_out")).await.unwrap();
    assert!(stm.contains_key("stm_test_10_output_req_out"));
}
