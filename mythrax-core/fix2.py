import re

with open("src/mcp_routes/manage_handlers.rs", "r") as f:
    content = f.read()

hook_func = """
pub async fn handle_post_invocation_hook(state: &ApiState, args: Value) -> Result<Value> {
    let session_id = args.get("session_id").and_then(|v| v.as_str()).unwrap_or("unknown");
    let workspace_path = args.get("workspace_path").and_then(|v| v.as_str()).unwrap_or("unknown");
    let exit_code = args.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(0);
    let status = args.get("status").and_then(|v| v.as_str()).unwrap_or(if exit_code == 0 { "success" } else { "failed" });
    let summary = args.get("summary").and_then(|v| v.as_str()).unwrap_or("");

    tracing::info!("Post-invocation hook called for session {}, workspace {}, status: {}", session_id, workspace_path, status);

    if status != "success" || exit_code != 0 {
        if let Err(e) = state.backend.save_stm(session_id, "last_error_summary", summary).await {
            tracing::error!("Failed to save post-invocation error summary to STM: {:?}", e);
        }
    } else {
        if let Err(e) = state.backend.save_stm(session_id, "last_success_summary", summary).await {
            tracing::error!("Failed to save post-invocation success summary to STM: {:?}", e);
        }
    }

    Ok(json!({ "content": [{ "type": "text", "text": "Post-invocation hook processed successfully" }] }))
}
"""

if "pub async fn handle_post_invocation_hook" not in content:
    content += hook_func

with open("src/mcp_routes/manage_handlers.rs", "w") as f:
    f.write(content)
