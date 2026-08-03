use crate::api::ApiState;
use crate::contracts::BeliefState;
use crate::db::{StorageBackend, SurrealBackend, backend::format_record_id, parse_record_id};
use anyhow::Result;
use serde_json::{Value, json};

pub mod arbor_handlers;
pub mod dtos;
pub mod htr_handlers;
pub mod manage_handlers;
pub mod read_handlers;
pub mod vault_handlers;
pub mod write_handlers;

pub use arbor_handlers::handle_manage_arbor;
pub use htr_handlers::handle_manage_htr;
pub use manage_handlers::{
    handle_agent, handle_manage, handle_manage_config,
    handle_manage_file, handle_manage_stm, handle_post_invocation_hook, handle_pre_invocation_hook,
};
pub use read_handlers::{handle_query_memory, handle_read};
pub use vault_handlers::{handle_ingest_knowledge, handle_manage_vault};
pub use write_handlers::{handle_record_memory, handle_write, run_llm_critic};

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

#[derive(Debug, PartialEq, Eq)]
enum FenceState {
    Outside,
    InNormalFence,
    InDiffFence,
}

pub fn strip_diffs(content: &str) -> String {
    let mut cleaned_lines = Vec::new();
    let mut state = FenceState::Outside;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            match state {
                FenceState::Outside => {
                    if trimmed.starts_with("```diff") {
                        state = FenceState::InDiffFence;
                        cleaned_lines.push("[Diff Truncated]");
                    } else {
                        state = FenceState::InNormalFence;
                        cleaned_lines.push(line);
                    }
                }
                FenceState::InNormalFence => {
                    state = FenceState::Outside;
                    cleaned_lines.push(line);
                }
                FenceState::InDiffFence => {
                    state = FenceState::Outside;
                }
            }
            continue;
        }

        match state {
            FenceState::InDiffFence => continue,
            FenceState::InNormalFence => cleaned_lines.push(line),
            FenceState::Outside => {
                if trimmed.starts_with("diff --git ")
                    || trimmed.starts_with("--- ")
                    || trimmed.starts_with("+++ ")
                    || trimmed.starts_with("@@ ")
                {
                    continue;
                }
                cleaned_lines.push(line);
            }
        }
    }
    cleaned_lines.join("\n")
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
    let ep_content = strip_diffs(ep_content);
    if let Ok(rec_id) = parse_record_id(ep_id) {
        let mut parent_resp = db
            .query("SELECT VALUE out FROM relates_to WHERE in = $ep_id;")
            .bind(("ep_id", rec_id))
            .await?;
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
                    p_wisdom.target_pattern,
                    p_wisdom.action_to_avoid,
                    p_wisdom.causal_explanation,
                    p_wisdom.prescribed_remedy
                ));
            }
            if !parts.is_empty() {
                return Ok(parts.join("\n"));
            }
        }
    }

    let summary = truncate_summary(&ep_content);
    Ok(format!(
        "#### 📑 Memory Card: {}\n- **ID**: `{}`\n- **Scope**: `{}`\n- **Summary**: {}\n*For follow-up queries on this memory, use:* `get_memory_nodes [\"{}\"]`\n",
        ep_title,
        ep_id,
        ep_scope.unwrap_or("general"),
        summary,
        ep_id
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
                        "action": { "type": "string", "enum": ["view", "search", "rules", "nodes", "root", "query_symbolic", "search_index", "timeline", "get_full", "get", "search_by_concept", "diff_sessions"], "description": "Action type to execute" },
                        "path": { "type": "string", "description": "File path to view or inspect" },
                        "AbsolutePath": { "type": "string", "description": "Absolute file path alias" },
                        "TargetFile": { "type": "string", "description": "Target file path alias" },
                        "start_line": { "type": "integer", "description": "Starting line number (1-indexed)" },
                        "StartLine": { "type": "integer", "description": "Starting line number alias" },
                        "end_line": { "type": "integer", "description": "Ending line number (1-indexed)" },
                        "EndLine": { "type": "integer", "description": "Ending line number alias" },
                        "query": { "type": "string", "description": "Natural language or keyword search query string" },
                        "scope": { "type": "string", "description": "Project scope partition filter (e.g. 'mythrax', 'general')" },
                        "limit": { "type": "integer", "default": 15, "description": "Maximum number of candidate results to return" },
                        "offset": { "type": "integer", "default": 0, "description": "Pagination offset" },
                        "threshold": { "type": "number", "default": 0.55, "description": "Cosine similarity cutoff threshold (0.0 to 1.0)" },
                        "token_budget": { "type": "integer", "description": "Max token budget for formatted context rendering" },
                        "allow_downward": { "type": "boolean", "default": false, "description": "Allow downward link traversal in Arbor graph" },
                        "include_episodes": { "type": "boolean", "default": false, "description": "Include raw episode transcripts alongside synthesized wiki nodes" },
                        "include_artifacts": { "type": "boolean", "default": false, "description": "Include forged document artifacts in retrieval candidates" },
                        "include_archived": { "type": "boolean", "default": false, "description": "Include soft-deleted or archived nodes in search results" },
                        "temporal_anchor": { "type": "string", "description": "UUID or ISO timestamp anchor for temporal proximity decay" },
                        "full_content": { "type": "boolean", "default": false, "description": "Return complete untruncated content for matched memory cards" },
                        "session_id": { "type": "string", "description": "Active session UUID" },
                        "tier": { "type": "string", "description": "Memory tier filter ('working', 'arbor', 'wisdom')" },
                        "node_ids": { "type": "array", "items": { "type": "string" }, "description": "List of node UUIDs to fetch directly" },
                        "ids": { "type": "array", "items": { "type": "string" }, "description": "List of node UUIDs alias" },
                        "depth_before": { "type": "integer", "default": 3, "description": "Timeline depth before anchor" },
                        "depth_after": { "type": "integer", "default": 3, "description": "Timeline depth after anchor" },
                        "anchor_id": { "type": "string", "description": "Timeline anchor node ID" },
                        "node_id": { "type": "string", "description": "Target graph node ID" },
                        "relation": { "type": "string", "description": "Graph edge relationship filter" },
                        "max_depth": { "type": "integer", "default": 3, "description": "Graph traversal maximum depth" },
                        "key": { "type": "string", "description": "Short term memory KV key" },
                        "is_skill_file": { "type": "boolean", "description": "Flag indicating file is a skill instruction" },
                        "concept": { "type": "string", "description": "Target concept string for spreading activation" },
                        "session_a": { "type": "string", "description": "First session ID for diff_sessions" },
                        "session_b": { "type": "string", "description": "Second session ID for diff_sessions" }
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
                        "action": { "type": "string", "enum": ["replace", "multi_replace", "save", "feedback", "thought", "put", "clear", "handoff", "set", "cognitive_callback"] },
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
                        "action": { "type": "string", "enum": ["verify", "organize", "reprocess", "reprocess_markdown", "summarize", "audit", "ingest_bulk", "ingest_forge", "save_forged_assets", "init", "ideate", "execute", "backprop", "merge", "run", "pre_invocation", "precompact", "audit_compliance", "clean", "bootstrap", "prune", "tree_add_node", "tree_update_node", "tree_prune", "tree_view", "git_merge_branch"] },
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
                        "transcript_path": { "type": "string" },
                        "dry_run": { "type": "boolean", "default": false },
                        "since": { "type": "string" },
                        "distill_model": { "type": "string" },
                        "force": { "type": "boolean", "default": false },
                        "async_mode": { "type": "boolean", "default": true }
                    },
                    "required": ["action"]
                }
            },
            {
                "name": "agent",
                "description": "Consolidated tool for subagent delegation handoff contract registration and context linking.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["handoff"] },
                        "parent_conversation_id": { "type": "string" },
                        "subagent_conversation_id": { "type": "string" },
                        "summary": { "type": "string" },
                        "handoff_file_path": { "type": "string" },
                        "scope": { "type": "string" }
                    },
                    "required": ["action", "parent_conversation_id", "subagent_conversation_id", "summary", "handoff_file_path"]
                }
            }
        ]
    })
}

