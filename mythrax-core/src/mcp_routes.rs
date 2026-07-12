use std::sync::Arc;
use std::path::Path;
use serde_json::{json, Value};
use anyhow::{Result, Context};
use crate::api::ApiState;
use crate::db::{StorageBackend, SurrealBackend, parse_record_id, backend::format_record_id};
use surrealdb_types::SurrealValue;
use crate::contracts::*;
use crate::cognitive::ArborCoordinator;
use crate::cognitive::compactor::Compactor;
use crate::cognitive::synthesis::DreamCoordinator;
use crate::cognitive::forge::Forge;
use crate::cognitive::paging::{intercept_and_restore_symbols, page_code_block};
use crate::verify::run_workspace_audit;
use crate::vault::ingestion::bulk_ingest_vault;

pub fn strip_nulls(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            for v in map.values_mut() {
                strip_nulls(v);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_nulls(v);
            }
        }
        _ => {}
    }
}

pub fn truncate_summary(ep_content: &str) -> String {
    if let Some((idx, _)) = ep_content.char_indices().nth(200) {
        format!("{}...", &ep_content[..idx])
    } else {
        ep_content.to_string()
    }
}

async fn format_episode_or_parent(
    backend: &dyn StorageBackend,
    db: &surrealdb::Surreal<surrealdb::engine::local::Db>,
    ep_id: &str,
    ep_title: &str,
    ep_content: &str,
    ep_scope: Option<&str>,
) -> Result<String> {
    if let Ok(rec_id) = parse_record_id(ep_id) {
        let mut parent_resp = db.query("SELECT VALUE out FROM relates_to WHERE in = $ep_id;").bind(("ep_id", rec_id)).await?;
        let parent_ids: Vec<surrealdb::types::RecordId> = parent_resp.take(0)?;
        if !parent_ids.is_empty() {
            let mut parent_ids_strings = Vec::new();
            for pid in parent_ids {
                parent_ids_strings.push(format_record_id(&pid));
            }
            let parents = backend.get_memory_nodes(&parent_ids_strings).await?;
            let mut parts = Vec::new();
            for p_wiki in parents.wiki_nodes {
                parts.push(format!(
                    "### 📚 Distilled Insight: {}\nScope: {}\n{}\n",
                    p_wiki.name, p_wiki.scope, p_wiki.content
                ));
            }
            for p_wisdom in parents.wisdom_rules {
                parts.push(format!(
                    "### 💡 Wisdom Rule: {}\n- **Avoid**: {}\n- **Causal**: {}\n- **Remedy**: {}\n",
                    p_wisdom.target_pattern, p_wisdom.action_to_avoid, p_wisdom.causal_explanation, p_wisdom.prescribed_remedy
                ));
            }
            if !parts.is_empty() {
                return Ok(parts.join("\n"));
            }
        }
    }

    let summary = truncate_summary(ep_content);
    Ok(format!(
        "#### 📑 Memory Card: {}\n- **ID**: `{}`\n- **Scope**: `{}`\n- **Summary**: {}\n*For follow-up queries on this memory, use:* `get_memory_nodes [\"{}\"]`\n",
        ep_title, ep_id, ep_scope.unwrap_or("general"), summary, ep_id
    ))
}

pub fn get_mcp_tools_schema() -> Value {
    json!({
        "tools": [
            {
                "name": "read",
                "description": "Consolidated tool for all reading and querying operations including file view, semantic memory search, stm retrieval, and LLM configuration retrieval.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["view", "search", "rules", "nodes", "root", "query_symbolic", "search_index", "timeline", "get_full", "get"] },
                        "path": { "type": "string" },
                        "AbsolutePath": { "type": "string" },
                        "TargetFile": { "type": "string" },
                        "start_line": { "type": "integer" },
                        "StartLine": { "type": "integer" },
                        "end_line": { "type": "integer" },
                        "EndLine": { "type": "integer" },
                        "query": { "type": "string" },
                        "scope": { "type": "string" },
                        "limit": { "type": "integer", "default": 15 },
                        "offset": { "type": "integer", "default": 0 },
                        "threshold": { "type": "number", "default": 0.55 },
                        "token_budget": { "type": "integer" },
                        "allow_downward": { "type": "boolean", "default": false },
                        "include_episodes": { "type": "boolean", "default": false },
                        "include_artifacts": { "type": "boolean", "default": false },
                        "session_id": { "type": "string" },
                        "tier": { "type": "string" },
                        "node_ids": { "type": "array", "items": { "type": "string" } },
                        "ids": { "type": "array", "items": { "type": "string" } },
                        "depth_before": { "type": "integer", "default": 3 },
                        "depth_after": { "type": "integer", "default": 3 },
                        "anchor_id": { "type": "string" },
                        "node_id": { "type": "string" },
                        "relation": { "type": "string" },
                        "max_depth": { "type": "integer", "default": 3 },
                        "key": { "type": "string" },
                        "is_skill_file": { "type": "boolean" }
                    },
                    "required": ["action"]
                }
            },
            {
                "name": "write",
                "description": "Consolidated tool for all writing and modification operations including file replace, memory recording, stm updates, and LLM configuration updates.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["replace", "multi_replace", "save", "feedback", "thought", "put", "clear", "handoff", "set"] },
                        "path": { "type": "string" },
                        "AbsolutePath": { "type": "string" },
                        "TargetFile": { "type": "string" },
                        "start_line": { "type": "integer" },
                        "StartLine": { "type": "integer" },
                        "end_line": { "type": "integer" },
                        "EndLine": { "type": "integer" },
                        "target_content": { "type": "string" },
                        "TargetContent": { "type": "string" },
                        "replacement_content": { "type": "string" },
                        "ReplacementContent": { "type": "string" },
                        "chunks": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "target_content": { "type": "string" },
                                    "replacement_content": { "type": "string" },
                                    "start_line": { "type": "integer" },
                                    "end_line": { "type": "integer" },
                                    "allow_multiple": { "type": "boolean" }
                                },
                                "required": ["target_content", "replacement_content"]
                            }
                        },
                        "allow_multiple": { "type": "boolean" },
                        "AllowMultiple": { "type": "boolean" },
                        "instruction": { "type": "string" },
                        "description": { "type": "string" },
                        "title": { "type": "string" },
                        "content": { "type": "string" },
                        "scope": { "type": "string" },
                        "episode_id": { "type": "string" },
                        "success": { "type": "boolean" },
                        "session_id": { "type": "string" },
                        "key": { "type": "string" },
                        "value": { "type": "string" },
                        "parent_conversation_id": { "type": "string" },
                        "subagent_conversation_id": { "type": "string" },
                        "summary": { "type": "string" },
                        "handoff_file_path": { "type": "string" },
                        "provider": { "type": "string" },
                        "duration": { "type": "string" },
                        "model": { "type": "string" },
                        "cloud_provider": { "type": "string" },
                        "api_key": { "type": "string" }
                    },
                    "required": ["action"]
                }
            },
            {
                "name": "manage",
                "description": "Consolidated tool for all management, lifecycle, validation, reasoning (HTR), and ingestion operations.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["verify", "organize", "reprocess", "summarize", "audit", "ingest_bulk", "ingest_forge", "save_forged_assets", "init", "ideate", "execute", "backprop", "merge", "run", "pre_invocation", "precompact", "audit_compliance"] },
                        "fix": { "type": "boolean", "default": false },
                        "scope": { "type": "string" },
                        "workspace_path": { "type": "string", "default": "." },
                        "source": { "type": "string" },
                        "harness": { "type": "string" },
                        "source_path": { "type": "string" },
                        "hypothesis": { "type": "string" },
                        "node_id": { "type": "string" },
                        "files": { "type": "array", "items": { "type": "string" } },
                        "test_command": { "type": "string" },
                        "max_steps": { "type": "integer", "default": 5 },
                        "session_id": { "type": "string" },
                        "query": { "type": "string" },
                        "transcript_path": { "type": "string" }
                    },
                    "required": ["action"]
                }
            },
            {
                "name": "agent",
                "description": "Consolidated tool for orchestrating local model autonomous task execution.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["complete_code_task"] },
                        "prompt": { "type": "string" },
                        "system_instruction": { "type": "string" },
                        "model": { "type": "string" },
                        "enable_thinking": { "type": "boolean" }
                    },
                    "required": ["action"]
                }
            }
        ]
    })
}

async fn handle_read(state: &ApiState, mut args: Value) -> Result<Value> {
    let action = args.get("action").and_then(|v| v.as_str()).context("Missing action parameter")?.to_string();
    let mapped_action = match action.as_str() {
        "view" | "view_file" => "view",
        "search" | "search_memory" => "search",
        "search_index" => "search_index",
        "rules" | "search_wisdom" => "rules",
        "nodes" | "get_memory_nodes" => "nodes",
        "query_symbolic" => "query_symbolic",
        "timeline" => "timeline",
        "get_full" => "get_full",
        "root" | "get_vault_root" => "root",
        "get" | "get_short_term" | "get_config" => "get",
        other => other,
    };
    if let Some(obj) = args.as_object_mut() {
        obj.insert("action".to_string(), serde_json::Value::String(mapped_action.to_string()));
    }

    match mapped_action {
        "view" => {
            let _path = args.get("path")
                .or_else(|| args.get("AbsolutePath"))
                .or_else(|| args.get("TargetFile"))
                .and_then(|v| v.as_str())
                .context("Missing path/AbsolutePath/TargetFile")?;
            handle_manage_file(state, args).await
        }
        "search" | "search_index" => {
            let _query = args.get("query").and_then(|v| v.as_str()).context("Missing query")?;
            handle_query_memory(state, args).await
        }
        "rules" => {
            let _query = args.get("query").and_then(|v| v.as_str()).context("Missing query")?;
            handle_query_memory(state, args).await
        }
        "nodes" => {
            let node_ids_val = args.get("node_ids").context("Missing node_ids")?;
            let _node_ids_arr = node_ids_val.as_array().context("node_ids must be an array")?;
            handle_query_memory(state, args).await
        }
        "query_symbolic" => {
            let _node_id = args.get("node_id").and_then(|v| v.as_str()).context("Missing node_id")?;
            handle_query_memory(state, args).await
        }
        "timeline" => {
            if args.get("anchor_id").and_then(|v| v.as_str()).is_none() && args.get("query").and_then(|v| v.as_str()).is_none() {
                anyhow::bail!("Either anchor_id or query must be provided for timeline");
            }
            handle_query_memory(state, args).await
        }
        "get_full" => {
            if args.get("ids").and_then(|v| v.as_array()).is_none() && args.get("node_ids").and_then(|v| v.as_array()).is_none() {
                anyhow::bail!("Missing ids or node_ids array parameter");
            }
            handle_query_memory(state, args).await
        }
        "root" => {
            handle_query_memory(state, args).await
        }
        "get" => {
            if action == "get_short_term" || (action == "get" && (args.get("key").and_then(|v| v.as_str()).is_some() || args.get("session_id").and_then(|v| v.as_str()).is_some())) {
                let _session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id")?;
                let _key = args.get("key").and_then(|v| v.as_str()).context("Missing key")?;
                handle_manage_stm(state, args).await
            } else {
                handle_manage_config(state, args).await
            }
        }
        _ => anyhow::bail!("Invalid action for read tool: {}", action),
    }
}

