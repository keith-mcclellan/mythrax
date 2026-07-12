use serde_json::{json, Value};
use anyhow::Result;
use crate::api::ApiState;
use crate::db::{StorageBackend, SurrealBackend, parse_record_id, backend::format_record_id};
use crate::contracts::BeliefState;

pub mod read_handlers;
pub mod write_handlers;
pub mod manage_handlers;
pub mod vault_handlers;
pub mod htr_handlers;

pub use read_handlers::{handle_read, handle_query_memory};
pub use write_handlers::{handle_write, handle_record_memory, run_llm_critic};
pub use manage_handlers::{
    handle_manage, handle_pre_invocation_hook, handle_complete_code_task, handle_agent,
    handle_manage_stm, handle_manage_file, handle_manage_config,
};
pub use vault_handlers::{handle_manage_vault, handle_ingest_knowledge};
pub use htr_handlers::handle_manage_htr;


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

pub async fn format_episode_or_parent(
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
                        "action": { "type": "string", "enum": ["verify", "organize", "reprocess", "summarize", "audit", "ingest_bulk", "ingest_forge", "save_forged_assets", "init", "ideate", "execute", "backprop", "merge", "run", "pre_invocation", "precompact", "audit_compliance", "clean"] },
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

pub async fn call_mcp_tool(
    state: &ApiState,
    name: &str,
    args: Value,
) -> Result<Value> {
    let result = match name {
        "read" => read_handlers::handle_read(state, args.clone()).await,
        "write" => write_handlers::handle_write(state, args.clone()).await,
        "manage" => manage_handlers::handle_manage(state, args.clone()).await,
        "agent" => manage_handlers::handle_agent(state, args.clone()).await,
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

pub const CHARS_PER_TOKEN: usize = 4;