pub async fn call_mcp_tool(state: &ApiState, name: &str, args: Value) -> Result<Value> {
    let result = match name {
        "read" => read_handlers::handle_read(state, args.clone()).await,
        "write" => write_handlers::handle_write(state, args.clone()).await,
        "manage" => manage_handlers::handle_manage(state, args.clone()).await,
        "agent" => manage_handlers::handle_agent(state, args.clone()).await,
        _ => anyhow::bail!("Tool not found: {}", name),
    };

    let session_id_opt = args
        .get("session_id")
        .or_else(|| args.get("subagent_id"))
        .or_else(|| args.get("subagent_conversation_id"))
        .and_then(|v| v.as_str());

    let action_opt = args.get("action").and_then(|v| v.as_str());
    let resolved_action = if name == "manage" && action_opt.is_none() {
        if args.get("session_id").and_then(|v| v.as_str()).is_some() {
            "pre_invocation"
        } else if args
            .get("workspace_path")
            .and_then(|v| v.as_str())
            .is_some()
        {
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
                    let content_str =
                        if let Some(arr) = val.get("content").and_then(|c| c.as_array()) {
                            let mut s = String::new();
                            for item in arr {
                                if let Some(txt) = item.get("text").and_then(|t| t.as_str()) {
                                    s.push_str(txt);
                                    s.push('\n');
                                }
                            }
                            if s.is_empty() {
                                val.to_string()
                            } else {
                                s.trim().to_string()
                            }
                        } else if let Some(txt) = val.get("text").and_then(|t| t.as_str()) {
                            txt.to_string()
                        } else {
                            val.to_string()
                        };

                    let insert_sql = "INSERT INTO chat_history { session_id: $session_id, role: 'assistant', content: $content, created_at: time::now() };";
                    let _ = surreal_backend
                        .db
                        .query(insert_sql)
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

                        let _ = surreal_backend
                            .db
                            .query(
                                "
                            UPDATE type::record('belief_state', $session_id) CONTENT {
                                session_id: $session_id,
                                tasks_todo: $tasks_todo,
                                hypotheses_tested: $hypotheses_tested,
                                confidence_score: $confidence_score,
                                uncertainty_areas: $uncertainty_areas,
                                updated_at: $updated_at
                            };
                        ",
                            )
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

                        let _ = surreal_backend
                            .db
                            .query(
                                "
                            UPSERT type::record('belief_state', $session_id) CONTENT {
                                session_id: $session_id,
                                tasks_todo: $tasks_todo,
                                hypotheses_tested: $hypotheses_tested,
                                confidence_score: $confidence_score,
                                uncertainty_areas: $uncertainty_areas,
                                updated_at: $updated_at
                            };
                        ",
                            )
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

    if result.is_ok() && (name == "read" || name == "write" || name == "manage" || name == "agent")
    {
        let session_id_opt = args
            .get("session_id")
            .or_else(|| args.get("subagent_id"))
            .or_else(|| args.get("subagent_conversation_id"))
            .or_else(|| args.get("scope"))
            .and_then(|v| v.as_str());
        if let Err(e) = state
            .backend
            .journal_state(&state.store.vault_root, session_id_opt)
            .await
        {
            tracing::error!("Failed to write dual-durability journal: {:?}", e);
        }
    }

    result
}

pub const CHARS_PER_TOKEN: usize = 4;