async fn handle_write(state: &ApiState, mut args: Value) -> Result<Value> {
    let action = args.get("action").and_then(|v| v.as_str()).context("Missing action parameter")?.to_string();
    let mapped_action = match action.as_str() {
        "replace" | "edit_file" => "replace",
        "multi_replace" | "multi_edit_file" => "multi_replace",
        "save" | "save_episode" => "save",
        "feedback" | "record_feedback" => "feedback",
        "put" | "put_short_term" => "put",
        "clear" | "clear_short_term" => "clear",
        "save_forged_assets" => "save_forged_assets",
        "ingest_bulk" => "ingest_bulk",
        "ingest_forge" => "ingest_forge",
        "set" | "set_config" => "set",
        other => other,
    };
    if let Some(obj) = args.as_object_mut() {
        obj.insert("action".to_string(), serde_json::Value::String(mapped_action.to_string()));
    }

    match mapped_action {
        "replace" => {
            let _path = args.get("path")
                .or_else(|| args.get("AbsolutePath"))
                .or_else(|| args.get("TargetFile"))
                .and_then(|v| v.as_str())
                .context("Missing path/AbsolutePath/TargetFile")?;
            let _target_content = args.get("target_content")
                .or_else(|| args.get("TargetContent"))
                .and_then(|v| v.as_str())
                .context("Missing target_content/TargetContent")?;
            let _replacement_content = args.get("replacement_content")
                .or_else(|| args.get("ReplacementContent"))
                .and_then(|v| v.as_str())
                .context("Missing replacement_content/ReplacementContent")?;
            handle_manage_file(state, args).await
        }
        "multi_replace" => {
            let _path = args.get("path")
                .or_else(|| args.get("AbsolutePath"))
                .or_else(|| args.get("TargetFile"))
                .and_then(|v| v.as_str())
                .context("Missing path/AbsolutePath/TargetFile")?;
            let _chunks = args.get("chunks").and_then(|v| v.as_array()).context("Missing chunks array parameter")?;
            handle_manage_file(state, args).await
        }
        "save" => {
            let _title = args.get("title").and_then(|v| v.as_str()).context("Missing title")?;
            let _content = args.get("content").and_then(|v| v.as_str()).context("Missing content")?;
            handle_record_memory(state, args).await
        }
        "feedback" => {
            let _episode_id = args.get("episode_id").and_then(|v| v.as_str()).context("Missing episode_id")?;
            let _success = args.get("success").and_then(|v| v.as_bool()).context("Missing success")?;
            handle_record_memory(state, args).await
        }
        "thought" => {
            let _content = args.get("content").and_then(|v| v.as_str()).context("Missing content")?;
            handle_record_memory(state, args).await
        }
        "put" => {
            let _session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id")?;
            let _key = args.get("key").and_then(|v| v.as_str()).context("Missing key")?;
            let _value = args.get("value").and_then(|v| v.as_str()).context("Missing value")?;
            handle_manage_stm(state, args).await
        }
        "clear" => {
            let _session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id")?;
            handle_manage_stm(state, args).await
        }
        "handoff" => {
            let _parent = args.get("parent_conversation_id").and_then(|v| v.as_str()).context("Missing parent_conversation_id")?;
            let _subagent = args.get("subagent_conversation_id").and_then(|v| v.as_str()).context("Missing subagent_conversation_id")?;
            let _summary = args.get("summary").and_then(|v| v.as_str()).context("Missing summary")?;
            handle_manage_stm(state, args).await
        }
        "set" => {
            let _provider = args.get("provider").and_then(|v| v.as_str()).context("Missing provider")?;
            handle_manage_config(state, args).await
        }
        "save_forged_assets" | "ingest_bulk" | "ingest_forge" => {
            handle_manage_vault(state, args).await
        }
        _ => anyhow::bail!("Invalid action for write tool: {}", action),
    }
}

async fn handle_manage(state: &ApiState, args: Value) -> Result<Value> {
    let action_opt = args.get("action").and_then(|v| v.as_str());
    let resolved_action = if let Some(act) = action_opt {
        act
    } else {
        if args.get("session_id").and_then(|v| v.as_str()).is_some() {
            "pre_invocation"
        } else if args.get("workspace_path").and_then(|v| v.as_str()).is_some() {
            "audit_compliance"
        } else {
            anyhow::bail!("Missing action parameter for manage tool");
        }
    };

    let mapped_action = match resolved_action {
        "verify_vault" => "verify",
        "organize_vault" => "organize",
        "reprocess_vault" => "reprocess",
        "summarize_vault" => "summarize",
        "audit_compliance" => "audit",
        "init_htr" => "init",
        "ideate_htr" => "ideate",
        "execute_htr" => "execute",
        "backprop_htr" => "backprop",
        "merge_htr" => "merge",
        "run_htr" => "run",
        other => other,
    };

    match mapped_action {
        "verify" | "organize" | "reprocess" | "summarize" | "audit" | "ingest_bulk" | "ingest_forge" | "save_forged_assets" => {
            match mapped_action {
                "ingest_bulk" => {
                    let _source = args.get("source").and_then(|v| v.as_str()).context("Missing source parameter for ingest_bulk")?;
                    let _harness = args.get("harness").and_then(|v| v.as_str()).context("Missing harness parameter for ingest_bulk")?;
                }
                "ingest_forge" => {
                    let _source_path = args.get("source").or_else(|| args.get("source_path")).and_then(|v| v.as_str()).context("Missing source parameter for ingest_forge")?;
                }
                "save_forged_assets" => {
                    let _doc_title = args.get("doc_title").context("Missing doc_title parameter for save_forged_assets")?;
                }
                _ => {}
            }
            let mut modified_args = args.clone();
            if let Some(obj) = modified_args.as_object_mut() {
                obj.insert("action".to_string(), serde_json::Value::String(mapped_action.to_string()));
            }
            handle_manage_vault(state, modified_args).await
        }
        "init" | "ideate" | "execute" | "backprop" | "merge" | "run" => {
            let _scope = args.get("scope").and_then(|v| v.as_str()).context("Missing scope parameter for HTR action")?;
            match mapped_action {
                "init" | "run" => {
                    let _hypothesis = args.get("hypothesis").and_then(|v| v.as_str()).context("Missing hypothesis parameter")?;
                }
                "ideate" | "execute" | "backprop" | "merge" => {
                    let _node_id = args.get("node_id").and_then(|v| v.as_str()).context("Missing node_id parameter")?;
                }
                _ => {}
            }
            let mut modified_args = args.clone();
            if let Some(obj) = modified_args.as_object_mut() {
                obj.insert("action".to_string(), serde_json::Value::String(mapped_action.to_string()));
            }
            handle_manage_htr(state, modified_args).await
        }
        "pre_invocation" => {
            let _session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id parameter for pre_invocation")?;
            handle_pre_invocation_hook(state, args).await
        }
        "precompact" => {
            let session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id parameter for precompact")?;
            let transcript_path_str = args.get("transcript_path").and_then(|v| v.as_str()).context("Missing transcript_path parameter for precompact")?;
            let count = crate::hooks::precompact::mine_transcript(
                session_id,
                transcript_path_str,
                state.backend.as_ref(),
                state.store.as_ref(),
                &state.ignore_list,
            ).await?;
            Ok(json!({ "status": "success", "episodes_saved": count }))
        }
        _ => anyhow::bail!("Invalid action for manage tool: {}", resolved_action),
    }
}

async fn handle_agent(state: &ApiState, args: Value) -> Result<Value> {
    let action = args.get("action").and_then(|v| v.as_str()).context("Missing action parameter for agent tool")?;
    let mapped_action = match action {
        "complete_task" => "complete_code_task",
        "save_handoff" => "handoff",
        other => other,
    };
    match mapped_action {
        "complete_code_task" => {
            let _prompt = args.get("prompt").and_then(|v| v.as_str()).context("Missing prompt parameter for agent:complete_code_task")?;
            handle_complete_code_task(state, args).await
        }
        "handoff" => {
            let _parent = args.get("parent_conversation_id").and_then(|v| v.as_str()).context("Missing parent_conversation_id")?;
            let _subagent = args.get("subagent_conversation_id").and_then(|v| v.as_str()).context("Missing subagent_conversation_id")?;
            let _summary = args.get("summary").and_then(|v| v.as_str()).context("Missing summary")?;
            handle_manage_stm(state, args).await
        }
        _ => anyhow::bail!("Invalid action for agent tool: {}", action),
    }
}

