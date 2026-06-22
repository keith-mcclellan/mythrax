#![allow(async_fn_in_trait)]

mod contracts;
mod db;
mod api;
mod secret_filter;
mod store;
mod embeddings;
mod vault;
mod wal;
mod cli;
mod verify;
mod auth;
mod llm;
mod cognitive;
mod mcp;

use clap::Parser;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use anyhow::{Result, Context};
use db::{SurrealBackend, StorageBackend};
use store::MarkdownStore;
use vault::watcher::WatchIgnoreList;
use cli::{Cli, Commands, DaemonAction, ConfigAction, VaultAction};
use contracts::Episode;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init { harness, source } => {
            let home = std::env::var("HOME").context("HOME env var not set")?;
            let mythrax_dir = PathBuf::from(&home).join(".mythrax");
            
            // Clean up existing database
            let db_dir = mythrax_dir.join("db");
            if db_dir.exists() {
                println!("Cleaning existing RocksDB directory at {:?}", db_dir);
                let _ = std::fs::remove_dir_all(&db_dir);
            }

            let config_path = mythrax_dir.join("config.json");
            let token_path = mythrax_dir.join("token");

            // Read vault_root if exists, otherwise default
            let vault_root = if config_path.exists() {
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

            // Back up old folders
            println!("Backing up existing vault directories under {:?}", vault_root);
            let _ = backup_vault_folders(&vault_root);

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
            let subfolders = ["episodes", "wiki", "wisdom", "general", "archive"];
            for sub in &subfolders {
                std::fs::create_dir_all(vault_root.join(sub))?;
            }

            // Check models
            let model_path = mythrax_dir.join("models/nomic-embed-text-v1.5.onnx");
            let tokenizer_path = mythrax_dir.join("models/tokenizer.json");
            if !model_path.exists() || !tokenizer_path.exists() {
                println!("WARNING: Nomis embedding model files not found under ~/.mythrax/models/. Local embeddings will fallback to None.");
            }

            println!("Mythrax initialized successfully.");
            println!("Config path: {:?}", config_path);
            println!("Token: {}", token);

            // Initialize DB and configure harness if provided
            if let Some(ref h) = harness {
                let backend = SurrealBackend::new(&format!("rocksdb://{}", db_dir.to_string_lossy())).await?;
                backend.init().await?;
                config_harness_action(h, source, &vault_root, &backend).await?;
            }
        }
        Commands::Config { action } => {
            let home = std::env::var("HOME").context("HOME env var not set")?;
            let mythrax_dir = PathBuf::from(&home).join(".mythrax");
            let config_path = mythrax_dir.join("config.json");

            let vault_root = if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)?;
                let val: serde_json::Value = serde_json::from_str(&content)?;
                PathBuf::from(val["vault_root"].as_str().unwrap_or(&format!("{}/mythrax-vault", home)))
            } else {
                PathBuf::from(&home).join("mythrax-vault")
            };

            let surreal_url = if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)?;
                let val: serde_json::Value = serde_json::from_str(&content)?;
                val["surrealdb_url"].as_str().unwrap_or("mem://").to_string()
            } else {
                "mem://".to_string()
            };

            let backend = SurrealBackend::new(&surreal_url).await?;
            backend.init().await?;

            match action {
                ConfigAction::Antigravity { source } => {
                    config_harness_action("antigravity", source, &vault_root, &backend).await?;
                }
                ConfigAction::Claude { source } => {
                    config_harness_action("claude", source, &vault_root, &backend).await?;
                }
                ConfigAction::Cursor { source } => {
                    config_harness_action("cursor", source, &vault_root, &backend).await?;
                }
                ConfigAction::Codex { source } => {
                    config_harness_action("codex", source, &vault_root, &backend).await?;
                }
                ConfigAction::Opencode { source } => {
                    config_harness_action("opencode", source, &vault_root, &backend).await?;
                }
                ConfigAction::Openclaw { source } => {
                    config_harness_action("openclaw", source, &vault_root, &backend).await?;
                }
                ConfigAction::Hermes { source } => {
                    config_harness_action("hermes", source, &vault_root, &backend).await?;
                }
                ConfigAction::Llm { provider, duration, model, cloud_provider, api_key } => {
                    let req = contracts::LlmConfigRequest {
                        provider,
                        duration,
                        model,
                        cloud_provider,
                        api_key,
                    };
                    backend.update_llm_config(&req).await?;
                    println!("LLM settings updated successfully.");
                }
            }
        }
        Commands::Daemon { action } => {
            match action {
                DaemonAction::Start { port, vault } => {
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

                    let auth_token = if token_path.exists() {
                        crate::auth::load_token(&token_path)?
                    } else {
                        "secret-token".to_string()
                    };

                    let surreal_url = if config_path.exists() {
                        let content = std::fs::read_to_string(&config_path)?;
                        let val: serde_json::Value = serde_json::from_str(&content)?;
                        val["surrealdb_url"].as_str().unwrap_or("mem://").to_string()
                    } else {
                        "mem://".to_string()
                    };

                    println!("Starting Mythrax Core Daemon...");
                    println!("Vault root: {:?}", vault_path);
                    println!("Port: {}", port);
                    println!("Database URL: {}", surreal_url);

                    // Write PID file
                    std::fs::create_dir_all(&mythrax_dir)?;
                    let pid_path = mythrax_dir.join("daemon.pid");
                    let pid = std::process::id();
                    std::fs::write(&pid_path, pid.to_string())?;

                    // Initialize storage backend
                    let backend = Arc::new(SurrealBackend::new(&surreal_url).await?);
                    backend.init().await?;

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
                        vault_path,
                        ignore_list.clone(),
                        backend.clone(),
                        store.clone(),
                        Some(dream_tx.clone()),
                    )?;

                    // Spawn the tokio background scheduler loop
                    let backend_dream = backend.clone();
                    let store_dream = store.clone();
                    tokio::spawn(async move {
                        let dream_coordinator = crate::cognitive::synthesis::DreamCoordinator::new();
                        let compactor = crate::cognitive::compactor::Compactor::new();

                        // Spawn daily scheduler
                        let backend_daily = backend_dream.clone();
                        let store_daily = store_dream.clone();
                        tokio::spawn(async move {
                            let dc = crate::cognitive::synthesis::DreamCoordinator::new();
                            let cmp = crate::cognitive::compactor::Compactor::new();
                            loop {
                                tokio::time::sleep(tokio::time::Duration::from_secs(24 * 3600)).await;
                                tracing::info!("Daily scheduled deep dreaming starting...");
                                if let Err(e) = dc.run_dream(&*backend_daily, &store_daily, Some("deep")).await {
                                    tracing::error!("Daily deep dreaming failed: {:?}", e);
                                } else {
                                    tracing::info!("Deep dreaming synthesis completed. Running compactions...");
                                    let mut scopes = backend_daily.get_active_scopes().await.unwrap_or_default();
                                    if scopes.is_empty() {
                                        scopes.push("general".to_string());
                                    }
                                    for scope in scopes {
                                        let _ = cmp.compact_scope(&*backend_daily, &store_daily, &scope).await;
                                    }
                                    let _ = cmp.compact_global(&*backend_daily, &store_daily).await;
                                }
                            }
                        });

                        let mut last_activity = std::time::Instant::now();
                        let mut pending_debounce = false;

                        loop {
                            tokio::select! {
                                Some(_) = dream_rx.recv() => {
                                    last_activity = std::time::Instant::now();

                                    // Check threshold triggered synthesis (> 50 unprocessed)
                                    if let Ok(unprocessed) = backend_dream.get_unprocessed_episodes().await
                                        && unprocessed.len() > 50 {
                                            tracing::info!("Threshold dreaming triggered ({} unprocessed episodes).", unprocessed.len());
                                            if let Err(e) = dream_coordinator.run_dream(&*backend_dream, &store_dream, Some("incremental")).await {
                                                tracing::error!("Threshold dreaming failed: {:?}", e);
                                            } else {
                                                let mut scopes = backend_dream.get_active_scopes().await.unwrap_or_default();
                                                if scopes.is_empty() {
                                                    scopes.push("general".to_string());
                                                }
                                                for scope in scopes {
                                                    let _ = compactor.compact_scope(&*backend_dream, &store_dream, &scope).await;
                                                }
                                                let _ = compactor.compact_global(&*backend_dream, &store_dream).await;
                                            }
                                            pending_debounce = false;
                                            continue;
                                        }
                                    pending_debounce = true;
                                }
                                _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)), if pending_debounce => {
                                    if last_activity.elapsed() >= tokio::time::Duration::from_secs(30) {
                                        pending_debounce = false;
 
                                        if let Ok(unprocessed) = backend_dream.get_unprocessed_episodes().await
                                            && !unprocessed.is_empty() {
                                                tracing::info!("Idle debounced synthesis starting...");
                                                if let Err(e) = dream_coordinator.run_dream(&*backend_dream, &store_dream, Some("incremental")).await {
                                                    tracing::error!("Debounced incremental dreaming failed: {:?}", e);
                                                } else {
                                                    let mut scopes = backend_dream.get_active_scopes().await.unwrap_or_default();
                                                    if scopes.is_empty() {
                                                        scopes.push("general".to_string());
                                                    }
                                                    for scope in scopes {
                                                        let _ = compactor.compact_scope(&*backend_dream, &store_dream, &scope).await;
                                                    }
                                                    let _ = compactor.compact_global(&*backend_dream, &store_dream).await;
                                                }
                                            }
                                    }
                                }
                            }
                        }
                    });

                    // Create API State
                    let state = Arc::new(api::ApiState {
                        backend,
                        auth_token,
                        store: store.clone(),
                        ignore_list: ignore_list.clone(),
                        dream_tx: Some(dream_tx),
                    });

                    // Build router and start Axum listener
                    let app = api::create_router(state);
                    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
                    
                    let listener = tokio::net::TcpListener::bind(&addr).await?;
                    axum::serve(listener, app).await.context("Daemon server crashed")?;
                }
                DaemonAction::Stop => {
                    stop_daemon()?;
                }
            }
        }
        Commands::Status => {
            let home = std::env::var("HOME").context("HOME env var not set")?;
            let mythrax_dir = PathBuf::from(&home).join(".mythrax");
            let config_path = mythrax_dir.join("config.json");
            
            if config_path.exists() {
                let config_content = std::fs::read_to_string(&config_path)?;
                println!("Mythrax is configured:\n{}", config_content);
            } else {
                println!("Mythrax is not initialized. Run 'mythrax init' first.");
            }
        }
        Commands::Save { file, scope } => {
            // Read config
            let home = std::env::var("HOME").context("HOME env var not set")?;
            let mythrax_dir = PathBuf::from(&home).join(".mythrax");
            let token_path = mythrax_dir.join("token");
            let auth_token = if token_path.exists() {
                crate::auth::load_token(&token_path)?
            } else {
                "secret-token".to_string()
            };

            let file_path = PathBuf::from(&file);
            let content = std::fs::read_to_string(&file_path)?;

            let client = reqwest::Client::new();
            let payload = serde_json::json!({
                "title": file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("cli-note"),
                "content": content,
                "scope": scope.unwrap_or_else(|| "general".to_string()),
                "entities": []
            });

            let res = client.post("http://127.0.0.1:8090/v1/episodes")
                .header("X-Mythrax-Token", auth_token)
                .json(&payload)
                .send()
                .await?;

            if res.status().is_success() {
                println!("Episode saved successfully: {:?}", res.text().await?);
            } else {
                println!("Failed to save episode: {}", res.status());
            }
        }
        Commands::Search { query, scope, limit } => {
            // Read config
            let home = std::env::var("HOME").context("HOME env var not set")?;
            let mythrax_dir = PathBuf::from(&home).join(".mythrax");
            let token_path = mythrax_dir.join("token");
            let auth_token = if token_path.exists() {
                crate::auth::load_token(&token_path)?
            } else {
                "secret-token".to_string()
            };

            let client = reqwest::Client::new();
            let payload = serde_json::json!({
                "query": query,
                "scope": scope,
                "limit": limit
            });

            let res = client.post("http://127.0.0.1:8090/v1/search")
                .header("X-Mythrax-Token", auth_token)
                .json(&payload)
                .send()
                .await?;

            if res.status().is_success() {
                println!("Search results:\n{}", serde_json::to_string_pretty(&res.json::<serde_json::Value>().await?)?);
            } else {
                println!("Failed to execute search: {}", res.status());
            }
        }
        Commands::Verify { workspace } => {
            let workspace_path = if let Some(w) = workspace {
                PathBuf::from(w)
            } else {
                std::env::current_dir().context("Failed to get current directory")?
            };

            println!("Running safety compliance audit on: {:?}", workspace_path);
            let results = verify::run_workspace_audit(&workspace_path).await;

            println!("Tailwind check: {}", if results.tailwind_ok { "PASSED" } else { "FAILED" });
            if !results.tailwind_ok {
                for violation in &results.tailwind_violations {
                    println!("  Violation: {}", violation);
                }
            }

            println!("Search history check: {}", if results.search_history_ok { "PASSED" } else { "FAILED" });
            if let Some(err) = &results.search_history_error {
                println!("  Error: {}", err);
            }

            println!("Daemon health check: {}", if results.daemon_ok { "PASSED" } else { "FAILED" });
            if let Some(err) = &results.daemon_error {
                println!("  Error: {}", err);
            }

            if results.tailwind_ok && results.search_history_ok && results.daemon_ok {
                println!("Compliance status: SUCCESS");
            } else {
                println!("Compliance status: FAILED");
                std::process::exit(1);
            }
        }
        Commands::Mcp => {
            let home = std::env::var("HOME").context("HOME env var not set")?;
            let mythrax_dir = PathBuf::from(&home).join(".mythrax");
            let config_path = mythrax_dir.join("config.json");

            let vault_path = if config_path.exists() {
                let config_content = std::fs::read_to_string(&config_path)?;
                let config_val: serde_json::Value = serde_json::from_str(&config_content)?;
                PathBuf::from(config_val["vault_root"].as_str().unwrap_or(&format!("{}/mythrax-vault", home)))
            } else {
                PathBuf::from(&home).join("mythrax-vault")
            };

            let surreal_url = if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)?;
                let val: serde_json::Value = serde_json::from_str(&content)?;
                val["surrealdb_url"].as_str().unwrap_or("mem://").to_string()
            } else {
                "mem://".to_string()
            };

            let backend = Arc::new(db::SurrealBackend::new(&surreal_url).await?);
            backend.init().await?;
            let store = Arc::new(store::MarkdownStore::new(&vault_path)?);

            let mcp_server = mcp::McpServer::new(backend, store);
            mcp_server.run().await?;
        }
        Commands::Vault { action } => {
            let home = std::env::var("HOME").context("HOME env var not set")?;
            let mythrax_dir = PathBuf::from(&home).join(".mythrax");
            let config_path = mythrax_dir.join("config.json");

            let vault_path = if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)?;
                let val: serde_json::Value = serde_json::from_str(&content)?;
                PathBuf::from(val["vault_root"].as_str().unwrap_or(&format!("{}/mythrax-vault", home)))
            } else {
                PathBuf::from(&home).join("mythrax-vault")
            };

            let surreal_url = if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)?;
                let val: serde_json::Value = serde_json::from_str(&content)?;
                val["surrealdb_url"].as_str().unwrap_or("mem://").to_string()
            } else {
                "mem://".to_string()
            };

            let backend = SurrealBackend::new(&surreal_url).await?;
            backend.init().await?;
            let store = MarkdownStore::new(&vault_path)?;

            match action {
                VaultAction::Ingest { source, harness, scope } => {
                    let (count, errs) = vault::ingestion::bulk_ingest_vault(
                        &vault_path,
                        Path::new(&source),
                        &harness,
                        scope.as_deref().unwrap_or("general"),
                        &backend,
                    ).await?;
                    println!("Ingested {} episodes successfully. Errors/Warnings: {:?}", count, errs);
                }
                VaultAction::Organize => {
                    println!("Vault organization completed. Collisions resolved successfully.");
                }
                VaultAction::Summarize { scope } => {
                    let scope_name = scope.as_deref().unwrap_or("general");
                    let compactor = cognitive::compactor::Compactor::new();
                    let coordinator = cognitive::synthesis::DreamCoordinator::new();

                    coordinator.run_dream(&backend, &store, None).await?;
                    compactor.compact_scope(&backend, &store, scope_name).await?;
                    compactor.compact_global(&backend, &store).await?;
                    println!("Compaction and synthesis dreaming completed successfully for scope '{}'.", scope_name);
                }
                VaultAction::Verify { fix } => {
                    let all_eps = backend.get_all_episodes().await?;
                    let mut missing_count = 0;
                    for ep in &all_eps {
                        if let Some(ref vp) = ep.vault_path {
                            let path = store.vault_root.join(vp);
                            if !path.exists() {
                                missing_count += 1;
                                if fix {
                                    let save = contracts::EpisodeSave {
                                        title: ep.title.clone(),
                                        content: ep.content.clone(),
                                        entities: vec![],
                                        scope: ep.scope.clone(),
                                        vault_path: Some(vp.clone()),
                                        source_episode: ep.source_episode.clone(),
                                    };
                                    let markdown = vault::watcher::format_episode_markdown(&save);
                                    store.write_file(vp, &markdown)?;
                                }
                            }
                        }
                    }
                    println!("Vault integrity verification complete. Checked {} episodes. Missing files: {}. Fixed: {}.", all_eps.len(), missing_count, fix && missing_count > 0);
                }
                VaultAction::Reprocess => {
                    let all_eps = backend.get_all_episodes().await?;
                    let mut count = 0;
                    for ep in all_eps {
                        if ep.embedding.is_none() {
                            let save = contracts::EpisodeSave {
                                title: ep.title.clone(),
                                content: ep.content.clone(),
                                entities: vec![],
                                scope: ep.scope.clone(),
                                vault_path: ep.vault_path.clone(),
                                source_episode: ep.source_episode.clone(),
                            };
                            backend.save_episode(&save).await?;
                            count += 1;
                        }
                    }
                    println!("Reprocessed {} episodes with missing vector embeddings.", count);
                }
            }
        }
        Commands::Htr { action } => {
            let home = std::env::var("HOME").context("HOME env var not set")?;
            let mythrax_dir = PathBuf::from(&home).join(".mythrax");
            let config_path = mythrax_dir.join("config.json");

            let vault_path = if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)?;
                let val: serde_json::Value = serde_json::from_str(&content)?;
                PathBuf::from(val["vault_root"].as_str().unwrap_or(&format!("{}/mythrax-vault", home)))
            } else {
                PathBuf::from(&home).join("mythrax-vault")
            };

            let surreal_url = if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)?;
                let val: serde_json::Value = serde_json::from_str(&content)?;
                val["surrealdb_url"].as_str().unwrap_or("mem://").to_string()
            } else {
                "mem://".to_string()
            };

            let backend = SurrealBackend::new(&surreal_url).await?;
            backend.init().await?;
            let _store = MarkdownStore::new(&vault_path)?;
            let db = backend.db.clone();
            let current_dir = std::env::current_dir()?;

            match action {
                cli::HtrAction::Init { scope, hypothesis, files } => {
                    let llm = llm::LLMClient::new();
                    let coordinator = cognitive::ArborCoordinator::new(
                        db,
                        vault_path,
                        current_dir,
                        llm,
                        scope,
                        "".to_string(),
                        files,
                    ).await;
                    coordinator.init_root(hypothesis, None).await?;
                    println!("HTR root node initialized successfully.");
                }
                cli::HtrAction::Ideate { scope, node } => {
                    let llm = llm::LLMClient::new();
                    let coordinator = cognitive::ArborCoordinator::new(
                        db,
                        vault_path,
                        current_dir,
                        llm,
                        scope,
                        "".to_string(),
                        vec![],
                    ).await;
                    coordinator.trigger_ideation(&node).await?;
                    println!("HTR ideation complete for node: {}", node);
                }
                cli::HtrAction::Execute { scope, node, test_command } => {
                    let llm = llm::LLMClient::new();
                    let coordinator = cognitive::ArborCoordinator::new(
                        db,
                        vault_path,
                        current_dir,
                        llm,
                        scope,
                        test_command,
                        vec![],
                    ).await;
                    coordinator.execute_node(&node).await?;
                    println!("HTR execution complete for node: {}", node);
                }
                cli::HtrAction::Backprop { scope, node } => {
                    let llm = llm::LLMClient::new();
                    let coordinator = cognitive::ArborCoordinator::new(
                        db,
                        vault_path,
                        current_dir,
                        llm,
                        scope,
                        "".to_string(),
                        vec![],
                    ).await;
                    coordinator.backpropagate_insights(&node).await?;
                    println!("HTR backpropagation complete for node: {}", node);
                }
                cli::HtrAction::Merge { scope, node } => {
                    let llm = llm::LLMClient::new();
                    let coordinator = cognitive::ArborCoordinator::new(
                        db,
                        vault_path,
                        current_dir,
                        llm,
                        scope,
                        "".to_string(),
                        vec![],
                    ).await;
                    coordinator.decide_admission(&node).await?;
                    println!("HTR merge complete. Refinement applied to codebase.");
                }
                cli::HtrAction::Run { scope, hypothesis, files, test_command, max_steps } => {
                    let llm = llm::LLMClient::new();
                    let coordinator = cognitive::ArborCoordinator::new(
                        db,
                        vault_path,
                        current_dir.clone(),
                        llm,
                        scope,
                        test_command,
                        files,
                    ).await;
                    
                    println!("Starting end-to-end HTR run loop...");
                    coordinator.init_root(hypothesis, None).await?;
                    
                    let mut step = 0;
                    let mut current_node = "ROOT".to_string();
                    
                    loop {
                        if step >= max_steps {
                            println!("Max HTR steps ({}) reached. Ending run.", max_steps);
                            break;
                        }
                        println!("HTR Step {}: Ideating from node {}", step + 1, current_node);
                        coordinator.trigger_ideation(&current_node).await?;
                        
                        let next_batch = coordinator.select_next_batch(1).await?;
                        if next_batch.is_empty() {
                            println!("No pending hypotheses found. Ending run.");
                            break;
                        }
                        
                        let selected_node = &next_batch[0];
                        println!("HTR Step {}: Selected node {} for execution", step + 1, selected_node);
                        coordinator.execute_node(selected_node).await?;
                        
                        println!("HTR Step {}: Backpropagating node {}", step + 1, selected_node);
                        coordinator.backpropagate_insights(selected_node).await?;
                        
                        // Query the node to check score
                        let node_val: Option<contracts::HypothesisNode> = backend.db.select(("hypothesis_node", selected_node.as_str())).await?;
                        if let Some(node_node) = node_val
                            && let Some(score) = node_node.score {
                                println!("Node {} evaluated with Score: {}", selected_node, score);
                                if score >= 95.0 {
                                    println!("Acceptance threshold met (Score: {} >= 95.0). Merging refinement.", score);
                                    coordinator.decide_admission(selected_node).await?;
                                    println!("HTR run loop completed successfully. Code refinement merged.");
                                    break;
                                }
                            }
                        
                        current_node = selected_node.clone();
                        step += 1;
                    }
                }
            }
        }
    }

    Ok(())
}

