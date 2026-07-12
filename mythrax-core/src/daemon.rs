use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, Duration};
use sysinfo::{Pid, System, Signal};
use anyhow::{Context, Result};
use crate::db::{SurrealBackend, StorageBackend};
use crate::store::MarkdownStore;
use crate::vault::watcher::WatchIgnoreList;
use crate::contracts::Episode;
use crate::cli::{DaemonAction, run_auditor};
use crate::api;
use crate::auth;
use crate::cognitive;
use crate::vault;

/// Handles background daemon operations (start, run, stop).
pub async fn handle_daemon(action: DaemonAction) -> Result<()> {
    match action {
        DaemonAction::Start { port, vault } | DaemonAction::Run { port, vault } => {
            let home = std::env::var("HOME").context("HOME env var not set")?;
            let mythrax_dir = PathBuf::from(&home).join(".mythrax");
            let config_path = mythrax_dir.join("config.json");
            let token_path = mythrax_dir.join("token");

            let vault_path = if let Some(v) = vault {
                PathBuf::from(v)
            } else if config_path.exists() {
                let config_content = std::fs::read_to_string(&config_path)?;
                let config_val: serde_json::Value = serde_json::from_str(&config_content)?;
                PathBuf::from(config_val["vault_root"].as_str().unwrap_or(&format!("{}/mythrax-vault", home)))
            } else {
                PathBuf::from(&home).join("mythrax-vault")
            };

            let auth_token = auth::get_or_create_token(&token_path)?;

            let surreal_url = std::env::var("MYTHRAX_DB_URL")
                .ok()
                .or_else(|| {
                    if config_path.exists() {
                        let content = std::fs::read_to_string(&config_path).ok()?;
                        let val: serde_json::Value = serde_json::from_str(&content).ok()?;
                        val["surrealdb_url"].as_str().map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| format!("surrealkv://{}/.mythrax/db", home));

            println!("Starting Mythrax Core Daemon...");
            println!("Vault root: {:?}", vault_path);
            println!("Port: {}", port);
            println!("Database URL: {}", surreal_url);

            // Write PID file
            std::fs::create_dir_all(&mythrax_dir)?;
            let pid_path = mythrax_dir.join("daemon.pid");
            let pid = std::process::id();
            std::fs::write(&pid_path, pid.to_string())?;

            let run_res = async {
                // Initialize storage backend
                let backend = Arc::new(SurrealBackend::new(&surreal_url).await?);
                backend.init().await?;

                // Initialize Model Broker and set globalOnceLock
                if let Ok(broker) = crate::llm::DynamicModelBroker::new(mythrax_dir.join("models")).await {
                    let _ = crate::llm::DYNAMIC_MODEL_BROKER.set(Arc::new(broker));
                }

                // Run initial stale memory/handoff pruning on startup
                if let Err(e) = backend.prune_stale_memories(&vault_path).await {
                    tracing::error!("Failed to run startup memory pruning: {:?}", e);
                }

                // Reprocess missing embeddings on startup
                let backend_startup = backend.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    if backend_startup.embedder.is_some() {
                        tracing::info!("Checking for episodes with missing embeddings...");
                        let sql = "SELECT * FROM episode WHERE embedding IS NONE;";
                        match backend_startup.db.query(sql).await {
                            Ok(mut response) => {
                                if let Ok(episodes) = response.take::<Vec<Episode>>(0)
                                    && !episodes.is_empty() {
                                        tracing::info!("Found {} episodes with missing embeddings. Regenerating...", episodes.len());
                                        for ep in episodes {
                                            if let (Some(id_str), Some(embedder)) = (&ep.id, &backend_startup.embedder) {
                                                let text_to_embed = format!("{}: {}", ep.title, ep.content);
                                                  if let Ok(vec) = embedder.embed(&text_to_embed)
                                                     && let Ok(thing) = crate::db::parse_record_id(id_str) {
                                                         let update_sql = "UPDATE $id SET embedding = $embedding;";
                                                        let _ = backend_startup.db.query(update_sql)
                                                            .bind(("id", thing))
                                                            .bind(("embedding", vec))
                                                            .await;
                                                    }
                                            }
                                        }
                                        tracing::info!("Finished regenerating missing embeddings.");
                                    }
                            }
                            Err(e) => {
                                tracing::error!("Failed to query missing embeddings on startup: {:?}", e);
                            }
                        }
                    }
                });

                // Initialize Markdown Store
                let store = Arc::new(MarkdownStore::new(&vault_path)?);

                // Initialize Watch Ignore List
                let ignore_list = Arc::new(WatchIgnoreList::new());

                // Setup dreaming channel
                let (dream_tx, mut dream_rx) = tokio::sync::mpsc::channel::<()>(100);

                // Start File-Watcher
                let _watcher = vault::watcher::start_watching(
                    vault_path.clone(),
                    ignore_list.clone(),
                    backend.clone(),
                    store.clone(),
                    Some(dream_tx.clone()),
                )?;

                // Spawn background checkpointing daemon
                let backend_chk = backend.clone();
                let vault_chk = vault_path.clone();
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_secs(600)).await; // 10 minutes
                        if let Err(e) = run_checkpoint(&*backend_chk, &vault_chk).await {
                            tracing::error!("Checkpointing daemon error: {:?}", e);
                        }
                    }
                });

                // Spawn background embedding cache flusher
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await; // every 60 seconds
                        if let Err(e) = crate::embeddings::flush_dirty_default() {
                            tracing::error!("Background embedding cache flush failed: {:?}", e);
                        }
                    }
                });

                // Spawn the tokio background scheduler loop
                let backend_dream = backend.clone();
                let store_dream = store.clone();
                tokio::spawn(async move {
                    let dream_coordinator = cognitive::synthesis::DreamCoordinator::new();
                    let compactor = cognitive::compactor::Compactor::new();

                    // Spawn daily scheduler
                    let backend_daily = backend_dream.clone();
                    let store_daily = store_dream.clone();
                    tokio::spawn(async move {
                        let dc = cognitive::synthesis::DreamCoordinator::new();
                        let cmp = cognitive::compactor::Compactor::new();
                        loop {
                            tokio::time::sleep(tokio::time::Duration::from_secs(24 * 3600)).await;
                            
                            tracing::info!("Daily scheduled background handoff cleanup starting...");
                            let pruning_days = match backend_daily.get_profile_key("stm.pruning_days").await {
                                Ok(Some(val_str)) => val_str.parse::<i64>().unwrap_or(7),
                                _ => std::env::var("MYTHRAX_STM_PRUNING_DAYS")
                                    .ok()
                                    .and_then(|v| v.parse::<i64>().ok())
                                    .unwrap_or(7),
                            };
                            if let Err(e) = backend_daily.delete_stale_handoffs(pruning_days).await {
                                tracing::error!("Daily stale handoff cleanup failed: {:?}", e);
                            }

                            tracing::info!("Daily scheduled deep dreaming starting...");
                            if let Err(e) = dc.run_dream(&*backend_daily, &store_daily, Some("deep"), backend_daily.embedder.clone()).await {
                                tracing::error!("Daily deep dreaming failed: {:?}", e);
                            } else {
                                tracing::info!("Deep dreaming synthesis completed. Running compactions...");
                                let mut scopes = backend_daily.get_active_scopes().await.unwrap_or_default();
                                if scopes.is_empty() {
                                    scopes.push("general".to_string());
                                }
                                for scope in scopes {
                                    let _ = cmp.compact_scope(&*backend_daily, &store_daily, &scope, backend_daily.embedder.clone()).await;
                                }
                                let _ = cmp.compact_global(&*backend_daily, &store_daily).await;
                            }
                            
                            tracing::info!("Daily scheduled auditor calibration starting...");
                            if let Err(e) = run_auditor(&*backend_daily).await {
                                tracing::error!("Daily auditor calibration failed: {:?}", e);
                            }
                        }
                    });

                    let mut last_activity = Instant::now();
                    let mut pending_debounce = false;

                    loop {
                        tokio::select! {
                            val = dream_rx.recv() => {
                                match val {
                                    Some(_) => {
                                        last_activity = Instant::now();

                                        // Check threshold triggered synthesis (> 50 unprocessed)
                                        if let Ok(unprocessed) = backend_dream.get_unprocessed_episodes().await
                                            && unprocessed.len() > 50 {
                                                tracing::info!("Threshold dreaming triggered ({} unprocessed episodes).", unprocessed.len());
                                                if let Err(e) = dream_coordinator.run_dream(&*backend_dream, &store_dream, Some("incremental"), backend_dream.embedder.clone()).await {
                                                    tracing::error!("Threshold dreaming failed: {:?}", e);
                                                } else {
                                                    let mut scopes = backend_dream.get_active_scopes().await.unwrap_or_default();
                                                    if scopes.is_empty() {
                                                        scopes.push("general".to_string());
                                                    }
                                                    for scope in scopes {
                                                        let _ = compactor.compact_scope(&*backend_dream, &store_dream, &scope, backend_dream.embedder.clone()).await;
                                                    }
                                                    let _ = compactor.compact_global(&*backend_dream, &store_dream).await;
                                                }
                                                pending_debounce = false;
                                                continue;
                                            }
                                        pending_debounce = true;
                                    }
                                    None => {
                                        break;
                                    }
                                }
                            }
                            _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)), if pending_debounce => {
                                if last_activity.elapsed() >= tokio::time::Duration::from_secs(30) {
                                    pending_debounce = false;

                                    if let Ok(unprocessed) = backend_dream.get_unprocessed_episodes().await
                                        && !unprocessed.is_empty() {
                                            tracing::info!("Idle debounced synthesis starting...");
                                            if let Err(e) = dream_coordinator.run_dream(&*backend_dream, &store_dream, Some("incremental"), backend_dream.embedder.clone()).await {
                                                tracing::error!("Debounced incremental dreaming failed: {:?}", e);
                                            } else {
                                                let mut scopes = backend_dream.get_active_scopes().await.unwrap_or_default();
                                                if scopes.is_empty() {
                                                    scopes.push("general".to_string());
                                                }
                                                for scope in scopes {
                                                    let _ = compactor.compact_scope(&*backend_dream, &store_dream, &scope, backend_dream.embedder.clone()).await;
                                                }
                                                let _ = compactor.compact_global(&*backend_dream, &store_dream).await;
                                            }
                                        }
                                }
                            }
                        }
                    }
                });

                let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

                // Create API State
                let state = Arc::new(api::ApiState {
                    backend,
                    auth_token,
                    store: store.clone(),
                    ignore_list: ignore_list.clone(),
                    dream_tx: Some(dream_tx),
                    shutdown_tx: Some(shutdown_tx),
                });

                // Build router and start Axum listener
                let app = api::create_router(state);
                let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
                
                let listener = tokio::net::TcpListener::bind(&addr).await?;
                let pid_path_clone = pid_path.clone();
                
                #[cfg(unix)]
                let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("Failed to register SIGTERM handler");

                tokio::select! {
                    res = axum::serve(listener, app) => {
                        if let Err(e) = res {
                            tracing::error!("Daemon server crashed: {:?}", e);
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        tracing::info!("Shutdown channel triggered. Initiating graceful shutdown...");
                        let shutdown_sequence = async {
                            run_shutdown(pid_path_clone).await;
                        };
                        if let Err(_) = tokio::time::timeout(tokio::time::Duration::from_secs(5), shutdown_sequence).await {
                            tracing::warn!("Graceful shutdown timed out after 5 seconds.");
                            let _ = std::fs::remove_file(&pid_path);
                        }
                        tracing::info!("Shutdown complete.");
                    }
                    _ = tokio::signal::ctrl_c() => {
                         tracing::info!("SIGINT/Ctrl+C received. Initiating graceful shutdown...");
                         let shutdown_sequence = async {
                             run_shutdown(pid_path_clone).await;
                         };
                         if let Err(_) = tokio::time::timeout(tokio::time::Duration::from_secs(5), shutdown_sequence).await {
                             tracing::warn!("Graceful shutdown timed out after 5 seconds.");
                             let _ = std::fs::remove_file(&pid_path);
                         }
                         tracing::info!("Shutdown complete.");
                    }
                    _ = async {
                        #[cfg(unix)]
                        {
                            sigterm.recv().await;
                        }
                        #[cfg(not(unix))]
                        {
                            std::future::pending::<()>().await;
                        }
                    } => {
                        tracing::info!("SIGTERM received. Initiating graceful shutdown...");
                        let shutdown_sequence = async {
                            run_shutdown(pid_path_clone).await;
                        };
                        if let Err(_) = tokio::time::timeout(tokio::time::Duration::from_secs(5), shutdown_sequence).await {
                            tracing::warn!("Graceful shutdown timed out after 5 seconds.");
                            let _ = std::fs::remove_file(&pid_path);
                        }
                        tracing::info!("Shutdown complete.");
                    }
                }
                Ok::<(), anyhow::Error>(())
            }.await;

            let _ = std::fs::remove_file(pid_path);
            run_res?;
        }
        DaemonAction::Stop => {
            stop_daemon().await?;
        }
    }
    Ok(())
}

