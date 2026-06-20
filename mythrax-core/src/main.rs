mod contracts;
mod db;
mod api;
mod secret_filter;
mod store;
mod embeddings;
mod watcher;
mod wal;
mod cli;

use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use anyhow::{Result, Context};
use db::{SurrealBackend, StorageBackend};
use store::MarkdownStore;
use watcher::WatchIgnoreList;
use cli::{Cli, Commands, DaemonAction};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            let home = std::env::var("HOME").context("HOME env var not set")?;
            let mythrax_dir = PathBuf::from(&home).join(".mythrax");
            std::fs::create_dir_all(&mythrax_dir)?;

            let config_path = mythrax_dir.join("config.json");
            let token_path = mythrax_dir.join("token");

            // Generate token if not exists
            let token = if token_path.exists() {
                std::fs::read_to_string(&token_path)?
            } else {
                let new_token = uuid::Uuid::new_v4().to_string();
                std::fs::write(&token_path, &new_token)?;
                new_token
            };

            // Write default config
            let default_vault = PathBuf::from(&home).join("mythrax-vault");
            let config_data = serde_json::json!({
                "vault_root": default_vault.to_string_lossy().to_string(),
                "auth_token_path": token_path.to_string_lossy().to_string(),
                "surrealdb_url": "mem://"
            });
            std::fs::write(&config_path, serde_json::to_string_pretty(&config_data)?)?;

            println!("Mythrax initialized successfully.");
            println!("Config path: {:?}", config_path);
            println!("Token: {}", token);
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
                        std::fs::read_to_string(&token_path)?.trim().to_string()
                    } else {
                        "secret-token".to_string()
                    };

                    println!("Starting Mythrax Core Daemon...");
                    println!("Vault root: {:?}", vault_path);
                    println!("Port: {}", port);

                    // Initialize storage backend
                    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
                    backend.init().await?;

                    // Initialize Markdown Store
                    let store = Arc::new(MarkdownStore::new(&vault_path)?);

                    // Initialize Watch Ignore List
                    let ignore_list = Arc::new(WatchIgnoreList::new());

                    // Start File-Watcher
                    let _watcher = watcher::start_watching(
                        vault_path,
                        ignore_list.clone(),
                        backend.clone(),
                        store.clone(),
                    )?;

                    // Create API State
                    let state = Arc::new(api::ApiState {
                        backend,
                        auth_token,
                    });

                    // Build router and start Axum listener
                    let app = api::create_router(state);
                    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
                    
                    let listener = tokio::net::TcpListener::bind(&addr).await?;
                    axum::serve(listener, app).await.context("Daemon server crashed")?;
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
                std::fs::read_to_string(&token_path)?.trim().to_string()
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
                std::fs::read_to_string(&token_path)?.trim().to_string()
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
    }

    Ok(())
}
