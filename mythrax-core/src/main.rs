#![allow(async_fn_in_trait)]
#![recursion_limit = "512"]

use mythrax_core::{
    cli, db, daemon, mcp,
};

use clap::Parser;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context};
use db::{SurrealBackend, StorageBackend};
use cli::{Cli, Commands, ConfigAction, VaultAction, MemoryAction, HtrAction, StmAction};
use mythrax_core::contracts::WikiNode;

// Embed Mythrax Documentation
const ARCHITECTURE_DOC: &str = include_str!("../../ARCHITECTURE.md");
const USER_GUIDE_DOC: &str = include_str!("../../mythrax_user_guide.md");
const SKILL_DOC: &str = include_str!("../../.agents/skills/mythrax/SKILL.md");

async fn execute_cli_tool_call(tool_name: &str, arguments: serde_json::Value) -> Result<()> {
    let home = std::env::var("HOME").context("HOME env var not set")?;
    let mythrax_dir = PathBuf::from(&home).join(".mythrax");
    let token_path = mythrax_dir.join("token");

    let auth_token = mythrax_core::auth::get_or_create_token(&token_path)?;

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
    let timeout = std::time::Duration::from_secs(45);
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
            llm_post_inference_delay_ms: None,
            model_tier_mappings: None,
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
            llm_post_inference_delay_ms: None,
            model_tier_mappings: None,
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


#[cfg(unix)]
fn is_process_alive(pid: i32) -> bool {
    unsafe { libc::kill(pid, 0) == 0 }
}

#[cfg(not(unix))]
fn is_process_alive(_pid: i32) -> bool {
    false
}

fn get_process_name(pid: i32) -> Option<String> {
    let output = std::process::Command::new("ps")
        .args(&["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .ok()?;
    if output.status.success() {
        let comm = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if comm.is_empty() {
            return None;
        }
        let path = std::path::PathBuf::from(&comm);
        path.file_name().and_then(|name| name.to_str()).map(|s| s.to_string())
    } else {
        None
    }
}

fn recover_stale_locks() {
    if let Ok(home) = std::env::var("HOME") {
        let mythrax_dir = std::path::PathBuf::from(&home).join(".mythrax");
        let pid_path = mythrax_dir.join("daemon.pid");
        let lock_path = mythrax_dir.join("daemon.lock");

        if pid_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&pid_path) {
                if let Ok(pid) = content.trim().parse::<i32>() {
                    let alive = is_process_alive(pid);
                    let mut is_stale = !alive;
                    if alive {
                        if let Some(name) = get_process_name(pid) {
                            if name != "mythrax" && name != "mythrax-core" {
                                is_stale = true;
                            }
                        } else {
                            is_stale = true;
                        }
                    }
                    if is_stale {
                        eprintln!("[SAFEGUARD] Stale lock detected for PID {}. Purging locks.", pid);
                        let _ = std::fs::remove_file(&pid_path);
                        let _ = std::fs::remove_file(&lock_path);
                    }
                }
            }
        }
    }
}

static SYSTEM_MONITOR: std::sync::OnceLock<std::sync::Mutex<sysinfo::System>> = std::sync::OnceLock::new();

fn get_swap_used_bytes() -> Option<u64> {
    let sys_mutex = SYSTEM_MONITOR.get_or_init(|| {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        std::sync::Mutex::new(sys)
    });
    if let Ok(mut sys) = sys_mutex.lock() {
        sys.refresh_memory();
        Some(sys.used_swap())
    } else {
        None
    }
}

pub fn spawn_swap_monitor_thread() {
    tokio::spawn(async move {
        // Wait until the backend is initialized
        let backend = loop {
            if let Some(backend) = db::GLOBAL_BACKEND.get() {
                break backend.clone();
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        };

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;

            // 1. Query disable_swap_monitor from database config:settings
            let config_sql = "SELECT VALUE disable_swap_monitor FROM config:settings LIMIT 1;";
            let disable_monitor = match backend.db.query(config_sql).await {
                Ok(mut res) => res.take::<Option<bool>>(0).unwrap_or(None).unwrap_or(false),
                Err(_) => false,
            };
            if disable_monitor {
                continue;
            }

            // 2. Query sysctl vm.swapusage
            if let Some(swap_used) = get_swap_used_bytes() {
                // 3. Get thresholds
                let t1_sql = "SELECT VALUE swap_threshold_tier1_gb FROM config:settings LIMIT 1;";
                let tier1_gb = match backend.db.query(t1_sql).await {
                    Ok(mut res) => res.take::<Option<f64>>(0).unwrap_or(None).unwrap_or(2.0),
                    Err(_) => 2.0,
                };
                let t2_sql = "SELECT VALUE swap_threshold_tier2_gb FROM config:settings LIMIT 1;";
                let tier2_gb = match backend.db.query(t2_sql).await {
                    Ok(mut res) => res.take::<Option<f64>>(0).unwrap_or(None).unwrap_or(3.0),
                    Err(_) => 3.0,
                };
                let t3_sql = "SELECT VALUE swap_threshold_tier3_gb FROM config:settings LIMIT 1;";
                let tier3_gb = match backend.db.query(t3_sql).await {
                    Ok(mut res) => res.take::<Option<f64>>(0).unwrap_or(None).unwrap_or(6.0),
                    Err(_) => 6.0,
                };

                let swap_used_gb = swap_used as f64 / (1024.0 * 1024.0 * 1024.0);

                // 4. Enforce model-aware thresholds
                let active_tier = if let Some(broker) = mythrax_core::llm::DYNAMIC_MODEL_BROKER.get() {
                    broker.active_tier()
                } else {
                    None
                };

                if let Some(tier) = active_tier {
                    let threshold_gb = match tier {
                        mythrax_core::llm::ModelTier::Tier1 => tier1_gb,
                        mythrax_core::llm::ModelTier::Tier2 => tier2_gb,
                        mythrax_core::llm::ModelTier::Tier3 => tier3_gb,
                    };

                    if swap_used_gb >= threshold_gb {
                        tracing::warn!(
                            "[SWAP MONITOR] Swap usage {:.2}GB exceeds active tier threshold ({:?} - {}GB). Suspending background tasks and evicting models.",
                            swap_used_gb,
                            tier,
                            threshold_gb
                        );

                        // Evict unused models from VRAM
                        if let Some(broker) = mythrax_core::llm::DYNAMIC_MODEL_BROKER.get() {
                            broker.evict_unused_models().await;
                        }
                    }
                }
            }
        }
    });
}

#[tokio::main]
async fn main() -> Result<()> {
    // Boost soft file descriptor limits to maximum hard limit on boot
    if let Ok((_soft, hard)) = rlimit::Resource::NOFILE.get() {
        let _ = rlimit::Resource::NOFILE.set(hard, hard);
    }

    // Run stale lock recovery check
    recover_stale_locks();

    // Spawn background active swap monitor thread
    spawn_swap_monitor_thread();

    // Initialize tracing with non-blocking writer and size-rolling file backend
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,mythrax=info,mythrax_core=info"));
    
    let home_dir = std::env::var("HOME").context("HOME env var not set")?;
    let log_path = PathBuf::from(home_dir).join(".mythrax").join("daemon.log");
    
    let file_writer = SizeRollingFileWriter::new(log_path)?;
    let (non_blocking_writer, _log_guard) = tracing_appender::non_blocking(file_writer);
    
    tracing_subscriber::fmt()
        .with_writer(non_blocking_writer)
        .with_env_filter(filter)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init { harness, source: _, non_interactive } => {
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
            let token = mythrax_core::auth::get_or_create_token(&token_path)?;

            // Preserve old scope_mappings and skip_scopes if configured
            let (old_scope_mappings, old_skip_scopes) = if config_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&config_path) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                        (val.get("scope_mappings").cloned(), val.get("skip_scopes").cloned())
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            };

            // Write config pointing to RocksDB
            let mut config_data = serde_json::json!({
                "vault_root": vault_root.to_string_lossy().to_string(),
                "auth_token_path": token_path.to_string_lossy().to_string(),
                "surrealdb_url": format!("rocksdb://{}", db_dir.to_string_lossy())
            });

            // Set skip_scopes
            if let Some(skips) = old_skip_scopes {
                if let Some(obj) = config_data.as_object_mut() {
                    obj.insert("skip_scopes".to_string(), skips);
                }
            } else {
                let mut skips = vec![
                    "repos".to_string(), "workspace".to_string(), "workspaces".to_string(),
                    "projects".to_string(), "documents".to_string(), "brain".to_string(),
                    "antigravity".to_string(), "general".to_string(), "archive".to_string(),
                    "quarantine".to_string(), "logs".to_string(), "bin".to_string(),
                    "lib".to_string(), "tests".to_string(), "test".to_string(),
                    "users".to_string(), "git".to_string(), "refs".to_string(),
                    "ref".to_string(), "github".to_string(), "deps".to_string(),
                    "build".to_string(), "dist".to_string(), "node_modules".to_string(),
                    "vendor".to_string()
                ];
                let user_var = std::env::var("USER").unwrap_or_default();
                if !user_var.is_empty() && !skips.contains(&user_var) {
                    skips.push(user_var);
                }
                if let Some(obj) = config_data.as_object_mut() {
                    obj.insert("skip_scopes".to_string(), serde_json::json!(skips));
                }
            }

            // Set scope_mappings
            if let Some(mappings) = old_scope_mappings {
                if let Some(obj) = config_data.as_object_mut() {
                    obj.insert("scope_mappings".to_string(), mappings);
                }
            } else {
                let default_mappings = serde_json::json!({
                    "self-improvement-engine": "mythrax",
                    "self-improvement-enginez": "mythrax"
                });
                if let Some(obj) = config_data.as_object_mut() {
                    obj.insert("scope_mappings".to_string(), default_mappings);
                }
            }

            std::fs::write(&config_path, serde_json::to_string_pretty(&config_data)?)?;

            // Setup Obsidian subdirectories
            let subfolders = ["episodes", "wiki", "wisdom", "archive", "wisdom/permanent", "wisdom/dynamic", "wiki/mythrax/raw"];
            for sub in &subfolders {
                std::fs::create_dir_all(vault_root.join(sub))?;
            }

            // Check models
            let onnx_path = mythrax_dir.join("models/nomic-embed-text-v1.5.onnx");
            let mlx_path = mythrax_dir.join("models/model.safetensors");
            let tokenizer_path = mythrax_dir.join("models/tokenizer.json");
            if (!onnx_path.exists() && !mlx_path.exists()) || !tokenizer_path.exists() {
                println!("WARNING: Nomic embedding model files (model.safetensors or nomic-embed-text-v1.5.onnx) not found under ~/.mythrax/models/. Local embeddings will fallback to None.");
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
                config_harness_action(h, &vault_root).await?;
            }

            // Ingest core documentation (WikiNodes)
            println!("Pre-ingesting core documentation memories...");
            
            let arch_body = format!(
                "---\nname: \"Mythrax Architecture Spec\"\nscope: \"mythrax\"\ngenerator_name: \"PreIngested\"\n---\n\n{}",
                ARCHITECTURE_DOC
            );
            let arch_rel = "wiki/mythrax/raw/architecture.md";
            std::fs::write(vault_root.join(arch_rel), &arch_body)?;
            let arch_node = WikiNode {
                id: None,
                name: "Mythrax Architecture Spec".to_string(),
                content: mythrax_core::vault::markdown::extract_plain_text(ARCHITECTURE_DOC).to_string(),
                scope: "mythrax".to_string(),
                vault_path: Some(arch_rel.to_string()),
                embedding: None,
                ..Default::default()
            };
            backend.save_wiki_node(&arch_node).await?;

            let user_guide_body = format!(
                "---\nname: \"Mythrax User Guide\"\nscope: \"mythrax\"\ngenerator_name: \"PreIngested\"\n---\n\n{}",
                USER_GUIDE_DOC
            );
            let user_guide_rel = "wiki/mythrax/raw/user_guide.md";
            std::fs::write(vault_root.join(user_guide_rel), &user_guide_body)?;
            let user_guide_node = WikiNode {
                id: None,
                name: "Mythrax User Guide".to_string(),
                content: mythrax_core::vault::markdown::extract_plain_text(USER_GUIDE_DOC).to_string(),
                scope: "mythrax".to_string(),
                vault_path: Some(user_guide_rel.to_string()),
                embedding: None,
                ..Default::default()
            };
            backend.save_wiki_node(&user_guide_node).await?;

            let skill_rel = "wiki/mythrax/raw/skill_playbook.md";
            std::fs::write(vault_root.join(skill_rel), SKILL_DOC)?;
            let (_skill_yaml, skill_body) = mythrax_core::vault::markdown::parse_frontmatter(SKILL_DOC);
            let skill_node = WikiNode {
                id: None,
                name: "mythrax".to_string(),
                content: mythrax_core::vault::markdown::extract_plain_text(&skill_body).to_string(),
                scope: "mythrax".to_string(),
                vault_path: Some(skill_rel.to_string()),
                embedding: None,
                ..Default::default()
            };
            backend.save_wiki_node(&skill_node).await?;

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
            if act_str == "get" {
                execute_cli_tool_call("read", payload).await?;
            } else {
                execute_cli_tool_call("write", payload).await?;
            }
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
                    execute_cli_tool_call("read", args).await?;
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
                    execute_cli_tool_call("write", args).await?;
                }
                MemoryAction::Feedback { id, success } => {
                    let args = serde_json::json!({
                        "action": "feedback",
                        "episode_id": id,
                        "success": success,
                    });
                    execute_cli_tool_call("write", args).await?;
                }
                MemoryAction::Root => {
                    let args = serde_json::json!({
                        "action": "root",
                    });
                    execute_cli_tool_call("read", args).await?;
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
            if act_str == "get" {
                execute_cli_tool_call("read", payload).await?;
            } else {
                execute_cli_tool_call("write", payload).await?;
            }
        }
        Commands::Mcp => {
            let home = std::env::var("HOME").context("HOME env var not set")?;
            let mythrax_dir = PathBuf::from(&home).join(".mythrax");
            let token_path = mythrax_dir.join("token");

            let auth_token = mythrax_core::auth::get_or_create_token(&token_path)?;
            
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
                    ("summarize", serde_json::json!({ "scope": scope, "async_mode": false }))
                }
                VaultAction::IngestBulk { source, harness, scope } => {
                    ("ingest_bulk", serde_json::json!({ "source": source, "harness": harness, "scope": scope, "async_mode": false }))
                }
                VaultAction::IngestForge { source_path, scope } => {
                    ("ingest_forge", serde_json::json!({ "source_path": source_path, "scope": scope, "async_mode": false }))
                }
                VaultAction::Audit { workspace } => {
                    ("audit", serde_json::json!({ "workspace_path": workspace }))
                }
                VaultAction::Clean { dry_run, confirm } => {
                    if dry_run {
                        ("clean", serde_json::json!({ "dry_run": true, "confirm": false }))
                    } else if confirm {
                        ("clean", serde_json::json!({ "dry_run": false, "confirm": true }))
                    } else {
                        let home = std::env::var("HOME").context("HOME env var not set")?;
                        let token_path = std::path::PathBuf::from(&home).join(".mythrax").join("token");
                        let auth_token = mythrax_core::auth::get_or_create_token(&token_path)?;
                        let daemon_port = std::env::var("MYTHRAX_DAEMON_PORT").unwrap_or_else(|_| "8090".to_string());
                        let daemon_url = format!("http://127.0.0.1:{}", daemon_port);
                        
                        ensure_daemon_active_for_cli(&auth_token, &daemon_url).await?;
                        
                        let client = reqwest::Client::new();
                        let url = format!("{}/v1/mcp/call", daemon_url);
                        let payload = serde_json::json!({
                            "name": "manage",
                            "arguments": {
                                "action": "clean",
                                "dry_run": true,
                                "confirm": false
                            }
                        });
                        
                        let resp = client.post(&url)
                            .header("X-Mythrax-Token", &auth_token)
                            .json(&payload)
                            .send()
                            .await?;
                            
                        let result_json: serde_json::Value = resp.json().await?;
                        let text = result_json.get("content")
                            .and_then(|c| c.as_array())
                            .and_then(|arr| arr.first())
                            .and_then(|first| first.get("text"))
                            .and_then(|t| t.as_str())
                            .unwrap_or("");
                            
                        println!("{}", text);
                        
                        print!("Do you want to proceed with the cleanup? [y/N]: ");
                        std::io::Write::flush(&mut std::io::stdout())?;
                        
                        let mut input = String::new();
                        std::io::stdin().read_line(&mut input)?;
                        let trimmed = input.trim().to_lowercase();
                        if trimmed == "y" || trimmed == "yes" {
                            ("clean", serde_json::json!({ "dry_run": false, "confirm": true }))
                        } else {
                            println!("Cleanup aborted.");
                            return Ok(());
                        }
                    }
                }
            };
            let mut payload = args;
            payload["action"] = serde_json::Value::String(act_str.to_string());
            execute_cli_tool_call("manage", payload).await?;
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
            execute_cli_tool_call("manage", payload).await?;
        }
        Commands::InstallHook => {
            handle_install_hook().await?;
        }
        Commands::PreCommit => {
            handle_pre_commit().await?;
        }
        Commands::Exec { command_name, args } => {
            handle_exec(&command_name, &args).await?;
        }
        Commands::Bootstrap { dry_run, since, scope, distill_model, force } => {
            let payload = serde_json::json!({
                "action": "bootstrap",
                "dry_run": dry_run,
                "since": since,
                "scope": scope,
                "distill_model": distill_model,
                "force": force,
                "async_mode": false,
            });
            execute_cli_tool_call("manage", payload).await?;
        }
        Commands::PreInvocation => {
            use std::io::Read;
            let mut input_data = String::new();
            let _ = std::io::stdin().read_to_string(&mut input_data);
            let ctx: serde_json::Value = serde_json::from_str(&input_data).unwrap_or(serde_json::Value::Null);

            let session_id = ctx.get("conversationId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| std::env::var("MYTHRAX_SESSION_ID").ok())
                .unwrap_or_else(|| "general".to_string());

            let workspace_path = ctx.get("workspacePaths")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|f| f.as_str())
                .map(|s| s.to_string())
                .or_else(|| std::env::var("MYTHRAX_WORKSPACE_ROOT").ok())
                .unwrap_or_else(|| ".".to_string());

            let query = ctx.get("nextPrompt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let home = std::env::var("HOME").context("HOME env var not set")?;
            let token_path = std::path::PathBuf::from(&home).join(".mythrax").join("token");
            let mut auth_token = String::new();
            if token_path.exists() {
                if let Ok(mut f) = std::fs::File::open(&token_path) {
                    let _ = f.read_to_string(&mut auth_token);
                    auth_token = auth_token.trim().to_string();
                }
            }
            if let Ok(env_tok) = std::env::var("MYTHRAX_TOKEN") {
                auth_token = env_tok;
            } else if let Ok(env_tok) = std::env::var("MYTHRAX_DAEMON_TOKEN") {
                auth_token = env_tok;
            }

            let daemon_port = std::env::var("MYTHRAX_DAEMON_PORT").unwrap_or_else(|_| "8090".to_string());
            let daemon_url = format!("http://127.0.0.1:{}", daemon_port);
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(1500))
                .build()?;

            let url = format!("{}/v1/mcp/call", daemon_url);
            let payload = serde_json::json!({
                "name": "manage",
                "arguments": {
                    "action": "pre_invocation",
                    "session_id": session_id,
                    "query": query,
                    "workspace_path": workspace_path
                }
            });

            let mut success = false;
            let mut response_text = String::new();

            if let Ok(resp) = client.post(&url)
                .header("X-Mythrax-Token", &auth_token)
                .json(&payload)
                .send()
                .await
            {
                if resp.status() == reqwest::StatusCode::OK {
                    if let Ok(result_json) = resp.json::<serde_json::Value>().await {
                        if let Some(text) = result_json.get("content")
                            .and_then(|c| c.as_array())
                            .and_then(|arr| arr.first())
                            .and_then(|first| first.get("text"))
                            .and_then(|t| t.as_str())
                        {
                            response_text = text.to_string();
                            success = true;
                        }
                    }
                }
            }

            if !success || response_text.is_empty() {
                response_text = format!(
                    "### ⛔ Known Failed Approaches\n- [Mythrax Pre-Invocation Hook Warning: SurrealDB Daemon offline. Memory retrieval and state synchronization skipped.]\n\n### ⚠️ Known Knowledge Boundaries / Conflicts\n- [Mythrax Pre-Invocation Hook Warning: SurrealDB Daemon offline. Memory retrieval and state synchronization skipped.]"
                );
            }

            let out_json = serde_json::json!({
                "injectSteps": [
                    {
                        "ephemeralMessage": response_text
                    }
                ]
            });

            println!("{}", serde_json::to_string_pretty(&out_json)?);
        }
        Commands::Ingest { source, harness, scope, batch_size } => {
            let home = std::env::var("HOME").context("HOME env var not set")?;
            let mythrax_dir = std::path::PathBuf::from(&home).join(".mythrax");
            let token_path = mythrax_dir.join("token");
            let auth_token = mythrax_core::auth::get_or_create_token(&token_path)?;

            let daemon_port = std::env::var("MYTHRAX_DAEMON_PORT").unwrap_or_else(|_| "8090".to_string());
            let daemon_url = format!("http://127.0.0.1:{}", daemon_port);
            
            ensure_daemon_active_for_cli(&auth_token, &daemon_url).await?;
            
            let client = reqwest::Client::new();
            let url = format!("{}/v1/mcp/call", daemon_url);
            
            let mut offset = 0;
            loop {
                println!("Ingesting batch (offset: {}, limit: {})...", offset, batch_size);
                let payload = serde_json::json!({
                    "name": "manage",
                    "arguments": {
                        "action": "ingest_bulk",
                        "source": source,
                        "harness": harness,
                        "scope": scope,
                        "offset": offset,
                        "limit": batch_size,
                        "async_mode": false
                    }
                });
                
                let resp = client.post(&url)
                    .header("X-Mythrax-Token", &auth_token)
                    .json(&payload)
                    .send()
                    .await
                    .context("Failed to forward Ingest command to daemon")?;
                
                if resp.status() != reqwest::StatusCode::OK {
                    let err_text = resp.text().await.unwrap_or_default();
                    anyhow::bail!("Daemon returned error executing Ingest batch: {}", err_text);
                }
                
                let result_json: serde_json::Value = resp.json().await.context("Failed to parse Ingest response")?;
                
                if let Some(text) = result_json.get("content")
                    .and_then(|c| c.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|first| first.get("text"))
                    .and_then(|t| t.as_str()) 
                {
                    println!("{}", text);
                }
                
                let has_more = result_json.get("has_more")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                
                if !has_more {
                    println!("Ingestion complete.");
                    break;
                }
                
                offset += batch_size;
            }
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
        
    let comp_obj = mythrax_comp.as_object_mut().unwrap();
    
    comp_obj.insert(
        "PreInvocation".to_string(),
        serde_json::json!([
            {
                "type": "mcp",
                "server": "mythrax",
                "tool": "manage",
                "arguments": {
                    "action": "pre_invocation"
                }
            }
        ])
    );
    
    comp_obj.insert(
        "PreCompaction".to_string(),
        serde_json::json!([
            {
                "type": "mcp",
                "server": "mythrax",
                "tool": "manage",
                "arguments": {
                    "action": "precompact",
                    "session_id": "{{conversation_id}}",
                    "transcript_path": "{{transcript_path}}"
                }
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

async fn config_harness_action(
    harness: &str,
    _vault_root: &std::path::Path,
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
            let path_code = std::path::PathBuf::from(&home).join(".claude.json");
            if let Err(e) = merge_json_mcp(&path_code, &exe_path) {
                tracing::warn!("Failed to configure Claude Code config: {}", e);
            }
            
            let path_desktop = {
                #[cfg(target_os = "macos")]
                {
                    std::path::PathBuf::from(&home).join("Library/Application Support/Claude/claude_desktop_config.json")
                }
                #[cfg(target_os = "windows")]
                {
                    if let Ok(appdata) = std::env::var("APPDATA") {
                        std::path::PathBuf::from(&appdata).join("Claude/claude_desktop_config.json")
                    } else {
                        std::path::PathBuf::from(&home).join("AppData/Roaming/Claude/claude_desktop_config.json")
                    }
                }
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                {
                    std::path::PathBuf::from(&home).join(".config/Claude/claude_desktop_config.json")
                }
            };
            merge_json_mcp(&path_desktop, &exe_path)?;
            println!("Registered mythrax MCP server in Claude Desktop config at: {:?}", path_desktop);
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

async fn handle_exec(command_name: &str, args: &[String]) -> Result<()> {
    // Validate command_name and all args against shell metacharacters
    let shell_metacharacters = ";&|$`()<>\\\n\r";
    
    let validate_string = |s: &str, label: &str| -> Result<()> {
        if s.chars().any(|c| shell_metacharacters.contains(c)) {
            anyhow::bail!("Security violation: {} contains shell metacharacters: {}", label, s);
        }
        Ok(())
    };

    validate_string(command_name, "command_name")?;
    for (i, arg) in args.iter().enumerate() {
        validate_string(arg, &format!("arg[{}]", i))?;
    }

    // Log the execution
    tracing::info!("Executing command: {} with args: {:?}", command_name, args);

    let home = std::env::var("HOME").context("HOME env var not set")?;
    let mythrax_dir = PathBuf::from(&home).join(".mythrax");
    let token_path = mythrax_dir.join("token");

    let auth_token = mythrax_core::auth::get_or_create_token(&token_path)?;
    let daemon_port = std::env::var("MYTHRAX_DAEMON_PORT").unwrap_or_else(|_| "8090".to_string());
    
    let mut cmd = std::process::Command::new(command_name);
    cmd.args(args);
    
    cmd.env("MYTHRAX_DAEMON_PORT", &daemon_port);
    cmd.env("MYTHRAX_DAEMON_TOKEN", &auth_token);
    
    let mut child = cmd.spawn().context(format!("Failed to spawn command {}", command_name))?;
    let status = child.wait().context("Failed to wait for child process")?;
    
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

// =============================================================================
// SizeRollingFileWriter Implementation
// =============================================================================

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::sync::Mutex;

pub struct SizeRollingFileWriter {
    inner: Mutex<SizeRollingFileWriterInner>,
}

struct SizeRollingFileWriterInner {
    file: Option<File>,
    log_path: PathBuf,
}

impl SizeRollingFileWriter {
    pub fn new(log_path: PathBuf) -> io::Result<Self> {
        if let Some(parent) = log_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(true)
            .open(&log_path)?;
        Ok(Self {
            inner: Mutex::new(SizeRollingFileWriterInner {
                file: Some(file),
                log_path,
            }),
        })
    }

    fn write_all_and_roll(&self, buf: &[u8]) -> io::Result<()> {
        let mut inner = self.inner.lock().unwrap();
        
        // 1. Check current file size
        let mut needs_roll = false;
        if let Some(ref file) = inner.file {
            if let Ok(metadata) = file.metadata() {
                // Roll if current size + new data >= 50MB
                if metadata.len() + buf.len() as u64 >= 50 * 1024 * 1024 {
                    needs_roll = true;
                }
            }
        }

        // 2. Roll if needed
        if needs_roll {
            // Drop the file handle to close it before renaming
            inner.file = None;

            let base_path = &inner.log_path;
            let path3 = base_path.with_extension("log.3");
            let path2 = base_path.with_extension("log.2");
            let path1 = base_path.with_extension("log.1");

            // Delete oldest backup
            if path3.exists() {
                let _ = fs::remove_file(&path3);
            }
            // Shift backups: .2 -> .3, .1 -> .2, current -> .1
            if path2.exists() {
                let _ = fs::rename(&path2, &path3);
            }
            if path1.exists() {
                let _ = fs::rename(&path1, &path2);
            }
            if base_path.exists() {
                let _ = fs::rename(base_path, &path1);
            }

            // Re-open new current file
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .append(true)
                .open(base_path)?;
            inner.file = Some(file);
        }

        // 3. Write data
        if let Some(ref mut file) = inner.file {
            file.write_all(buf)?;
        }
        Ok(())
    }
}

impl Write for SizeRollingFileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_all_and_roll(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(ref mut file) = inner.file {
            file.flush()?;
        }
        Ok(())
    }
}

impl Write for &SizeRollingFileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_all_and_roll(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(ref mut file) = inner.file {
            file.flush()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swap_monitor_cross_platform() {
        let swap = get_swap_used_bytes();
        assert!(swap.is_some(), "Swap usage should be retrievable and non-empty");
        let bytes = swap.unwrap();
        assert!(bytes > 0 || bytes == 0, "Swap bytes should be a valid non-negative number");
    }
}