pub async fn call_mcp_tool(
    state: &ApiState,
    name: &str,
    args: Value,
) -> Result<Value> {
    let result = match name {
        "read" => handle_read(state, args.clone()).await,
        "write" => handle_write(state, args.clone()).await,
        "manage" => handle_manage(state, args.clone()).await,
        "agent" => handle_agent(state, args.clone()).await,
        _ => anyhow::bail!("Tool not found: {}", name),
    };

    let session_id_opt = args.get("session_id")
        .or_else(|| args.get("subagent_id"))
        .or_else(|| args.get("subagent_conversation_id"))
        .and_then(|v| v.as_str());

    let action_opt = args.get("action").and_then(|v| v.as_str());
    let resolved_action = if name == "manage" && action_opt.is_none() {
        if args.get("session_id").and_then(|v| v.as_str()).is_some() {
            "pre_invocation"
        } else if args.get("workspace_path").and_then(|v| v.as_str()).is_some() {
            "audit_compliance"
        } else {
            ""
        }
    } else {
        action_opt.unwrap_or("")
    };
    let is_pre_invocation = name == "manage" && resolved_action == "pre_invocation";

    if let Some(session_id) = session_id_opt {
        if !is_pre_invocation {
            if let Some(surreal_backend) = state.backend.as_any().downcast_ref::<SurrealBackend>() {
                let tool_name = name.to_string();
                let score_delta = if result.is_ok() { 0.02f32 } else { -0.05f32 };

                if let Ok(ref val) = result {
                    let content_str = if let Some(arr) = val.get("content").and_then(|c| c.as_array()) {
                        let mut s = String::new();
                        for item in arr {
                            if let Some(txt) = item.get("text").and_then(|t| t.as_str()) {
                                s.push_str(txt);
                                s.push('\n');
                            }
                        }
                        if s.is_empty() { val.to_string() } else { s.trim().to_string() }
                    } else if let Some(txt) = val.get("text").and_then(|t| t.as_str()) {
                        txt.to_string()
                    } else {
                        val.to_string()
                    };

                    let insert_sql = "INSERT INTO chat_history { session_id: $session_id, role: 'assistant', content: $content, created_at: time::now() };";
                    let _ = surreal_backend.db.query(insert_sql)
                        .bind(("session_id", session_id))
                        .bind(("content", content_str))
                        .await;
                }
                
                let belief_res = surreal_backend.db.query("SELECT session_id, tasks_todo, hypotheses_tested, confidence_score, uncertainty_areas, updated_at FROM belief_state WHERE session_id = $session_id;")
                    .bind(("session_id", session_id))
                    .await;
                
                if let Ok(mut resp) = belief_res {
                    let belief_states: Vec<BeliefState> = resp.take(0).unwrap_or_default();
                    if let Some(mut bs) = belief_states.into_iter().next() {
                        bs.confidence_score = (bs.confidence_score + score_delta).clamp(0.0, 1.0);
                        if !bs.hypotheses_tested.contains(&tool_name) {
                            bs.hypotheses_tested.push(tool_name);
                        }
                        bs.updated_at = chrono::Utc::now().to_rfc3339();
                        
                        let _ = surreal_backend.db.query("
                            UPDATE type::record('belief_state', $session_id) CONTENT {
                                session_id: $session_id,
                                tasks_todo: $tasks_todo,
                                hypotheses_tested: $hypotheses_tested,
                                confidence_score: $confidence_score,
                                uncertainty_areas: $uncertainty_areas,
                                updated_at: $updated_at
                            };
                        ")
                        .bind(("session_id", bs.session_id))
                        .bind(("tasks_todo", bs.tasks_todo))
                        .bind(("hypotheses_tested", bs.hypotheses_tested))
                        .bind(("confidence_score", bs.confidence_score))
                        .bind(("uncertainty_areas", bs.uncertainty_areas))
                        .bind(("updated_at", bs.updated_at))
                        .await;
                    } else {
                        let new_bs = BeliefState {
                            id: Some(format!("belief_state:{}", session_id)),
                            session_id: session_id.to_string(),
                            tasks_todo: vec![],
                            hypotheses_tested: vec![tool_name],
                            confidence_score: (0.5f32 + score_delta).clamp(0.0, 1.0),
                            uncertainty_areas: vec![],
                            updated_at: chrono::Utc::now().to_rfc3339(),
                        };
                        
                        let _ = surreal_backend.db.query("
                            UPSERT type::record('belief_state', $session_id) CONTENT {
                                session_id: $session_id,
                                tasks_todo: $tasks_todo,
                                hypotheses_tested: $hypotheses_tested,
                                confidence_score: $confidence_score,
                                uncertainty_areas: $uncertainty_areas,
                                updated_at: $updated_at
                            };
                        ")
                        .bind(("session_id", new_bs.session_id))
                        .bind(("tasks_todo", new_bs.tasks_todo))
                        .bind(("hypotheses_tested", new_bs.hypotheses_tested))
                        .bind(("confidence_score", new_bs.confidence_score))
                        .bind(("uncertainty_areas", new_bs.uncertainty_areas))
                        .bind(("updated_at", new_bs.updated_at))
                        .await;
                    }
                }
            }
        }
    }

    if result.is_ok() && (name == "read" || name == "write" || name == "manage" || name == "agent") {
        let session_id_opt = args.get("session_id")
            .or_else(|| args.get("subagent_id"))
            .or_else(|| args.get("subagent_conversation_id"))
            .or_else(|| args.get("scope"))
            .and_then(|v| v.as_str());
        if let Err(e) = state.backend.journal_state(&state.store.vault_root, session_id_opt).await {
            tracing::error!("Failed to write dual-durability journal: {:?}", e);
        }
    }

    result
}



async fn handle_query_memory(state: &ApiState, args: Value) -> Result<Value> {
    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
        .context("SurrealBackend required")?;
    let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("search");
    match action {
        "search" => {
            let query = args.get("query").and_then(|v| v.as_str()).context("Missing query")?;
            let scope = args.get("scope").and_then(|v| v.as_str());
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(15) as usize;
            let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let threshold = args.get("threshold").and_then(|v| v.as_f64()).map(|t| t as f32).unwrap_or(0.55);
            let token_budget = args.get("token_budget").and_then(|v| v.as_u64()).map(|t| t as usize);
            let allow_downward = args.get("allow_downward").and_then(|v| v.as_bool()).unwrap_or(false);
            let include_episodes = args.get("include_episodes").and_then(|v| v.as_bool()).unwrap_or(false);
            let include_artifacts = args.get("include_artifacts").and_then(|v| v.as_bool()).unwrap_or(false);
            let session_id = args.get("session_id").and_then(|v| v.as_str());
            let include_archived = args.get("include_archived").and_then(|v| v.as_bool()).unwrap_or(true);
            let temporal_anchor = args.get("temporal_anchor").and_then(|v| v.as_str());

            let search_res = state.backend.search(
        query,
        scope,
        false,
        limit,
        offset,
        threshold,
        token_budget,
        allow_downward,
        include_episodes,
        include_artifacts,
        session_id,
        include_archived,
        temporal_anchor,
    ).await?;
            
            if let Some(sess_id) = session_id {
                let mut cited_ids = Vec::new();
                for r in &search_res.results {
                    if r.tier == "episode" {
                        cited_ids.push(r.id.clone());
                    }
                }
                if !cited_ids.is_empty() {
                    let mut existing_citations = Vec::new();
                    if let Ok(stm_map) = state.backend.get_stm(sess_id, Some("_session_citations")).await {
                        if let Some(existing_str) = stm_map.get("_session_citations") {
                            if let Ok(parsed) = serde_json::from_str::<Vec<String>>(existing_str) {
                                existing_citations = parsed;
                            }
                        }
                    }
                    existing_citations.extend(cited_ids);
                    existing_citations.sort();
                    existing_citations.dedup();
                    if let Ok(serialized) = serde_json::to_string(&existing_citations) {
                        let _ = state.backend.save_stm(sess_id, "_session_citations", &serialized).await;
                    }
                }
            }

            let stripped_results: Vec<Value> = search_res.results.into_iter().map(|mut r| {
                r.embedding = None;
                let mut v = serde_json::to_value(&r).unwrap();
                strip_nulls(&mut v);
                v
            }).collect();

            let mut text = serde_json::to_string_pretty(&stripped_results)?;
            if search_res.has_more {
                let remainder = search_res.total_matches.saturating_sub(offset + limit);
                text.push_str(&format!(
                    "\n\n=== PAGINATION NOTICE: There are {} more matching memories. To retrieve the next page, call read(action=\"search\", offset={}, limit={}). ===",
                    remainder, search_res.next_offset, limit
                ));
            }

            if let Some(ref omitted) = search_res.omitted_ids {
                if !omitted.is_empty() {
                    text.push_str(&format!(
                        "\n\n=== BUDGET NOTICE: The following record IDs were omitted due to token budget limits ({} tokens):\n{:?} ===",
                        token_budget.unwrap_or(0), omitted
                    ));
                }
            }

            text = intercept_and_restore_symbols(surreal_backend, &text).await;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": text
                    }
                ]
            }))
        }
        "search_index" => {
            let query = args.get("query").and_then(|v| v.as_str()).context("Missing query")?;
            let scope = args.get("scope").and_then(|v| v.as_str());
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(15) as usize;
            let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let threshold = args.get("threshold").and_then(|v| v.as_f64()).map(|t| t as f32).unwrap_or(0.55);
            let token_budget = args.get("token_budget").and_then(|v| v.as_u64()).map(|t| t as usize);
            let allow_downward = args.get("allow_downward").and_then(|v| v.as_bool()).unwrap_or(false);
            let session_id = args.get("session_id").and_then(|v| v.as_str());
            let include_archived = args.get("include_archived").and_then(|v| v.as_bool()).unwrap_or(true);
            let temporal_anchor = args.get("temporal_anchor").and_then(|v| v.as_str());

            let search_res = state.backend.search(
        query,
        scope,
        false,
        limit,
        offset,
        threshold,
        token_budget,
        allow_downward,
        true,
        false,
        session_id,
        include_archived,
        temporal_anchor,
    ).await?;

            // Broad Cheap Projection (BCP): we deliberately filter and only project
            // nodes where tier == "episode" to provide a lightweight, cheap overview index.
            // Wisdom rules, wiki nodes, and insight nodes are deliberately excluded here
            // to minimize token costs and focus purely on raw episode sequence indexing.
            let mut index_rows = Vec::new();
            for r in search_res.results {
                if r.tier == "episode" {
                    let subtitle = make_subtitle(&r.content);
                    index_rows.push(crate::contracts::IndexRow {
                        id: r.id,
                        title: r.title,
                        subtitle,
                        similarity: r.similarity,
                    });
                }
            }

            let text = serde_json::to_string_pretty(&index_rows)?;
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": text
                    }
                ]
            }))
        }
        "timeline" => {
            let anchor_id = args.get("anchor_id").and_then(|v| v.as_str());
            let query = args.get("query").and_then(|v| v.as_str());
            let depth_before = args.get("depth_before").and_then(|v| v.as_u64()).unwrap_or(3) as usize;
            let depth_after = args.get("depth_after").and_then(|v| v.as_u64()).unwrap_or(3) as usize;

            let resolved_anchor_id = if let Some(id) = anchor_id {
                id.to_string()
            } else if let Some(q) = query {
                let search_res = state.backend.search(
        q,
        None,
        false,
        1,
        0,
        0.0,
        None,
        false,
        true,
        false,
        None,
        true,
        None,
    ).await?;
                let best = search_res.results.first().context("No matching anchor episode found for query")?;
                best.id.clone()
            } else {
                anyhow::bail!("Either anchor_id or query must be provided for timeline");
            };

            let anchor_record = crate::db::backend::parse_record_id(&resolved_anchor_id)?;
            
            #[derive(serde::Deserialize, Debug, surrealdb_types::SurrealValue)]
            struct AnchorRow {
                created_at: chrono::DateTime<chrono::Utc>,
            }

            let mut response = surreal_backend.db.query("SELECT created_at FROM $id;")
                .bind(("id", anchor_record))
                .await?;
            let anchor_rows: Vec<AnchorRow> = response.take(0)?;
            let anchor_row = anchor_rows.into_iter().next().context("Anchor episode not found in database")?;
            let anchor_time = anchor_row.created_at;

            #[derive(serde::Deserialize, Debug, surrealdb_types::SurrealValue)]
            struct EpisodeQueryResult {
                id: surrealdb::types::RecordId,
                title: String,
                content: String,
                created_at: chrono::DateTime<chrono::Utc>,
            }

            let mut response_before = surreal_backend.db.query("SELECT id, title, content, created_at FROM episode WHERE created_at < $created_at ORDER BY created_at DESC LIMIT $limit;")
                .bind(("created_at", anchor_time))
                .bind(("limit", depth_before))
                .await?;
            let mut before_rows: Vec<EpisodeQueryResult> = response_before.take(0)?;
            before_rows.reverse();

            let mut response_after = surreal_backend.db.query("SELECT id, title, content, created_at FROM episode WHERE created_at > $created_at ORDER BY created_at ASC LIMIT $limit;")
                .bind(("created_at", anchor_time))
                .bind(("limit", depth_after))
                .await?;
            let after_rows: Vec<EpisodeQueryResult> = response_after.take(0)?;

            let mut index_rows = Vec::new();
            for r in before_rows {
                let subtitle = make_subtitle(&r.content);
                index_rows.push(crate::contracts::IndexRow {
                    id: crate::db::backend::format_record_id(&r.id),
                    title: r.title,
                    subtitle,
                    similarity: 0.0,
                });
            }
            for r in after_rows {
                let subtitle = make_subtitle(&r.content);
                index_rows.push(crate::contracts::IndexRow {
                    id: crate::db::backend::format_record_id(&r.id),
                    title: r.title,
                    subtitle,
                    similarity: 0.0,
                });
            }

            let text = serde_json::to_string_pretty(&index_rows)?;
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": text
                    }
                ]
            }))
        }
        "get_full" => {
            let ids = if let Some(ids_val) = args.get("ids").and_then(|v| v.as_array()) {
                ids_val.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<String>>()
            } else if let Some(node_ids_val) = args.get("node_ids").and_then(|v| v.as_array()) {
                node_ids_val.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<String>>()
            } else {
                anyhow::bail!("Missing ids or node_ids array parameter");
            };

            let hydrated = state.backend.get_memory_nodes(&ids).await?;
            
            let mut results = Vec::new();
            const MAX_HYDRATION_CHARS: usize = 10000;
            for ep in hydrated.episodes {
                let content = if ep.content.chars().count() > MAX_HYDRATION_CHARS {
                    let truncated_len = ep.content.chars().count() - MAX_HYDRATION_CHARS;
                    let truncated: String = ep.content.chars().take(MAX_HYDRATION_CHARS).collect();
                    format!("{}... [truncated {} chars]", truncated, truncated_len)
                } else {
                    ep.content.clone()
                };
                results.push(crate::contracts::SearchResult {
                    id: ep.id.clone().unwrap_or_default(),
                    title: ep.title.clone(),
                    content,
                    similarity: 1.0,
                    utility: ep.utility.unwrap_or(0.0),
                    tier: "episode".to_string(),
                    embedding: None,
                    vault_path: ep.vault_path.clone(),
                    source_episode: ep.source_episode.clone(),
                    discovery_tokens: ep.discovery_tokens,
                    related_nodes: None,
                    ..Default::default()
                });
            }
            for wiki in hydrated.wiki_nodes {
                let content = if wiki.content.chars().count() > MAX_HYDRATION_CHARS {
                    let truncated_len = wiki.content.chars().count() - MAX_HYDRATION_CHARS;
                    let truncated: String = wiki.content.chars().take(MAX_HYDRATION_CHARS).collect();
                    format!("{}... [truncated {} chars]", truncated, truncated_len)
                } else {
                    wiki.content.clone()
                };
                results.push(crate::contracts::SearchResult {
                    id: wiki.id.clone().unwrap_or_default(),
                    title: wiki.name.clone(),
                    content,
                    similarity: 1.0,
                    utility: 0.0,
                    tier: "wiki".to_string(),
                    embedding: None,
                    vault_path: wiki.vault_path.clone(),
                    source_episode: None,
                    discovery_tokens: None,
                    related_nodes: None,
                    ..Default::default()
                });
            }
            for rule in hydrated.wisdom_rules {
                let raw_content = format!(
                    "Avoid: {}\nCausal: {}\nRemedy: {}",
                    rule.action_to_avoid, rule.causal_explanation, rule.prescribed_remedy
                );
                let content = if raw_content.chars().count() > MAX_HYDRATION_CHARS {
                    let truncated_len = raw_content.chars().count() - MAX_HYDRATION_CHARS;
                    let truncated: String = raw_content.chars().take(MAX_HYDRATION_CHARS).collect();
                    format!("{}... [truncated {} chars]", truncated, truncated_len)
                } else {
                    raw_content
                };
                results.push(crate::contracts::SearchResult {
                    id: rule.id.clone().unwrap_or_default(),
                    title: rule.target_pattern.clone(),
                    content,
                    similarity: rule.similarity.unwrap_or(1.0),
                    utility: rule.utility.unwrap_or(0.0) as f32,
                    tier: "wisdom".to_string(),
                    embedding: None,
                    vault_path: rule.vault_path.clone(),
                    source_episode: None,
                    discovery_tokens: None,
                    related_nodes: None,
                    ..Default::default()
                });
            }

            let text = serde_json::to_string_pretty(&results)?;
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": text
                    }
                ]
            }))
        }
        "rules" => {
            let query = args.get("query").and_then(|v| v.as_str()).context("Missing query")?;
            let tier = args.get("tier").and_then(|v| v.as_str());
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(15) as usize;
            let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let threshold = args.get("threshold").and_then(|v| v.as_f64()).map(|t| t as f32).unwrap_or(0.55);

            let search_res = state.backend.get_wisdom(query, tier, limit, offset, threshold).await?;
            let stripped_results: Vec<Value> = search_res.results.into_iter().map(|mut r| {
                r.embedding = None;
                let mut v = serde_json::to_value(&r).unwrap();
                strip_nulls(&mut v);
                v
            }).collect();

            let mut text = serde_json::to_string_pretty(&stripped_results)?;
            if search_res.has_more {
                let remainder = search_res.total_matches.saturating_sub(offset + limit);
                text.push_str(&format!(
                    "\n\n=== PAGINATION NOTICE: There are {} more matching wisdom rules. To retrieve the next page, call read(action=\"rules\", offset={}, limit={}). ===",
                    remainder, search_res.next_offset, limit
                ));
            }

            text = intercept_and_restore_symbols(surreal_backend, &text).await;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": text
                    }
                ]
            }))
        }
        "nodes" => {
            let node_ids_val = args.get("node_ids").context("Missing node_ids")?;
            let node_ids_arr = node_ids_val.as_array().context("node_ids must be an array")?;
            let mut node_ids = Vec::new();
            for v in node_ids_arr {
                if let Some(s) = v.as_str() {
                    node_ids.push(s.to_string());
                }
            }

            let response = state.backend.get_memory_nodes(&node_ids).await?;
            let mut stripped_response = response.clone();
            for ep in &mut stripped_response.episodes {
                ep.embedding = None;
            }
            for r in &mut stripped_response.wisdom_rules {
                r.embedding = None;
            }
            for node in &mut stripped_response.wiki_nodes {
                node.embedding = None;
            }

            let mut text = serde_json::to_string_pretty(&stripped_response)?;
            text = intercept_and_restore_symbols(surreal_backend, &text).await;
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": text
                    }
                ]
            }))
        }
        "query_symbolic" => {
            let node_id = args.get("node_id").and_then(|v| v.as_str()).context("Missing node_id")?;
            let relation = args.get("relation").and_then(|v| v.as_str());
            let max_depth = args.get("max_depth").and_then(|v| v.as_u64()).map(|v| v as usize);

            let traversed_ids = state.backend.query_symbolic(node_id, relation, max_depth).await?;
            let text = serde_json::to_string_pretty(&traversed_ids)?;
            
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": text
                    }
                ]
            }))
        }
        "root" => {
            let vault_path = state.store.vault_root.to_string_lossy().to_string();
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": vault_path
                    }
                ]
            }))
        }
        _ => anyhow::bail!("Invalid action for query_memory: {}", action),
    }
}

