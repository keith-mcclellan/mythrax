#![allow(async_fn_in_trait)]
#![recursion_limit = "512"]

use mythrax_core::{
    cli, db, daemon, mcp, vault,
};

use clap::Parser;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context};
use db::{SurrealBackend, StorageBackend};
use cli::{Cli, Commands, ConfigAction, VaultAction, MemoryAction, HtrAction, StmAction};
use mythrax_core::contracts::{WikiNode, WisdomRule};

// Embed Mythrax Documentation
const ARCHITECTURE_DOC: &str = include_str!("../../ARCHITECTURE.md");
const USER_GUIDE_DOC: &str = include_str!("../../mythrax_user_guide.md");
const SKILL_DOC: &str = include_str!("../../.agents/skills/mythrax/SKILL.md");

async fn execute_cli_tool_call(tool_name: &str, arguments: serde_json::Value) -> Result<()> {
    let home = std::env::var("HOME").context("HOME env var not set")?;
    let mythrax_dir = PathBuf::from(&home).join(".mythrax");
    let token_path = mythrax_dir.join("token");

    // Read token
    let auth_token = if token_path.exists() {
        std::fs::read_to_string(&token_path)?.trim().to_string()
    } else {
        // Fallback to default token for headless, test, or first-time environments
        "secret-token".to_string()
    };

    let daemon_port = std::env::var("MYTHRAX_DAEMON_PORT").unwrap_or_else(|_| "8090".to_string());
    let daemon_url = format!("http://127.0.0.1:{}", daemon_port);
    
    // 1. Ensure daemon is active (auto-spawn if not)
    ensure_daemon_active_for_cli(&auth_token, &daemon_url).await?;

    // 2. Forward to daemon v1/mcp/call
    let client = reqwest::Client::new();
    let url = format!("{}/v1/mcp/call", daemon_url);
    let payload = serde_json::json!({
        "name": tool_name,
        "arguments": arguments
    });

    let resp = client.post(&url)
        .header("X-Mythrax-Token", &auth_token)
        .json(&payload)
        .send()
        .await
        .context("Failed to forward CLI command to daemon")?;

    if resp.status() != reqwest::StatusCode::OK {
        let err_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Daemon returned error executing command: {}", err_text);
    }

    let result_json: serde_json::Value = resp.json().await.context("Failed to parse daemon response")?;
    
    // Print text content to stdout
    if let Some(text) = result_json.get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|first| first.get("text"))
        .and_then(|t| t.as_str()) 
    {
        println!("{}", text);
    } else {
        println!("{}", serde_json::to_string_pretty(&result_json)?);
    }

    Ok(())
}

async fn ensure_daemon_active_for_cli(auth_token: &str, daemon_url: &str) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()?;
    
    let ping_url = format!("{}/v1/config/llm", daemon_url);
    
    if client.get(&ping_url).header("X-Mythrax-Token", auth_token).send().await.is_ok() {
        return Ok(());
    }

    println!("Daemon inactive. Spawning background daemon...");
    let current_exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("mythrax"));
    let home = std::env::var("HOME").context("HOME env var not set")?;
    let mythrax_dir = PathBuf::from(&home).join(".mythrax");
    let log_file = mythrax_dir.join("daemon.log");
    let pid_file = mythrax_dir.join("daemon.pid");

    std::fs::create_dir_all(&mythrax_dir).context("Failed to create .mythrax directory")?;

    let mut cmd = std::process::Command::new(&current_exe);
    cmd.arg("daemon").arg("start");
    if let Ok(port_val) = std::env::var("MYTHRAX_DAEMON_PORT") {
        cmd.arg("--port").arg(port_val);
    }

    if let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(&log_file) {
        cmd.stdout(file.try_clone().map(std::process::Stdio::from).unwrap_or_else(|_| std::process::Stdio::null()));
        cmd.stderr(file);
    } else {
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());
    }

    let child = cmd.spawn().context("Failed to spawn background daemon")?;
    let pid = child.id();
    let _ = std::fs::write(&pid_file, pid.to_string());

    // Poll daemon
    let poll_client = reqwest::Client::builder().timeout(std::time::Duration::from_millis(200)).build()?;
    let start_time = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(5);
    let mut healthy = false;

    while start_time.elapsed() < timeout {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        if let Ok(resp) = poll_client.get(&ping_url).header("X-Mythrax-Token", auth_token).send().await {
            if resp.status() == reqwest::StatusCode::OK {
                healthy = true;
                break;
            }
        }
    }

    if !healthy {
        anyhow::bail!("Timed out waiting for daemon to bind to port 8090.");
    }

    println!("Daemon spawned successfully (PID: {}).", pid);
    Ok(())
}

