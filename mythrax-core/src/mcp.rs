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
}

impl McpServer {
    pub fn new(auth_token: String, daemon_url: String) -> Self {
        Self {
            auth_token,
            daemon_url,
            local_state: None,
        }
    }

    pub fn new_local(backend: Arc<dyn StorageBackend>, store: Arc<MarkdownStore>) -> Self {
        Self {
            auth_token: String::new(),
            daemon_url: String::new(),
            local_state: Some(LocalState { backend, store }),
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
                
                // Poll port 8090 for up to 5 seconds
                let poll_client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_millis(200))
                    .build()?;
                
                let start_time = std::time::Instant::now();
                let timeout = std::time::Duration::from_secs(5);
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

        // Map legacy tool names to the new consolidated ones
        let (mapped_name, mapped_args) = match name {
            "save_episode" => {
                let mut m_args = args.clone();
                m_args["action"] = json!("save");
                ("record_memory", m_args)
            }
            "record_feedback" => {
                let mut m_args = args.clone();
                m_args["action"] = json!("feedback");
                ("record_memory", m_args)
            }
            "search_memories" => {
                let mut m_args = args.clone();
                m_args["action"] = json!("search");
                ("query_memory", m_args)
            }
            "search_wisdom" => {
                let mut m_args = args.clone();
                m_args["action"] = json!("rules");
                ("query_memory", m_args)
            }
            "get_memory_nodes" => {
                let mut m_args = args.clone();
                m_args["action"] = json!("nodes");
                ("query_memory", m_args)
            }
            "get_vault_root" => {
                let mut m_args = args.clone();
                m_args["action"] = json!("root");
                ("query_memory", m_args)
            }
            "put_short_term" => {
                let mut m_args = args.clone();
                m_args["action"] = json!("put");
                ("manage_stm", m_args)
            }
            "get_short_term" => {
                let mut m_args = args.clone();
                m_args["action"] = json!("get");
                ("manage_stm", m_args)
            }
            "clear_short_term" => {
                let mut m_args = args.clone();
                m_args["action"] = json!("clear");
                ("manage_stm", m_args)
            }
            "save_handoff" => {
                let mut m_args = args.clone();
                m_args["action"] = json!("handoff");
                ("manage_stm", m_args)
            }
            "save_forged_assets" => {
                let mut m_args = args.clone();
                m_args["action"] = json!("save_forged_assets");
                ("ingest_knowledge", m_args)
            }
            _ => (name, args)
        };

        if let Some(ref local) = self.local_state {
            let api_state = crate::api::ApiState {
                backend: local.backend.clone(),
                store: local.store.clone(),
                auth_token: "".to_string(),
                ignore_list: std::sync::Arc::new(crate::vault::watcher::WatchIgnoreList::new()),
                dream_tx: None,
            };
            crate::mcp_routes::call_mcp_tool(&api_state, mapped_name, mapped_args).await
        } else {
            let client = reqwest::Client::new();
            let url = format!("{}/v1/mcp/call", self.daemon_url);
            let payload = json!({
                "name": mapped_name,
                "arguments": mapped_args
            });
            let resp = client.post(&url)
                .header("X-Mythrax-Token", &self.auth_token)
                .json(&payload)
                .send()
                .await
                .context("Failed to contact daemon call endpoint")?;
            if resp.status() != reqwest::StatusCode::OK {
                anyhow::bail!("Daemon returned error status calling tool '{}': {}", mapped_name, resp.status());
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
                            "tools": {}
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
                _ => anyhow::bail!("Method not found: {}", method),
            }
        } else {
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
                    let client = reqwest::Client::new();
                    let url = format!("{}/v1/mcp/tools", self.daemon_url);
                    let resp = client.get(&url)
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
                    let name = if name == "verify_compliance" { "compliance_audit" } else { name };
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    
                    let client = reqwest::Client::new();
                    let url = format!("{}/v1/mcp/call", self.daemon_url);
                    
                    let payload = json!({
                        "name": name,
                        "arguments": arguments
                    });
                    
                    let resp = client.post(&url)
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
                _ => anyhow::bail!("Method not found: {}", method),
            }
        }
    }
}