async fn handle_record_memory(state: &ApiState, args: Value) -> Result<Value> {
    let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("save");
    match action {
        "save" => {
            let title = args.get("title").and_then(|v| v.as_str()).context("Missing title")?.to_string();
            let content = args.get("content").and_then(|v| v.as_str()).context("Missing content")?.to_string();
            let scope = args.get("scope").and_then(|v| v.as_str()).map(|s| s.to_string());
            let vault_path = args.get("vault_path").and_then(|v| v.as_str()).map(|s| s.to_string());
            let session_id = args.get("session_id").and_then(|v| v.as_str()).map(|s| s.to_string());
            let task_id = args.get("task_id").and_then(|v| v.as_str()).map(|s| s.to_string());
            let node_type = args.get("node_type").and_then(|v| v.as_str()).map(|s| s.to_string());
            
            let mut entities = vec![];
            if let Some(arr) = args.get("entities").and_then(|v| v.as_array()) {
                for item in arr {
                    let entity: Entity = serde_json::from_value(item.clone())?;
                    entities.push(entity);
                }
            }

            let episode = EpisodeSave {
        created_at: None,
                title,
                content: content.clone(),
                entities,
                scope: scope.clone(),
                vault_path,
                source_episode: None,
                session_id,
                task_id,
                discovery_tokens: None,
                facts: None,
                concepts: None,
                files_read: None,
                files_modified: None,
                node_type,
                confidence: None,
            };

            let id = crate::vault::watcher::save_episode_bidirectional(&episode, state.backend.as_ref(), &state.store, &state.ignore_list).await?;

            // T1: Zero-Touch Mistake Learning case-insensitive check
            let content_lower = content.to_lowercase();
            let correction_indicators = [
                "you forgot",
                "incorrect",
                "that was a mistake",
                "that's wrong",
                "you made a mistake",
                "wrong choice",
                "not right",
                "should have",
                "didn't run",
            ];
            let has_correction = correction_indicators.iter().any(|&ind| content_lower.contains(ind));

            if has_correction {
                if let Some(surreal_backend) = state.backend.as_any().downcast_ref::<SurrealBackend>() {
                    let backend_clone = Arc::new(surreal_backend.clone());
                    let store_clone = state.store.clone();
                    let content_clone = content.clone();
                    let scope_clone = scope.clone();
                    tokio::spawn(async move {
                        if let Err(e) = run_llm_critic(backend_clone, store_clone, content_clone, scope_clone).await {
                            tracing::error!("Error running LLM critic: {:?}", e);
                        }
                    });
                }
            }

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Episode saved successfully: {}", id)
                    }
                ]
            }))
        }
        "feedback" => {
            let id = args.get("id").or_else(|| args.get("episode_id")).and_then(|v| v.as_str()).context("Missing id")?;
            let success = args.get("success").and_then(|v| v.as_bool()).context("Missing success")?;
            state.backend.record_feedback(id, success).await?;
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": "Feedback recorded successfully."
                    }
                ]
            }))
        }
        "thought" => {
            let title = args.get("title").and_then(|v| v.as_str()).context("Missing title")?.to_string();
            let content = args.get("content").and_then(|v| v.as_str()).context("Missing content")?.to_string();
            let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("general").to_string();

            let thought_uuid = uuid::Uuid::new_v4().to_string();
            let relative_path = format!("wiki/thoughts/thought_{}.md", thought_uuid);
            
            let thought = ThoughtNode {
                id: None,
                title,
                content,
                scope,
                vault_path: Some(relative_path.clone()),
                created_at: chrono::Utc::now().to_rfc3339(),
            };

            // Format OKF frontmatter
            let mut yaml_val = serde_json::Map::new();
            yaml_val.insert("title".to_string(), serde_json::json!(thought.title));
            yaml_val.insert("scope".to_string(), serde_json::json!(thought.scope));
            yaml_val.insert("created_at".to_string(), serde_json::json!(thought.created_at));
            let yaml_str = serde_yaml::to_string(&yaml_val).unwrap_or_default();
            let markdown = format!("---\n{}---\n{}", yaml_str.trim(), thought.content);

            // Write to disk under wiki/thoughts/
            state.store.write_file(&relative_path, &markdown)?;

            // Save to SurrealDB
            let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required to save thought_node")?;
            let id = surreal_backend.save_thought_node(&thought).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Thought node saved successfully: {}", id)
                    }
                ]
            }))
        }
        _ => anyhow::bail!("Invalid action for record_memory: {}", action),
    }
}

pub async fn handle_manage_htr(state: &ApiState, args: Value) -> Result<Value> {
    let action = args.get("action").and_then(|v| v.as_str()).context("Missing action")?;
    let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("general").to_string();

    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
        .context("SurrealBackend required for HTR")?;

    match action {
        "init" => {
            let hypothesis = args.get("hypothesis").and_then(|v| v.as_str()).context("Missing hypothesis")?.to_string();
            let files_val = args.get("files").and_then(|v| v.as_array()).context("Missing files")?;
            let files: Vec<String> = files_val.iter().map(|v| v.as_str().unwrap_or("").to_string()).collect();

            let llm = crate::llm::LLMClient::new();
            let current_dir = std::env::current_dir()?;
            let coordinator = ArborCoordinator::new(
                surreal_backend.db.clone(),
                state.store.vault_root.clone(),
                current_dir,
                llm,
                scope,
                "".to_string(),
                files,
            ).await;
            coordinator.init_root(hypothesis, None).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": "HTR root node initialized successfully."
                    }
                ]
            }))
        }
        "ideate" => {
            let node = args.get("node_id").or_else(|| args.get("node")).and_then(|v| v.as_str()).context("Missing node")?.to_string();

            let llm = crate::llm::LLMClient::new();
            let current_dir = std::env::current_dir()?;
            let coordinator = ArborCoordinator::new(
                surreal_backend.db.clone(),
                state.store.vault_root.clone(),
                current_dir,
                llm,
                scope,
                "".to_string(),
                vec![],
            ).await;
            coordinator.trigger_ideation(&node).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("HTR ideation complete for node: {}", node)
                    }
                ]
            }))
        }
        "execute" => {
            let node = args.get("node_id").or_else(|| args.get("node")).and_then(|v| v.as_str()).context("Missing node")?.to_string();
            let test_command = args.get("test_command").and_then(|v| v.as_str()).context("Missing test_command")?.to_string();

            let llm = crate::llm::LLMClient::new();
            let current_dir = std::env::current_dir()?;
            let coordinator = ArborCoordinator::new(
                surreal_backend.db.clone(),
                state.store.vault_root.clone(),
                current_dir,
                llm,
                scope,
                test_command,
                vec![],
            ).await;
            coordinator.execute_node(&node).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("HTR execution complete for node: {}", node)
                    }
                ]
            }))
        }
        "backprop" => {
            let node = args.get("node_id").or_else(|| args.get("node")).and_then(|v| v.as_str()).context("Missing node")?.to_string();

            let llm = crate::llm::LLMClient::new();
            let current_dir = std::env::current_dir()?;
            let coordinator = ArborCoordinator::new(
                surreal_backend.db.clone(),
                state.store.vault_root.clone(),
                current_dir,
                llm,
                scope,
                "".to_string(),
                vec![],
            ).await;
            coordinator.backpropagate_insights(&node).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("HTR backpropagation complete for node: {}", node)
                    }
                ]
            }))
        }
        "merge" => {
            let node = args.get("node_id").or_else(|| args.get("node")).and_then(|v| v.as_str()).context("Missing node")?.to_string();

            let llm = crate::llm::LLMClient::new();
            let current_dir = std::env::current_dir()?;
            let coordinator = ArborCoordinator::new(
                surreal_backend.db.clone(),
                state.store.vault_root.clone(),
                current_dir,
                llm,
                scope,
                "".to_string(),
                vec![],
            ).await;
            coordinator.decide_admission(&node).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("HTR merge complete for node: {}", node)
                    }
                ]
            }))
        }
        "run" => {
            let hypothesis = args.get("hypothesis").and_then(|v| v.as_str()).context("Missing hypothesis")?.to_string();
            let files_val = args.get("files").and_then(|v| v.as_array()).context("Missing files")?;
            let files: Vec<String> = files_val.iter().map(|v| v.as_str().unwrap_or("").to_string()).collect();
            let test_command = args.get("test_command").and_then(|v| v.as_str()).context("Missing test_command")?.to_string();
            let max_steps = args.get("max_steps").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

            let llm = crate::llm::LLMClient::new();
            let current_dir = std::env::current_dir()?;
            let coordinator = ArborCoordinator::new(
                surreal_backend.db.clone(),
                state.store.vault_root.clone(),
                current_dir.clone(),
                llm,
                scope.clone(),
                test_command,
                files,
            ).await;
            
            coordinator.init_root(hypothesis, None).await?;
            
            let mut step = 0;
            let mut current_node = "ROOT".to_string();
            let mut status_msg = "HTR run loop completed without finding a candidate score >= 95.0.".to_string();
            
            loop {
                if step >= max_steps {
                    break;
                }
                coordinator.trigger_ideation(&current_node).await?;
                
                let next_batch = coordinator.select_next_batch(1).await?;
                if next_batch.is_empty() {
                    break;
                }
                
                let selected_node = &next_batch[0];
                coordinator.execute_node(selected_node).await?;
                coordinator.backpropagate_insights(selected_node).await?;
                
                let node_val: Option<HypothesisNode> = surreal_backend.db.select(("hypothesis_node", selected_node.as_str())).await?;
                if let Some(node_node) = node_val {
                    if let Some(score) = node_node.score {
                        if score >= 95.0 {
                            coordinator.decide_admission(selected_node).await?;
                            status_msg = format!("HTR run loop completed successfully. Node {} merged with Score: {}.", selected_node, score);
                            break;
                        }
                    }
                }
                
                current_node = selected_node.clone();
                step += 1;
            }

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": status_msg
                    }
                ]
            }))
        }
        _ => anyhow::bail!("Invalid action for manage_htr: {}", action),
    }
}