struct OnboardingConfig {
    vault_root: PathBuf,
    harness: Option<String>,
    llm_config: Option<mythrax_core::contracts::LlmConfigRequest>,
}

async fn run_onboarding_interview() -> Result<OnboardingConfig> {
    use std::io::{self, Write};
    let home = std::env::var("HOME").context("HOME env var not set")?;
    let default_vault = PathBuf::from(&home).join("mythrax-vault");

    println!("====================================================");
    println!("        Welcome to the Mythrax Onboarding Wizard    ");
    println!("====================================================");
    println!("This wizard will help you bootstrap your local memory engine.");
    println!();

    // 1. Vault Root Path
    print!("Please enter the path to your Mythrax Obsidian vault [default: {}]: ", default_vault.to_string_lossy());
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();
    let vault_root = if trimmed.is_empty() {
        default_vault
    } else {
        PathBuf::from(trimmed)
    };

    // 2. AI Agent Harness
    println!();
    println!("Supported developer/agent harnesses:");
    println!("  - antigravity (Google Antigravity SDK - highly recommended)");
    println!("  - claude (Claude projects/MCP config)");
    println!("  - cursor (Cursor IDE)");
    println!("  - codex / opencode / openclaw / hermes / none");
    print!("Which harness are you using? [default: antigravity]: ");
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();
    let harness = if trimmed.is_empty() {
        Some("antigravity".to_string())
    } else if trimmed.to_lowercase() == "none" {
        None
    } else {
        Some(trimmed.to_string())
    };

    // 3. LLM Settings
    println!();
    print!("Please choose your LLM provider (local/cloud) [default: local]: ");
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim().to_lowercase();
    let provider = if trimmed.is_empty() || trimmed == "local" {
        "local".to_string()
    } else {
        "cloud".to_string()
    };

    let llm_config;

    if provider == "local" {
        print!("Select local model [default: mlx-community/Qwen3.6-35B-A3B-4bit]: ");
        io::stdout().flush()?;
        input.clear();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();
        let model = if trimmed.is_empty() {
            "mlx-community/Qwen3.6-35B-A3B-4bit".to_string()
        } else {
            trimmed.to_string()
        };

        llm_config = Some(mythrax_core::contracts::LlmConfigRequest {
            provider: "local".to_string(),
            duration: Some("permanent".to_string()),
            model: Some(model),
            cloud_provider: Some("gemini".to_string()),
            api_key: None,
        });
    } else {
        print!("Select cloud provider (openai/anthropic/gemini) [default: openai]: ");
        io::stdout().flush()?;
        input.clear();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim().to_lowercase();
        let cloud_provider = if trimmed.is_empty() {
            "openai".to_string()
        } else {
            trimmed
        };

        print!("Enter API Key: ");
        io::stdout().flush()?;
        input.clear();
        io::stdin().read_line(&mut input)?;
        let api_key = input.trim().to_string();

        let default_model = match cloud_provider.as_str() {
            "openai" => "gpt-4o",
            "anthravity" | "anthropic" => "claude-3-5-sonnet",
            "gemini" => "gemini-1.5-pro",
            _ => "gpt-4o",
        };
        print!("Enter model ID [default: {}]: ", default_model);
        io::stdout().flush()?;
        input.clear();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();
        let model = if trimmed.is_empty() {
            default_model.to_string()
        } else {
            trimmed.to_string()
        };

        llm_config = Some(mythrax_core::contracts::LlmConfigRequest {
            provider: "cloud".to_string(),
            duration: Some("permanent".to_string()),
            model: Some(model),
            cloud_provider: Some(cloud_provider),
            api_key: Some(api_key),
        });
    }

    println!();
    println!("Onboarding configuration collected.");
    println!("----------------------------------------------------");

    Ok(OnboardingConfig {
        vault_root,
        harness,
        llm_config,
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing to stderr with a default filter level (warn for external, info for mythrax)
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,mythrax=info,mythrax_core=info"));
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init { harness, source, non_interactive } => {
            let home = std::env::var("HOME").context("HOME env var not set")?;
            let mythrax_dir = PathBuf::from(&home).join(".mythrax");
            
            // 1. Determine vault_root, harness, and llm_config
            use std::io::IsTerminal;
            let (vault_root, harness_to_use, llm_config_opt) = if harness.is_none() && !non_interactive && std::io::stdin().is_terminal() {
                let onboard = run_onboarding_interview().await?;
                (onboard.vault_root, onboard.harness, onboard.llm_config)
            } else {
                let config_path = mythrax_dir.join("config.json");
                let resolved_vault_root = if config_path.exists() {
                    if let Ok(content) = std::fs::read_to_string(&config_path) {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                            PathBuf::from(val["vault_root"].as_str().unwrap_or(&format!("{}/mythrax-vault", home)))
                        } else {
                            PathBuf::from(&home).join("mythrax-vault")
                        }
                    } else {
                        PathBuf::from(&home).join("mythrax-vault")
                    }
                } else {
                    PathBuf::from(&home).join("mythrax-vault")
                };
                (resolved_vault_root, harness, None)
            };

            // Clean up existing database
            let db_dir = mythrax_dir.join("db");
            if db_dir.exists() {
                println!("Cleaning existing RocksDB directory at {:?}", db_dir);
                let _ = std::fs::remove_dir_all(&db_dir);
            }

            let config_path = mythrax_dir.join("config.json");
            let token_path = mythrax_dir.join("token");

            // Back up old folders
            println!("Backing up existing vault directories under {:?}", vault_root);
            let _ = daemon::backup_vault_folders(&vault_root);

            std::fs::create_dir_all(&mythrax_dir)?;

            // Generate token if not exists
            let token = if token_path.exists() {
                std::fs::read_to_string(&token_path)?
            } else {
                let new_token = uuid::Uuid::new_v4().to_string();
                std::fs::write(&token_path, &new_token)?;
                new_token
            };

            // Write config pointing to RocksDB
            let config_data = serde_json::json!({
                "vault_root": vault_root.to_string_lossy().to_string(),
                "auth_token_path": token_path.to_string_lossy().to_string(),
                "surrealdb_url": format!("rocksdb://{}", db_dir.to_string_lossy())
            });
            std::fs::write(&config_path, serde_json::to_string_pretty(&config_data)?)?;

            // Setup Obsidian subdirectories
            let subfolders = ["episodes", "wiki", "wisdom", "general", "archive", "wisdom/permanent", "wiki/mythrax"];
            for sub in &subfolders {
                std::fs::create_dir_all(vault_root.join(sub))?;
            }

            // Check models
            let model_path = mythrax_dir.join("models/nomic-embed-text-v1.5.onnx");
            let tokenizer_path = mythrax_dir.join("models/tokenizer.json");
            if !model_path.exists() || !tokenizer_path.exists() {
                println!("WARNING: Nomis embedding model files not found under ~/.mythrax/models/. Local embeddings will fallback to None.");
            }

            // Always initialize the database in-process for pre-ingestion
            let backend = SurrealBackend::new(&format!("rocksdb://{}", db_dir.to_string_lossy())).await?;
            backend.init().await?;

            // Persist LLM config if provided in onboarding
            if let Some(ref config) = llm_config_opt {
                backend.update_llm_config(config).await?;
                println!("LLM configuration persisted.");
            }

            // Configure harness if provided
            if let Some(ref h) = harness_to_use {
                config_harness_action(h, source, &vault_root, &backend).await?;
            }

            // Ingest core documentation (WikiNodes)
            println!("Pre-ingesting core documentation memories...");
            
            let arch_body = format!(
                "---\nname: \"Mythrax Architecture Spec\"\nscope: \"general\"\ngenerator_name: \"PreIngested\"\n---\n\n{}",
                ARCHITECTURE_DOC
            );
            let arch_rel = "wiki/mythrax/architecture.md";
            std::fs::write(vault_root.join(arch_rel), &arch_body)?;
            let arch_node = WikiNode {
                id: None,
                name: "Mythrax Architecture Spec".to_string(),
                content: mythrax_core::vault::markdown::extract_plain_text(ARCHITECTURE_DOC).to_string(),
                scope: "general".to_string(),
                vault_path: Some(arch_rel.to_string()),
                embedding: None,
            };
            backend.save_wiki_node(&arch_node).await?;

            let user_guide_body = format!(
                "---\nname: \"Mythrax User Guide\"\nscope: \"general\"\ngenerator_name: \"PreIngested\"\n---\n\n{}",
                USER_GUIDE_DOC
            );
            let user_guide_rel = "wiki/mythrax/user_guide.md";
            std::fs::write(vault_root.join(user_guide_rel), &user_guide_body)?;
            let user_guide_node = WikiNode {
                id: None,
                name: "Mythrax User Guide".to_string(),
                content: mythrax_core::vault::markdown::extract_plain_text(USER_GUIDE_DOC).to_string(),
                scope: "general".to_string(),
                vault_path: Some(user_guide_rel.to_string()),
                embedding: None,
            };
            backend.save_wiki_node(&user_guide_node).await?;

            let skill_rel = "wiki/mythrax/skill_playbook.md";
            std::fs::write(vault_root.join(skill_rel), SKILL_DOC)?;
            let (_skill_yaml, skill_body) = mythrax_core::vault::markdown::parse_frontmatter(SKILL_DOC);
            let skill_node = WikiNode {
                id: None,
                name: "mythrax".to_string(),
                content: mythrax_core::vault::markdown::extract_plain_text(&skill_body).to_string(),
                scope: "general".to_string(),
                vault_path: Some(skill_rel.to_string()),
                embedding: None,
            };
            backend.save_wiki_node(&skill_node).await?;

            // Ingest pre-dreamed wisdom rules
            println!("Pre-dreaming core wisdom rules...");
            let wisdom_rules = vec![
                WisdomRule {
                    id: None,
                    target_pattern: "rocksdb lock contention or multiple process access".to_string(),
                    action_to_avoid: "Opening RocksDB database directly from multiple concurrent CLI or client processes.".to_string(),
                    causal_explanation: "RocksDB is a single-writer database. Multiple processes attempting to acquire the write lock simultaneously will cause panic or crash due to lock contention.".to_string(),
                    prescribed_remedy: "Always route all queries and operations through the centralized Mythrax background daemon. The daemon exclusively holds the write lock and serves requests over HTTP.".to_string(),
                    tier: "permanent".to_string(),
                    scope: "general".to_string(),
                    vault_path: Some("wisdom/permanent/rocksdb_integrity.md".to_string()),
                    embedding: None,
                    source_episodes: vec![],
                    generator_name: "PreDreamedWisdom".to_string(),
                    similarity: None,
                    utility: Some(100.0),
                    status: None,
                    superseded_at: None,
                    superseded_by: None,
                },
                WisdomRule {
                    id: None,
                    target_pattern: "subagent delegation or sharing context between agents".to_string(),
                    action_to_avoid: "Pasting full file contents, database dumps, or extensive histories directly into the subagent prompt.".to_string(),
                    causal_explanation: "Pasting full context wastes token budget, causes context window pollution, and degrades subagent focus.".to_string(),
                    prescribed_remedy: "Store context node record IDs in Short Term Memory (STM) and write a minimal contract file under `.handoffs/handoff_<task_id>.md`. Spawn the subagent pointing to the handoff file URL and let it hydrate context dynamically.".to_string(),
                    tier: "permanent".to_string(),
                    scope: "general".to_string(),
                    vault_path: Some("wisdom/permanent/smart_handoffs.md".to_string()),
                    embedding: None,
                    source_episodes: vec![],
                    generator_name: "PreDreamedWisdom".to_string(),
                    similarity: None,
                    utility: Some(100.0),
                    status: None,
                    superseded_at: None,
                    superseded_by: None,
                },
                WisdomRule {
                    id: None,
                    target_pattern: "agent boot initialization and compliance check".to_string(),
                    action_to_avoid: "Proceeding with code modification or tool execution without checking the pre-invocation hook context.".to_string(),
                    causal_explanation: "Failing to verify pre-invocation hook context leads to duplicate effort, rule violations, and lack of alignment with parent guidelines.".to_string(),
                    prescribed_remedy: "Output a 1-line compliance check (`Execution Check: ...`) on the very first line of your response, and query Mythrax memory if hook context is empty.".to_string(),
                    tier: "permanent".to_string(),
                    scope: "general".to_string(),
                    vault_path: Some("wisdom/permanent/pre_invocation_compliance.md".to_string()),
                    embedding: None,
                    source_episodes: vec![],
                    generator_name: "PreDreamedWisdom".to_string(),
                    similarity: None,
                    utility: Some(100.0),
                    status: None,
                    superseded_at: None,
                    superseded_by: None,
                },
                WisdomRule {
                    id: None,
                    target_pattern: "file deletion or cleanup".to_string(),
                    action_to_avoid: "Using `rm` to permanently delete files in the vault or workspace.".to_string(),
                    causal_explanation: "Permanent deletions are irreversible, making accidental data loss or breaking changes impossible to recover from.".to_string(),
                    prescribed_remedy: "Always move deleted files to the `.trash/` directory under the vault or workspace root.".to_string(),
                    tier: "permanent".to_string(),
                    scope: "general".to_string(),
                    vault_path: Some("wisdom/permanent/safe_deletions.md".to_string()),
                    embedding: None,
                    source_episodes: vec![],
                    generator_name: "PreDreamedWisdom".to_string(),
                    similarity: None,
                    utility: Some(100.0),
                    status: None,
                    superseded_at: None,
                    superseded_by: None,
                },
                WisdomRule {
                    id: None,
                    target_pattern: "mythrax v1.2 capabilities and tools".to_string(),
                    action_to_avoid: "Using old granular file tools (view_file, replace_file_content) or old standalone audit tools, or bypassing MemoryOS virtual paging.".to_string(),
                    causal_explanation: "Old tools are deprecated and removed. Bypassing virtual paging and paging-aware editing leads to token budget exhaustion and context window bloat.".to_string(),
                    prescribed_remedy: "Always use 'manage_file' (actions: 'view', 'replace', 'multi_replace') for files, and 'manage_vault' (action: 'audit') for compliance. Target virtual placeholders directly during edits as the paging-aware manager resolves them on disk.".to_string(),
                    tier: "permanent".to_string(),
                    scope: "general".to_string(),
                    vault_path: Some("wisdom/permanent/mythrax_v1_2_capabilities.md".to_string()),
                    embedding: None,
                    source_episodes: vec![],
                    generator_name: "PreDreamedWisdom".to_string(),
                    similarity: None,
                    utility: Some(100.0),
                    status: None,
                    superseded_at: None,
                    superseded_by: None,
                },
            ];

            for rule in wisdom_rules {
                let frontmatter = mythrax_core::vault::watcher::format_wisdom_markdown(&rule);
                let rule_body = format!(
                    "{}\n# Wisdom Rule: {}\n\n**Action to Avoid:** {}\n\n**Why:** {}\n\n**Prescribed Remedy:** {}",
                    frontmatter, rule.target_pattern, rule.action_to_avoid, rule.causal_explanation, rule.prescribed_remedy
                );
                let vp = rule.vault_path.as_ref().unwrap();
                std::fs::write(vault_root.join(vp), &rule_body)?;
                backend.save_wisdom_rule(&rule).await?;
            }

            println!("Mythrax initialized successfully.");
            println!("Config path: {:?}", config_path);
            println!("Token: {}", token);
        }
        Commands::Config { action } => {
            let (act_str, args) = match action {
                ConfigAction::Get => {
                    ("get", serde_json::json!({}))
                }
                ConfigAction::Set { provider, duration, model, cloud_provider, api_key } => {
                    ("set", serde_json::json!({
                        "provider": provider,
                        "duration": duration,
                        "model": model,
                        "cloud_provider": cloud_provider,
                        "api_key": api_key,
                    }))
                }
            };
            let mut payload = args;
            payload["action"] = serde_json::Value::String(act_str.to_string());
            execute_cli_tool_call("manage_config", payload).await?;
        }
        Commands::Daemon { action } => {
            daemon::handle_daemon(action).await?;
        }
        Commands::Memory { action } => {
            match action {
                MemoryAction::Query {
                    query,
                    scope,
                    limit,
                    offset,
                    threshold,
                    token_budget,
                    allow_downward,
                    include_episodes,
                    include_artifacts,
                    session_id,
                } => {
                    let args = serde_json::json!({
                        "action": "search",
                        "query": query,
                        "scope": scope,
                        "limit": limit,
                        "offset": offset,
                        "threshold": threshold,
                        "token_budget": token_budget,
                        "allow_downward": allow_downward,
                        "include_episodes": include_episodes,
                        "include_artifacts": include_artifacts,
                        "session_id": session_id,
                    });
                    execute_cli_tool_call("manage_memory", args).await?;
                }
                MemoryAction::Record { title, file, scope } => {
                    let path = Path::new(&file);
                    let content = std::fs::read_to_string(path)
                        .with_context(|| format!("Failed to read file at {:?}", path))?;
                    let args = serde_json::json!({
                        "action": "save",
                        "title": title,
                        "content": content,
                        "scope": scope,
                    });
                    execute_cli_tool_call("manage_memory", args).await?;
                }
                MemoryAction::Feedback { id, success } => {
                    let args = serde_json::json!({
                        "action": "feedback",
                        "episode_id": id,
                        "success": success,
                    });
                    execute_cli_tool_call("manage_memory", args).await?;
                }
                MemoryAction::Root => {
                    let args = serde_json::json!({
                        "action": "root",
                    });
                    execute_cli_tool_call("manage_memory", args).await?;
                }
            }
        }
        Commands::Stm { action } => {
            let (act_str, args) = match action {
                StmAction::Put { session_id, key, value } => {
                    ("put", serde_json::json!({ "session_id": session_id, "key": key, "value": value }))
                }
                StmAction::Get { session_id, key } => {
                    ("get", serde_json::json!({ "session_id": session_id, "key": key }))
                }
                StmAction::Clear { session_id } => {
                    ("clear", serde_json::json!({ "session_id": session_id }))
                }
                StmAction::Handoff { parent_conversation_id, subagent_conversation_id, summary, handoff_file_path, scope } => {
                    ("handoff", serde_json::json!({
                        "parent_conversation_id": parent_conversation_id,
                        "subagent_conversation_id": subagent_conversation_id,
                        "summary": summary,
                        "handoff_file_path": handoff_file_path,
                        "scope": scope,
                    }))
                }
            };
            let mut payload = args;
            payload["action"] = serde_json::Value::String(act_str.to_string());
            execute_cli_tool_call("manage_stm", payload).await?;
        }
        Commands::Mcp => {
            let home = std::env::var("HOME").context("HOME env var not set")?;
            let mythrax_dir = PathBuf::from(&home).join(".mythrax");
            let token_path = mythrax_dir.join("token");

            // Read token
            let auth_token = if token_path.exists() {
                std::fs::read_to_string(&token_path)?.trim().to_string()
            } else {
                // Fallback to default token for headless, test, or first-time environments
                "secret-token".to_string()
            };
            
            let daemon_port = std::env::var("MYTHRAX_DAEMON_PORT").unwrap_or_else(|_| "8090".to_string());
            let daemon_url = format!("http://127.0.0.1:{}", daemon_port);
            let mcp_server = mcp::McpServer::new(auth_token, daemon_url);
            mcp_server.run().await?;
        }
        Commands::Vault { action } => {
            let (act_str, args) = match action {
                VaultAction::Organize => {
                    ("organize", serde_json::json!({}))
                }
                VaultAction::Verify { fix } => {
                    ("verify", serde_json::json!({ "fix": fix }))
                }
                VaultAction::Reprocess => {
                    ("reprocess", serde_json::json!({}))
                }
                VaultAction::Summarize { scope } => {
                    ("summarize", serde_json::json!({ "scope": scope }))
                }
                VaultAction::IngestBulk { source, harness, scope } => {
                    ("ingest_bulk", serde_json::json!({ "source": source, "harness": harness, "scope": scope }))
                }
                VaultAction::IngestForge { source_path, scope } => {
                    ("ingest_forge", serde_json::json!({ "source_path": source_path, "scope": scope }))
                }
                VaultAction::Audit { workspace } => {
                    ("audit", serde_json::json!({ "workspace_path": workspace }))
                }
            };
            let mut payload = args;
            payload["action"] = serde_json::Value::String(act_str.to_string());
            execute_cli_tool_call("manage_vault", payload).await?;
        }
        Commands::Htr { action } => {
            let (act_str, args) = match action {
                HtrAction::Init { scope, hypothesis, files } => {
                    ("init", serde_json::json!({ "scope": scope, "hypothesis": hypothesis, "files": files }))
                }
                HtrAction::Ideate { scope, node } => {
                    ("ideate", serde_json::json!({ "scope": scope, "node_id": node }))
                }
                HtrAction::Execute { scope, node, test_command } => {
                    ("execute", serde_json::json!({ "scope": scope, "node_id": node, "test_command": test_command }))
                }
                HtrAction::Backprop { scope, node } => {
                    ("backprop", serde_json::json!({ "scope": scope, "node_id": node }))
                }
                HtrAction::Merge { scope, node } => {
                    ("merge", serde_json::json!({ "scope": scope, "node_id": node }))
                }
                HtrAction::Run { scope, hypothesis, files, test_command, max_steps } => {
                    ("run", serde_json::json!({ "scope": scope, "hypothesis": hypothesis, "files": files, "test_command": test_command, "max_steps": max_steps }))
                }
            };
            let mut payload = args;
            payload["action"] = serde_json::Value::String(act_str.to_string());
            execute_cli_tool_call("manage_htr", payload).await?;
        }
        Commands::InstallHook => {
            handle_install_hook().await?;
        }
        Commands::PreCommit => {
            handle_pre_commit().await?;
        }
    }

    Ok(())
}



