use crate::embeddings::TextEmbedder;
use crate::llm::{LLMClient, MxbaiReranker};
use anyhow::{Context, Result};
use std::sync::Arc;
use surrealdb::engine::local::{Db, Mem, SurrealKv};
use surrealdb::Surreal;
use uuid::Uuid;
use crate::db::GLOBAL_BACKEND;

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub count: usize,
    pub expires_at: std::time::Instant,
}

#[derive(Clone)]
pub struct SurrealBackend {
    pub db: Surreal<Db>,
    pub embedder: Option<Arc<dyn TextEmbedder>>,
    pub client_port: Option<u16>,
    pub write_lock: Arc<tokio::sync::Mutex<()>>,
    pub db_path: Option<std::path::PathBuf>,
    pub indexing_writes: Arc<tokio::sync::Mutex<std::collections::HashMap<String, usize>>>,
    pub term_counts_cache: Arc<
        tokio::sync::RwLock<
            std::collections::HashMap<
                String,
                Arc<tokio::sync::RwLock<std::collections::HashMap<String, CacheEntry>>>,
            >,
        >,
    >,
    pub global_cache_size: Arc<std::sync::atomic::AtomicUsize>,
    pub avg_dl_cache:
        Arc<tokio::sync::RwLock<std::collections::HashMap<String, (f32, std::time::Instant)>>>,
    pub search_mode: Arc<tokio::sync::Mutex<String>>,
    pub reranker: Arc<tokio::sync::Mutex<Option<MxbaiReranker>>>,
    pub reinforcement_semaphore: Arc<tokio::sync::Semaphore>,
    pub watch_ignore_list: Arc<tokio::sync::RwLock<Option<Arc<crate::vault::watcher::WatchIgnoreList>>>>,
    pub(crate) blackboard_tx:
        std::sync::OnceLock<tokio::sync::mpsc::Sender<crate::db::blackboard::EventMessage>>,
}

pub struct BackendConfig {
    pub check_daemon: bool,
    pub embedder: Option<Arc<dyn TextEmbedder>>,
    pub llm: Option<LLMClient>,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            check_daemon: true,
            embedder: None,
            llm: None,
        }
    }
}

impl SurrealBackend {
    pub async fn new(url: &str, config: BackendConfig) -> Result<Self> {
        // 1. Determine daemon port from env or default
        let env_port = std::env::var("MYTHRAX_DAEMON_PORT").ok();
        let daemon_port = env_port
            .as_ref()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(8090);

        // 2. Only check the daemon port if check_daemon is true
        let is_daemon_available = if config.check_daemon {
            match tokio::time::timeout(
                std::time::Duration::from_millis(50),
                tokio::net::TcpStream::connect(format!("127.0.0.1:{}", daemon_port)),
            )
            .await
            {
                Ok(Ok(_)) => true,
                _ => false,
            }
        } else {
            false
        };

        let mut db_path = None;
        let (db, client_port) = if is_daemon_available {
            // Client Mode: Connect to running daemon
            // We use an in-memory DB struct as a placeholder because the actual
            // operations will be routed via HTTP to the daemon.
            let db = Surreal::new::<Mem>(())
                .await
                .context("Failed to initialize in-memory store for client mode")?;

            // Initialize namespace/database context as required by the SDK structure
            db.use_ns("mythrax").use_db("memory").await?;

            (db, Some(daemon_port))
        } else {
            // Server Mode: Open local database
            let db = if url.starts_with("surrealkv://") || url.starts_with("rocksdb://") {
                let path = url
                    .strip_prefix("surrealkv://")
                    .or_else(|| url.strip_prefix("rocksdb://"))
                    .context("Invalid database URL format; expected surrealkv:// or rocksdb://")?;
                db_path = Some(std::path::PathBuf::from(path));
                if let Some(parent) = std::path::Path::new(path).parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let mut attempt = 0;
                loop {
                    match Surreal::new::<SurrealKv>(path).await {
                        Ok(conn) => break conn,
                        Err(e) => {
                            let err_str = e.to_string();
                            if (err_str.contains("locked") || err_str.contains("LOCK"))
                                && attempt < 10
                            {
                                attempt += 1;
                                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                            } else {
                                return Err(e).context(format!(
                                    "Failed to initialize SurrealDB with SurrealKV at: {}",
                                    path
                                ));
                            }
                        }
                    }
                }
            } else {
                Surreal::new::<Mem>(())
                    .await
                    .context("Failed to initialize SurrealDB with in-memory store")?
            };
            db.use_ns("mythrax").use_db("memory").await?;
            (db, None)
        };

        let embedder = config.embedder.or_else(|| {
            match crate::embeddings::LocalEmbedder::get_global() {
                Ok(emb) => Some(emb as Arc<dyn crate::embeddings::TextEmbedder>),
                Err(e) => {
                    tracing::warn!("Failed to initialize LocalEmbedder: {}. Falling back to non-embedded mode.", e);
                    None
                }
            }
        });

        // 4. Initialize write lock
        let write_lock = Arc::new(tokio::sync::Mutex::new(()));

        let indexing_writes = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));

        let backend = Self {
            db,
            embedder,
            client_port,
            write_lock,
            db_path,
            indexing_writes,
            term_counts_cache: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            global_cache_size: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            avg_dl_cache: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            search_mode: Arc::new(tokio::sync::Mutex::new("hybrid".to_string())),
            reranker: Arc::new(tokio::sync::Mutex::new(None)),
            reinforcement_semaphore: Arc::new(tokio::sync::Semaphore::new(10)),
            watch_ignore_list: Arc::new(tokio::sync::RwLock::new(None)),
            blackboard_tx: std::sync::OnceLock::new(),
        };
        let _ = GLOBAL_BACKEND.set(Arc::new(backend.clone()));
        Ok(backend)
    }

    pub fn new_with_db(db: Surreal<Db>) -> Self {
        Self {
            db,
            embedder: None,
            client_port: None,
            write_lock: Arc::new(tokio::sync::Mutex::new(())),
            db_path: None,
            indexing_writes: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            term_counts_cache: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            global_cache_size: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            avg_dl_cache: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            search_mode: Arc::new(tokio::sync::Mutex::new("hybrid".to_string())),
            reranker: Arc::new(tokio::sync::Mutex::new(None)),
            reinforcement_semaphore: Arc::new(tokio::sync::Semaphore::new(10)),
            watch_ignore_list: Arc::new(tokio::sync::RwLock::new(None)),
            blackboard_tx: std::sync::OnceLock::new(),
        }
    }

    pub fn is_client_mode(&self) -> bool {
        self.client_port.is_some()
    }

    pub async fn new_client_connection() -> Result<Self> {
        Self::new("mem://", BackendConfig::default()).await
    }

    pub async fn new_in_memory() -> Result<Self> {
        let config = BackendConfig {
            check_daemon: false,
            embedder: Some(Arc::new(crate::embeddings::MockEmbedder)),
            llm: Some(crate::llm::LLMClient::new_mock()),
        };
        let backend = Self::new("mem://", config).await?;
        let random_ns = format!("ns_{}", Uuid::new_v4().to_string().replace("-", "_"));
        let random_db = format!("db_{}", Uuid::new_v4().to_string().replace("-", "_"));
        backend.db.use_ns(&random_ns).use_db(&random_db).await?;
        Ok(backend)
    }

}