async fn run_checkpoint(backend: &SurrealBackend, _vault_root: &Path) -> Result<()> {
    let workspace_root = std::env::var("MYTHRAX_WORKSPACE_ROOT")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let mut project_type = "unknown";
    let mut check_cmd = vec![];
    
    if workspace_root.join("Cargo.toml").exists() {
        project_type = "rust";
        check_cmd = vec!["cargo", "check"];
    } else if workspace_root.join("package.json").exists() {
        project_type = "typescript";
        check_cmd = vec!["npx", "tsc", "--noEmit"];
    } else {
        let has_py = std::fs::read_dir(&workspace_root)
            .map(|dir| dir.flatten().any(|entry| entry.path().extension().map_or(false, |ext| ext == "py")))
            .unwrap_or(false);
        if has_py {
            project_type = "python";
            check_cmd = vec!["python", "-m", "py_compile"];
        }
    }

    let check_cmd_clone = check_cmd.clone();
    let workspace_clone = workspace_root.clone();
    
    let compile_result = tokio::task::spawn_blocking(move || {
        if check_cmd_clone.is_empty() {
            return (0, String::new());
        }
        let output = std::process::Command::new(check_cmd_clone[0])
            .args(&check_cmd_clone[1..])
            .current_dir(&workspace_clone)
            .output();
        match output {
            Ok(out) => {
                let exit_code = out.status.code().unwrap_or(0);
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                (exit_code, stderr)
            }
            Err(e) => (-1, e.to_string())
        }
    }).await.unwrap_or((-2, "Thread panic".to_string()));

    let git_diff = tokio::task::spawn_blocking(move || {
        let output = std::process::Command::new("git")
            .args(&["diff"])
            .current_dir(&workspace_root)
            .output();
        match output {
            Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
            Err(e) => e.to_string()
        }
    }).await.unwrap_or_else(|_| "Thread panic".to_string());

    let checkpoint_id = format!("checkpoint_{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs());

    let sql = "
        UPSERT type::record('checkpoint_node', $id) CONTENT {
            project_type: $project_type,
            exit_code: $exit_code,
            compiler_errors: $compiler_errors,
            git_diff: $git_diff,
            timestamp: time::now()
        };
    ";
    backend.db.query(sql)
        .bind(("id", checkpoint_id.clone()))
        .bind(("project_type", project_type))
        .bind(("exit_code", compile_result.0))
        .bind(("compiler_errors", compile_result.1))
        .bind(("git_diff", git_diff))
        .await?.check()?;

    tracing::info!("Saved CheckpointNode: {}", checkpoint_id);
    Ok(())
}
async fn run_shutdown(pid_path: PathBuf) {
    // Sleep for 500ms to allow pending watcher/DB operations to settle
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Flush dirty embedding cache entries robustly on shutdown
    if let Err(e) = crate::embeddings::flush_dirty_default() {
        tracing::error!("Failed to flush embedding cache on shutdown: {:?}", e);
    }

    // Evict unused models
    if let Some(broker) = crate::llm::DYNAMIC_MODEL_BROKER.get() {
        broker.evict_unused_models().await;
    }

    // Log Metal cache clearing
    tracing::info!("Metal cache cleared.");

    // Remove PID file
    let _ = std::fs::remove_file(&pid_path);
}

pub async fn stop_daemon() -> Result<()> {
    let home = std::env::var("HOME").context("HOME env var not set")?;
    let mythrax_dir = PathBuf::from(&home).join(".mythrax");
    
    // Attempt stopping via HTTP POST request
    let token_path = mythrax_dir.join("token");
    let auth_token = crate::auth::get_or_create_token(&token_path).ok();
    
    let port_str = std::env::var("MYTHRAX_DAEMON_PORT").unwrap_or_else(|_| "8090".to_string());
    if let (Some(token), Ok(port)) = (auth_token, port_str.parse::<u16>()) {
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/v1/daemon/stop", port);
        match client.post(&url)
            .header("X-Mythrax-Token", &token)
            .timeout(Duration::from_secs(2))
            .send().await {
                Ok(resp) if resp.status().is_success() => {
                    println!("Successfully sent stop request to daemon on port {}", port);
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    return Ok(());
                }
                _ => {}
            }
    }

    let pid_path = mythrax_dir.join("daemon.pid");
    if pid_path.exists() {
        let content = std::fs::read_to_string(&pid_path)?;
        let pid_str = content.trim();
        if let Ok(pid_usize) = pid_str.parse::<usize>() {
            let pid = Pid::from(pid_usize);
            println!("Stopping daemon process with PID: {}", pid);
            let mut system = System::new_all();
            if system.process(pid).is_some() {
                if let Some(process) = system.process(pid) {
                    process.kill_with(Signal::Term);
                }
                let start = std::time::Instant::now();
                while start.elapsed() < Duration::from_secs(1) {
                    system.refresh_processes();
                    if system.process(pid).is_none() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                if system.process(pid).is_some() {
                    println!("Process did not exit, sending SIGKILL...");
                    if let Some(process) = system.process(pid) {
                        process.kill_with(Signal::Kill);
                    }
                    std::thread::sleep(Duration::from_millis(500));
                }
            } else {
                println!("Process with PID {} not found.", pid);
            }
        }
        let _ = std::fs::remove_file(pid_path);
        println!("Daemon stopped.");
    } else {
        println!("No running daemon found (no PID file).");
    }
    Ok(())
}

pub fn backup_vault_folders(vault_root: &Path) -> Result<()> {
    let folders = ["episodes", "wiki", "wisdom", "general", "archive"];
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let backup_dir = vault_root.join(".trash").join(format!("backup_{}", timestamp));
    
    let mut has_files = false;
    for f in &folders {
        if vault_root.join(f).exists() {
            has_files = true;
            break;
        }
    }
    
    if has_files {
        std::fs::create_dir_all(&backup_dir)?;
        for f in &folders {
            let src = vault_root.join(f);
            if src.exists() {
                let dst = backup_dir.join(f);
                if std::fs::rename(&src, &dst).is_err() {
                    copy_dir_all(&src, &dst)?;
                    let _ = std::fs::remove_dir_all(&src);
                }
            }
        }
    }
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

pub mod monitor {
    #[cfg(target_os = "macos")]
    use std::ffi::CString;

    #[cfg(target_os = "linux")]
    use std::ffi::CString;

    pub fn check_disk_space(path: &std::path::Path, required_bytes: u64) -> anyhow::Result<()> {
        let canonical_path = path.canonicalize()?;

        #[cfg(target_os = "macos")]
        {
            let c_path = CString::new(canonical_path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?)?;
            let mut buf: libc::statfs = unsafe { std::mem::zeroed() };
            let res = unsafe { libc::statfs(c_path.as_ptr(), &mut buf) };
            if res != 0 {
                return Err(anyhow::anyhow!("Failed to get filesystem stats"));
            }
            let available_bytes = (buf.f_bavail as u64) * (buf.f_bsize as u64);
            if available_bytes < required_bytes {
                return Err(anyhow::anyhow!(
                    "Insufficient disk space. Required: {}, Available: {}",
                    required_bytes,
                    available_bytes
                ));
            }
        }

        #[cfg(target_os = "linux")]
        {
            let c_path = CString::new(canonical_path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?)?;
            let mut buf: libc::statfs = unsafe { std::mem::zeroed() };
            let res = unsafe { libc::statfs(c_path.as_ptr(), &mut buf) };
            if res != 0 {
                return Err(anyhow::anyhow!("Failed to get filesystem stats"));
            }
            let available_bytes = (buf.f_bavail as u64) * (buf.f_bsize as u64);
            if available_bytes < required_bytes {
                return Err(anyhow::anyhow!(
                    "Insufficient disk space. Required: {}, Available: {}",
                    required_bytes,
                    available_bytes
                ));
            }
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            return Err(anyhow::anyhow!("Unsupported platform for disk space check"));
        }

        Ok(())
    }
    pub fn check_swap_pressure(tier: crate::llm::ModelTier, swap_used_bytes: u64) -> bool {
        let threshold = match tier {
            crate::llm::ModelTier::Tier1 => 2_000 * 1024 * 1024,
            crate::llm::ModelTier::Tier2 => 3_000 * 1024 * 1024,
            crate::llm::ModelTier::Tier3 => 6_000 * 1024 * 1024,
        };
        swap_used_bytes >= threshold
    }
}
