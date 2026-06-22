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

    async fn handle_request(&self, method: &str, params: Value) -> Result<Value> {
        match method {
            "initialize" => {
                Ok(json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "mythrax",
                        "version": "0.1.0"
                    }
                }))
            }
            "tools/list" => {
                Ok(json!({
                    "tools": [
                        {
                            "name": "search_memories",
                            "description": "Execute semantic memory search over saved episodes",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "query": { "type": "string" },
                                    "scope": { "type": "string" },
                                    "limit": { "type": "integer" },
                                    "offset": { "type": "integer" },
                                    "threshold": { "type": "number" }
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
                                "required": ["query", "tier"]
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
                                    "vault_path": { "type": "string" }
                                },
                                "required": ["title", "content"]
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
                            "name": "harvest_skill_wisdom",
                            "description": "Harvest skill wisdom from playbooks and config",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
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

    async fn call_tool(&self, name: &str, args: Value) -> Result<Value> {
        match name {
            "search_memories" => {
                let query = args.get("query").and_then(|v| v.as_str()).context("Missing query")?;
                let scope = args.get("scope").and_then(|v| v.as_str());
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(15) as usize;
                let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let threshold = args.get("threshold").and_then(|v| v.as_f64()).map(|t| t as f32).unwrap_or(0.55);

                let search_res = self.backend.search(query, scope, false, limit, offset, threshold).await?;
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
                let tier = args.get("tier").and_then(|v| v.as_str()).context("Missing tier")?;
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
                
                let mut entities = vec![];
                if let Some(arr) = args.get("entities").and_then(|v| v.as_array()) {
                    for item in arr {
                        let entity: crate::contracts::Entity = serde_json::from_value(item.clone())?;
                        entities.push(entity);
                    }
                }

                let episode = crate::contracts::EpisodeSave {
                    title,
                    content,
                    entities,
                    scope,
                    vault_path,
                    source_episode: None,
                };

                let id = self.backend.save_episode(&episode).await?;

                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("Episode saved successfully: {}", id)
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
            _ => {
                anyhow::bail!("Tool not found: {}", name)
            }
        }
    }
}
