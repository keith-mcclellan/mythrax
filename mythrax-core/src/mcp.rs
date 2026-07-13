use std::io::{BufRead, Write};
use serde_json::{json, Value};
use anyhow::{Result, Context};
use std::sync::Arc;
use crate::db::StorageBackend;
use crate::store::MarkdownStore;

pub use crate::mcp_routes::run_llm_critic; // Re-export for E0425 in tests

pub struct LocalState {
    pub backend: Arc<dyn StorageBackend>,
    pub store: Arc<MarkdownStore>,
}

pub struct McpServer {
    auth_token: String,
    daemon_url: String,
    local_state: Option<LocalState>,
    http_client: reqwest::Client,
}

impl McpServer {
    pub fn new(auth_token: String, daemon_url: String) -> Self {
        Self {
            auth_token,
            daemon_url,
            local_state: None,
            http_client: reqwest::Client::new(),
        }
    }

    pub fn new_local(backend: Arc<dyn StorageBackend>, store: Arc<MarkdownStore>) -> Self {
        Self {
            auth_token: String::new(),
            daemon_url: String::new(),
            local_state: Some(LocalState { backend, store }),
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn run(&self) -> Result<()> {
        // 1. Check if daemon is active, auto-spawn if not (only in proxy mode)
        if self.local_state.is_none() {
            self.ensure_daemon_active().await?;
        }

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

    async fn ensure_daemon_active(&self) -> Result<()> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(1))
            .build()?;
        
        let ping_url = format!("{}/v1/config/llm", self.daemon_url);
        
        // Try to ping the daemon
        let ping = client.get(&ping_url)
            .header("X-Mythrax-Token", &self.auth_token)
            .send()
            .await;
        
        if let Ok(resp) = ping {
            if resp.status() == reqwest::StatusCode::OK {
                return Ok(());
            }
        }
        
        // If inactive, spawn daemon in background
        eprintln!("Daemon inactive. Spawning background daemon process...");
        let current_exe = std::env::current_exe()
            .unwrap_or_else(|_| std::path::PathBuf::from("mythrax"));
        
        let home = std::env::var("HOME").context("HOME env var not set")?;
        let mythrax_dir = std::path::PathBuf::from(&home).join(".mythrax");
        std::fs::create_dir_all(&mythrax_dir)?;
        let log_file = mythrax_dir.join("daemon.log");
        let pid_file = mythrax_dir.join("daemon.pid");
        
        let mut cmd = std::process::Command::new(&current_exe);
        cmd.arg("daemon").arg("start");
        
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)
        {
            let stdout_stdio = file.try_clone()
                .map(std::process::Stdio::from)
                .unwrap_or_else(|_| std::process::Stdio::null());
            cmd.stdout(stdout_stdio);
            cmd.stderr(file);
        } else {
            cmd.stdout(std::process::Stdio::null());
            cmd.stderr(std::process::Stdio::null());
        }
        
        match cmd.spawn() {
            Ok(child) => {
                let pid = child.id();
                let _ = std::fs::write(&pid_file, pid.to_string());
                
                // Poll port 8090 for up to 15 seconds
                let poll_client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_millis(200))
                    .build()?;
                
                let start_time = std::time::Instant::now();
                let timeout = std::time::Duration::from_secs(15);
                let mut healthy = false;
                
                while start_time.elapsed() < timeout {
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    let poll_ping = poll_client.get(&ping_url)
                        .header("X-Mythrax-Token", &self.auth_token)
                        .send()
                        .await;
                    if let Ok(resp) = poll_ping {
                        if resp.status() == reqwest::StatusCode::OK {
                            healthy = true;
                            break;
                        }
                    }
                }
                
                if !healthy {
                    anyhow::bail!("Timed out waiting for daemon to start on {}", self.daemon_url);
                }
                
                eprintln!("Daemon successfully spawned (PID: {}) and bound to port 8090.", pid);
                Ok(())
            }
            Err(e) => {
                anyhow::bail!("Failed to spawn daemon process: {:?}", e);
            }
        }
    }

    pub async fn call_tool(&self, name: &str, args: Value) -> Result<Value> {
        // Handle hardcoded tool responses
        if name == "get_forge_instructions" {
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
            return Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": instructions
                    }
                ]
            }));
        }

        if let Some(ref local) = self.local_state {
            let api_state = crate::api::ApiState {
                backend: local.backend.clone(),
                store: local.store.clone(),
                auth_token: "".to_string(),
                ignore_list: std::sync::Arc::new(crate::vault::watcher::WatchIgnoreList::new()),
                dream_tx: None,
                shutdown_tx: None,
            };
            crate::mcp_routes::call_mcp_tool(&api_state, name, args).await
        } else {
            let url = format!("{}/v1/mcp/call", self.daemon_url);
            let payload = json!({
                "name": name,
                "arguments": args
            });
            let resp = self.http_client.post(&url)
                .header("X-Mythrax-Token", &self.auth_token)
                .json(&payload)
                .send()
                .await
                .context("Failed to contact daemon call endpoint")?;
            if resp.status() != reqwest::StatusCode::OK {
                anyhow::bail!("Daemon returned error status calling tool '{}': {}", name, resp.status());
            }
            resp.json().await.map_err(Into::into)
        }
    }

    pub async fn handle_request(&self, method: &str, params: Value) -> Result<Value> {
        if let Some(ref _local) = self.local_state {
            match method {
                "initialize" => {
                    Ok(json!({
                        "protocolVersion": "2024-11-05",
                        "capabilities": {
                            "tools": {},
                            "resources": {}
                        },
                        "serverInfo": {
                            "name": "mythrax",
                            "version": "0.5.0"
                        }
                    }))
                }
                "tools/list" => {
                    let schema = crate::mcp_routes::get_mcp_tools_schema();
                    Ok(schema)
                }
                "tools/call" => {
                    let name = params.get("name").and_then(|v| v.as_str()).context("Missing tool name in tools/call")?;
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    self.call_tool(name, arguments).await
                }
                "tools/call_batch" => {
                    let calls_val = if params.is_array() {
                        &params
                    } else {
                        params.get("calls").ok_or_else(|| anyhow::anyhow!("Missing 'calls' parameter"))?
                    };
                    let calls_arr = calls_val.as_array().ok_or_else(|| anyhow::anyhow!("'calls' must be an array"))?;
                    let mut futures = Vec::new();
                    for call in calls_arr {
                        let name = call.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let arguments = call.get("arguments").or_else(|| call.get("args")).cloned().unwrap_or(Value::Null);
                        let self_ref = self;
                        futures.push(async move {
                            match self_ref.call_tool(&name, arguments).await {
                                Ok(res) => json!({ "status": "success", "result": res }),
                                Err(e) => json!({ "status": "error", "message": e.to_string() }),
                            }
                        });
                    }
                    let results = futures_util::future::join_all(futures).await;
                    Ok(Value::Array(results))
                }
                "resources/list" => {
                    Ok(json!({
                        "resources": [
                            {
                                "uri": "htr://tree",
                                "name": "Active Hypothesis Tree",
                                "description": "Structured JSON representation of the active hypothesis tree.",
                                "mimeType": "application/json"
                            }
                        ]
                    }))
                }
                "resources/read" => {
                    let uri = params.get("uri").and_then(|v| v.as_str()).context("Missing uri in resources/read")?;
                    if uri == "htr://tree" {
                        let tree_str = get_htr_tree_json(_local.backend.as_ref()).await?;
                        Ok(json!({
                            "contents": [
                                {
                                    "uri": "htr://tree",
                                    "mimeType": "application/json",
                                    "text": tree_str
                                }
                            ]
                        }))
                    } else {
                        anyhow::bail!("Resource not found: {}", uri)
                    }
                }
                _ => anyhow::bail!("Method not found: {}", method),
            }
        } else {
            match method {
                "initialize" => {
                    if let Some(root_uri_str) = params.get("rootUri").and_then(|v| v.as_str()) {
                        if let Ok(url) = url::Url::parse(root_uri_str) {
                            if let Ok(path) = url.to_file_path() {
                                crate::store::set_workspace_root(path);
                            }
                        }
                    }
                    Ok(json!({
                        "protocolVersion": "2024-11-05",
                        "capabilities": {
                            "tools": {},
                            "resources": {}
                        },
                        "serverInfo": {
                            "name": "mythrax",
                            "version": "0.5.0"
                        }
                    }))
                }
                "tools/list" => {
                    let url = format!("{}/v1/mcp/tools", self.daemon_url);
                    let resp = self.http_client.get(&url)
                        .header("X-Mythrax-Token", &self.auth_token)
                        .send()
                        .await
                        .context("Failed to contact daemon tools endpoint")?;
                    
                    if resp.status() != reqwest::StatusCode::OK {
                        anyhow::bail!("Daemon returned error status listing tools: {}", resp.status());
                    }
                    
                    let tools_list: Value = resp.json().await.context("Failed to parse daemon tools JSON")?;
                    Ok(tools_list)
                }
                "tools/call" => {
                    let name = params.get("name").and_then(|v| v.as_str()).context("Missing tool name in tools/call")?;
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    
                    let url = format!("{}/v1/mcp/call", self.daemon_url);
                    
                    let payload = json!({
                        "name": name,
                        "arguments": arguments
                    });
                    
                    let resp = self.http_client.post(&url)
                        .header("X-Mythrax-Token", &self.auth_token)
                        .json(&payload)
                        .send()
                        .await
                        .context("Failed to contact daemon call endpoint")?;
                    
                    if resp.status() != reqwest::StatusCode::OK {
                        anyhow::bail!("Daemon returned error status calling tool '{}': {}", name, resp.status());
                    }
                    
                    let call_result: Value = resp.json().await.context("Failed to parse daemon tool call result JSON")?;
                    Ok(call_result)
                }
                "tools/call_batch" => {
                    let calls_val = if params.is_array() {
                        &params
                    } else {
                        params.get("calls").ok_or_else(|| anyhow::anyhow!("Missing 'calls' parameter"))?
                    };
                    let calls_arr = calls_val.as_array().ok_or_else(|| anyhow::anyhow!("'calls' must be an array"))?;
                    let mut futures = Vec::new();
                    for call in calls_arr {
                        let name = call.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let arguments = call.get("arguments").or_else(|| call.get("args")).cloned().unwrap_or(Value::Null);
                        let self_ref = self;
                        futures.push(async move {
                            match self_ref.call_tool(&name, arguments).await {
                                Ok(res) => json!({ "status": "success", "result": res }),
                                Err(e) => json!({ "status": "error", "message": e.to_string() }),
                            }
                        });
                    }
                    let results = futures_util::future::join_all(futures).await;
                    Ok(Value::Array(results))
                }
                "resources/list" => {
                    let url = format!("{}/v1/mcp/resources", self.daemon_url);
                    let resp = self.http_client.get(&url)
                        .header("X-Mythrax-Token", &self.auth_token)
                        .send()
                        .await
                        .context("Failed to contact daemon resources endpoint")?;
                    if resp.status() != reqwest::StatusCode::OK {
                        anyhow::bail!("Daemon returned error status listing resources: {}", resp.status());
                    }
                    let resources_list: Value = resp.json().await.context("Failed to parse daemon resources JSON")?;
                    Ok(resources_list)
                }
                "resources/read" => {
                    let uri = params.get("uri").and_then(|v| v.as_str()).context("Missing uri in resources/read")?;
                    let url = format!("{}/v1/mcp/resources/read", self.daemon_url);
                    let payload = json!({ "uri": uri });
                    let resp = self.http_client.post(&url)
                        .header("X-Mythrax-Token", &self.auth_token)
                        .json(&payload)
                        .send()
                        .await
                        .context("Failed to contact daemon resources/read endpoint")?;
                    if resp.status() != reqwest::StatusCode::OK {
                        anyhow::bail!("Daemon returned error status reading resource '{}': {}", uri, resp.status());
                    }
                    let read_result: Value = resp.json().await.context("Failed to parse daemon resource read result JSON")?;
                    Ok(read_result)
                }
                _ => anyhow::bail!("Method not found: {}", method),
            }
        }
    }
}

async fn get_htr_tree_json(backend: &dyn StorageBackend) -> Result<String> {
    let sql = "SELECT * FROM hypothesis_node;";
    let surreal_backend = backend.as_any().downcast_ref::<crate::db::SurrealBackend>()
        .context("SurrealBackend required")?;
    let mut response = surreal_backend.db.query(sql).await?.check()?;
    let nodes: Vec<crate::contracts::HypothesisNode> = response.take(0)?;
    let tree_json = serde_json::to_string_pretty(&nodes)?;
    Ok(tree_json)
}