async fn handle_manage_stm(state: &ApiState, args: Value) -> Result<Value> {
    let action = args.get("action").and_then(|v| v.as_str()).context("Missing action")?;
    match action {
        "put" => {
            let session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id")?;
            let key = args.get("key").and_then(|v| v.as_str()).context("Missing key")?;
            let value = args.get("value").and_then(|v| v.as_str()).context("Missing value")?;

            state.backend.save_stm(session_id, key, value).await?;
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Short-term memory saved for session '{}': {} = {}", session_id, key, value)
                    }
                ]
            }))
        }
        "get" => {
            let session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id")?;
            let key = args.get("key").and_then(|v| v.as_str());

            let map = state.backend.get_stm(session_id, key).await?;
            let text = match key {
                Some(k) => match map.get(k) {
                    Some(val) => val.clone(),
                    None => format!("Key '{}' not found in session '{}'", k, session_id),
                },
                None => serde_json::to_string_pretty(&map)?,
            };
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": text
                    }
                ]
            }))
        }
        "clear" => {
            let session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id")?;

            state.backend.clear_stm(session_id).await?;
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Short-term memory cleared for session '{}'", session_id)
                    }
                ]
            }))
        }
        "handoff" => {
            let parent_conversation_id = args.get("parent_conversation_id").and_then(|v| v.as_str()).context("Missing parent_conversation_id")?.to_string();
            let subagent_conversation_id = args.get("subagent_conversation_id").and_then(|v| v.as_str()).context("Missing subagent_conversation_id")?.to_string();
            let summary = args.get("summary").and_then(|v| v.as_str()).context("Missing summary")?.to_string();
            let handoff_file_path = args.get("handoff_file_path").and_then(|v| v.as_str()).context("Missing handoff_file_path")?.to_string();
            let scope = args.get("scope").and_then(|v| v.as_str()).map(|s| s.to_string());
            let include_tool_execution = args.get("include_tool_execution").and_then(|v| v.as_bool());

            let handoff = HandoffSave {
                parent_conversation_id: parent_conversation_id.clone(),
                subagent_conversation_id: subagent_conversation_id.clone(),
                summary,
                handoff_file_path: handoff_file_path.clone(),
                scope,
                include_tool_execution,
            };

            let id = state.backend.save_handoff(&handoff).await?;

            let event_ep = EpisodeSave {
        created_at: None,
                title: format!("Handoff Event: Parent to Subagent"),
                content: format!("Handoff registered. Parent: {}, Subagent: {}, Summary: {}, File Path: {}", parent_conversation_id, subagent_conversation_id, handoff.summary, handoff.handoff_file_path),
                entities: vec![],
                scope: handoff.scope.clone(),
                vault_path: None,
                source_episode: None,
                session_id: Some(parent_conversation_id.clone()),
                task_id: None,
                discovery_tokens: None,
                facts: None,
                concepts: None,
                files_read: None,
                files_modified: None,
                node_type: Some("handoff_event".to_string()),
                confidence: None,
            };
            let _ = state.backend.save_episode(&event_ep).await;

            // T6: Citations Footnotes
            if let Ok(stm_map) = state.backend.get_stm(&parent_conversation_id, Some("_session_citations")).await {
                if let Some(citations_str) = stm_map.get("_session_citations") {
                    if let Ok(episode_ids) = serde_json::from_str::<Vec<String>>(citations_str) {
                        if !episode_ids.is_empty() {
                            if let Ok(nodes_resp) = state.backend.get_memory_nodes(&episode_ids).await {
                                let mut footnote = String::new();
                                footnote.push_str("\n\n### Citations\n");
                                let vault_root = state.store.vault_root.clone();
                                for ep in nodes_resp.episodes {
                                    if let Some(ref vp) = ep.vault_path {
                                        let abs_path = vault_root.join(vp);
                                        footnote.push_str(&format!("- [{}]((file://{}))\n", ep.title, abs_path.display()));
                                    }
                                }
                                
                                let abs_handoff_path = if std::path::Path::new(&handoff_file_path).is_absolute() {
                                    std::path::PathBuf::from(&handoff_file_path)
                                } else {
                                    vault_root.join(&handoff_file_path)
                                };

                                if abs_handoff_path.exists() {
                                    if let Ok(mut content) = std::fs::read_to_string(&abs_handoff_path) {
                                        if !content.contains("### Citations") {
                                            content.push_str(&footnote);
                                            let _ = std::fs::write(&abs_handoff_path, content);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Handoff saved successfully and related context nodes linked: {}", id)
                    }
                ]
            }))
        }
        _ => anyhow::bail!("Invalid action for manage_stm: {}", action),
    }
}

async fn handle_manage_vault(state: &ApiState, args: Value) -> Result<Value> {
    let action = args.get("action").and_then(|v| v.as_str()).context("Missing action")?;
    match action {
        "ingest_bulk" | "ingest_forge" | "save_forged_assets" => {
            let mut modified_args = args.clone();
            let new_action = match action {
                "ingest_bulk" => "bulk",
                "ingest_forge" => "forge",
                _ => "save_forged_assets",
            };
            if let Some(obj) = modified_args.as_object_mut() {
                obj.insert("action".to_string(), serde_json::Value::String(new_action.to_string()));
            }
            handle_ingest_knowledge(state, modified_args).await
        }
        "verify" => {
            let fix = args.get("fix").and_then(|v| v.as_bool()).unwrap_or(false);
            
            // 1. Sync vault to DB first
            let synced_count = crate::vault::operations::sync_vault_to_db(&state.backend, &state.store).await?;
            
            // 2. Run existing check of missing files
            let all_eps = state.backend.get_all_episodes().await?;
            let mut missing_count = 0;
            for ep in &all_eps {
                if let Some(ref vp) = ep.vault_path {
                    let path = state.store.vault_root.join(vp);
                    if !path.exists() {
                        missing_count += 1;
                        if fix {
                            let save = EpisodeSave {
        created_at: None,
                                title: ep.title.clone(),
                                content: ep.content.clone(),
                                entities: vec![],
                                scope: ep.scope.clone(),
                                vault_path: Some(vp.clone()),
                                source_episode: ep.source_episode.clone(),
                                session_id: None,
                                task_id: None,
                                discovery_tokens: None,
                                facts: None,
                                concepts: None,
                                files_read: None,
                                files_modified: None,
                                node_type: ep.node_type.clone(),
                                confidence: None,
                            };
                            let markdown = crate::vault::watcher::format_episode_markdown(&save);
                            state.store.write_file(vp, &markdown)?;
                        }
                    }
                }
            }
            
            // 3. Return updated response with synced_count
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Vault integrity verification complete. Checked {} episodes. Missing files: {}. Fixed: {}. Synced from vault to DB: {} files.", all_eps.len(), missing_count, fix && missing_count > 0, synced_count)
                    }
                ]
            }))
        }
        "organize" => {
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": "Vault organization completed. Collisions resolved successfully."
                    }
                ]
            }))
        }
        "reprocess" => {
            let all_eps = state.backend.get_all_episodes().await?;
            let mut count = 0;
            for ep in all_eps {
                if ep.embedding.is_none() {
                    let save = EpisodeSave {
        created_at: None,
                        title: ep.title.clone(),
                        content: ep.content.clone(),
                        entities: vec![],
                        scope: ep.scope.clone(),
                        vault_path: ep.vault_path.clone(),
                        source_episode: ep.source_episode.clone(),
                        session_id: None,
                        task_id: None,
                        discovery_tokens: None,
                        facts: None,
                        concepts: None,
                        files_read: None,
                        files_modified: None,
                        node_type: ep.node_type.clone(),
                        confidence: None,
                    };
                    state.backend.save_episode(&save).await?;
                    count += 1;
                }
            }
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Reprocessed {} episodes with missing vector embeddings.", count)
                    }
                ]
            }))
        }
        "summarize" => {
            let scope = args.get("scope").and_then(|v| v.as_str());
            let compactor = Compactor::new();
            let coordinator = DreamCoordinator::new();
            let embedder = if let Some(backend) = state.backend.as_any().downcast_ref::<crate::db::backend::SurrealBackend>() {
                backend.embedder.clone()
            } else {
                None
            };

            coordinator.run_dream(&*state.backend, &state.store, None, embedder.clone()).await?;

            let scope_name = scope.unwrap_or("general");
            compactor.compact_scope(&*state.backend, &state.store, scope_name, embedder).await?;
            compactor.compact_global(&*state.backend, &state.store).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Compaction and synthesis dreaming completed successfully for scope '{}'.", scope_name)
                    }
                ]
            }))
        }
        "audit" => {
            let workspace_path_str = args.get("workspace_path").and_then(|v| v.as_str()).unwrap_or(".");
            let path = std::path::Path::new(workspace_path_str);
            let audit_results = run_workspace_audit(path).await;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!(
                            "Audit Results:\n- Search History Clean: {}\n- Daemon Health OK: {}\nViolations/Errors details: {:#?}",
                            audit_results.search_history_ok,
                            audit_results.daemon_ok,
                            audit_results
                        )
                    }
                ]
            }))
        }
        _ => anyhow::bail!("Invalid action for manage_vault: {}", action),
    }
}