fn merge_json_mcp(path: &std::path::Path, exe_path: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut data: serde_json::Value = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    
    let mcp_servers = data.as_object_mut()
        .unwrap()
        .entry("mcpServers".to_string())
        .or_insert_with(|| serde_json::json!({}));
        
    mcp_servers.as_object_mut().unwrap().insert(
        "mythrax".to_string(),
        serde_json::json!({
            "command": exe_path,
            "args": ["mcp"]
        })
    );
    
    std::fs::write(path, serde_json::to_string_pretty(&data)?)?;
    Ok(())
}

fn merge_antigravity_permissions(path: &std::path::Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut data: serde_json::Value = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    
    let user_settings = data.as_object_mut()
        .unwrap()
        .entry("userSettings".to_string())
        .or_insert_with(|| serde_json::json!({}));
        
    let global_grants = user_settings.as_object_mut()
        .unwrap()
        .entry("globalPermissionGrants".to_string())
        .or_insert_with(|| serde_json::json!({}));
        
    let allow_list = global_grants.as_object_mut()
        .unwrap()
        .entry("allow".to_string())
        .or_insert_with(|| serde_json::json!([]));
        
    let allow_arr = allow_list.as_array_mut().unwrap();
    let grant = "mcp(mythrax/*)".to_string();
    if !allow_arr.iter().any(|v| v.as_str() == Some(&grant)) {
        allow_arr.push(serde_json::Value::String(grant));
    }
    
    std::fs::write(path, serde_json::to_string_pretty(&data)?)?;
    Ok(())
}

