use crate::api;
use crate::auth;
use crate::cli::{DaemonAction, run_auditor};
use crate::cognitive;
use crate::contracts::Episode;
use crate::db::{StorageBackend, SurrealBackend};
use crate::store::MarkdownStore;
use crate::vault;
use crate::vault::watcher::WatchIgnoreList;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::{Pid, Signal, System};

pub static LAST_ACTIVITY_TIME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static IS_SYNCING_WORKSPACE_DOCS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub struct SyncWorkspaceDocsGuard;
impl SyncWorkspaceDocsGuard {
    pub fn new() -> Option<Self> {
        if IS_SYNCING_WORKSPACE_DOCS
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            )
            .is_ok()
        {
            Some(Self)
        } else {
            None
        }
    }
}
impl Drop for SyncWorkspaceDocsGuard {
    fn drop(&mut self) {
        IS_SYNCING_WORKSPACE_DOCS.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

pub fn update_last_activity() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    LAST_ACTIVITY_TIME.store(now, std::sync::atomic::Ordering::SeqCst);
}

/// Handles background daemon operations (start, run, stop).
pub async fn handle_daemon(action: DaemonAction) -> Result<()> {
    update_last_activity();
    match action {
        DaemonAction::Start { port, vault } | DaemonAction::Run { port, vault } => {
            #[cfg(unix)]
            {
                if let Ok((soft, hard)) = rlimit::getrlimit(rlimit::Resource::NOFILE) {
                    if soft < 1024 {
                        let new_soft = if hard >= 1024 { 1024 } else { hard };
                        if let Err(e) = rlimit::setrlimit(rlimit::Resource::NOFILE, new_soft, hard)
                        {
                            tracing::warn!(
                                "Failed to set RLIMIT_NOFILE soft limit to {}: {:?}",
                                new_soft,
                                e
                            );
                        } else {
                            tracing::info!(
                                "Successfully increased RLIMIT_NOFILE soft limit to {}",
                                new_soft
                            );
                        }
                    }
                }
            }

            let home = std::env::var("HOME").context("HOME env var not set")?;
            let mythrax_dir = PathBuf::from(&home).join(".mythrax");
            let config_path = mythrax_dir.join("config.json");
            let token_path = mythrax_dir.join("token");

            let vault_path = if let Some(v) = vault {
                PathBuf::from(v)
            } else if config_path.exists() {
                let config_content = std::fs::read_to_string(&config_path)?;
                let config_val: serde_json::Value = serde_json::from_str(&config_content)?;
                PathBuf::from(
                    config_val["vault_root"]
                        .as_str()
                        .unwrap_or(&format!("{}/mythrax-vault", home)),
                )
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
                .unwrap_or_else(|| format!("surrealkv://{}/.mythrax/db.nosync", home));

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
                let cancel_token = tokio_util::sync::CancellationToken::new();

                // Composition root: inject mock dependencies when test env vars are set
                let backend_config = if crate::is_test_mock() {
                    crate::db::BackendConfig {
                        check_daemon: false,
                        embedder: Some(Arc::new(crate::embeddings::MockEmbedder)),
                        llm: Some(crate::llm::LLMClient::new_mock()),
                    }
                } else {
                    crate::db::BackendConfig::default()
                };
                // Initialize storage backend
                let backend = Arc::new(SurrealBackend::new(&surreal_url, backend_config).await?);
                backend.init().await?;

                // Initialize Bounded MPSC Blackboard channel
                let (blackboard_tx, blackboard_rx) = tokio::sync::mpsc::channel(1000);
                backend.set_blackboard_sender(blackboard_tx.clone());

                // Spawn MaterializerActor loop as a background Tokio task
                let blackboard_actor = crate::db::blackboard::MaterializerActor::new(backend.clone(), blackboard_rx);
                let blackboard_handle = tokio::spawn(blackboard_actor.run());

                // Initialize Model Broker and set globalOnceLock
                if let Ok(broker) = crate::llm::DynamicModelBroker::new(mythrax_dir.join("models")).await {
                    let _ = crate::llm::DYNAMIC_MODEL_BROKER.set(Arc::new(broker));
                }

                // Run initial stale memory/handoff pruning on startup
                if let Err(e) = backend.prune_stale_memories(&vault_path).await {
                    tracing::error!("Failed to run startup memory pruning: {:?}", e);
                }

                async fn backfill_missing_embeddings<T, F>(
                    db: &surrealdb::Surreal<surrealdb::engine::local::Db>,
                    embedder: &std::sync::Arc<dyn crate::embeddings::TextEmbedder>,
                    cancel_token: &tokio_util::sync::CancellationToken,
                    query_sql: &str,
                    table_name: &str,
                    extract_text_and_id: F,
                ) where
                    T: serde::de::DeserializeOwned + surrealdb_types::SurrealValue,
                    F: Fn(&T) -> Option<(String, String)>,
                {
                    loop {
                        if cancel_token.is_cancelled() {
                            break;
                        }
                        match db.query(query_sql).await {
                            Ok(mut response) => {
                                let items: Vec<T> = response.take(0).unwrap_or_default();
                                if items.is_empty() {
                                    break;
                                }
                                tracing::info!("Found {} {} with missing embeddings batch. Regenerating...", items.len(), table_name);
                                let mut updated_any = false;
                                for item in items {
                                    update_last_activity();
                                    if cancel_token.is_cancelled() {
                                        break;
                                    }
                                    if let Some((id_str, text_to_embed)) = extract_text_and_id(&item) {
                                        if let Ok(vec) = embedder.embed(&text_to_embed).await {
                                            if let Ok(thing) = crate::db::parse_record_id(&id_str) {
                                                let update_sql = "UPDATE $id SET embedding = $embedding;";
                                                if let Ok(mut u_res) = db.query(update_sql)
                                                    .bind(("id", thing))
                                                    .bind(("embedding", vec))
                                                    .await {
                                                    if let Ok(updated_rows) = u_res.take::<Vec<serde_json::Value>>(0) {
                                                        if !updated_rows.is_empty() {
                                                            updated_any = true;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if !updated_any {
                                    break;
                                }
                            }
                            Err(e) => {
                                tracing::error!("Failed to query missing {} embeddings on startup: {:?}", table_name, e);
                                break;
                            }
                        }
                    }
                }

                // Reprocess missing embeddings on startup
                let backend_startup = backend.clone();
                let cancel_token_startup = cancel_token.clone();
                tokio::spawn(async move {
                    tokio::select! {
                        _ = cancel_token_startup.cancelled() => {
                            tracing::info!("Startup missing embeddings task received cancellation signal, stopping");
                            return;
                        }
                        _ = tokio::time::sleep(tokio::time::Duration::from_secs(2)) => {}
                    }
                    if let Some(ref embedder) = backend_startup.embedder {
                        tracing::info!("Checking for episodes and wisdom rules with missing embeddings...");

                        backfill_missing_embeddings::<Episode, _>(
                            &backend_startup.db,
                            embedder,
                            &cancel_token_startup,
                            "SELECT * FROM episode WHERE embedding IS NONE LIMIT 50;",
                            "episodes",
                            |ep| {
                                let id = ep.id.as_ref()?;
                                let insight_str = ep.causal_insight.as_ref().map(|v| v.to_string()).or_else(|| ep.causal_explanation.clone());
                                let text = if let Some(ref insight) = insight_str {
                                    format!("{}: {}", ep.title, insight)
                                } else if let Some(ref summary) = ep.summary {
                                    format!("{}: {}", ep.title, summary)
                                } else {
                                    format!("{}: {}", ep.title, ep.content)
                                };
                                Some((id.clone(), text))
                            },
                        ).await;

                        backfill_missing_embeddings::<crate::contracts::WisdomRule, _>(
                            &backend_startup.db,
                            embedder,
                            &cancel_token_startup,
                            "SELECT * FROM wisdom WHERE embedding IS NONE OR embedding = [] LIMIT 50;",
                            "wisdom rules",
                            |r| {
                                let id = r.id.as_ref()?;
                                let text = format!("{}: Avoid {}. Remedy: {}. Reason: {}", r.target_pattern, r.action_to_avoid, r.prescribed_remedy, r.causal_explanation);
                                Some((id.clone(), text))
                            },
                        ).await;

                        backfill_missing_embeddings::<crate::contracts::WikiNode, _>(
                            &backend_startup.db,
                            embedder,
                            &cancel_token_startup,
                            "SELECT * FROM wiki_node WHERE embedding IS NONE OR embedding = [] LIMIT 50;",
                            "wiki nodes",
                            |node| {
                                let id = node.id.as_ref()?;
                                let text = format!("{}: {}", node.name, node.content);
                                Some((id.clone(), text))
                            },
                        ).await;

                        tracing::info!("Finished regenerating missing embeddings.");
                    }
                });

                // Initialize Markdown Store
                let store = Arc::new(MarkdownStore::new(&vault_path)?);

                // Initialize Watch Ignore List
                let ignore_list = Arc::new(WatchIgnoreList::new());
                if let Some(surreal_backend) = backend.as_any().downcast_ref::<crate::db::SurrealBackend>() {
                    *surreal_backend.watch_ignore_list.write().await = Some(ignore_list.clone());
                }

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
                let store_chk = store.clone();
                let vault_chk = vault_path.clone();
                let cancel_token_chk = cancel_token.clone();
                tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            _ = cancel_token_chk.cancelled() => {
                                tracing::info!("Checkpointing daemon received cancellation signal, stopping loop");
                                break;
                            }
                            _ = tokio::time::sleep(tokio::time::Duration::from_secs(600)) => {
                                update_last_activity();
                                if let Err(e) = run_checkpoint(&*backend_chk, &vault_chk).await {
                                    tracing::error!("Checkpointing daemon error: {:?}", e);
                                }
                                let ws_root = std::env::var("MYTHRAX_WORKSPACE_ROOT")
                                    .ok()
                                    .map(PathBuf::from)
                                    .unwrap_or_else(|| crate::store::get_workspace_root().unwrap_or_else(|| std::env::current_dir().unwrap_or_default()));
                                if let Err(e) = crate::vault::ingestion::sync_workspace_docs_to_vault(&ws_root, &store_chk, &*backend_chk).await {
                                    tracing::error!("Checkpoint workspace docs sync failed: {:?}", e);
                                }
                            }
                        }
                    }
                });

                // Run workspace docs sync on startup
                let backend_ws = backend.clone();
                let store_ws = store.clone();
                let cancel_token_ws = cancel_token.clone();
                tokio::spawn(async move {
                    tokio::select! {
                        _ = cancel_token_ws.cancelled() => {
                            tracing::info!("Startup workspace docs sync received cancellation signal, stopping");
                        }
                        _ = tokio::time::sleep(tokio::time::Duration::from_millis(500)) => {
                            let ws_root = std::env::var("MYTHRAX_WORKSPACE_ROOT")
                                .ok()
                                .map(PathBuf::from)
                                .unwrap_or_else(|| crate::store::get_workspace_root().unwrap_or_else(|| std::env::current_dir().unwrap_or_default()));
                            if let Err(e) = crate::vault::ingestion::sync_workspace_docs_to_vault(&ws_root, &store_ws, &*backend_ws).await {
                                tracing::error!("Startup workspace docs sync failed: {:?}", e);
                            }
                        }
                    }
                });

                // Spawn background embedding cache flusher
                let cancel_token_flush = cancel_token.clone();
                tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            _ = cancel_token_flush.cancelled() => {
                                tracing::info!("Embedding cache flusher received cancellation signal, stopping loop");
                                break;
                            }
                            _ = tokio::time::sleep(tokio::time::Duration::from_secs(60)) => {
                                if let Err(e) = crate::embeddings::flush_dirty_default() {
                                    tracing::error!("Background embedding cache flush failed: {:?}", e);
                                }
                            }
                        }
                    }
                });

                // Spawn reflection harvester loop
                let backend_harvest = backend.clone();
                let cancel_token_harvest = cancel_token.clone();
                tokio::spawn(async move {
                    let mut task_rx = crate::vault::distillation::get_cognitive_task_bus().subscribe();
                    loop {
                        tokio::select! {
                            _ = cancel_token_harvest.cancelled() => {
                                tracing::info!("Reflection harvester received cancellation signal, stopping loop");
                                break;
                            }
                            _ = task_rx.recv() => {
                                update_last_activity();
                                if let Err(e) = crate::hooks::reflect::harvest_completed_reflections(&*backend_harvest).await {
                                    tracing::error!("Reflection harvester failed: {:?}", e);
                                }
                            }
                            _ = tokio::time::sleep(tokio::time::Duration::from_secs(60)) => {
                                update_last_activity();
                                if let Err(e) = crate::hooks::reflect::harvest_completed_reflections(&*backend_harvest).await {
                                    tracing::error!("Reflection harvester failed: {:?}", e);
                                }
                            }
                        }
                    }
                });

                // Spawn the tokio background scheduler loop
                let backend_dream = backend.clone();
                let store_dream = store.clone();
                let cancel_token_dream = cancel_token.clone();
                tokio::spawn(async move {
                    // Spawn daily scheduler
                    let backend_daily = backend_dream.clone();
                    let _store_daily = store_dream.clone();
                    let cancel_token_daily = cancel_token_dream.clone();
                    tokio::spawn(async move {
                        loop {
                            tokio::select! {
                                _ = cancel_token_daily.cancelled() => {
                                    tracing::info!("Daily scheduler received cancellation signal, stopping loop");
                                    break;
                                }
                                _ = tokio::time::sleep(tokio::time::Duration::from_secs(24 * 3600)) => {
                                    update_last_activity();
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
                                    let mut scopes = backend_daily.get_active_scopes().await.unwrap_or_default();
                                    if scopes.is_empty() {
                                        scopes.push("general".to_string());
                                    }
                                    for scope in scopes {
                                        let _ = cognitive::pipeline::refine_hypotheses(backend_daily.as_ref(), None, &scope).await;
                                    }

                                    tracing::info!("Daily scheduled auditor calibration starting...");
                                    if let Err(e) = run_auditor(&*backend_daily).await {
                                        tracing::error!("Daily auditor calibration failed: {:?}", e);
                                    }
                                }
                            }
                        }
                    });

                    let mut last_activity = Instant::now();
                    let mut pending_debounce = false;
                    let mut idle_timer = tokio::time::interval(tokio::time::Duration::from_secs(30));

                    loop {
                        tokio::select! {
                            _ = cancel_token_dream.cancelled() => {
                                tracing::info!("Dreaming coordinator received cancellation signal, stopping loop");
                                break;
                            }
                            _ = idle_timer.tick() => {
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_secs())
                                    .unwrap_or(0);
                                let last = LAST_ACTIVITY_TIME.load(std::sync::atomic::Ordering::SeqCst);
                                if last > 0 && now.saturating_sub(last) >= 30 {
                                    crate::llm::evict_global_reranker().await;
                                    if let Some(broker) = crate::llm::DYNAMIC_MODEL_BROKER.get() {
                                        broker.evict_unused_models().await;
                                    }
                                }
                            }
                            val = dream_rx.recv() => {
                                match val {
                                    Some(_) => {
                                        update_last_activity();
                                        last_activity = Instant::now();

                                        // Check threshold triggered synthesis (> 50 unprocessed)
                                        if let Ok(unprocessed) = backend_dream.get_unprocessed_episodes_paginated(51, 0).await
                                            && unprocessed.len() >= 50 {
                                                tracing::info!("Threshold dreaming triggered ({} unprocessed episodes).", unprocessed.len());
                                                for ep in &unprocessed {
                                                    if let Some(ref id) = ep.id {
                                                        let _ = backend_dream.mark_episode_processed(id).await;
                                                    }
                                                }
                                                let mut scopes = backend_dream.get_active_scopes().await.unwrap_or_default();
                                                if scopes.is_empty() {
                                                    scopes.push("general".to_string());
                                                }
                                                for scope in scopes {
                                                    let _ = cognitive::pipeline::refine_hypotheses(backend_dream.as_ref(), None, &scope).await;
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

                                    if let Ok(unprocessed) = backend_dream.get_unprocessed_episodes_paginated(51, 0).await
                                        && !unprocessed.is_empty() {
                                            tracing::info!("Idle debounced synthesis starting...");
                                            for ep in &unprocessed {
                                                if let Some(ref id) = ep.id {
                                                    let _ = backend_dream.mark_episode_processed(id).await;
                                                }
                                            }
                                            let mut scopes = backend_dream.get_active_scopes().await.unwrap_or_default();
                                            if scopes.is_empty() {
                                                scopes.push("general".to_string());
                                            }
                                            for scope in scopes {
                                                let _ = cognitive::pipeline::refine_hypotheses(backend_dream.as_ref(), None, &scope).await;
                                            }
                                        }

                                    crate::llm::evict_global_reranker().await;
                                    if let Some(broker) = crate::llm::DYNAMIC_MODEL_BROKER.get() {
                                        broker.evict_unused_models().await;
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
                    checked_sessions: Arc::new(tokio::sync::RwLock::new(std::collections::HashSet::new())),
                    degraded_mode: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                });

                // Build router and start Axum listener
                let app = api::create_router(state);
                let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
                let bind_addr = if addr.is_ipv6() {
                    std::net::SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], port))
                } else {
                    std::net::SocketAddr::from(([127, 0, 0, 1], port))
                };
                let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
                let pid_path_clone = pid_path.clone();

                #[cfg(unix)]
                let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .context("Failed to register SIGTERM handler")?;

                #[cfg(unix)]
                let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                    .context("Failed to register SIGINT handler")?;

                tokio::select! {
                    res = axum::serve(listener, app) => {
                        if let Err(e) = res {
                            tracing::error!("Daemon server crashed: {:?}", e);
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        tracing::info!("Shutdown channel triggered. Initiating graceful shutdown...");
                    }
                    _ = async {
                        #[cfg(unix)]
                        {
                            sigint.recv().await;
                        }
                        #[cfg(not(unix))]
                        {
                            let _ = tokio::signal::ctrl_c().await;
                        }
                    } => {
                         tracing::info!("SIGINT received. Initiating graceful shutdown...");
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
                    }
                }

                tracing::info!("Signalling background tasks to cancel...");
                cancel_token.cancel();

                let shutdown_sequence = async {
                    tracing::info!("Sending Shutdown event to blackboard actor...");
                    let (respond_to, rx) = tokio::sync::oneshot::channel();
                    if let Ok(_) = blackboard_tx.send(crate::db::blackboard::EventMessage {
                        event: crate::db::blackboard::WikiNodeEvent::Shutdown,
                        respond_to,
                    }).await {
                        let _ = rx.await;
                    }
                    tracing::info!("Waiting for blackboard actor to finish...");
                    if let Err(_) = tokio::time::timeout(std::time::Duration::from_secs(5), blackboard_handle).await {
                        tracing::warn!("Waiting for blackboard actor to finish timed out.");
                    }
                    run_shutdown(pid_path_clone).await;
                };
                if let Err(_) = tokio::time::timeout(std::time::Duration::from_secs(10), shutdown_sequence).await {
                    tracing::warn!("Graceful shutdown timed out.");
                    let _ = std::fs::remove_file(&pid_path);
                }
                tracing::info!("Shutdown complete.");
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
            .map(|dir| {
                dir.flatten()
                    .any(|entry| entry.path().extension().map_or(false, |ext| ext == "py"))
            })
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
            Err(e) => (-1, e.to_string()),
        }
    })
    .await
    .unwrap_or((-2, "Thread panic".to_string()));

    let git_diff = tokio::task::spawn_blocking(move || {
        let output = std::process::Command::new("git")
            .args(&["diff"])
            .current_dir(&workspace_root)
            .output();
        match output {
            Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
            Err(e) => e.to_string(),
        }
    })
    .await
    .unwrap_or_else(|_| "Thread panic".to_string());

    let checkpoint_id = format!(
        "checkpoint_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );

    let sql = "
        UPSERT type::record('checkpoint_node', $id) CONTENT {
            project_type: $project_type,
            exit_code: $exit_code,
            compiler_errors: $compiler_errors,
            git_diff: $git_diff,
            timestamp: time::now()
        };
    ";
    backend
        .db
        .query(sql)
        .bind(("id", checkpoint_id.clone()))
        .bind(("project_type", project_type))
        .bind(("exit_code", compile_result.0))
        .bind(("compiler_errors", compile_result.1))
        .bind(("git_diff", git_diff))
        .await?
        .check()?;

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
        match client
            .post(&url)
            .header("X-Mythrax-Token", &token)
            .timeout(Duration::from_secs(2))
            .send()
            .await
        {
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
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                if system.process(pid).is_some() {
                    println!("Process did not exit, sending SIGKILL...");
                    if let Some(process) = system.process(pid) {
                        process.kill_with(Signal::Kill);
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
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
    let backup_dir = vault_root
        .join(".trash")
        .join(format!("backup_{}", timestamp));

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
            let c_path = CString::new(
                canonical_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Invalid path"))?,
            )?;
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
            let c_path = CString::new(
                canonical_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Invalid path"))?,
            )?;
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