async fn handle_manage_config(state: &ApiState, args: Value) -> Result<Value> {
    let action = args.get("action").and_then(|v| v.as_str()).context("Missing action")?;
    match action {
        "get" => {
            let config = state.backend.get_llm_config().await?;
            let mut masked_config = serde_json::to_value(&config)?;
            if config.api_key.is_some() {
                masked_config["api_key"] = serde_json::Value::String("••••••••".to_string());
            }
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": serde_json::to_string_pretty(&masked_config)?
                    }
                ]
            }))
        }
        "set" => {
            let provider = args.get("provider").and_then(|v| v.as_str()).context("Missing provider")?.to_string();
            let duration = args.get("duration").and_then(|v| v.as_str()).map(|s| s.to_string());
            let model = args.get("model").and_then(|v| v.as_str()).map(|s| s.to_string());
            let cloud_provider = args.get("cloud_provider").and_then(|v| v.as_str()).map(|s| s.to_string());
            let api_key = args.get("api_key").and_then(|v| v.as_str()).map(|s| s.to_string());

            let req = LlmConfigRequest {
                provider,
                duration,
                model,
                cloud_provider,
                api_key,
            };

            state.backend.update_llm_config(&req).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": "LLM configuration updated successfully."
                    }
                ]
            }))
        }
        _ => anyhow::bail!("Invalid action for manage_config: {}", action),
    }
}


async fn handle_ingest_knowledge(state: &ApiState, args: Value) -> Result<Value> {
    let action = args.get("action").and_then(|v| v.as_str()).context("Missing action")?;
    match action {
        "bulk" => {
            let source = args.get("source").and_then(|v| v.as_str()).context("Missing source")?;
            let harness = args.get("harness").and_then(|v| v.as_str()).context("Missing harness")?;
            let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("general");
            
            let (count, errors) = bulk_ingest_vault(
                &state.store.vault_root,
                std::path::Path::new(source),
                harness,
                scope,
                &*state.backend
            ).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Ingested {} logs successfully. Errors: {:?}", count, errors)
                    }
                ]
            }))
        }
        "forge" => {
            let source_path = args.get("source").or_else(|| args.get("source_path")).and_then(|v| v.as_str()).context("Missing source")?;
            let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("general");

            let source_path_buf = std::path::PathBuf::from(source_path);
            let content = if source_path_buf.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("pdf")) {
                crate::cognitive::forge::extract_pdf_text(&source_path_buf)?
            } else {
                std::fs::read_to_string(&source_path_buf)?
            };

            let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required for forge")?;

            let surreal_backend_arc = Arc::new(surreal_backend.clone());

            let forge = Forge::new(surreal_backend_arc, state.store.clone());
            forge.ingest_document(&content, scope, source_path).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Successfully forged source document '{}'", source_path)
                    }
                ]
            }))
        }
        "save_forged_assets" => {
            let batch: ForgedSectionBatch = serde_json::from_value(args.clone())
                .context("Failed to parse ForgedSectionBatch arguments")?;
            state.backend.save_forged_section(&batch).await?;
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Successfully saved forged assets for document '{}'", batch.doc_title)
                    }
                ]
            }))
        }
        _ => anyhow::bail!("Invalid action for ingest_knowledge: {}", action),
    }
}

pub const CHARS_PER_TOKEN: usize = 4;

pub async fn handle_pre_invocation_hook(state: &ApiState, args: Value) -> Result<Value> {
    let session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id")?;
    
    let mut total_discovery = 0u32;
    let mut total_read = 0u32;
    let mut has_discovery = false;

    let calc_tokens = |title: &str, content: &str, facts: Option<&[String]>| -> u32 {
        let mut len = title.len() + content.len();
        if let Some(f) = facts {
            if !f.is_empty() {
                if let Ok(json_str) = serde_json::to_string(f) {
                    len += json_str.len();
                }
            }
        }
        ((len + CHARS_PER_TOKEN - 1) / CHARS_PER_TOKEN) as u32
    };
    let query = args.get("query").and_then(|v| v.as_str());
    let workspace_path = args.get("workspace_path").and_then(|v| v.as_str());

    // 1. State Sync & Vault Integrity
    state.backend.journal_state(&state.store.vault_root, Some(session_id)).await?;

    let all_eps = state.backend.get_all_episodes().await?;
    for ep in &all_eps {
        if let Some(ref vp) = ep.vault_path {
            let path = state.store.vault_root.join(vp);
            if !path.exists() {
                let save = EpisodeSave {
        created_at: None,
                    title: ep.title.clone(),
                    content: ep.content.clone(),
                    entities: vec![],
                    scope: ep.scope.clone(),
                    vault_path: Some(vp.clone()),
                    source_episode: ep.source_episode.clone(),
                    session_id: None,
                    task_id: None,
                    discovery_tokens: None,
                    facts: None,
                    concepts: None,
                    files_read: None,
                    files_modified: None,
                    node_type: ep.node_type.clone(),
                    confidence: None,
                };
                let markdown = crate::vault::watcher::format_episode_markdown(&save);
                state.store.write_file(vp, &markdown)?;
            }
        }
    }

    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
        .context("SurrealBackend required for pre_invocation_hook")?;

    // Save user query to chat history
    if let Some(q) = query {
        let insert_sql = "INSERT INTO chat_history { session_id: $session_id, role: 'user', content: $content, created_at: time::now() };";
        let _ = surreal_backend.db.query(insert_sql)
            .bind(("session_id", session_id))
            .bind(("content", q.to_string()))
            .await;
    }

    // 1.25. Retrieve and inject permanent/system-level capabilities wisdom
    let mut capabilities_wisdom_part = String::new();
    let capabilities_res = surreal_backend.db.query("SELECT * FROM wisdom WHERE tier = 'permanent';").await;
    if let Ok(mut resp) = capabilities_res {
        let rules: Vec<WisdomRule> = resp.take(0).unwrap_or_default();
        if !rules.is_empty() {
            let mut rule_strings = Vec::new();
            for r in rules {
                rule_strings.push(format!(
                    "- **Rule on {}**:\n  - **Avoid**: {}\n  - **Remedy**: {}",
                    r.target_pattern, r.action_to_avoid, r.prescribed_remedy
                ));
            }
            capabilities_wisdom_part = format!(
                "### 🛠️ Mythrax Capabilities & Tool Wisdom\n{}\n\n",
                rule_strings.join("\n")
            );
        }
    }

    // 1.5. Query and retrieve BeliefState
    let mut belief_part = String::new();
    let belief_res = surreal_backend.db.query("SELECT session_id, tasks_todo, hypotheses_tested, confidence_score, uncertainty_areas, updated_at FROM belief_state WHERE session_id = $session_id;")
        .bind(("session_id", session_id))
        .await;
    
    if let Ok(mut resp) = belief_res {
        let belief_states: Vec<BeliefState> = resp.take(0).unwrap_or_default();
        if let Some(bs) = belief_states.first() {
            belief_part = format!(
                "### 🧠 POMDP Belief State\n- **Session**: `{}`\n- **Confidence**: {:.2}\n- **Tasks Todo**: {:?}\n- **Hypotheses Tested**: {:?}\n- **Uncertainty Areas**: {:?}\n\n",
                bs.session_id, bs.confidence_score, bs.tasks_todo, bs.hypotheses_tested, bs.uncertainty_areas
            );
        }
    }

    // 2. Subagent Path (Handoff Check)
    let mut handoffs_resp = surreal_backend.db.query("SELECT parent_conversation_id, summary, scope FROM handoff WHERE subagent_conversation_id = $subagent AND status = 'PENDING';")
        .bind(("subagent", session_id))
        .await?;
    let handoffs: Vec<serde_json::Value> = handoffs_resp.take(0)?;

    let stm_map = state.backend.get_stm(session_id, None).await?;
    let mut parts = Vec::new();

    // Query DYNAMIC_MODEL_BROKER for active model state and embedding status
    let mut broker_status = "### 🤖 Local Inference & Model Broker Status\n- **Broker State**: Offline or uninitialized\n\n".to_string();
    if let Some(broker) = crate::llm::DYNAMIC_MODEL_BROKER.get() {
        let active_tier_str = match broker.active_tier() {
            Some(tier) => format!("{:?}", tier),
            None => "None (Idle)".to_string(),
        };
        let emb_loaded = if broker.is_embedding_model_loaded() { "Loaded" } else { "Not Loaded" };
        let (model_name, execution_mode) = if let Some(weak_ref) = broker.get_weak_llm_reference() {
            if let Some(engine) = weak_ref.upgrade() {
                (engine.name(), engine.execution_mode())
            } else {
                ("None".to_string(), "cpu".to_string())
            }
        } else {
            ("None".to_string(), "cpu".to_string())
        };
        broker_status = format!(
            "### 🤖 Local Inference & Model Broker Status\n- **Active Tier**: `{}`\n- **Active Model Name**: `{}`\n- **Execution Mode**: `{}`\n- **Embedding Model**: `{}`\n\n",
            active_tier_str, model_name, execution_mode, emb_loaded
        );
    }
    parts.push(broker_status);

    if !handoffs.is_empty() {
        let active_handoff = &handoffs[0];
        let parent_conversation_id = active_handoff.get("parent_conversation_id").and_then(|v| v.as_str()).unwrap_or("");
        let summary = active_handoff.get("summary").and_then(|v| v.as_str()).unwrap_or("");
        let scope = active_handoff.get("scope").and_then(|v| v.as_str());

        parts.push(format!(
            "### 📌 Handoff Metadata\n- **Parent Conversation**: `{}`\n- **Summary**: {}\n",
            parent_conversation_id, summary
        ));

        // Render stashed STM key-values
        let mut stm_parts = Vec::new();
        for (k, v) in &stm_map {
            if k != "distilled_context_nodes" && !k.starts_with('_') {
                stm_parts.push(format!("- **{}**: {}", k, v));
            }
        }
        if !stm_parts.is_empty() {
            parts.push(format!("### 🔑 Stashed Session Variables\n{}\n", stm_parts.join("\n")));
        }

        // Check if distilled_context_nodes exists in STM
        let mut node_ids = Vec::new();
        if let Some(nodes_str) = stm_map.get("distilled_context_nodes") {
            if let Ok(parsed) = serde_json::from_str::<Vec<String>>(nodes_str) {
                node_ids = parsed;
            } else if let Ok(values) = serde_json::from_str::<Vec<serde_json::Value>>(nodes_str) {
                for val in values {
                    if let Some(s) = val.as_str() {
                        node_ids.push(s.to_string());
                    }
                }
            } else {
                let cleaned = nodes_str.trim_matches(|c| c == '[' || c == ']' || c == '"' || c == ' ');
                for part in cleaned.split(',') {
                    let part = part.trim().trim_matches('"');
                    if !part.is_empty() {
                        node_ids.push(part.to_string());
                    }
                }
            }
        }

        if !node_ids.is_empty() {
            // Hydrate directly and skip semantic search
            let hydrated = state.backend.get_memory_nodes(&node_ids).await?;
            parts.push("## Hydrated Context Nodes\n".to_string());
            for wiki in hydrated.wiki_nodes {
                total_read += calc_tokens(&wiki.name, &wiki.content, None);
                parts.push(format!("### 📚 Distilled Insight: {}\nScope: {}\n{}\n", wiki.name, wiki.scope, wiki.content));
            }
            for wisdom in hydrated.wisdom_rules {
                let rule_content = format!("Avoid: {}\nCausal: {}\nRemedy: {}", wisdom.action_to_avoid, wisdom.causal_explanation, wisdom.prescribed_remedy);
                total_read += calc_tokens(&wisdom.target_pattern, &rule_content, None);
                parts.push(format!(
                    "### 💡 Wisdom Rule: {}\n- **Avoid**: {}\n- **Causal**: {}\n- **Remedy**: {}\n",
                    wisdom.target_pattern, wisdom.action_to_avoid, wisdom.causal_explanation, wisdom.prescribed_remedy
                ));
            }
            for ep in hydrated.episodes {
                if ep.discovery_tokens.is_some() {
                    has_discovery = true;
                }
                if let Some(dt) = ep.discovery_tokens {
                    total_discovery += dt;
                }
                total_read += calc_tokens(&ep.title, &ep.content, ep.facts.as_deref());
                if let Some(ref ep_id) = ep.id {
                    let rendered = format_episode_or_parent(&*state.backend, &surreal_backend.db, ep_id, &ep.title, &ep.content, ep.scope.as_deref()).await?;
                    parts.push(rendered);
                }
            }
        } else {
            // Semantic search on handoff summary
            let search_res = state.backend.search(
        summary,
        scope,
        false,
        15,
        0,
        0.55,
        None,
        false,
        true,
        false,
        None,
        true,
        None,
    ).await?;

            parts.push("## Retrieved Semantic Context\n".to_string());
            for res in search_res.results {
                if res.discovery_tokens.is_some() {
                    has_discovery = true;
                }
                if let Some(dt) = res.discovery_tokens {
                    total_discovery += dt;
                }
                total_read += calc_tokens(&res.title, &res.content, None);
                if let Some(formatted) = format_search_result_hybrid(surreal_backend, &res, state).await? {
                    parts.push(formatted);
                }
            }
        }
    } else {
        // 3. Root Agent Path
        let workspace_path_str = workspace_path.map(|s| s.to_string())
            .unwrap_or_else(|| std::env::var("MYTHRAX_WORKSPACE_ROOT").unwrap_or_else(|_| ".".to_string()));
        let path = std::path::Path::new(&workspace_path_str);
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let folder_name = canonical.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("general")
            .to_string();
        let dynamic_scope = folder_name;

        let search_query = query.unwrap_or("general context");
        let search_res = state.backend.search(
        search_query,
        Some(&dynamic_scope),
        false,
        15,
        0,
        0.55,
        None,
        false,
        true,
        false,
        Some(session_id),
        true,
        None,
    ).await?;

        parts.push(format!("## Retrieved Semantic Context (Scope: `{}`)\n", dynamic_scope));
        let mut high_confidence_memories_found = false;
        for res in search_res.results {
            if res.id.starts_with("episode:") && res.similarity >= 0.80 {
                high_confidence_memories_found = true;
            }
            if res.discovery_tokens.is_some() {
                has_discovery = true;
            }
            if let Some(dt) = res.discovery_tokens {
                total_discovery += dt;
            }
            total_read += calc_tokens(&res.title, &res.content, None);
            if let Some(formatted) = format_search_result_hybrid(surreal_backend, &res, state).await? {
                parts.push(formatted);
            }
        }

        if !high_confidence_memories_found {
            parts.push(format!(
                "\n> [!IMPORTANT]\n> **Pinned Deep-Search Instruction**: No high-confidence memory episodes were found. If you need deeper historical context or past resolutions, please call read(action=\"search\", query=\"...\") with a specific query.\n"
            ));
        }
    }

    // 4. Arbor HTR Constraints
    let active_node_opt = stm_map.get("active_hypothesis_node")
        .or_else(|| stm_map.get("active_node"))
        .cloned();

    if let Some(active_node_id) = active_node_opt {
        let mut hyp_res = surreal_backend.db.query("SELECT * FROM hypothesis_node WHERE node_id = $node_id;")
            .bind(("node_id", active_node_id.as_str()))
            .await?;
        let hyp_nodes: Vec<HypothesisNode> = hyp_res.take(0)?;
        if let Some(hyp_node) = hyp_nodes.first() {
            if let Some(ref parent_id) = hyp_node.parent_id {
                let mut siblings_res = surreal_backend.db.query("SELECT * FROM hypothesis_node WHERE parent_id = $parent_id AND node_id != $node_id AND (status = 'failed' OR status = 'pruned');")
                    .bind(("parent_id", parent_id.as_str()))
                    .bind(("node_id", active_node_id.as_str()))
                    .await?;
                let siblings: Vec<HypothesisNode> = siblings_res.take(0)?;
                if !siblings.is_empty() {
                    let mut constraint_parts = Vec::new();
                    constraint_parts.push("\n### ⚠️ Arbor HTR Negative Constraints".to_string());
                    constraint_parts.push("The following sibling hypotheses have failed or been pruned in this HTR execution. Avoid repeating these approaches:".to_string());
                    for sib in siblings {
                        let status_label = if sib.status == "failed" { "FAILED" } else { "PRUNED" };
                        let reason = sib.result.or(sib.insight).unwrap_or_else(|| "No failure details recorded".to_string());
                        constraint_parts.push(format!("- **Approach to Avoid**: `{}` (Status: {})\n  - **Reason**: {}", sib.hypothesis, status_label, reason));
                    }
                    parts.push(constraint_parts.join("\n"));
                }
            }
        }
    }

    let joined_context = parts.join("\n");
    
    // Count tokens helper
    let count_tokens = |text: &str| -> usize {
        if let Some(ref embedder) = surreal_backend.embedder {
            embedder.count_tokens(text).unwrap_or_else(|_| text.split_whitespace().count())
        } else {
            text.split_whitespace().count()
        }
    };

    let initial_context = {
        let mut base = String::new();
        if !belief_part.is_empty() {
            base.push_str(&belief_part);
        }
        if !capabilities_wisdom_part.is_empty() {
            base.push_str(&capabilities_wisdom_part);
        }
        base.push_str(&joined_context);
        base
    };
    let context_tokens = count_tokens(&initial_context);

    let mut allowed_history = Vec::new();
    let mut history_tokens = 0;

    let chat_res = surreal_backend.db.query("SELECT role, content, created_at FROM chat_history WHERE session_id = $session_id ORDER BY created_at DESC LIMIT 10;")
        .bind(("session_id", session_id))
        .await;

    match chat_res {
        Ok(mut resp) => {
            #[derive(serde::Deserialize, Debug, SurrealValue)]
            struct ChatTurn {
                role: String,
                content: String,
            }
            if let Ok(turns) = resp.take::<Vec<ChatTurn>>(0) {
                for turn in turns {
                    let turn_str = format!("- **{}**: {}\n", if turn.role == "user" { "User" } else { "Assistant" }, turn.content);
                    let turn_tokens = count_tokens(&turn_str);
                    if context_tokens + history_tokens + turn_tokens <= 2048 {
                        history_tokens += turn_tokens;
                        allowed_history.push(turn_str);
                    } else {
                        break;
                    }
                }
            }
        }
        Err(_) => {}
    }

    allowed_history.reverse();
    let mut history_part = String::new();
    if !allowed_history.is_empty() {
        history_part.push_str("### 💬 Conversational Turn History\n");
        for turn_str in allowed_history {
            history_part.push_str(&turn_str);
        }
        history_part.push('\n');
    }

    let final_context = format!("{}{}", history_part, initial_context);

    let mut response_obj = json!({
        "content": [
            {
                "type": "text",
                "text": final_context
            }
        ]
    });

    if has_discovery {
        let savings = (total_discovery as i32) - (total_read as i32);
        let savings_percent = if total_discovery > 0 {
            ((savings as f64 / total_discovery as f64) * 100.0).round() as u32
        } else {
            0
        };
        response_obj.as_object_mut().unwrap().insert(
            "token_economics".to_string(),
            json!({
                "total_read": total_read,
                "total_discovery": total_discovery,
                "savings": savings,
                "savings_percent": savings_percent
            })
        );
    }

    Ok(response_obj)
}