fn merge_antigravity_hooks(path: &std::path::Path, _exe_path: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut data: serde_json::Value = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    
    let mythrax_comp = data.as_object_mut()
        .unwrap()
        .entry("mythrax-compliance".to_string())
        .or_insert_with(|| serde_json::json!({}));
        
    mythrax_comp.as_object_mut()
        .unwrap()
        .insert(
            "PreInvocation".to_string(),
            serde_json::json!([
                {
                    "type": "mcp",
                    "server": "mythrax",
                    "tool": "compliance_audit"
                },
                {
                    "type": "mcp",
                    "server": "mythrax",
                    "tool": "pre_invocation_hook"
                }
            ])
        );
        
    std::fs::write(path, serde_json::to_string_pretty(&data)?)?;
    Ok(())
}

fn merge_toml_mcp(path: &std::path::Path, exe_path: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut content = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };
    
    if content.contains("[mcp.mythrax]") {
        let lines: Vec<&str> = content.lines().collect();
        let mut new_lines: Vec<String> = Vec::new();
        let mut in_block = false;
        for line in lines {
            if line.trim().starts_with("[mcp.mythrax]") {
                in_block = true;
                new_lines.push("[mcp.mythrax]".to_string());
                new_lines.push(format!("command = \"{}\"", exe_path.replace("\\", "\\\\")));
                new_lines.push("args = [\"mcp\"]".to_string());
                continue;
            }
            if in_block && line.trim().starts_with('[') {
                in_block = false;
            }
            if !in_block {
                new_lines.push(line.to_string());
            }
        }
        content = new_lines.join("\n");
    } else {
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str("[mcp.mythrax]\n");
        content.push_str(&format!("command = \"{}\"\n", exe_path.replace("\\", "\\\\")));
        content.push_str("args = [\"mcp\"]\n");
    }
    
    std::fs::write(path, content)?;
    Ok(())
}

