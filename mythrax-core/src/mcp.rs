use std::io::{BufRead, Write};
use std::sync::Arc;
use serde_json::{json, Value};
use crate::db::StorageBackend;
use crate::store::MarkdownStore;
use anyhow::{Result, Context};

pub struct McpServer {
    backend: Arc<dyn StorageBackend>,
    store: Arc<MarkdownStore>,
}

impl McpServer {
    pub fn new(backend: Arc<dyn StorageBackend>, store: Arc<MarkdownStore>) -> Self {
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
                                    "limit": { "type": "integer" }
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
                                    "limit": { "type": "integer" }
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
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

                let results = self.backend.search(query, scope, false, limit, 0).await?;
                let stripped_results: Vec<Value> = results.into_iter().map(|mut r| {
                    r.embedding = None;
                    serde_json::to_value(&r).unwrap()
                }).collect();

                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": serde_json::to_string_pretty(&stripped_results)?
                        }
                    ]
                }))
            }
            "search_wisdom" => {
                let query = args.get("query").and_then(|v| v.as_str()).context("Missing query")?;
                let tier = args.get("tier").and_then(|v| v.as_str()).context("Missing tier")?;
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

                let results = self.backend.get_wisdom(query, tier, limit).await?;
                let stripped_results: Vec<Value> = results.into_iter().map(|mut r| {
                    r.embedding = None;
                    serde_json::to_value(&r).unwrap()
                }).collect();

                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": serde_json::to_string_pretty(&stripped_results)?
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
                harvester.harvest_skills(&*self.backend, &*self.store).await?;

                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": "Wisdom rules harvested successfully."
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