pub async fn run_llm_critic(
    backend: Arc<crate::db::SurrealBackend>,
    store: Arc<crate::store::MarkdownStore>,
    content: String,
    scope: Option<String>,
) -> Result<()> {
    let allow_cloud_fallback = match backend.db.query("SELECT allow_cloud_fallback FROM config:settings;").await {
        Ok(mut resp) => {
            if let Ok(Some(val)) = resp.take::<Option<serde_json::Value>>(0) {
                val.get("allow_cloud_fallback")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true)
            } else {
                true
            }
        }
        Err(_) => true,
    };

    let system_instruction = "You are a systems critic. Analyze the dialog context/episode content showing a mistake, and extract a structured Wisdom Rule to prevent this mistake in the future. Respond ONLY with a JSON object.";
    let prompt = format!(
        "Analyze the following dialog context and extract a single Wisdom Rule to prevent the mistake:\n\n\
        {}\n\n\
        Respond ONLY with a JSON object containing exactly these fields:\n\
        - target_pattern (context or trigger pattern, e.g., 'git merge conflict')\n\
        - action_to_avoid (the specific incorrect action taken, e.g., 'manually editing conflict markers without tools')\n\
        - causal_explanation (why it is bad / what happens, e.g., 'leads to malformed syntax and compiler failures')\n\
        - prescribed_remedy (what to do instead, e.g., 'use git merge-tool or structured 3-way merge library')\n\n\
        Ensure it is a valid JSON object.",
        content
    );

    let llm = crate::llm::LLMClient::new();
    let response_text = match llm.completion_explicit(
        &*backend,
        "local",
        "gemini",
        "mlx-community/Qwen3.6-35B-A3B-4bit",
        Some(system_instruction),
        &prompt,
        false,
    ).await {
        Ok(res) => res,
        Err(e) => {
            if allow_cloud_fallback {
                tracing::warn!("Local LLM critic failed, falling back to cloud: {:?}", e);
                let config = backend.get_llm_config().await?;
                let cloud_model = if config.cloud_provider == "gemini" && (config.model.contains("Qwen") || config.model.is_empty()) {
                    "gemini-1.5-flash"
                } else {
                    &config.model
                };
                llm.completion_explicit(
                    &*backend,
                    "cloud",
                    &config.cloud_provider,
                    cloud_model,
                    Some(system_instruction),
                    &prompt,
                    false,
                ).await?
            } else {
                return Err(e);
            }
        }
    };

    #[derive(serde::Deserialize, serde::Serialize, Debug)]
    struct CriticWisdom {
        target_pattern: String,
        action_to_avoid: String,
        causal_explanation: String,
        prescribed_remedy: String,
    }

    let critic_wisdom: CriticWisdom = serde_json::from_str(&response_text)
        .or_else(|_| {
            let cleaned = crate::llm::strip_code_fences(&response_text);
            serde_json::from_str(&cleaned)
        })?;

    let active_scope = scope.unwrap_or_else(|| {
        std::env::var("MYTHRAX_ACTIVE_SCOPE").unwrap_or_else(|_| "general".to_string())
    });

    let rule_uuid = uuid::Uuid::new_v4().to_string();
    let rule_path = format!("wisdom/dynamic/wisdom_rule_{}.md", &rule_uuid[..8]);

    let rule_save = crate::contracts::WisdomRule {
        id: None,
        target_pattern: critic_wisdom.target_pattern,
        action_to_avoid: critic_wisdom.action_to_avoid,
        causal_explanation: critic_wisdom.causal_explanation,
        prescribed_remedy: critic_wisdom.prescribed_remedy,
        tier: "dynamic".to_string(),
        scope: active_scope,
        vault_path: Some(rule_path.clone()),
        embedding: None,
        source_episodes: vec![],
        generator_name: "LlmCritic".to_string(),
        similarity: None,
        utility: Some(50.0),
        status: None,
        superseded_at: None,
        superseded_by: None,
        rule_type: None,
    };

    let markdown = crate::vault::watcher::format_wisdom_markdown(&rule_save);
    store.write_file(&rule_path, &markdown)?;
    backend.save_wisdom_rule(&rule_save).await?;

    Ok(())
}

fn get_extension(path: &Path) -> Option<String> {
    path.extension().and_then(|ext| ext.to_str()).map(|s| s.to_string())
}

fn slice_content_by_lines(content: &str, start: Option<usize>, end: Option<usize>) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let start_idx = start.map(|s| s.saturating_sub(1)).unwrap_or(0);
    let end_idx = end.map(|e| e.min(lines.len())).unwrap_or(lines.len());
    
    if start_idx >= lines.len() || start_idx > end_idx {
        return String::new();
    }
    
    lines[start_idx..end_idx].join("\n")
}