fn resolve_default_history(harness: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let path = match harness {
        "antigravity" => PathBuf::from(&home).join(".gemini/antigravity/brain/"),
        "claude" => PathBuf::from(&home).join(".claude/projects/"),
        "cursor" => {
            #[cfg(target_os = "macos")]
            {
                PathBuf::from(&home).join("Library/Application Support/Cursor/User/globalStorage/")
            }
            #[cfg(target_os = "linux")]
            {
                PathBuf::from(&home).join(".config/Cursor/User/globalStorage/")
            }
            #[cfg(target_os = "windows")]
            {
                if let Ok(appdata) = std::env::var("APPDATA") {
                    PathBuf::from(&appdata).join("Cursor/User/globalStorage/")
                } else {
                    return None;
                }
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
            return None;
        }
        "codex" => {
            let p1 = PathBuf::from(&home).join(".codex/history/");
            if p1.exists() {
                p1
            } else {
                PathBuf::from(&home).join(".codex/logs/")
            }
        }
        "opencode" => PathBuf::from(&home).join(".opencode/sessions/"),
        "openclaw" => PathBuf::from(&home).join(".openclaw/history/"),
        "hermes" => PathBuf::from(&home).join(".hermes/"),
        _ => return None,
    };
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

async fn config_harness_action(
    harness: &str,
    source: Option<String>,
    vault_root: &std::path::Path,
    backend: &SurrealBackend,
) -> Result<()> {
    let home = std::env::var("HOME").context("HOME env var not set")?;
    let exe_path = std::env::current_exe()?.to_string_lossy().to_string();
    
    match harness {
        "antigravity" => {
            let config_dir = std::path::PathBuf::from(&home).join(".gemini/config");
            merge_json_mcp(&config_dir.join("mcp_config.json"), &exe_path)?;
            merge_antigravity_permissions(&config_dir.join("config.json"))?;
            merge_antigravity_hooks(&config_dir.join("hooks.json"), &exe_path)?;
            
            // Install global /mythrax skill playbook
            let skill_dest = config_dir.join("skills/mythrax/SKILL.md");
            if let Some(parent) = skill_dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&skill_dest, SKILL_DOC)?;
            println!("Installed global /mythrax skill playbook at: {:?}", skill_dest);
        }
        "claude" => {
            let path = std::path::PathBuf::from(&home).join(".claude.json");
            merge_json_mcp(&path, &exe_path)?;
        }
        "cursor" => {
            let path = std::path::PathBuf::from(&home).join(".cursor/mcp.json");
            merge_json_mcp(&path, &exe_path)?;
        }
        "codex" => {
            let path = std::path::PathBuf::from(&home).join(".codex/config.toml");
            merge_toml_mcp(&path, &exe_path)?;
        }
        "opencode" => {
            let path = std::path::PathBuf::from(&home).join(".opencode/config.json");
            merge_json_mcp(&path, &exe_path)?;
        }
        "openclaw" => {
            let path = std::path::PathBuf::from(&home).join(".openclaw/config.json");
            merge_json_mcp(&path, &exe_path)?;
        }
        "hermes" => {
            let path = std::path::PathBuf::from(&home).join(".hermes/config.json");
            merge_json_mcp(&path, &exe_path)?;
        }
        other => {
            anyhow::bail!("Unsupported harness type: {}", other);
        }
    }
    
    println!("Configured MCP server and settings for harness: {}", harness);
    
    let history_path = if let Some(ref s) = source {
        let p = std::path::PathBuf::from(s);
        if p.exists() {
            Some(p)
        } else {
            println!("WARNING: Provided source path {:?} does not exist. Skipping ingestion.", p);
            None
        }
    } else {
        resolve_default_history(harness)
    };
    
    if let Some(path) = history_path {
        println!("Auto-discovered/provided history source found at: {:?}", path);
        println!("Running bulk ingestion of historical transcripts...");
        match vault::ingestion::bulk_ingest_vault(
            vault_root,
            &path,
            harness,
            "general",
            backend,
        ).await {
            Ok((count, errs)) => {
                println!("Ingested {} episodes successfully.", count);
                if !errs.is_empty() {
                    println!("Warnings/Errors during ingestion: {:?}", errs);
                }
            }
            Err(e) => {
                println!("WARNING: History ingestion failed: {:?}", e);
            }
        }
    } else {
        println!("No pre-existing history source resolved or provided. Skipping initial ingestion.");
    }
    
    Ok(())
}



async fn handle_install_hook() -> Result<()> {
    let workspace_root = std::env::var("MYTHRAX_WORKSPACE_ROOT")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let git_dir = workspace_root.join(".git");
    if !git_dir.exists() {
        anyhow::bail!("Not a git repository (missing .git directory at {:?})", workspace_root);
    }

    let hooks_dir = git_dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;
    let pre_commit_path = hooks_dir.join("pre-commit");

    let hook_script = r#"#!/bin/sh
# Mythrax pre-commit hook to clean secrets from shared rules
exec mythrax pre-commit
"#;

    std::fs::write(&pre_commit_path, hook_script)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&pre_commit_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&pre_commit_path, perms)?;
    }

    println!("Successfully installed git pre-commit hook at: {:?}", pre_commit_path);
    Ok(())
}