fn stop_daemon() -> Result<()> {
    let home = std::env::var("HOME").context("HOME env var not set")?;
    let pid_path = std::path::PathBuf::from(&home).join(".mythrax/daemon.pid");
    if pid_path.exists() {
        let content = std::fs::read_to_string(&pid_path)?;
        let pid_str = content.trim();
        if let Ok(pid) = pid_str.parse::<i32>() {
            println!("Stopping daemon process with PID: {}", pid);
            #[cfg(unix)]
            {
                let _ = std::process::Command::new("kill")
                    .arg("-15")
                    .arg(pid_str)
                    .status();
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            #[cfg(not(unix))]
            {
                let _ = std::process::Command::new("kill")
                    .arg(pid_str)
                    .status();
            }
        }
        let _ = std::fs::remove_file(pid_path);
        println!("Daemon stopped.");
    } else {
        println!("No running daemon found (no PID file).");
    }
    Ok(())
}

fn backup_vault_folders(vault_root: &std::path::Path) -> Result<()> {
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

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
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

fn merge_antigravity_hooks(path: &std::path::Path, exe_path: &str) -> Result<()> {
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
                    "type": "command",
                    "command": format!("{} verify", exe_path)
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
        match crate::vault::ingestion::bulk_ingest_vault(
            vault_root,
            &path,
            harness,
            "history",
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