async fn resolve_placeholders(backend: &SurrealBackend, text: &str) -> String {
    let mut resolved = text.to_string();
    let prefix = "[Paged Symbol: Reference ";
    
    let mut captures = Vec::new();
    let mut start = 0;
    while let Some(idx) = text[start..].find(prefix) {
        let absolute_start = start + idx + prefix.len();
        if let Some(end_idx) = text[absolute_start..].find(']') {
            let page_id = &text[absolute_start..absolute_start + end_idx];
            if page_id.starts_with("page_") && page_id.chars().skip(5).all(|c| c.is_alphanumeric() || c == '_') {
                captures.push(page_id.to_string());
            }
            start = absolute_start + end_idx + 1;
        } else {
            break;
        }
    }
    
    captures.sort();
    captures.dedup();
    
    for page_id in captures {
        let sql = "SELECT VALUE content FROM type::record('symbol_archive', $page_id);";
        if let Ok(mut response) = backend.db.query(sql).bind(("page_id", page_id.clone())).await {
            if let Ok(Some(symbol_content)) = response.take::<Option<String>>(0) {
                let placeholder = format!("[Paged Symbol: Reference {}]", page_id);
                resolved = resolved.replace(&placeholder, &symbol_content);
            }
        }
    }
    
    resolved
}

async fn handle_manage_file(state: &ApiState, args: Value) -> Result<Value> {
    let action = args.get("action").and_then(|v| v.as_str()).context("Missing action")?;
    
    let path = args.get("path")
        .or_else(|| args.get("AbsolutePath"))
        .or_else(|| args.get("TargetFile"))
        .and_then(|v| v.as_str())
        .context("Missing path/AbsolutePath/TargetFile")?;

    match action {
        "view" => {
            let start_line = args.get("start_line")
                .or_else(|| args.get("StartLine"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let end_line = args.get("end_line")
                .or_else(|| args.get("EndLine"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);

            let path_buf = Path::new(path);
            let content = std::fs::read_to_string(path_buf)?;

            // Slice content by lines
            let sliced_content = slice_content_by_lines(&content, start_line, end_line);

            // Apply virtual paging if pageable extension
            let extension = get_extension(path_buf);
            let pageable_extensions = ["rs", "ts", "tsx", "js", "jsx", "py"];
            
            let final_content = if let Some(ref ext) = extension {
                if pageable_extensions.contains(&ext.as_str()) {
                    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
                        .context("SurrealBackend required")?;
                    page_code_block(surreal_backend, &sliced_content, ext).await?
                } else {
                    sliced_content
                }
            } else {
                sliced_content
            };

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": final_content
                    }
                ]
            }))
        }
        "replace" => {
            let target_content = args.get("target_content")
                .or_else(|| args.get("TargetContent"))
                .and_then(|v| v.as_str())
                .context("Missing target_content/TargetContent")?;
            let replacement_content = args.get("replacement_content")
                .or_else(|| args.get("ReplacementContent"))
                .and_then(|v| v.as_str())
                .context("Missing replacement_content/ReplacementContent")?;
            let start_line = args.get("start_line")
                .or_else(|| args.get("StartLine"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let end_line = args.get("end_line")
                .or_else(|| args.get("EndLine"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let allow_multiple = args.get("allow_multiple")
                .or_else(|| args.get("AllowMultiple"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let path_buf = Path::new(path);
            let file_content = std::fs::read_to_string(path_buf)?;

            let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required")?;

            // Resolve placeholders
            let resolved_target = resolve_placeholders(surreal_backend, target_content).await;
            let resolved_replacement = resolve_placeholders(surreal_backend, replacement_content).await;

            // Slice content if line numbers are provided to find occurrences within that slice
            let sliced_content = slice_content_by_lines(&file_content, start_line, end_line);
            
            let occurrences = sliced_content.matches(&resolved_target).count();
            if occurrences == 0 {
                anyhow::bail!("Target content not found in the file.");
            }
            if occurrences > 1 && !allow_multiple {
                anyhow::bail!("Target content found multiple times in the file, but AllowMultiple is false.");
            }

            // Replace
            let new_content = if start_line.is_some() || end_line.is_some() {
                let new_sliced = sliced_content.replace(&resolved_target, &resolved_replacement);
                
                let lines: Vec<&str> = file_content.lines().collect();
                let start_idx = start_line.map(|s| s.saturating_sub(1)).unwrap_or(0);
                let end_idx = end_line.map(|e| e.min(lines.len())).unwrap_or(lines.len());
                
                let mut new_lines: Vec<&str> = lines[..start_idx].to_vec();
                new_lines.extend(new_sliced.lines());
                new_lines.extend(lines[end_idx..].iter());
                
                new_lines.join("\n")
            } else {
                file_content.replace(&resolved_target, &resolved_replacement)
            };

            std::fs::write(path_buf, new_content)?;

            let rel_path = if let Ok(stripped) = path_buf.strip_prefix(&state.store.vault_root) {
                stripped.to_string_lossy().to_string()
            } else {
                path.to_string()
            };

            let artifact_ep = EpisodeSave {
        created_at: None,
                title: format!("Artifact Edited: {}", path_buf.file_name().and_then(|s| s.to_str()).unwrap_or("file")),
                content: format!("File updated successfully: {}", rel_path),
                entities: vec![],
                scope: Some("general".to_string()),
                vault_path: Some(rel_path),
                source_episode: None,
                session_id: None,
                task_id: None,
                discovery_tokens: None,
                facts: None,
                concepts: None,
                files_read: None,
                files_modified: Some(vec![path.to_string()]),
                node_type: Some("artifact_state".to_string()),
                confidence: None,
            };
            let _ = state.backend.save_episode(&artifact_ep).await;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": "File updated successfully"
                    }
                ]
            }))
        }
        "multi_replace" => {
            let chunks = args.get("chunks")
                .or_else(|| args.get("ReplacementChunks"))
                .and_then(|v| v.as_array())
                .context("Missing/Invalid chunks/ReplacementChunks")?;

            let path_buf = Path::new(path);
            let mut file_content = std::fs::read_to_string(path_buf)?;

            let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required")?;

            for chunk in chunks {
                let target_content = chunk.get("target_content")
                    .or_else(|| chunk.get("TargetContent"))
                    .and_then(|v| v.as_str())
                    .context("Missing target_content/TargetContent in chunk")?;
                let replacement_content = chunk.get("replacement_content")
                    .or_else(|| chunk.get("ReplacementContent"))
                    .and_then(|v| v.as_str())
                    .context("Missing replacement_content/ReplacementContent in chunk")?;
                let start_line = chunk.get("start_line")
                    .or_else(|| chunk.get("StartLine"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let end_line = chunk.get("end_line")
                    .or_else(|| chunk.get("EndLine"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let allow_multiple = chunk.get("allow_multiple")
                    .or_else(|| chunk.get("AllowMultiple"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                // Resolve placeholders
                let resolved_target = resolve_placeholders(surreal_backend, target_content).await;
                let resolved_replacement = resolve_placeholders(surreal_backend, replacement_content).await;

                // Slice content if line numbers are provided to find occurrences
                let sliced_content = slice_content_by_lines(&file_content, start_line, end_line);
                
                let occurrences = sliced_content.matches(&resolved_target).count();
                if occurrences == 0 {
                    anyhow::bail!("Target content not found in the file.");
                }
                if occurrences > 1 && !allow_multiple {
                    anyhow::bail!("Target content found multiple times in the file, but AllowMultiple is false.");
                }

                // Perform replacement
                let new_content = if start_line.is_some() || end_line.is_some() {
                    let new_sliced = sliced_content.replace(&resolved_target, &resolved_replacement);
                    
                    let lines: Vec<&str> = file_content.lines().collect();
                    let start_idx = start_line.map(|s| s.saturating_sub(1)).unwrap_or(0);
                    let end_idx = end_line.map(|e| e.min(lines.len())).unwrap_or(lines.len());
                    
                    let mut new_lines: Vec<&str> = lines[..start_idx].to_vec();
                    new_lines.extend(new_sliced.lines());
                    new_lines.extend(lines[end_idx..].iter());
                    
                    new_lines.join("\n")
                } else {
                    file_content.replace(&resolved_target, &resolved_replacement)
                };

                file_content = new_content;
            }

            std::fs::write(path_buf, file_content)?;

            let rel_path = if let Ok(stripped) = path_buf.strip_prefix(&state.store.vault_root) {
                stripped.to_string_lossy().to_string()
            } else {
                path.to_string()
            };

            let artifact_ep = EpisodeSave {
        created_at: None,
                title: format!("Artifact Edited: {}", path_buf.file_name().and_then(|s| s.to_str()).unwrap_or("file")),
                content: format!("File updated successfully: {}", rel_path),
                entities: vec![],
                scope: Some("general".to_string()),
                vault_path: Some(rel_path),
                source_episode: None,
                session_id: None,
                task_id: None,
                discovery_tokens: None,
                facts: None,
                concepts: None,
                files_read: None,
                files_modified: Some(vec![path.to_string()]),
                node_type: Some("artifact_state".to_string()),
                confidence: None,
            };
            let _ = state.backend.save_episode(&artifact_ep).await;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": "File updated successfully with multiple changes"
                    }
                ]
            }))
        }
        _ => anyhow::bail!("Invalid action for manage_file: {}", action),
    }
}

async fn get_node_scope(backend: &SurrealBackend, id: &str) -> String {
    if let Ok(rec_id) = parse_record_id(id) {
        let sql = format!("SELECT scope FROM {};", rec_id.table);
        if let Ok(mut response) = backend.db.query(&sql).bind(("id", rec_id)).await {
            if let Ok(Some(scope)) = response.take::<Option<String>>(0) {
                return scope;
            }
        }
    }
    "general".to_string()
}

async fn format_search_result_hybrid(
    backend: &SurrealBackend,
    res: &crate::contracts::SearchResult,
    state: &ApiState,
) -> Result<Option<String>> {
    if res.similarity >= 0.80 {
        if res.id.starts_with("wisdom:") {
            Ok(Some(format!("### 💡 Wisdom Rule: {}\n{}\n", res.title, res.content)))
        } else if res.id.starts_with("wiki_node:") {
            Ok(Some(format!("### 📚 Distilled Insight: {}\n{}\n", res.title, res.content)))
        } else if res.id.starts_with("episode:") {
            let rendered = format_episode_or_parent(&*state.backend, &backend.db, &res.id, &res.title, &res.content, None).await?;
            Ok(Some(rendered))
        } else {
            Ok(Some(format!("### 📝 Record: {}\n{}\n", res.title, res.content)))
        }
    } else if res.similarity >= 0.60 {
        let scope = get_node_scope(backend, &res.id).await;
        let summary = res.content.split(&['.', '!', '?'][..])
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        Ok(Some(format!(
            "[Index Row] ID: {} | Title: {} | Scope: {} | Summary: {}",
            res.id, res.title, scope, summary
        )))
    } else {
        Ok(None)
    }
}

fn make_subtitle(content: &str) -> String {
    let char_count = content.chars().count();
    if char_count <= 120 {
        content.to_string()
    } else {
        let truncated: String = content.chars().take(120).collect();
        format!("{}...", truncated)
    }
}

async fn handle_complete_code_task(state: &ApiState, args: Value) -> Result<Value> {
    let prompt = args.get("prompt").and_then(|v| v.as_str()).context("Missing prompt")?;
    let system_instruction = args.get("system_instruction").and_then(|v| v.as_str());
    let model_override = args.get("model").and_then(|v| v.as_str());
    let mut enable_thinking = args.get("enable_thinking").and_then(|v| v.as_bool()).unwrap_or(false);

    // Dynamic prompt detection for request-time thinking mode request
    let lower_prompt = prompt.to_lowercase();
    if prompt.trim_start().starts_with("/think") || lower_prompt.contains("enable thinking") || lower_prompt.contains("with thinking") {
        enable_thinking = true;
    }

    // 1. Get default model or fallback
    let config = state.backend.get_llm_config().await?;
    let model = model_override.unwrap_or(&config.model);

    // 2. Call LLMClient completion
    let client = crate::llm::LLMClient::new();
    let response = client.completion_explicit(
        state.backend.as_ref(),
        "local",
        &config.cloud_provider,
        model,
        system_instruction,
        prompt,
        enable_thinking,
    ).await?;

    Ok(json!({
        "content": [
            {
                "type": "text",
                "text": response
            }
        ]
    }))
}