async fn handle_pre_commit() -> Result<()> {
    let workspace_root = std::env::var("MYTHRAX_WORKSPACE_ROOT")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    println!("Running SecretFilter on staged files under .mythrax-shared...");
    
    let output = std::process::Command::new("git")
        .args(&["diff", "--cached", "--name-only", "--diff-filter=ACM"])
        .current_dir(&workspace_root)
        .output()?;
    
    if !output.status.success() {
        anyhow::bail!("Failed to run git diff: {}", String::from_utf8_lossy(&output.stderr));
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let staged_files: Vec<&str> = stdout_str.lines().collect();

    for file_rel in staged_files {
        if file_rel.starts_with(".mythrax-shared/") {
            let abs_path = workspace_root.join(file_rel);
            if abs_path.exists() && abs_path.is_file() {
                let content = std::fs::read_to_string(&abs_path)?;
                let cleaned = mythrax_core::secret_filter::SecretFilter::clean(&content);
                if cleaned != content {
                    std::fs::write(&abs_path, cleaned)?;
                    let _ = std::process::Command::new("git")
                        .args(&["add", file_rel])
                        .current_dir(&workspace_root)
                        .status();
                    println!("Sanitized and re-staged: {}", file_rel);
                }
            }
        }
    }

    Ok(())
}




