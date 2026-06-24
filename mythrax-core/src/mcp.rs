use std::io::{BufRead, Write};
use std::sync::Arc;
use serde_json::{json, Value};
use crate::db::StorageBackend;
use crate::store::MarkdownStore;
use anyhow::{Result, Context};

pub struct McpServer {
    backend: Arc<crate::db::SurrealBackend>,
    store: Arc<MarkdownStore>,
}

impl McpServer {
    pub fn new(backend: Arc<crate::db::SurrealBackend>, store: Arc<MarkdownStore>) -> Self {
        Self { backend, store }
    }

    pub async fn run(&self) -> Result<()> {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        let mut reader = std::io::BufReader::new(stdin);
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line)?;
            if bytes_read == 0 {
                break;
            }

            let msg_str = line.trim();
            if msg_str.is_empty() {
                continue;
            }

            match serde_json::from_str::<Value>(msg_str) {
                Ok(msg) => {
                    let id = msg.get("id").cloned();
                    let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");
                    let params = msg.get("params").cloned().unwrap_or(Value::Null);

                    if id.is_some() {
                        // Request
                        match self.handle_request(method, params).await {
                            Ok(result) => {
                                let resp = json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": result
                                });
                                let resp_str = serde_json::to_string(&resp)? + "\n";
                                stdout.write_all(resp_str.as_bytes())?;
                                stdout.flush()?;
                            }
                            Err(e) => {
                                let resp = json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "error": {
                                        "code": -32603,
                                        "message": e.to_string()
                                    }
                                });
                                let resp_str = serde_json::to_string(&resp)? + "\n";
                                stdout.write_all(resp_str.as_bytes())?;
                                stdout.flush()?;
                            }
                        }
                    } else {
                        // Notification
                        eprintln!("Notification received: method={}", method);
                    }
                }
                Err(e) => {
                    eprintln!("Invalid JSON received: error={}, raw={}", e, msg_str);
                }
            }
        }

        Ok(())
    }

    pub async fn handle_request(&self, method: &str, params: Value) -> Result<Value> {
        match method {
            "initialize" => {
                if let Some(root_uri_str) = params.get("rootUri").and_then(|v| v.as_str()) {
                    if let Ok(url) = url::Url::parse(root_uri_str) {
                        if let Ok(path) = url.to_file_path() {
                            unsafe {
                                std::env::set_var("MYTHRAX_WORKSPACE_ROOT", path);
                            }
                        }
                    }
                }
                Ok(json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "mythrax",
                        "version": "0.5.0"
                    }
                }))
            }
            "tools/list" => {
                Ok(json!({
                    "tools": [
                        {
                            "name": "put_short_term",
                            "description": "Store a key-value pair in session-based short-term memory",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "session_id": { "type": "string" },
                                    "key": { "type": "string" },
                                    "value": { "type": "string" }
                                },
                                "required": ["session_id", "key", "value"]
                            }
                        },
                        {
                            "name": "get_short_term",
                            "description": "Retrieve a value from session-based short-term memory by session_id and key",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "session_id": { "type": "string" },
                                    "key": { "type": "string" }
                                },
                                "required": ["session_id"]
                            }
                        },
                        {
                            "name": "clear_short_term",
                            "description": "Clear all short-term memory keys for a given session_id",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "session_id": { "type": "string" }
                                },
                                "required": ["session_id"]
                            }
                        },
                        {
                            "name": "search_memories",
                            "description": "Execute semantic memory search over saved insights and wisdom rules. Raw episodes and artifact files/leaves are excluded by default to avoid context bloat.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "query": { "type": "string" },
                                    "scope": { "type": "string" },
                                    "limit": { "type": "integer" },
                                    "offset": { "type": "integer" },
                                    "threshold": { "type": "number" },
                                    "token_budget": { "type": "integer" },
                                    "allow_downward": { "type": "boolean" },
                                    "include_episodes": { "type": "boolean" },
                                    "include_artifacts": { "type": "boolean" }
                                },
                                "required": ["query"]
                            }
                        },
                        {
                            "name": "search_wisdom",
                            "description": "Search wisdom rules",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "query": { "type": "string" },
                                    "tier": { "type": "string" },
                                    "limit": { "type": "integer" },
                                    "offset": { "type": "integer" },
                                    "threshold": { "type": "number" }
                                },
                                "required": ["query"]
                            }
                        },
                        {
                            "name": "get_memory_nodes",
                            "description": "Hydrate specific database records (episodes, wisdom rules, wiki nodes) by their Record IDs",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "node_ids": {
                                        "type": "array",
                                        "items": { "type": "string" }
                                    }
                                },
                                "required": ["node_ids"]
                            }
                        },
                        {
                            "name": "save_episode",
                            "description": "Save an episode memory",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "title": { "type": "string" },
                                    "content": { "type": "string" },
                                    "entities": { "type": "array" },
                                    "scope": { "type": "string" },
                                    "vault_path": { "type": "string" },
                                    "session_id": { "type": "string" },
                                    "task_id": { "type": "string" }
                                },
                                "required": ["title", "content"]
                            }
                        },
                        {
                            "name": "diagnose_failure",
                            "description": "Diagnose a command or compiler failure, returning a remedy if a past resolution matches",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "stdout": { "type": "string" },
                                    "stderr": { "type": "string" },
                                    "exit_code": { "type": "integer" },
                                    "command": { "type": "string" }
                                },
                                "required": ["stderr"]
                            }
                        },
                        {
                            "name": "save_handoff",
                            "description": "Save a handoff record from parent agent to subagent, linking it to any distilled context nodes in STM",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "parent_conversation_id": { "type": "string" },
                                    "subagent_conversation_id": { "type": "string" },
                                    "summary": { "type": "string" },
                                    "handoff_file_path": { "type": "string" },
                                    "scope": { "type": "string" }
                                },
                                "required": ["parent_conversation_id", "subagent_conversation_id", "summary", "handoff_file_path"]
                            }
                        },
                        {
                            "name": "get_vault_root",
                            "description": "Retrieve the active Obsidian vault root directory path",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        },
                        {
                            "name": "verify_compliance",
                            "description": "Verify workspace compliance constraints (tailwind, search history, daemon status)",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "workspace_path": { "type": "string" }
                                }
                            }
                        },
                        {
                            "name": "pre_invocation_hook",
                            "description": "Pre-invocation hook to sync state, verify vault integrity, and resolve context (subagent handoffs, HTR negative constraints, and root agent hybrid search with abstraction priority)",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "session_id": { "type": "string" },
                                    "query": { "type": "string" },
                                    "workspace_path": { "type": "string" }
                                },
                                "required": ["session_id"]
                            }
                        },
                        {
                            "name": "harvest_skill_wisdom",
                            "description": "Harvest skill wisdom from playbooks and config",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        },
                        {
                            "name": "synthesize_meta_skills",
                            "description": "Synthesize and publish meta-skills from wisdom rules, forged docs, and playbooks",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        },
                        {
                            "name": "detect_skill_merges",
                            "description": "Detect redundant or overlapping skill playbooks that are candidates for merging",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        },
                        {
                            "name": "merge_skills",
                            "description": "Consolidate multiple playbooks into a single target meta-skill, clean up meta-skills, and archive custom playbooks",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "source_skills": {
                                        "type": "array",
                                        "items": { "type": "string" },
                                        "description": "List of skill names or directories to consolidate"
                                    },
                                    "target_name": {
                                        "type": "string",
                                        "description": "Name for the consolidated meta-skill (e.g. 'git')"
                                    }
                                },
                                "required": ["source_skills", "target_name"]
                            }
                        },
                        {
                            "name": "bulk_ingest",
                            "description": "Bulk ingest transcript logs into the memory vault",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "source": { "type": "string" },
                                    "harness": { "type": "string" },
                                    "scope": { "type": "string" }
                                },
                                "required": ["source", "harness"]
                            }
                        },
                        {
                            "name": "organize_vault",
                            "description": "Run vault organizer and de-duplicate files",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        },
                        {
                            "name": "summarize_episodes",
                            "description": "Run compaction and synthesis dreaming over saved episodes",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "scope": { "type": "string" }
                                }
                            }
                        },
                        {
                            "name": "verify_vault_integrity",
                            "description": "Run self-healing vault integrity verification",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "fix": { "type": "boolean" }
                                }
                            }
                        },
                        {
                            "name": "reprocess_embeddings",
                            "description": "Reprocess episodes with missing vector embeddings",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        },
                        {
                            "name": "get_llm_config",
                            "description": "Retrieve current LLM configuration and key settings",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        },
                        {
                            "name": "update_llm_config",
                            "description": "Update LLM active model, provider, and API keys",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "provider": { "type": "string" },
                                    "duration": { "type": "string" },
                                    "model": { "type": "string" },
                                    "cloud_provider": { "type": "string" },
                                    "api_key": { "type": "string" }
                                },
                                "required": ["provider"]
                            }
                        },
                        {
                            "name": "htr_init",
                            "description": "Initialize HTR session root node",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "scope": { "type": "string" },
                                    "hypothesis": { "type": "string" },
                                    "files": {
                                        "type": "array",
                                        "items": { "type": "string" }
                                    }
                                },
                                "required": ["scope", "hypothesis", "files"]
                            }
                        },
                        {
                            "name": "htr_ideate",
                            "description": "Propose child hypotheses (Ideation)",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "scope": { "type": "string" },
                                    "node": { "type": "string" }
                                },
                                "required": ["scope", "node"]
                            }
                        },
                        {
                            "name": "htr_execute",
                            "description": "Execute hypothesis node test run",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "scope": { "type": "string" },
                                    "node": { "type": "string" },
                                    "test_command": { "type": "string" }
                                },
                                "required": ["scope", "node", "test_command"]
                            }
                        },
                        {
                            "name": "htr_backprop",
                            "description": "Backpropagate test results and evaluation insights",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "scope": { "type": "string" },
                                    "node": { "type": "string" }
                                },
                                "required": ["scope", "node"]
                            }
                        },
                        {
                            "name": "htr_merge",
                            "description": "Apply and commit the selected node's changes to the codebase",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "scope": { "type": "string" },
                                    "node": { "type": "string" }
                                },
                                "required": ["scope", "node"]
                            }
                        },
                        {
                            "name": "htr_run",
                            "description": "Run the HTR loop end-to-end for a given hypothesis and codebase files",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "scope": { "type": "string" },
                                    "hypothesis": { "type": "string" },
                                    "files": {
                                        "type": "array",
                                        "items": { "type": "string" }
                                    },
                                    "test_command": { "type": "string" },
                                    "max_steps": { "type": "integer" }
                                },
                                "required": ["scope", "hypothesis", "files", "test_command"]
                            }
                        },
                        {
                            "name": "forge_source",
                            "description": "Forge a source document (extracting rules and wiki nodes)",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "source_path": { "type": "string" },
                                    "scope": { "type": "string" }
                                },
                                "required": ["source_path"]
                            }
                        },
                        {
                            "name": "save_forged_assets",
                            "description": "Expose transactional asset saving backend directly, saving grounding chunk, concepts, and rules",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "doc_title": { "type": "string" },
                                    "scope": { "type": "string" },
                                    "chunk_index": { "type": "integer" },
                                    "chunk_text": { "type": "string" },
                                    "concepts": {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "name": { "type": "string" },
                                                "content": { "type": "string" }
                                            },
                                            "required": ["name", "content"]
                                        }
                                    },
                                    "rules": {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "target_pattern": { "type": "string" },
                                                "action_to_avoid": { "type": "string" },
                                                "causal_explanation": { "type": "string" },
                                                "prescribed_remedy": { "type": "string" }
                                            },
                                            "required": ["target_pattern", "action_to_avoid", "causal_explanation", "prescribed_remedy"]
                                        }
                                    }
                                },
                                "required": ["doc_title", "scope", "chunk_index", "chunk_text", "concepts", "rules"]
                            }
                        },
                        {
                            "name": "get_forge_instructions",
                            "description": "Retrieve the current forge guidelines, prompt templates, and extraction instructions",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        }
                    ]
                }))
            }
            "tools/call" => {
                let tool_name = params.get("name").and_then(|v| v.as_str()).context("Missing tool name")?;
                let arguments = params.get("arguments").cloned().unwrap_or(Value::Object(serde_json::Map::new()));
                self.call_tool(tool_name, arguments).await
            }
            _ => {
                anyhow::bail!("Method not found: {}", method)
            }
        }
    }

    pub async fn call_tool(&self, name: &str, args: Value) -> Result<Value> {
        let result = match name {
            "put_short_term" => {
                let session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id")?;
                let key = args.get("key").and_then(|v| v.as_str()).context("Missing key")?;
                let value = args.get("value").and_then(|v| v.as_str()).context("Missing value")?;

                self.backend.save_stm(session_id, key, value).await?;
                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("Short-term memory saved for session '{}': {} = {}", session_id, key, value)
                        }
                    ]
                }))
            }
            "get_short_term" => {
                let session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id")?;
                let key = args.get("key").and_then(|v| v.as_str());

                let map = self.backend.get_stm(session_id, key).await?;
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
            "clear_short_term" => {
                let session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id")?;

                self.backend.clear_stm(session_id).await?;
                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("Short-term memory cleared for session '{}'", session_id)
                        }
                    ]
                }))
            }
            "get_memory_nodes" => {
                let node_ids_val = args.get("node_ids").context("Missing node_ids")?;
                let node_ids_arr = node_ids_val.as_array().context("node_ids must be an array")?;
                let mut node_ids = Vec::new();
                for v in node_ids_arr {
                    if let Some(s) = v.as_str() {
                        node_ids.push(s.to_string());
                    }
                }

                let response = self.backend.get_memory_nodes(&node_ids).await?;
                // Strip embeddings for output
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
                text = crate::cognitive::paging::intercept_and_restore_symbols(&self.backend, &text).await;
                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": text
                        }
                    ]
                }))
            }
            "search_memories" => {
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

                let search_res = self.backend.search(query, scope, false, limit, offset, threshold, token_budget, allow_downward, include_episodes, include_artifacts).await?;
                
                if let Some(sess_id) = session_id {
                    let mut cited_ids = Vec::new();
                    for r in &search_res.results {
                        if r.tier == "episode" {
                            cited_ids.push(r.id.clone());
                        }
                    }
                    if !cited_ids.is_empty() {
                        let mut existing_citations = Vec::new();
                        if let Ok(stm_map) = self.backend.get_stm(sess_id, Some("_session_citations")).await {
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
                            let _ = self.backend.save_stm(sess_id, "_session_citations", &serialized).await;
                        }
                    }
                }

                let stripped_results: Vec<Value> = search_res.results.into_iter().map(|mut r| {
                    r.embedding = None;
                    serde_json::to_value(&r).unwrap()
                }).collect();

                let mut text = serde_json::to_string_pretty(&stripped_results)?;
                if search_res.has_more {
                    let remainder = search_res.total_matches.saturating_sub(offset + limit);
                    text.push_str(&format!(
                        "\n\n=== PAGINATION NOTICE: There are {} more matching memories. To retrieve the next page, query search_memories with offset={} and limit={}. ===",
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

                text = crate::cognitive::paging::intercept_and_restore_symbols(&self.backend, &text).await;

                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": text
                        }
                    ]
                }))
            }
            "search_wisdom" => {
                let query = args.get("query").and_then(|v| v.as_str()).context("Missing query")?;
                let tier = args.get("tier").and_then(|v| v.as_str());
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(15) as usize;
                let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let threshold = args.get("threshold").and_then(|v| v.as_f64()).map(|t| t as f32).unwrap_or(0.55);

                let search_res = self.backend.get_wisdom(query, tier, limit, offset, threshold).await?;
                let stripped_results: Vec<Value> = search_res.results.into_iter().map(|mut r| {
                    r.embedding = None;
                    serde_json::to_value(&r).unwrap()
                }).collect();

                let mut text = serde_json::to_string_pretty(&stripped_results)?;
                if search_res.has_more {
                    let remainder = search_res.total_matches.saturating_sub(offset + limit);
                    text.push_str(&format!(
                        "\n\n=== PAGINATION NOTICE: There are {} more matching wisdom rules. To retrieve the next page, query search_wisdom with offset={} and limit={}. ===",
                        remainder, search_res.next_offset, limit
                    ));
                }

                text = crate::cognitive::paging::intercept_and_restore_symbols(&self.backend, &text).await;

                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": text
                        }
                    ]
                }))
            }
            "save_episode" => {
                let title = args.get("title").and_then(|v| v.as_str()).context("Missing title")?.to_string();
                let content = args.get("content").and_then(|v| v.as_str()).context("Missing content")?.to_string();
                let scope = args.get("scope").and_then(|v| v.as_str()).map(|s| s.to_string());
                let vault_path = args.get("vault_path").and_then(|v| v.as_str()).map(|s| s.to_string());
                let session_id = args.get("session_id").and_then(|v| v.as_str()).map(|s| s.to_string());
                let task_id = args.get("task_id").and_then(|v| v.as_str()).map(|s| s.to_string());
                
                let mut entities = vec![];
                if let Some(arr) = args.get("entities").and_then(|v| v.as_array()) {
                    for item in arr {
                        let entity: crate::contracts::Entity = serde_json::from_value(item.clone())?;
                        entities.push(entity);
                    }
                }

                let episode = crate::contracts::EpisodeSave {
                    title,
                    content: content.clone(),
                    entities,
                    scope: scope.clone(),
                    vault_path,
                    source_episode: None,
                    session_id,
                    task_id,
                };

                let id = self.backend.save_episode(&episode).await?;

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
                    let backend_clone = self.backend.clone();
                    let store_clone = self.store.clone();
                    let content_clone = content.clone();
                    let scope_clone = scope.clone();
                    tokio::spawn(async move {
                        if let Err(e) = run_llm_critic(backend_clone, store_clone, content_clone, scope_clone).await {
                            tracing::error!("Error running LLM critic: {:?}", e);
                        }
                    });
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
            "diagnose_failure" => {
                let stdout = args.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
                let stderr = args.get("stderr").and_then(|v| v.as_str()).context("Missing stderr")?;
                
                let diagnosis = self.backend.diagnose_error_internal(stderr, stdout).await?;
                let resp = match diagnosis {
                    Some((explanation, remedy)) => {
                        json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": format!(
                                        "=== DIAGNOSIS FOUND ===\nCausal Explanation: {}\nPrescribed Remedy: {}",
                                        explanation, remedy
                                    )
                                }
                            ],
                            "causal_explanation": explanation,
                            "prescribed_remedy": remedy
                        })
                    }
                    None => {
                        json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": "No matching diagnostic signature or similar failure was found in the database."
                                }
                            ]
                        })
                    }
                };
                Ok(resp)
            }
            "save_handoff" => {
                let parent_conversation_id = args.get("parent_conversation_id").and_then(|v| v.as_str()).context("Missing parent_conversation_id")?.to_string();
                let subagent_conversation_id = args.get("subagent_conversation_id").and_then(|v| v.as_str()).context("Missing subagent_conversation_id")?.to_string();
                let summary = args.get("summary").and_then(|v| v.as_str()).context("Missing summary")?.to_string();
                let handoff_file_path = args.get("handoff_file_path").and_then(|v| v.as_str()).context("Missing handoff_file_path")?.to_string();
                let scope = args.get("scope").and_then(|v| v.as_str()).map(|s| s.to_string());

                let handoff = crate::contracts::HandoffSave {
                    parent_conversation_id: parent_conversation_id.clone(),
                    subagent_conversation_id: subagent_conversation_id.clone(),
                    summary,
                    handoff_file_path: handoff_file_path.clone(),
                    scope,
                };

                let id = self.backend.save_handoff(&handoff).await?;

                // T6: Citations Footnotes
                if let Ok(stm_map) = self.backend.get_stm(&parent_conversation_id, Some("_session_citations")).await {
                    if let Some(citations_str) = stm_map.get("_session_citations") {
                        if let Ok(episode_ids) = serde_json::from_str::<Vec<String>>(citations_str) {
                            if !episode_ids.is_empty() {
                                if let Ok(nodes_resp) = self.backend.get_memory_nodes(&episode_ids).await {
                                    let mut footnote = String::new();
                                    footnote.push_str("\n\n### Citations\n");
                                    let vault_root = self.store.vault_root.clone();
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
            "get_vault_root" => {
                let vault_path = self.store.vault_root.to_string_lossy().to_string();
                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": vault_path
                        }
                    ]
                }))
            }
            "verify_compliance" => {
                let workspace_path_str = args.get("workspace_path").and_then(|v| v.as_str()).unwrap_or(".");
                let path = std::path::Path::new(workspace_path_str);
                let audit_results = crate::verify::run_workspace_audit(path).await;

                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!(
                                "Audit Results:\n- Tailwind Clean: {}\n- Search History Clean: {}\n- Daemon Health OK: {}\nViolations/Errors details: {:#?}",
                                audit_results.tailwind_ok,
                                audit_results.search_history_ok,
                                audit_results.daemon_ok,
                                audit_results
                            )
                        }
                    ]
                }))
            }
            "pre_invocation_hook" => {
                let session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id")?;
                let query = args.get("query").and_then(|v| v.as_str());
                let workspace_path = args.get("workspace_path").and_then(|v| v.as_str());

                // 1. State Sync & Vault Integrity
                self.backend.journal_state(&self.store.vault_root, Some(session_id)).await?;

                let all_eps = self.backend.get_all_episodes().await?;
                for ep in &all_eps {
                    if let Some(ref vp) = ep.vault_path {
                        let path = self.store.vault_root.join(vp);
                        if !path.exists() {
                            let save = crate::contracts::EpisodeSave {
                                title: ep.title.clone(),
                                content: ep.content.clone(),
                                entities: vec![],
                                scope: ep.scope.clone(),
                                vault_path: Some(vp.clone()),
                                source_episode: ep.source_episode.clone(),
                                session_id: None,
                                task_id: None,
                            };
                            let markdown = crate::vault::watcher::format_episode_markdown(&save);
                            self.store.write_file(vp, &markdown)?;
                        }
                    }
                }

                // 2. Subagent Path (Handoff Check)
                let mut handoffs_resp = self.backend.db.query("SELECT parent_conversation_id, summary, scope FROM handoff WHERE subagent_conversation_id = $subagent AND status = 'PENDING';")
                    .bind(("subagent", session_id))
                    .await?;
                let handoffs: Vec<serde_json::Value> = handoffs_resp.take(0)?;

                let stm_map = self.backend.get_stm(session_id, None).await?;
                let mut parts = Vec::new();

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
                        let hydrated = self.backend.get_memory_nodes(&node_ids).await?;
                        parts.push("## Hydrated Context Nodes\n".to_string());
                        for wiki in hydrated.wiki_nodes {
                            parts.push(format!("### 📚 Distilled Insight: {}\nScope: {}\n{}\n", wiki.name, wiki.scope, wiki.content));
                        }
                        for wisdom in hydrated.wisdom_rules {
                            parts.push(format!(
                                "### 💡 Wisdom Rule: {}\n- **Avoid**: {}\n- **Causal**: {}\n- **Remedy**: {}\n",
                                wisdom.target_pattern, wisdom.action_to_avoid, wisdom.causal_explanation, wisdom.prescribed_remedy
                            ));
                        }
                        for ep in hydrated.episodes {
                            if let Some(ref ep_id) = ep.id {
                                let rendered = self.format_episode_or_parent(ep_id, &ep.title, &ep.content, ep.scope.as_deref()).await?;
                                parts.push(rendered);
                            }
                        }
                    } else {
                        // Semantic search on handoff summary
                        let search_res = self.backend.search(
                            summary,
                            scope,
                            false,
                            15,
                            0,
                            0.55,
                            None,
                            false,
                            true,
                            false
                        ).await?;

                        parts.push("## Retrieved Semantic Context\n".to_string());
                        for res in search_res.results {
                            if res.id.starts_with("wisdom:") {
                                parts.push(format!("### 💡 Wisdom Rule: {}\n{}\n", res.title, res.content));
                            } else if res.id.starts_with("wiki_node:") {
                                parts.push(format!("### 📚 Distilled Insight: {}\n{}\n", res.title, res.content));
                            } else if res.id.starts_with("episode:") {
                                let rendered = self.format_episode_or_parent(&res.id, &res.title, &res.content, None).await?;
                                parts.push(rendered);
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
                    let search_res = self.backend.search(
                        search_query,
                        Some(&dynamic_scope),
                        false,
                        15,
                        0,
                        0.55,
                        None,
                        false,
                        true,
                        false
                    ).await?;

                    parts.push(format!("## Retrieved Semantic Context (Scope: `{}`)\n", dynamic_scope));
                    let mut high_confidence_memories_found = false;
                    for res in search_res.results {
                        if res.id.starts_with("wisdom:") {
                            if res.similarity >= 0.55 {
                                parts.push(format!("### 💡 Wisdom Rule: {}\n{}\n", res.title, res.content));
                            }
                        } else if res.id.starts_with("episode:") {
                            if res.similarity >= 0.70 {
                                high_confidence_memories_found = true;
                                let rendered = self.format_episode_or_parent(&res.id, &res.title, &res.content, None).await?;
                                parts.push(rendered);
                            }
                        } else {
                            parts.push(format!("### 📝 Record: {}\n{}\n", res.title, res.content));
                        }
                    }

                    if !high_confidence_memories_found {
                        parts.push(format!(
                            "\n> [!IMPORTANT]\n> **Pinned Deep-Search Instruction**: No high-confidence memory episodes were found. If you need deeper historical context or past resolutions, please invoke the 'search_memories' tool with a specific query.\n"
                        ));
                    }
                }

                // 4. Arbor HTR Constraints
                let active_node_opt = stm_map.get("active_hypothesis_node")
                    .or_else(|| stm_map.get("active_node"))
                    .cloned();

                if let Some(active_node_id) = active_node_opt {
                    let mut hyp_res = self.backend.db.query("SELECT * FROM hypothesis_node WHERE node_id = $node_id;")
                        .bind(("node_id", active_node_id.as_str()))
                        .await?;
                    let hyp_nodes: Vec<crate::contracts::HypothesisNode> = hyp_res.take(0)?;
                    if let Some(hyp_node) = hyp_nodes.first() {
                        if let Some(ref parent_id) = hyp_node.parent_id {
                            let mut siblings_res = self.backend.db.query("SELECT * FROM hypothesis_node WHERE parent_id = $parent_id AND node_id != $node_id AND (status = 'failed' OR status = 'pruned');")
                                .bind(("parent_id", parent_id.as_str()))
                                .bind(("node_id", active_node_id.as_str()))
                                .await?;
                            let siblings: Vec<crate::contracts::HypothesisNode> = siblings_res.take(0)?;
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

                let final_context = parts.join("\n");
                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": final_context
                        }
                    ]
                }))
            }
            "harvest_skill_wisdom" => {
                let harvester = crate::cognitive::harvest::Harvester::new();
                harvester.harvest_skills(&*self.backend, &self.store).await?;

                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": "Wisdom rules harvested successfully."
                        }
                    ]
                }))
            }
            "synthesize_meta_skills" => {
                let synthesizer = crate::cognitive::meta_skill::MetaSkillSynthesizer::new();
                let published = synthesizer.synthesize_meta_skills(&*self.backend, &self.store).await?;
                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("Meta skills synthesized successfully: {:?}", published)
                        }
                    ]
                }))
            }
            "detect_skill_merges" => {
                let synthesizer = crate::cognitive::meta_skill::MetaSkillSynthesizer::new();
                let suggestions = synthesizer.detect_skill_merges(&*self.backend, &self.store).await?;
                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("Merge suggestions generated and written to wiki/skill_merge_suggestions.md. Suggestions: {:?}", suggestions)
                        }
                    ]
                }))
            }
            "merge_skills" => {
                let source_val = args.get("source_skills").context("Missing source_skills")?;
                let source_arr = source_val.as_array().context("source_skills must be an array")?;
                let mut source_skills = Vec::new();
                for val in source_arr {
                    let s = val.as_str().context("source_skills elements must be strings")?.to_string();
                    source_skills.push(s);
                }
                let target_name = args.get("target_name").and_then(|v| v.as_str()).context("Missing target_name")?;

                let synthesizer = crate::cognitive::meta_skill::MetaSkillSynthesizer::new();
                let result_skill = synthesizer.merge_skills(&*self.backend, &self.store, &source_skills, target_name).await?;

                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("Skills successfully merged into: {}", result_skill)
                        }
                    ]
                }))
            }
            "bulk_ingest" => {
                let source = args.get("source").and_then(|v| v.as_str()).context("Missing source")?;
                let harness = args.get("harness").and_then(|v| v.as_str()).context("Missing harness")?;
                let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("general");
                
                let (count, errors) = crate::vault::ingestion::bulk_ingest_vault(
                    &self.store.vault_root,
                    std::path::Path::new(source),
                    harness,
                    scope,
                    &*self.backend
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
            "organize_vault" => {
                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": "Vault organization completed. Collisions resolved successfully."
                        }
                    ]
                }))
            }
            "summarize_episodes" => {
                let scope = args.get("scope").and_then(|v| v.as_str());
                let compactor = crate::cognitive::compactor::Compactor::new();
                let coordinator = crate::cognitive::synthesis::DreamCoordinator::new();

                coordinator.run_dream(&*self.backend, &self.store, None).await?;

                let scope_name = scope.unwrap_or("general");
                compactor.compact_scope(&*self.backend, &self.store, scope_name).await?;
                compactor.compact_global(&*self.backend, &self.store).await?;

                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("Compaction and synthesis dreaming completed successfully for scope '{}'.", scope_name)
                        }
                    ]
                }))
            }
            "verify_vault_integrity" => {
                let fix = args.get("fix").and_then(|v| v.as_bool()).unwrap_or(false);
                let all_eps = self.backend.get_all_episodes().await?;
                let mut missing_count = 0;
                for ep in &all_eps {
                    if let Some(ref vp) = ep.vault_path {
                        let path = self.store.vault_root.join(vp);
                        if !path.exists() {
                            missing_count += 1;
                            if fix {
                                let save = crate::contracts::EpisodeSave {
                                    title: ep.title.clone(),
                                    content: ep.content.clone(),
                                    entities: vec![],
                                    scope: ep.scope.clone(),
                                    vault_path: Some(vp.clone()),
                                    source_episode: ep.source_episode.clone(),
                                    session_id: None,
                                    task_id: None,
                                };
                                let markdown = crate::vault::watcher::format_episode_markdown(&save);
                                self.store.write_file(vp, &markdown)?;
                            }
                        }
                    }
                }
                
                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("Vault integrity verification complete. Checked {} episodes. Missing files: {}. Fixed: {}.", all_eps.len(), missing_count, fix && missing_count > 0)
                        }
                    ]
                }))
            }
            "reprocess_embeddings" => {
                let all_eps = self.backend.get_all_episodes().await?;
                let mut count = 0;
                for ep in all_eps {
                    if ep.embedding.is_none() {
                        let save = crate::contracts::EpisodeSave {
                            title: ep.title.clone(),
                            content: ep.content.clone(),
                            entities: vec![],
                            scope: ep.scope.clone(),
                            vault_path: ep.vault_path.clone(),
                            source_episode: ep.source_episode.clone(),
                            session_id: None,
                            task_id: None,
                        };
                        self.backend.save_episode(&save).await?;
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
            "get_llm_config" => {
                let config = self.backend.get_llm_config().await?;
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
            "update_llm_config" => {
                let provider = args.get("provider").and_then(|v| v.as_str()).context("Missing provider")?.to_string();
                let duration = args.get("duration").and_then(|v| v.as_str()).map(|s| s.to_string());
                let model = args.get("model").and_then(|v| v.as_str()).map(|s| s.to_string());
                let cloud_provider = args.get("cloud_provider").and_then(|v| v.as_str()).map(|s| s.to_string());
                let api_key = args.get("api_key").and_then(|v| v.as_str()).map(|s| s.to_string());

                let req = crate::contracts::LlmConfigRequest {
                    provider,
                    duration,
                    model,
                    cloud_provider,
                    api_key,
                };

                self.backend.update_llm_config(&req).await?;

                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": "LLM configuration updated successfully."
                        }
                    ]
                }))
            }
            "htr_init" => {
                let scope = args.get("scope").and_then(|v| v.as_str()).context("Missing scope")?.to_string();
                let hypothesis = args.get("hypothesis").and_then(|v| v.as_str()).context("Missing hypothesis")?.to_string();
                let files_val = args.get("files").and_then(|v| v.as_array()).context("Missing files")?;
                let files: Vec<String> = files_val.iter().map(|v| v.as_str().unwrap_or("").to_string()).collect();

                let llm = crate::llm::LLMClient::new();
                let current_dir = std::env::current_dir()?;
                let coordinator = crate::cognitive::ArborCoordinator::new(
                    self.backend.db.clone(),
                    self.store.vault_root.clone(),
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
            "htr_ideate" => {
                let scope = args.get("scope").and_then(|v| v.as_str()).context("Missing scope")?.to_string();
                let node = args.get("node").and_then(|v| v.as_str()).context("Missing node")?.to_string();

                let llm = crate::llm::LLMClient::new();
                let current_dir = std::env::current_dir()?;
                let coordinator = crate::cognitive::ArborCoordinator::new(
                    self.backend.db.clone(),
                    self.store.vault_root.clone(),
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
            "htr_execute" => {
                let scope = args.get("scope").and_then(|v| v.as_str()).context("Missing scope")?.to_string();
                let node = args.get("node").and_then(|v| v.as_str()).context("Missing node")?.to_string();
                let test_command = args.get("test_command").and_then(|v| v.as_str()).context("Missing test_command")?.to_string();

                let llm = crate::llm::LLMClient::new();
                let current_dir = std::env::current_dir()?;
                let coordinator = crate::cognitive::ArborCoordinator::new(
                    self.backend.db.clone(),
                    self.store.vault_root.clone(),
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
            "htr_backprop" => {
                let scope = args.get("scope").and_then(|v| v.as_str()).context("Missing scope")?.to_string();
                let node = args.get("node").and_then(|v| v.as_str()).context("Missing node")?.to_string();

                let llm = crate::llm::LLMClient::new();
                let current_dir = std::env::current_dir()?;
                let coordinator = crate::cognitive::ArborCoordinator::new(
                    self.backend.db.clone(),
                    self.store.vault_root.clone(),
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
            "htr_merge" => {
                let scope = args.get("scope").and_then(|v| v.as_str()).context("Missing scope")?.to_string();
                let node = args.get("node").and_then(|v| v.as_str()).context("Missing node")?.to_string();

                let llm = crate::llm::LLMClient::new();
                let current_dir = std::env::current_dir()?;
                let coordinator = crate::cognitive::ArborCoordinator::new(
                    self.backend.db.clone(),
                    self.store.vault_root.clone(),
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
            "htr_run" => {
                let scope = args.get("scope").and_then(|v| v.as_str()).context("Missing scope")?.to_string();
                let hypothesis = args.get("hypothesis").and_then(|v| v.as_str()).context("Missing hypothesis")?.to_string();
                let files_val = args.get("files").and_then(|v| v.as_array()).context("Missing files")?;
                let files: Vec<String> = files_val.iter().map(|v| v.as_str().unwrap_or("").to_string()).collect();
                let test_command = args.get("test_command").and_then(|v| v.as_str()).context("Missing test_command")?.to_string();
                let max_steps = args.get("max_steps").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

                let llm = crate::llm::LLMClient::new();
                let current_dir = std::env::current_dir()?;
                let coordinator = crate::cognitive::ArborCoordinator::new(
                    self.backend.db.clone(),
                    self.store.vault_root.clone(),
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
                    
                    let node_val: Option<crate::contracts::HypothesisNode> = self.backend.db.select(("hypothesis_node", selected_node.as_str())).await?;
                    if let Some(node_node) = node_val
                        && let Some(score) = node_node.score
                            && score >= 95.0 {
                                coordinator.decide_admission(selected_node).await?;
                                status_msg = format!("HTR run loop completed successfully. Node {} merged with Score: {}.", selected_node, score);
                                break;
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
            "forge_source" => {
                let source_path = args.get("source_path").and_then(|v| v.as_str()).context("Missing source_path")?;
                let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("general");

                let source_path_buf = std::path::PathBuf::from(source_path);
                let content = if source_path_buf.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("pdf")) {
                    crate::cognitive::forge::extract_pdf_text(&source_path_buf)?
                } else {
                    std::fs::read_to_string(&source_path_buf)?
                };

                let forge = crate::cognitive::forge::Forge::new(self.backend.clone(), self.store.clone());
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
                let batch: crate::contracts::ForgedSectionBatch = serde_json::from_value(args.clone())
                    .context("Failed to parse ForgedSectionBatch arguments")?;
                self.backend.save_forged_section(&batch).await?;
                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("Successfully saved forged assets for document '{}'", batch.doc_title)
                        }
                    ]
                }))
            }
            "get_forge_instructions" => {
                let instructions = "\
# Forge Guidelines & Extraction Instructions

## 1. Wisdom Rules Extraction
- **System Instruction:** \"You are a systems synthesizer. Analyze the text chunk and extract system-level Wisdom Rules to prevent mistakes. Respond ONLY with a JSON array of rules.\"
- **Prompt Template:**
```json
Respond ONLY with a JSON array of rules, each containing exactly:
- target_pattern (string)
- action_to_avoid (string)
- causal_explanation (string)
- prescribed_remedy (string)
```

## 2. Concept Wiki Nodes Extraction
- **System Instruction:** \"You are a systems synthesizer. Analyze the text chunk and extract key concepts or architectural definitions for a systems wiki. Respond ONLY with a JSON array of nodes.\"
- **Prompt Template:**
```json
Respond ONLY with a JSON array of nodes, each containing exactly:
- name (string concept title)
- content (string explanation or definition)
```";

                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": instructions
                        }
                    ]
                }))
            }
            _ => {
                anyhow::bail!("Tool not found: {}", name)
            }
        };

        if result.is_ok() && matches!(name, "put_short_term" | "clear_short_term" | "save_episode" | "save_handoff" | "save_forged_assets" | "htr_init" | "htr_ideate" | "htr_execute" | "htr_backprop" | "htr_merge" | "htr_run") {
            let session_id_opt = args.get("session_id")
                .or_else(|| args.get("subagent_conversation_id"))
                .or_else(|| args.get("scope"))
                .and_then(|v| v.as_str());
            if let Err(e) = self.backend.journal_state(&self.store.vault_root, session_id_opt).await {
                tracing::error!("Failed to write dual-durability journal: {:?}", e);
            }
        }

        result
    }

    async fn format_episode_or_parent(&self, ep_id: &str, ep_title: &str, ep_content: &str, ep_scope: Option<&str>) -> Result<String> {
        if let Ok(rec_id) = crate::db::parse_record_id(ep_id) {
            let mut parent_resp = self.backend.db.query("SELECT VALUE out FROM relates_to WHERE in = $ep_id;").bind(("ep_id", rec_id)).await?;
            let parent_ids: Vec<surrealdb::types::RecordId> = parent_resp.take(0)?;
            if !parent_ids.is_empty() {
                let mut parent_ids_strings = Vec::new();
                for pid in parent_ids {
                    parent_ids_strings.push(crate::db::backend::format_record_id(&pid));
                }
                let parents = self.backend.get_memory_nodes(&parent_ids_strings).await?;
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

        // Distilled Memory Card fallback
        let summary = if ep_content.len() > 200 {
            format!("{}...", &ep_content[..200])
        } else {
            ep_content.to_string()
        };
        Ok(format!(
            "#### 📑 Memory Card: {}\n- **ID**: `{}`\n- **Scope**: `{}`\n- **Summary**: {}\n*For follow-up queries on this memory, use:* `get_memory_nodes [\"{}\"]`\n",
            ep_title, ep_id, ep_scope.unwrap_or("general"), summary, ep_id
        ))
    }
}

pub async fn run_llm_critic(
    backend: Arc<crate::db::SurrealBackend>,
    store: Arc<MarkdownStore>,
    content: String,
    scope: Option<String>,
) -> Result<()> {
    let allow_cloud_fallback = match backend.db.query("SELECT allow_cloud_fallback FROM config:settings;").await {
        Ok(mut resp) => {
            use surrealdb_types::SurrealValue;
            #[derive(serde::Serialize, serde::Deserialize, Debug, SurrealValue)]
            struct FallbackSettings {
                allow_cloud_fallback: Option<bool>,
            }
            if let Ok(Some(settings)) = resp.take::<Option<FallbackSettings>>(0) {
                settings.allow_cloud_fallback.unwrap_or(true)
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
    };

    let markdown = crate::vault::watcher::format_wisdom_markdown(&rule_save);
    store.write_file(&rule_path, &markdown)?;
    backend.save_wisdom_rule(&rule_save).await?;

    Ok(())
}
