use axum::async_trait;
use surrealdb_types::SurrealValue;
use crate::contracts::{EpisodeSave, SearchResult, WisdomRule, LlmConfigResponse, LlmConfigRequest, Episode, HandoffSave, WikiNode, SearchResponse, WisdomSearchResponse, GetMemoryNodesResponse, ForgedSectionBatch};
use anyhow::{Result, Context};
use surrealdb::engine::local::{Db, Mem, SurrealKv};
use surrealdb::{Surreal, IndexedResults};
use std::sync::Arc;
use uuid::Uuid;
pub use crate::db::query_classification::{QueryCategory, get_decay_factor, split_temporal_query, normalize_spelling, expand_synonyms, classify_query};

pub static GLOBAL_BACKEND: std::sync::OnceLock<Arc<SurrealBackend>> = std::sync::OnceLock::new();
pub static GLOBAL_RERANKER: tokio::sync::Mutex<Option<crate::llm::MxbaiReranker>> = tokio::sync::Mutex::const_new(None);

pub fn unescape_id_part(part: &str) -> String {
    let mut s = part.trim();
    while s.starts_with('⟨') {
        s = &s['⟨'.len_utf8()..];
    }
    while s.ends_with('⟩') {
        s = &s[..s.len() - '⟩'.len_utf8()];
    }
    while s.starts_with('`') {
        s = &s['`'.len_utf8()..];
    }
    while s.ends_with('`') {
        s = &s[..s.len() - '`'.len_utf8()];
    }
    
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next_c) = chars.peek() {
                result.push(next_c);
                chars.next();
            } else {
                result.push('\\');
            }
        } else {
            result.push(c);
        }
    }
    result
}

pub fn record_key_to_string(key: &surrealdb::types::RecordIdKey) -> String {
    match key {
        surrealdb::types::RecordIdKey::String(s) => s.clone(),
        surrealdb::types::RecordIdKey::Number(n) => n.to_string(),
        surrealdb::types::RecordIdKey::Uuid(u) => u.to_string(),
        other => format!("{:?}", other),
    }
}

pub fn parse_record_id(id_str: &str) -> Result<surrealdb::types::RecordId> {
    let parts: Vec<&str> = id_str.splitn(2, ':').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid Record ID format: {}", id_str);
    }
    let table = parts[0].to_string();
    let raw_id = unescape_id_part(parts[1]);
    Ok(surrealdb::types::RecordId {
        table: table.into(),
        key: surrealdb::types::RecordIdKey::from(raw_id),
    })
}

pub fn format_record_id(thing: &surrealdb::types::RecordId) -> String {
    let raw_id = match &thing.key {
        surrealdb::types::RecordIdKey::String(s) => unescape_id_part(s),
        other => unescape_id_part(&record_key_to_string(other)),
    };
    format!("{}:{}", thing.table, raw_id)
}

#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn init(&self) -> Result<()>;
    async fn save_episode(&self, episode: &EpisodeSave) -> Result<String>;
    async fn save_wisdom_rule(&self, rule: &WisdomRule) -> Result<String>;
    async fn search(
        &self,
        params: crate::contracts::SearchParams,
    ) -> Result<SearchResponse>;
    async fn get_wisdom(&self, query: &str, tier: Option<&str>, limit: usize, offset: usize, threshold: f32) -> Result<WisdomSearchResponse>;
    async fn record_feedback(&self, id: &str, success: bool) -> Result<()>;
    /// Reserved: schema migration runner (deferred until multi-version migration support).
    #[allow(dead_code)]
    async fn apply_migrations(&self) -> Result<()>;
    async fn get_llm_config(&self) -> Result<LlmConfigResponse>;
    async fn update_llm_config(&self, req: &LlmConfigRequest) -> Result<()>;
    async fn get_unprocessed_episodes(&self) -> Result<Vec<Episode>>;
    async fn mark_episode_processed(&self, id: &str) -> Result<()>;
    async fn get_all_episodes(&self) -> Result<Vec<Episode>>;
    async fn get_episodes_by_node_type(&self, node_type: &str) -> Result<Vec<Episode>>;
    async fn is_feature_enabled(&self, feature_key: &str, default: bool) -> bool;
    async fn save_profile_key(&self, key: &str, value: &str) -> Result<()>;
    #[allow(dead_code)]
    async fn get_profile_key(&self, key: &str) -> Result<Option<String>>;
    async fn save_handoff(&self, handoff: &HandoffSave) -> Result<String>;
    async fn save_wiki_node(&self, node: &WikiNode) -> Result<String>;
    async fn relate_nodes(
        &self,
        from_id: &str,
        to_id: &str,
        valid_from: Option<chrono::DateTime<chrono::Utc>>,
        valid_to: Option<chrono::DateTime<chrono::Utc>>,
        confidence: Option<f32>,
    ) -> Result<()>;
    async fn relate_followed_by(&self, from_id: &str, to_id: &str) -> Result<()>;
    async fn invalidate_edge(&self, from_id: &str, to_id: &str, ended: Option<chrono::DateTime<chrono::Utc>>) -> Result<()>;
    async fn query_edges_as_of(&self, node_id: &str, as_of: chrono::DateTime<chrono::Utc>) -> Result<Vec<String>>;
    async fn get_related_node_ids(&self, from_id: &str) -> Result<Vec<String>>;
    async fn get_wiki_node_id_by_vault_path(&self, vault_path: &str) -> Result<Option<String>>;
    async fn get_active_scopes(&self) -> Result<Vec<String>>;
    async fn delete_by_vault_path(&self, vault_path: &str) -> Result<()>;
    async fn save_stm(&self, session_id: &str, key: &str, value: &str) -> Result<()>;
    async fn get_stm(&self, session_id: &str, key: Option<&str>) -> Result<std::collections::HashMap<String, String>>;
    async fn clear_stm(&self, session_id: &str) -> Result<()>;
    /// Reserved: external handoff status updates (deferred pending MCP handoff tool).
    #[allow(dead_code)]
    async fn update_handoff_status(&self, id: &str, status: &str) -> Result<()>;
    async fn delete_stale_handoffs(&self, pruning_days: i64) -> Result<()>;
    async fn get_memory_nodes(&self, node_ids: &[String]) -> Result<GetMemoryNodesResponse>;
    async fn save_forged_section(&self, batch: &ForgedSectionBatch) -> Result<()>;
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    async fn get_all_wisdom_rules(&self) -> Result<Vec<WisdomRule>>;
    async fn get_all_wiki_nodes(&self) -> Result<Vec<WikiNode>>;
    async fn prune_stale_memories(&self, vault_root: &std::path::Path) -> Result<()>;
    async fn diagnose_error_internal(&self, stderr: &str, stdout: &str) -> Result<Option<(String, String)>>;
    async fn reinforce_episode(&self, id: &str) -> Result<()>;
    async fn journal_state(&self, vault_root: &std::path::Path, session_id: Option<&str>) -> Result<()>;
    async fn get_checkpoints(&self) -> Result<Vec<serde_json::Value>>;
    async fn query_symbolic(&self, node_id: &str, relation: Option<&str>, max_depth: Option<usize>) -> Result<Vec<String>>;
    async fn query_symbolic_scored(
        &self,
        node_id: &str,
        relation: Option<&str>,
        max_depth: Option<usize>,
        as_of: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<crate::contracts::SymbolicHit>>;
    async fn save_thought_node(&self, thought: &crate::contracts::ThoughtNode) -> Result<String>;
    async fn get_indexing_write_count(&self, vault_path: &str) -> Result<usize>;
    async fn get_max_concurrent_background_embeddings(&self) -> Result<usize>;
    async fn get_max_concurrent_tasks(&self) -> usize;
    async fn search_filtered(
        &self,
        query: &str,
        scope: Option<&str>,
        limit: usize,
        threshold: f32,
        concepts: &[String],
        files: &[String],
    ) -> Result<SearchResponse>;
    async fn get_all_registered_transcripts(&self) -> Result<Vec<(String, String)>>;
    async fn get_session_last_activity(&self, session_id: &str) -> Result<Option<chrono::DateTime<chrono::Utc>>>;
    fn as_any(&self) -> &dyn std::any::Any;
}

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub count: usize,
    pub expires_at: std::time::Instant,
}

#[derive(Clone)]
pub struct SurrealBackend {
    pub db: Surreal<Db>,
    pub embedder: Option<Arc<crate::embeddings::LocalEmbedder>>,
    pub client_port: Option<u16>,
    pub write_lock: Arc<tokio::sync::Mutex<()>>,
    pub wal_tx: Option<tokio::sync::mpsc::Sender<(EpisodeSave, std::path::PathBuf)>>,
    pub db_path: Option<std::path::PathBuf>,
    pub active_embeddings: Arc<std::sync::atomic::AtomicUsize>,
    pub max_concurrent_embeddings: Arc<std::sync::atomic::AtomicUsize>,
    pub indexing_writes: Arc<tokio::sync::Mutex<std::collections::HashMap<String, usize>>>,
    pub embedding_semaphore: Arc<tokio::sync::Mutex<Option<(usize, Arc<tokio::sync::Semaphore>)>>>,
    pub term_counts_cache: Arc<tokio::sync::RwLock<std::collections::HashMap<String, Arc<tokio::sync::RwLock<std::collections::HashMap<String, CacheEntry>>>>>>,
    pub global_cache_size: Arc<std::sync::atomic::AtomicUsize>,
    pub avg_dl_cache: Arc<tokio::sync::RwLock<std::collections::HashMap<String, (f32, std::time::Instant)>>>,
    pub search_mode: Arc<tokio::sync::Mutex<String>>,
    pub reranker: Arc<tokio::sync::Mutex<Option<crate::llm::MxbaiReranker>>>,
    pub reinforcement_semaphore: Arc<tokio::sync::Semaphore>,
}

impl SurrealBackend {
    pub async fn get_category_profile_key(&self, category: QueryCategory, suffix: &str, global_default: &str) -> String {
        if category != QueryCategory::Default {
            let cat_key = format!("search.{}.{}", category.as_str(), suffix);
            if let Ok(Some(val)) = self.get_profile_key(&cat_key).await {
                return val;
            }
        }
        match self.get_profile_key(global_default).await {
            Ok(Some(val)) => val,
            _ => "".to_string(),
        }
    }

    pub async fn compile_user_profile(&self, session_id: &str) -> Result<String> {
        // 1. Retrieve profile key "search.user_profile_max_len"
        let max_len: usize = match self.get_profile_key("search.user_profile_max_len").await {
            Ok(Some(val)) => {
                let parsed = val.parse().unwrap_or(1000);
                if parsed == 0 { 1000 } else { parsed }
            }
            _ => 1000,
        };
        // Parse user session prefix to aggregate cross-session user memory while avoiding distractor pollution
        let user_session_prefix = get_user_prefix(session_id).to_string();

        // 2. Retrieve content and title for session user_input and user_feedback episodes
        #[derive(serde::Deserialize, SurrealValue, Debug)]
        struct EpisodeRecord {
            title: String,
            content: String,
            session_id: Option<String>,
        }
        let sql = "SELECT title, content, session_id FROM episode WHERE (node_type = 'user_input' OR node_type = 'user_feedback') AND session_id != NONE AND session_id != NULL AND string::starts_with(session_id, $prefix);";
        let mut response = self.db.query(sql)
            .bind(("prefix", user_session_prefix.as_str()))
            .await?.check().context("SELECT episodes for compile_user_profile failed")?;
        let filtered_records: Vec<EpisodeRecord> = response.take(0)?;

        // 3. Parse the turn index (Y) from title "Session X - Turn Y" in Rust and sort numerically
        let parse_turn_index = |title: &str| -> Option<u32> {
            if let Some(idx) = title.find("Turn ") {
                let after = &title[idx + 5..];
                let digit_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
                digit_str.parse::<u32>().ok()
            } else {
                None
            }
        };

        let mut turns: Vec<(u32, String)> = filtered_records.into_iter()
            .map(|r| {
                let turn_idx = parse_turn_index(&r.title).unwrap_or(0);
                (turn_idx, r.content)
            })
            .collect();
        turns.sort_by_key(|t| t.0);

        // 4. Query active STM key-values (cross-session based on user prefix)
        let stm_sql = "SELECT key, value, session_id FROM short_term_memory WHERE session_id != NONE AND session_id != NULL AND string::starts_with(session_id, $prefix);";
        let mut stm_res = self.db.query(stm_sql)
            .bind(("prefix", user_session_prefix.as_str()))
            .await?.check().context("SELECT stm failed in compile_user_profile")?;
        #[derive(serde::Deserialize, surrealdb_types::SurrealValue, Debug)]
        struct StmRecord {
            key: String,
            value: String,
            session_id: String,
        }
        let stm_records: Vec<StmRecord> = stm_res.take(0)?;
        let mut stm_facts: Vec<String> = stm_records.into_iter()
            .filter(|r| !r.key.starts_with('_'))
            .map(|r| format!("{}: {}", r.key, r.value))
            .collect();
        stm_facts.sort(); // Sort key alphabetically

        let stm_str = stm_facts.join("\n");

        // 5. Truncation logic (max limit search.user_profile_max_len)
        let res_str = if max_len == 0 {
            // Join everything chronologically/alphabetically
            let mut parts = Vec::new();
            for (_, content) in turns {
                parts.push(content);
            }
            if !stm_str.is_empty() {
                parts.push(stm_str);
            }
            parts.join("\n")
        } else {
            // Helper function to calculate length
            let calculate_length = |turns_slice: &[&str], stm_s: &str| -> usize {
                let mut parts = Vec::new();
                for &t in turns_slice {
                    parts.push(t);
                }
                if !stm_s.is_empty() {
                    parts.push(stm_s);
                }
                if parts.is_empty() {
                    0
                } else {
                    let sum_chars: usize = parts.iter().map(|p| p.chars().count()).sum();
                    sum_chars + (parts.len() - 1)
                }
            };

            let mut kept_turns: Vec<&str> = Vec::new();
            for (_, content) in turns.iter().rev() {
                let mut proposed = vec![content.as_str()];
                proposed.extend(&kept_turns);

                let proposed_len = calculate_length(&proposed, &stm_str);
                if proposed_len <= max_len {
                    kept_turns = proposed;
                } else {
                    break;
                }
            }

            let mut parts = Vec::new();
            for content in kept_turns {
                parts.push(content.to_string());
            }
            if !stm_str.is_empty() {
                parts.push(stm_str);
            }
            parts.join("\n")
        };

        let mut file_path = std::path::PathBuf::from("scratch/debug_profiles.txt");
        if !file_path.parent().map(|p| p.exists()).unwrap_or(false) {
            file_path = std::path::PathBuf::from("mythrax-core/scratch/debug_profiles.txt");
        }
        if let Some(parent) = file_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(true)
            .open(file_path)
        {
            use std::io::Write;
            let _ = writeln!(file, "=== PROFILE FOR session_id = {} ===\n{}\n====================================\n", session_id, res_str);
        }

        Ok(res_str)
    }

    pub async fn save_episode_with_wal_actor(&self, episode: &EpisodeSave, wal_path: &std::path::Path) -> Result<String> {
        self.save_episode_with_wal_actor_db(episode, wal_path).await
    }

    /// Saves a batch of episodes in a single transaction.
    pub async fn save_episodes_batch(&self, episodes: &[EpisodeSave]) -> Result<()> {
        self.save_episodes_batch_db(episodes).await
    }

    pub async fn record_episode_tokens_for_cache(&self, scope: &str, content: &str) {
        let tokens = crate::retrieval::bm25::tokenize(content);
        let unique_tokens: std::collections::HashSet<String> = tokens.into_iter().collect();
        let now = std::time::Instant::now();
        let mut outer_write = self.term_counts_cache.write().await;
        let inner_lock = outer_write.entry(scope.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())))
            .clone();
        drop(outer_write);
        
        let mut inner_write = inner_lock.write().await;
        for token in unique_tokens {
            let entry = inner_write.entry(token).or_insert_with(|| CacheEntry {
                count: 0,
                expires_at: now + std::time::Duration::from_secs(3600),
            });
            entry.count += 1;
        }
        
        let entry = inner_write.entry("__total_n__".to_string()).or_insert_with(|| CacheEntry {
            count: 0,
            expires_at: now + std::time::Duration::from_secs(3600),
        });
        entry.count += 1;
    }

    pub async fn record_episodes_batch_tokens_for_cache(&self, episodes: &[EpisodeSave]) {
        let mut scope_tokens: std::collections::HashMap<String, Vec<std::collections::HashSet<String>>> = std::collections::HashMap::new();
        for ep in episodes {
            let scope = ep.scope.clone().unwrap_or_else(|| "general".to_string());
            let tokens = crate::retrieval::bm25::tokenize(&ep.content);
            let unique_tokens: std::collections::HashSet<String> = tokens.into_iter().collect();
            scope_tokens.entry(scope).or_default().push(unique_tokens);
        }
        
        let now = std::time::Instant::now();
        let mut outer_write = self.term_counts_cache.write().await;
        for (scope, list) in scope_tokens {
            let inner_lock = outer_write.entry(scope)
                .or_insert_with(|| Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())))
                .clone();
            let mut inner_write = inner_lock.write().await;
            for unique_tokens in list {
                for token in unique_tokens {
                    let entry = inner_write.entry(token).or_insert_with(|| CacheEntry {
                        count: 0,
                        expires_at: now + std::time::Duration::from_secs(3600),
                    });
                    entry.count += 1;
                }
                let entry = inner_write.entry("__total_n__".to_string()).or_insert_with(|| CacheEntry {
                    count: 0,
                    expires_at: now + std::time::Duration::from_secs(3600),
                });
                entry.count += 1;
            }
        }
    }

    pub async fn replay_wal_if_fresh(&self, wal_path: &std::path::Path, initialized_marker: &std::path::Path) -> Result<()> {
        if self.is_client_mode() {
            return Ok(());
        }
        if initialized_marker.exists() {
            if let Ok(episodes) = self.get_all_episodes().await {
                if !episodes.is_empty() {
                    return Ok(());
                }
            }
        }
        if wal_path.exists() {
            let file = std::fs::File::open(wal_path)?;
            let reader = std::io::BufReader::new(file);
            use std::io::BufRead;
            for line in reader.lines() {
                if let Ok(l) = line {
                    if l.trim().is_empty() {
                        continue;
                    }
                    if let Ok(episode) = serde_json::from_str::<EpisodeSave>(&l) {
                        if let Err(e) = self.save_episode(&episode).await {
                            tracing::error!("Failed to save recovered episode from WAL: {:?}", e);
                        }
                    }
                }
            }
        }
        if let Some(parent) = initialized_marker.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(initialized_marker, "initialized")?;
        Ok(())
    }

    pub async fn compact_wal_file(&self, wal_path: &std::path::Path) -> Result<()> {
        if self.is_client_mode() {
            return Ok(());
        }
        if !wal_path.exists() {
            return Ok(());
        }
        let file = std::fs::File::open(wal_path)?;
        let reader = std::io::BufReader::new(file);
        use std::io::BufRead;
        let mut episodes = Vec::new();
        for line in reader.lines() {
            if let Ok(l) = line {
                if l.trim().is_empty() {
                    continue;
                }
                if let Ok(ep) = serde_json::from_str::<EpisodeSave>(&l) {
                    episodes.push(ep);
                }
            }
        }
        let mut unique_episodes = Vec::new();
        let mut seen_titles = std::collections::HashSet::new();
        for ep in episodes.into_iter().rev() {
            if seen_titles.insert(ep.title.clone()) {
                unique_episodes.push(ep);
            }
        }
        unique_episodes.reverse();

        let temp_path = wal_path.with_extension("tmp");
        {
            let mut file = {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    std::fs::OpenOptions::new()
                        .create(true)
                        .write(true)
                        .truncate(true)
                        .mode(0o600)
                        .open(&temp_path)?
                }
                #[cfg(not(unix))]
                {
                    std::fs::OpenOptions::new()
                        .create(true)
                        .write(true)
                        .truncate(true)
                        .open(&temp_path)?
                }
            };
            use std::io::Write;
            for ep in &unique_episodes {
                let line_str = serde_json::to_string(ep)?;
                writeln!(file, "{}", line_str)?;
            }
            file.sync_all()?;
        }
        std::fs::rename(temp_path, wal_path)?;
        Ok(())
    }

    pub async fn new(url: &str) -> Result<Self> {
        // Helper to detect if we are running inside cargo test
        let is_running_in_test = {
            let in_test_exe = if let Ok(exe) = std::env::current_exe() {
                let name = exe.to_string_lossy();
                name.contains("/deps/") || name.contains("test")
            } else {
                false
            };
            in_test_exe || std::env::args().any(|arg| arg.contains("test"))
        };

        // 1. Determine daemon port from env or default
        let env_port = std::env::var("MYTHRAX_DAEMON_PORT").ok();
        let daemon_port = env_port
            .as_ref()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(8090);

        // 2. Only check the daemon port if we are not running inside a unit test,
        // OR if MYTHRAX_DAEMON_PORT was explicitly set in the environment.
        let is_daemon_available = if env_port.is_some() || !is_running_in_test {
            match tokio::time::timeout(
                std::time::Duration::from_millis(50),
                tokio::net::TcpStream::connect(format!("127.0.0.1:{}", daemon_port))
            ).await {
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
            let db = Surreal::new::<Mem>(()).await
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
                    .unwrap();
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
                            if (err_str.contains("locked") || err_str.contains("LOCK")) && attempt < 10 {
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
                Surreal::new::<Mem>(()).await
                    .context("Failed to initialize SurrealDB with in-memory store")?
            };
            db.use_ns("mythrax").use_db("memory").await?;
            (db, None)
        };

        // 3. Initialize embedder using cached global LocalEmbedder
        let embedder = match crate::embeddings::LocalEmbedder::get_global() {
            Ok(emb) => Some(emb),
            Err(e) => {
                tracing::warn!("Failed to initialize LocalEmbedder: {}. Falling back to non-embedded mode.", e);
                None
            }
        };

        // 4. Initialize write lock and WAL channel
        let write_lock = Arc::new(tokio::sync::Mutex::new(()));
        let (wal_tx, mut wal_rx) = tokio::sync::mpsc::channel::<(EpisodeSave, std::path::PathBuf)>(100);

        // Spawn WAL actor task
        tokio::spawn(async move {
            use std::io::Write;
            while let Some((episode, wal_path)) = wal_rx.recv().await {
                // Open file with strict 0600 permissions
                let file = {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::OpenOptionsExt;
                        std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .mode(0o600)
                            .open(&wal_path)
                    }
                    #[cfg(not(unix))]
                    {
                        std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&wal_path)
                    }
                };

                match file {
                    Ok(mut f) => {
                        if let Ok(line) = serde_json::to_string(&episode) {
                            let _ = writeln!(f, "{}", line);
                            let _ = f.sync_all();
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to open WAL file for writing: {:?}", e);
                    }
                }
            }
        });

        let active_embeddings = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let max_concurrent_embeddings = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let indexing_writes = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
        let embedding_semaphore = Arc::new(tokio::sync::Mutex::new(None));

        let backend = Self {
            db,
            embedder,
            client_port,
            write_lock,
            wal_tx: Some(wal_tx),
            db_path,
            active_embeddings,
            max_concurrent_embeddings,
            indexing_writes,
            embedding_semaphore,
            term_counts_cache: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            global_cache_size: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            avg_dl_cache: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            search_mode: Arc::new(tokio::sync::Mutex::new("hybrid".to_string())),
            reranker: Arc::new(tokio::sync::Mutex::new(None)),
            reinforcement_semaphore: Arc::new(tokio::sync::Semaphore::new(10)),
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
            wal_tx: None,
            db_path: None,
            active_embeddings: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            max_concurrent_embeddings: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            indexing_writes: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            embedding_semaphore: Arc::new(tokio::sync::Mutex::new(None)),
            term_counts_cache: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            global_cache_size: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            avg_dl_cache: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            search_mode: Arc::new(tokio::sync::Mutex::new("hybrid".to_string())),
            reranker: Arc::new(tokio::sync::Mutex::new(None)),
            reinforcement_semaphore: Arc::new(tokio::sync::Semaphore::new(10)),
        }
    }

    pub fn is_client_mode(&self) -> bool {
        self.client_port.is_some()
    }

    pub async fn set_search_mode(&self, mode: &str) {
        let mut m = self.search_mode.lock().await;
        *m = mode.to_string();
    }

    pub async fn get_search_mode(&self) -> String {
        let m = self.search_mode.lock().await;
        m.clone()
    }

    pub async fn new_client_connection() -> Result<Self> {
        Self::new("mem://").await
    }

    pub async fn record_indexing_write(&self, vault_path: &str) {
        if !vault_path.is_empty() {
            let mut writes = self.indexing_writes.lock().await;
            *writes.entry(vault_path.to_string()).or_insert(0) += 1;
            if let Some(filename) = std::path::Path::new(vault_path).file_name().and_then(|s| s.to_str()) {
                if filename != vault_path {
                    *writes.entry(filename.to_string()).or_insert(0) += 1;
                }
            }
        }
    }

    pub async fn get_max_concurrent_tasks(&self) -> usize {
        let sql = "SELECT VALUE embeddings.max_concurrent_tasks FROM config:settings LIMIT 1;";
        if let Ok(mut resp) = self.db.query(sql).await {
            if let Ok(Some(val)) = resp.take::<Option<usize>>(0) {
                return val;
            }
        }
        2 // Default fallback
    }

    pub async fn get_embedding_semaphore(&self) -> Arc<tokio::sync::Semaphore> {
        let limit = self.get_max_concurrent_tasks().await;
        let mut guard = self.embedding_semaphore.lock().await;
        if let Some((current_limit, ref sem)) = *guard {
            if current_limit == limit {
                return sem.clone();
            }
        }
        let sem = Arc::new(tokio::sync::Semaphore::new(limit));
        *guard = Some((limit, sem.clone()));
        sem
    }

    /// Helper to load the auth token from the standard location or fallback
    fn get_auth_token() -> String {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let token_path = std::path::PathBuf::from(home).join(".mythrax/token");
        
        crate::auth::get_or_create_token(&token_path).unwrap_or_else(|_| "fallback-err-token".to_string())
    }

    pub async fn resolve_query_anchors(
        &self,
        query: &str,
        query_emb: Option<&Vec<f32>>,
    ) -> Vec<(String, f32)> {
        let mut anchors: Vec<(String, f32)> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        let sql_entities = "SELECT id FROM entity WHERE name = $query OR $query IN labels LIMIT 5;";
        if let Ok(mut response) = self.db.query(sql_entities).bind(("query", query)).await {
            if let Ok(rows) = response.take::<Vec<EntityRow>>(0) {
                for r in rows {
                    let id_str = format_record_id(&r.id);
                    if seen.insert(id_str.clone()) {
                        anchors.push((id_str, 1.0f32));
                    }
                }
            }
        }

        let sql_wiki = "SELECT id FROM wiki_node WHERE name = $query LIMIT 5;";
        if let Ok(mut response) = self.db.query(sql_wiki).bind(("query", query)).await {
            if let Ok(rows) = response.take::<Vec<WikiRow>>(0) {
                for r in rows {
                    let id_str = format_record_id(&r.id);
                    if seen.insert(id_str.clone()) {
                        anchors.push((id_str, 1.0f32));
                    }
                }
            }
        }

        if anchors.len() < 5 {
            if let Some(emb) = query_emb {
                let sql_knn_entities = "SELECT id, vector::similarity::cosine(embedding, $emb) AS similarity FROM entity WHERE embedding <|5, 100|> $emb LIMIT 5;";
                if let Ok(response) = self.db.query(sql_knn_entities).bind(("emb", emb.clone())).await {
                    if let Ok(mut response) = response.check() {
                        if let Ok(rows) = response.take::<Vec<KnnRow>>(0) {
                            for r in rows {
                                let id_str = format_record_id(&r.id);
                                if seen.insert(id_str.clone()) {
                                    let dist = r.similarity.unwrap_or(1.0);
                                    let sim = (1.0f32 - dist).clamp(0.0, 1.0);
                                    anchors.push((id_str, sim));
                                    if anchors.len() >= 5 {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }

                if anchors.len() < 5 {
                    let sql_knn_wiki = "SELECT id, vector::similarity::cosine(embedding, $emb) AS similarity FROM wiki_node WHERE embedding <|5, 100|> $emb LIMIT 5;";
                    if let Ok(response) = self.db.query(sql_knn_wiki).bind(("emb", emb.clone())).await {
                        if let Ok(mut response) = response.check() {
                            if let Ok(rows) = response.take::<Vec<KnnRow>>(0) {
                                for r in rows {
                                    let id_str = format_record_id(&r.id);
                                    if seen.insert(id_str.clone()) {
                                        let dist = r.similarity.unwrap_or(1.0);
                                        let sim = (1.0f32 - dist).clamp(0.0, 1.0);
                                        anchors.push((id_str, sim));
                                        if anchors.len() >= 5 {
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        anchors.truncate(5);
        anchors
    }

    /// Post a request to the running daemon
    pub async fn daemon_post<Req: serde::Serialize, Resp: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        payload: &Req,
    ) -> Result<Resp> {
        let port = self.client_port.ok_or_else(|| {
            anyhow::anyhow!("Not in client mode, cannot route to daemon")
        })?;

        let url = format!("http://127.0.0.1:{}/{}", port, path.trim_start_matches('/'));
        let token = Self::get_auth_token();

        let client = reqwest::Client::new();
        let res = client
            .post(&url)
            .header("X-Mythrax-Token", token)
            .json(payload)
            .send()
            .await
            .context(format!("Failed to send request to daemon at {}", url))?;

        let status = res.status();
        let body = res.text().await.context("Failed to read daemon response body")?;

        if !status.is_success() {
            return Err(anyhow::anyhow!("Daemon returned error {}: {}", status, body));
        }

        let resp: Resp = serde_json::from_str(&body)
            .context(format!("Failed to deserialize daemon response: {}", body))?;

        Ok(resp)
    }

    /// Get a request from the running daemon
    pub async fn daemon_get<Resp: serde::de::DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<Resp> {
        let port = self.client_port.ok_or_else(|| {
            anyhow::anyhow!("Not in client mode, cannot route to daemon")
        })?;

        let url = format!("http://127.0.0.1:{}/{}", port, path.trim_start_matches('/'));
        let token = Self::get_auth_token();

        let client = reqwest::Client::new();
        let res = client
            .get(&url)
            .header("X-Mythrax-Token", token)
            .send()
            .await
            .context(format!("Failed to send request to daemon at {}", url))?;

        let status = res.status();
        let body = res.text().await.context("Failed to read daemon response body")?;

        if !status.is_success() {
            return Err(anyhow::anyhow!("Daemon returned error {}: {}", status, body));
        }

        let resp: Resp = serde_json::from_str(&body)
            .context(format!("Failed to deserialize daemon response: {}", body))?;

        Ok(resp)
    }

    pub fn count_text_tokens(&self, text: &str) -> usize {
        if let Some(tok) = get_global_tokenizer() {
            if let Ok(encoding) = tok.encode(text, true) {
                return encoding.get_ids().len();
            }
        }
        if let Some(ref embedder) = self.embedder {
            if let Ok(count) = embedder.count_tokens(text) {
                return count;
            }
        }
        estimate_bpe_tokens(text)
    }

    #[allow(dead_code)]
    pub async fn new_in_memory() -> Result<Self> {
        let backend = Self::new("mem://").await?;
        let random_db = format!("db_{}", Uuid::new_v4().to_string().replace("-", "_"));
        backend.db.use_ns("mythrax").use_db(&random_db).await?;
        Ok(backend)
    }

    pub(crate) fn compact_search_result(&self, item: &mut SearchResult, remaining_budget: usize) -> bool {
        let title_tokens = self.count_text_tokens(&format!("{}\n", item.title));
        if title_tokens >= remaining_budget {
            return false;
        }
        let content_budget = remaining_budget - title_tokens;

        // Try wisdom rule compaction
        if item.content.contains("**Why**:") {
            let why_prefix = "\n**Why**:";
            let remedy_prefix = "\n**Prescribed Remedy**:";
            if let Some(why_start) = item.content.find(why_prefix) {
                if let Some(remedy_start) = item.content.find(remedy_prefix) {
                    let avoid_part = &item.content[..why_start];
                    let remedy_part = &item.content[remedy_start..];
                    let compacted_content = format!("{}{}", avoid_part, remedy_part);
                    let compacted_tokens = self.count_text_tokens(&compacted_content);
                    if compacted_tokens <= content_budget {
                        item.content = compacted_content;
                        return true;
                    }
                }
            }
        }

        // Try paragraph compaction
        let paragraphs: Vec<&str> = item.content.split("\n\n").collect();
        if paragraphs.len() > 1 {
            let mut compacted_content = paragraphs[0].to_string();
            compacted_content.push_str("\n\n... [Truncated (Inner-Node Compaction)]");
            let compacted_tokens = self.count_text_tokens(&compacted_content);
            if compacted_tokens <= content_budget {
                item.content = compacted_content;
                return true;
            }
        }

        // Hard character binary search truncation fallback
        let original_content = item.content.clone();
        let mut low = 0;
        let mut high = original_content.len();
        let mut best_fit = String::new();

        while low <= high {
            let mid = (low + high) / 2;
            let candidate_content = if mid < original_content.len() {
                format!("{}... [Truncated (Inner-Node Compaction)]", &original_content[..mid])
            } else {
                original_content.clone()
            };
            let tokens = self.count_text_tokens(&candidate_content);
            if tokens <= content_budget {
                best_fit = candidate_content;
                low = mid + 1;
            } else {
                if mid == 0 {
                    break;
                }
                high = mid - 1;
            }
        }

        if !best_fit.is_empty() {
            item.content = best_fit;
            true
        } else {
            false
        }
    }

    pub fn resolve_active_scope(&self) -> String {
        let start_path = if let Ok(workspace_root) = std::env::var("MYTHRAX_WORKSPACE_ROOT") {
            std::path::PathBuf::from(workspace_root)
        } else if let Ok(cwd) = std::env::current_dir() {
            cwd
        } else {
            return "general".to_string();
        };

        let mut current = start_path.as_path();
        loop {
            let is_marker = current.join(".git").exists()
                || current.join(".agents").exists()
                || current.join("Cargo.toml").exists()
                || current.join("package.json").exists();

            if is_marker {
                if let Some(name_os) = current.file_name() {
                    if let Some(name_str) = name_os.to_str() {
                        let normalized: String = name_str
                            .chars()
                            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
                            .map(|c| c.to_ascii_lowercase())
                            .collect::<String>()
                            .trim_matches('.')
                            .to_string();
                        if !normalized.is_empty() {
                            return normalized;
                        }
                    }
                }
                break;
            }

            if let Some(parent) = current.parent() {
                current = parent;
            } else {
                break;
            }
        }
        "general".to_string()
    }
}

#[derive(serde::Deserialize, Debug, SurrealValue)]
pub(crate) struct WisdomRaw {
    pub(crate) id: surrealdb::types::RecordId,
    pub(crate) target_pattern: String,
    pub(crate) action_to_avoid: String,
    pub(crate) causal_explanation: String,
    pub(crate) prescribed_remedy: String,
    pub(crate) tier: String,
    pub(crate) scope: String,
    pub(crate) vault_path: Option<String>,
    pub(crate) embedding: Option<Vec<f32>>,
    pub(crate) source_episodes: Option<Vec<String>>,
    pub(crate) generator_name: String,
    pub(crate) utility: Option<f32>,
    pub(crate) status: Option<String>,
    pub(crate) superseded_at: Option<String>,
    pub(crate) superseded_by: Option<String>,
    pub(crate) rule_type: Option<String>,
}

impl WisdomRaw {
    pub(crate) fn into_wisdom_rule(self) -> WisdomRule {
        let id_str = format_record_id(&self.id);
        WisdomRule {
            id: Some(id_str),
            target_pattern: self.target_pattern,
            action_to_avoid: self.action_to_avoid,
            causal_explanation: self.causal_explanation,
            prescribed_remedy: self.prescribed_remedy,
            tier: self.tier,
            scope: self.scope,
            vault_path: self.vault_path,
            embedding: self.embedding,
            source_episodes: self.source_episodes.unwrap_or_default(),
            generator_name: self.generator_name,
            similarity: None,
            utility: self.utility,
            status: self.status,
            superseded_at: self.superseded_at,
            superseded_by: self.superseded_by,
            rule_type: self.rule_type,
        }
    }
}

#[derive(serde::Deserialize, Debug, SurrealValue)]
struct SearchRaw {
    id: surrealdb::types::RecordId,
    title: String,
    content: String,
    utility: Option<f64>,
    embedding: Option<Vec<f32>>,
    vault_path: Option<String>,
    related_nodes: Option<Vec<RelatedNodeRaw>>,
    prev_episodes: Option<Vec<EpisodeRaw>>,
    next_episodes: Option<Vec<EpisodeRaw>>,
    last_retrieved_at: Option<String>,
    importance: Option<f64>,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
    archived: Option<bool>,
    archived_at: Option<chrono::DateTime<chrono::Utc>>,
    discovery_tokens: Option<u32>,
    session_id: Option<String>,
    word_count: Option<u32>,
    scope: Option<String>,
    bm25_score: Option<f32>,
    confidence: Option<f32>,
}

#[derive(serde::Deserialize, Debug, SurrealValue)]
struct RelatedNodeRaw {
    id: surrealdb::types::RecordId,
    title: Option<String>,
    name: Option<String>,
    content: Option<String>,
    summary: Option<String>,
    target_pattern: Option<String>,
    causal_explanation: Option<String>,
    action_to_avoid: Option<String>,
    prescribed_remedy: Option<String>,
    vault_path: Option<String>,
    source_episode: Option<surrealdb::types::RecordId>,
}

#[derive(serde::Deserialize, Debug, SurrealValue)]
struct SearchWisdomRaw {
    id: surrealdb::types::RecordId,
    target_pattern: String,
    action_to_avoid: String,
    causal_explanation: String,
    prescribed_remedy: String,
    tier: String,
    scope: String,
    embedding: Option<Vec<f32>>,
    generator_name: String,
    utility: Option<f64>,
    vault_path: Option<String>,
    related_nodes: Option<Vec<RelatedNodeRaw>>,
    importance: Option<f64>,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, SurrealValue, Clone)]
pub struct EpisodeRaw {
    pub id: surrealdb::types::RecordId,
    pub title: String,
    pub content: String,
    pub source: Option<String>,
    pub scope: Option<String>,
    pub vault_path: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub processed_in_dream: Option<bool>,
    pub source_episode: Option<surrealdb::types::RecordId>,
    pub last_retrieved_at: Option<String>,
    pub utility: Option<f32>,
    pub archived: Option<bool>,
    pub archived_at: Option<chrono::DateTime<chrono::Utc>>,
    pub discovery_tokens: Option<u32>,
    pub facts: Option<Vec<String>>,
    pub concepts: Option<Vec<String>>,
    pub files_read: Option<Vec<String>>,
    pub files_modified: Option<Vec<String>>,
    pub session_id: Option<String>,
    pub word_count: Option<u32>,
    pub node_type: Option<String>,
    pub confidence: Option<f32>,
}

impl From<EpisodeRaw> for Episode {
    fn from(raw: EpisodeRaw) -> Self {
        Episode {
            id: Some(format_record_id(&raw.id)),
            title: raw.title,
            content: raw.content,
            source: raw.source,
            scope: raw.scope,
            vault_path: raw.vault_path,
            embedding: raw.embedding,
            processed_in_dream: raw.processed_in_dream,
            source_episode: raw.source_episode.map(|t| format_record_id(&t)),
            last_retrieved_at: raw.last_retrieved_at,
            utility: raw.utility,
            archived: raw.archived,
            archived_at: raw.archived_at.map(|t| t.to_rfc3339()),
            discovery_tokens: raw.discovery_tokens,
            facts: raw.facts,
            concepts: raw.concepts,
            files_read: raw.files_read,
            files_modified: raw.files_modified,
            session_id: raw.session_id,
            word_count: raw.word_count,
            node_type: raw.node_type,
            confidence: raw.confidence,
        }
    }
}


/// Full hydrated Handoff contract — returned by queries; construction deferred pending
/// the agent-tracking dashboard feature. Suppressed until then.
#[allow(dead_code)]
#[derive(serde::Deserialize, Debug, SurrealValue)]
pub(crate) struct HandoffRaw {
    pub(crate) id: surrealdb::types::RecordId,
    pub(crate) parent_conversation_id: String,
    pub(crate) subagent_conversation_id: String,
    pub(crate) summary: String,
    pub(crate) handoff_file_path: String,
    pub(crate) scope: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) created_at: Option<serde_json::Value>,
    pub(crate) include_tool_execution: Option<bool>,
}

#[derive(serde::Deserialize, Debug, SurrealValue)]
pub(crate) struct WikiNodeRaw {
    pub(crate) id: surrealdb::types::RecordId,
    pub(crate) name: String,
    pub(crate) content: String,
    pub(crate) scope: String,
    pub(crate) vault_path: Option<String>,
    pub(crate) embedding: Option<Vec<f32>>,
}

impl WikiNodeRaw {
    pub(crate) fn into_wiki_node(self) -> WikiNode {
        let id_str = format_record_id(&self.id);
        WikiNode {
            id: Some(id_str),
            name: self.name,
            content: self.content,
            scope: self.scope,
            vault_path: self.vault_path,
            embedding: self.embedding,
        }
    }
}

#[derive(serde::Deserialize, SurrealValue)]
pub(crate) struct ScoredEdge {
    pub(crate) out: surrealdb::types::RecordId,
    pub(crate) confidence: Option<f32>,
}

#[derive(serde::Deserialize, SurrealValue)]
struct EntityRow {
    id: surrealdb::types::RecordId,
}

#[derive(serde::Deserialize, SurrealValue)]
struct WikiRow {
    id: surrealdb::types::RecordId,
}

#[derive(serde::Deserialize, SurrealValue)]
struct KnnRow {
    id: surrealdb::types::RecordId,
    similarity: Option<f32>,
}

#[derive(serde::Deserialize, SurrealValue, Debug)]
pub struct MetricAccess {
    pub target_id: surrealdb::types::RecordId,
    pub access_count: i64,
}

fn prepare_fts_query(query: &str, cap: usize) -> Vec<String> {
    let stop_words: std::collections::HashSet<&str> = [
        "a", "about", "above", "after", "again", "against", "all", "am", "an", "and", "any", "are", "arent",
        "as", "at", "be", "because", "been", "before", "being", "below", "between", "both", "but", "by",
        "cant", "cannot", "could", "couldnt", "did", "didnt", "do", "does", "doesnt", "doing", "dont",
        "down", "during", "each", "few", "for", "from", "further", "had", "hadnt", "has", "hasnt", "have",
        "havent", "having", "he", "hed", "hell", "hes", "her", "here", "heres", "hers", "herself", "him",
        "himself", "his", "how", "hows", "i", "id", "ill", "im", "ive", "if", "in", "into", "is", "isnt",
        "it", "its", "itself", "lets", "me", "more", "most", "mustnt", "my", "myself", "no", "nor", "not",
        "of", "off", "on", "once", "only", "or", "other", "ought", "our", "ours", "ourselves", "out",
        "over", "own", "same", "shant", "she", "shed", "shell", "shes", "should", "shouldnt", "so",
        "some", "such", "than", "that", "thats", "the", "their", "theirs", "them", "themselves", "then",
        "there", "theres", "these", "they", "theyd", "theyll", "theyre", "theyve", "this", "those",
        "through", "to", "too", "under", "until", "up", "very", "was", "wasnt", "we", "wed", "well",
        "were", "weve", "werent", "what", "whats", "when", "whens", "where", "wheres", "which", "while",
        "who", "whos", "whom", "why", "whys", "with", "wont", "would", "wouldnt", "you", "youd", "youll",
        "youre", "youve", "your", "yours", "yourself", "yourselves"
    ].iter().cloned().collect();

    let cleaned: String = query.chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { ' ' })
        .collect();

    let words: Vec<String> = cleaned.split_whitespace()
        .filter(|w| !stop_words.contains(w) && w.len() >= 2)
        .map(|w| {
            let normalized = normalize_spelling(w);
            let expanded = expand_synonyms(normalized);
            expanded.to_string()
        })
        .collect();

    if words.is_empty() {
        // Fallback: use the raw cleaned query as a single token
        let fallback = query.chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>();
        let fallback = fallback.trim().to_string();
        if fallback.is_empty() {
            vec![]
        } else {
            vec![fallback]
        }
    } else {
        words.into_iter().take(cap).collect()
    }
}

pub(crate) static PROFILE_CACHE: std::sync::OnceLock<std::sync::RwLock<std::collections::HashMap<String, Option<String>>>> = std::sync::OnceLock::new();

#[async_trait]
impl StorageBackend for SurrealBackend {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn get_all_registered_transcripts(&self) -> Result<Vec<(String, String)>> {
        let sql = "SELECT session_id, value FROM short_term_memory WHERE key = '_transcript_path';";
        let mut response = self.db.query(sql).await?.check()?;
        #[derive(serde::Deserialize, surrealdb_types::SurrealValue, Debug)]
        struct StmRecord {
            session_id: String,
            value: String,
        }
        let records: Vec<StmRecord> = response.take(0)?;
        let res = records.into_iter().map(|r| (r.session_id, r.value)).collect();
        Ok(res)
    }

    async fn get_session_last_activity(&self, session_id: &str) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
        let sql = "SELECT VALUE updated_at FROM short_term_memory WHERE session_id = $session_id ORDER BY updated_at DESC LIMIT 1;";
        let mut response = self.db.query(sql).bind(("session_id", session_id)).await?.check()?;
        let last_activity: Option<chrono::DateTime<chrono::Utc>> = response.take(0)?;
        Ok(last_activity)
    }

    async fn search_filtered(
        &self,
        query: &str,
        scope: Option<&str>,
        limit: usize,
        threshold: f32,
        concepts: &[String],
        files: &[String],
    ) -> Result<SearchResponse> {
        // Call the regular search. Since we are filtering, we over-fetch (limit * 3) to ensure we don't drop below the limit.
        let unfiltered = self.search(
            crate::contracts::SearchParams::from_positional(
                query,
                scope,
                false,
                limit * 3,
                0,
                threshold,
                None,
                false,
                true,
                false,
                None,
                true,
                None,
            )
        ).await?;

        let ep_ids: Vec<String> = unfiltered.results.iter()
            .filter(|r| r.id.starts_with("episode:"))
            .map(|r| r.id.clone())
            .collect();

        let hydrated = if !ep_ids.is_empty() {
            self.get_memory_nodes(&ep_ids).await?
        } else {
            crate::contracts::GetMemoryNodesResponse {
                wiki_nodes: Vec::new(),
                wisdom_rules: Vec::new(),
                episodes: Vec::new(),
            }
        };

        use std::collections::HashMap;
        let ep_map: HashMap<String, crate::contracts::Episode> = hydrated.episodes.into_iter()
            .filter_map(|ep| ep.id.clone().map(|id| (id, ep)))
            .collect();

        let mut filtered_results = Vec::new();
        for res in unfiltered.results.clone() {
            if res.id.starts_with("episode:") {
                if let Some(ep) = ep_map.get(&res.id) {
                    let mut matches_concept = concepts.is_empty();
                    if !concepts.is_empty() {
                        if let Some(ref ep_concepts) = ep.concepts {
                            matches_concept = concepts.iter().any(|c| ep_concepts.contains(c));
                        }
                    }

                    let mut matches_file = files.is_empty();
                    if !files.is_empty() {
                        let mut ep_files = Vec::new();
                        if let Some(ref fr) = ep.files_read {
                            ep_files.extend(fr.iter().cloned());
                        }
                        if let Some(ref fm) = ep.files_modified {
                            ep_files.extend(fm.iter().cloned());
                        }
                        matches_file = files.iter().any(|f| ep_files.contains(f));
                    }

                    if matches_concept && matches_file {
                        filtered_results.push(res);
                    }
                }
            }
        }

        let final_results = if filtered_results.is_empty() {
            unfiltered.results
        } else {
            filtered_results
        };

        // Truncate to the requested limit
        let truncated = final_results.into_iter().take(limit).collect::<Vec<_>>();
        let total = truncated.len();

        Ok(SearchResponse {
            results: truncated,
            total_matches: total,
            has_more: false,
            next_offset: 0,
            omitted_ids: None,
        })
    }

    async fn get_max_concurrent_tasks(&self) -> usize {
        SurrealBackend::get_max_concurrent_tasks(self).await
    }

    async fn init(&self) -> Result<()> {
        self.init_db().await
    }

    async fn save_episode(&self, episode: &EpisodeSave) -> Result<String> {
        self.save_episode_db(episode).await
    }

    async fn save_wisdom_rule(&self, rule: &WisdomRule) -> Result<String> {
        self.save_wisdom_rule_db(rule).await
    }

    async fn search(
        &self,
        params: crate::contracts::SearchParams,
    ) -> Result<SearchResponse> {
        self.search_pipeline(params).await
    }

    async fn get_wisdom(&self, query: &str, tier: Option<&str>, limit: usize, offset: usize, threshold: f32) -> Result<WisdomSearchResponse> {
        let active_scope = self.resolve_active_scope();

        let query_emb = if let Some(ref _embedder) = self.embedder {
            let formatted_query = format!("search_query: {}", query);
            match self.embed(&formatted_query).await {
                Ok(vec) => Some(vec),
                Err(e) => {
                    tracing::warn!("Embedding generation failed in get_wisdom: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let response = if let Some(ref q_vec) = query_emb {
            let sql = if tier.is_some() {
                "
                SELECT *,
                       (SELECT VALUE utility_score FROM metrics WHERE target_id = $parent.id LIMIT 1)[0] AS utility
                FROM wisdom
                WHERE status != 'superseded'
                  AND tier = $tier
                  AND (scope IN [$active_scope, 'general'] OR $active_scope = 'all')
                  AND (embedding <|200, 200|> $query_embedding);
                "
            } else {
                "
                SELECT *,
                       (SELECT VALUE utility_score FROM metrics WHERE target_id = $parent.id LIMIT 1)[0] AS utility
                FROM wisdom
                WHERE status != 'superseded'
                  AND (scope IN [$active_scope, 'general'] OR $active_scope = 'all')
                  AND (embedding <|200, 200|> $query_embedding);
                "
            };
            let mut q = self.db.query(sql);
            if let Some(t) = tier {
                q = q.bind(("tier", t));
            }
            q.bind(("active_scope", active_scope.as_str()))
                .bind(("query_embedding", q_vec.clone()))
                .await?
        } else {
            let sql = if tier.is_some() {
                "
                SELECT *,
                       (SELECT VALUE utility_score FROM metrics WHERE target_id = $parent.id LIMIT 1)[0] AS utility
                FROM wisdom
                WHERE status != 'superseded'
                  AND tier = $tier
                  AND (scope IN [$active_scope, 'general'] OR $active_scope = 'all')
                  AND (string::contains(target_pattern, $query) OR string::contains(causal_explanation, $query));
                "
            } else {
                "
                SELECT *,
                       (SELECT VALUE utility_score FROM metrics WHERE target_id = $parent.id LIMIT 1)[0] AS utility
                FROM wisdom
                WHERE status != 'superseded'
                  AND (scope IN [$active_scope, 'general'] OR $active_scope = 'all')
                  AND (string::contains(target_pattern, $query) OR string::contains(causal_explanation, $query));
                "
            };
            let mut q = self.db.query(sql);
            if let Some(t) = tier {
                q = q.bind(("tier", t));
            }
            q.bind(("query", query))
                .bind(("active_scope", active_scope.as_str()))
                .await?
        };

        let mut response = response.check().context("Get wisdom query failed")?;
        let raws: Vec<WisdomRaw> = response.take(0)?;
        let wisdom: Vec<WisdomRule> = raws.into_iter().map(|r| r.into_wisdom_rule()).collect();
        let mut candidates = Vec::new();

        for mut w in wisdom {
            if let Some(ref id_str) = w.id
                && let Ok(thing) = parse_record_id(id_str) {
                    w.id = Some(format_record_id(&thing));
                }

            let similarity = if let (Some(q_vec), Some(e_vec)) = (query_emb.as_ref(), w.embedding.as_ref()) {
                let dot: f32 = q_vec.iter().zip(e_vec.iter()).map(|(a, b)| a * b).sum();
                dot
            } else {
                1.0
            };

            let utility = w.utility.unwrap_or(1.0);
            let blended_score = similarity * (0.7 + 0.3 * utility);

            if blended_score >= threshold {
                w.similarity = Some(similarity);
                w.utility = Some(utility);
                candidates.push(w);
            }
        }

        // Sort by blended score descending
        candidates.sort_by(|a, b| {
            let score_a = a.similarity.unwrap_or(1.0) * (0.7 + 0.3 * a.utility.unwrap_or(1.0));
            let score_b = b.similarity.unwrap_or(1.0) * (0.7 + 0.3 * b.utility.unwrap_or(1.0));
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        let total_matches = candidates.len();
        let has_more = total_matches > offset + limit;
        let next_offset = offset + limit;

        let sliced_results = if offset < total_matches {
            let end = std::cmp::min(offset + limit, total_matches);
            candidates[offset..end].to_vec()
        } else {
            Vec::new()
        };

        Ok(WisdomSearchResponse {
            results: sliced_results,
            total_matches,
            has_more,
            next_offset,
        })
    }

    async fn record_feedback(&self, id: &str, success: bool) -> Result<()> {
        self.record_feedback_db(id, success).await
    }

    async fn apply_migrations(&self) -> Result<()> {
        Ok(())
    }

    async fn get_llm_config(&self) -> Result<LlmConfigResponse> {
        self.get_llm_config_db().await
    }

    async fn update_llm_config(&self, req: &LlmConfigRequest) -> Result<()> {
        self.update_llm_config_db(req).await
    }

    async fn get_unprocessed_episodes(&self) -> Result<Vec<Episode>> {
        self.get_unprocessed_episodes_db().await
    }

    async fn mark_episode_processed(&self, id: &str) -> Result<()> {
        self.mark_episode_processed_db(id).await
    }

    async fn get_all_episodes(&self) -> Result<Vec<Episode>> {
        self.get_all_episodes_db().await
    }

    async fn get_episodes_by_node_type(&self, node_type: &str) -> Result<Vec<Episode>> {
        self.get_episodes_by_node_type_db(node_type).await
    }

    async fn is_feature_enabled(&self, feature_key: &str, default: bool) -> bool {
        self.is_feature_enabled_db(feature_key, default).await
    }

    async fn save_profile_key(&self, key: &str, value: &str) -> Result<()> {
        self.save_profile_key_db(key, value).await
    }

    async fn get_profile_key(&self, key: &str) -> Result<Option<String>> {
        self.get_profile_key_db(key).await
    }

    async fn save_handoff(&self, handoff: &HandoffSave) -> Result<String> {
        self.save_handoff_db(handoff).await
    }

    async fn save_wiki_node(&self, node: &WikiNode) -> Result<String> {
        self.save_wiki_node_db(node).await
    }

    async fn relate_nodes(
        &self,
        from_id: &str,
        to_id: &str,
        valid_from: Option<chrono::DateTime<chrono::Utc>>,
        valid_to: Option<chrono::DateTime<chrono::Utc>>,
        confidence: Option<f32>,
    ) -> Result<()> {
        self.relate_nodes_db(from_id, to_id, valid_from, valid_to, confidence).await
    }

    async fn relate_followed_by(&self, from_id: &str, to_id: &str) -> Result<()> {
        self.relate_followed_by_db(from_id, to_id).await
    }

    async fn invalidate_edge(
        &self,
        from_id: &str,
        to_id: &str,
        ended: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<()> {
        self.invalidate_edge_db(from_id, to_id, ended).await
    }

    async fn query_edges_as_of(
        &self,
        node_id: &str,
        as_of: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<String>> {
        self.query_edges_as_of_db(node_id, as_of).await
    }

    async fn get_related_node_ids(&self, from_id: &str) -> Result<Vec<String>> {
        self.get_related_node_ids_db(from_id).await
    }

    async fn get_wiki_node_id_by_vault_path(&self, vault_path: &str) -> Result<Option<String>> {
        self.get_wiki_node_id_by_vault_path_db(vault_path).await
    }

    async fn get_active_scopes(&self) -> Result<Vec<String>> {
        self.get_active_scopes_db().await
    }

    async fn delete_by_vault_path(&self, vault_path: &str) -> Result<()> {
        self.delete_by_vault_path_db(vault_path).await
    }

    async fn save_stm(&self, session_id: &str, key: &str, value: &str) -> Result<()> {
        self.save_stm_db(session_id, key, value).await
    }

    async fn query_symbolic_scored(
        &self,
        node_id: &str,
        relation: Option<&str>,
        max_depth: Option<usize>,
        as_of: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<crate::contracts::SymbolicHit>> {
        self.query_symbolic_scored_db(node_id, relation, max_depth, as_of).await
    }

    async fn query_symbolic(&self, node_id: &str, relation: Option<&str>, max_depth: Option<usize>) -> Result<Vec<String>> {
        self.query_symbolic_db(node_id, relation, max_depth).await
    }

    async fn save_thought_node(&self, thought: &crate::contracts::ThoughtNode) -> Result<String> {
        self.save_thought_node_db(thought).await
    }

    async fn get_stm(&self, session_id: &str, key: Option<&str>) -> Result<std::collections::HashMap<String, String>> {
        self.get_stm_db(session_id, key).await
    }

    async fn clear_stm(&self, session_id: &str) -> Result<()> {
        self.clear_stm_db(session_id).await
    }

    async fn update_handoff_status(&self, id: &str, status: &str) -> Result<()> {
        self.update_handoff_status_db(id, status).await
    }

    async fn delete_stale_handoffs(&self, pruning_days: i64) -> Result<()> {
        self.delete_stale_handoffs_db(pruning_days).await
    }

    async fn prune_stale_memories(&self, vault_root: &std::path::Path) -> Result<()> {
        self.prune_stale_memories_db(vault_root).await
    }

    async fn get_memory_nodes(&self, node_ids: &[String]) -> Result<GetMemoryNodesResponse> {
        self.get_memory_nodes_db(node_ids).await
    }

    async fn save_forged_section(&self, batch: &ForgedSectionBatch) -> Result<()> {
        self.save_forged_section_db(batch).await
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        if let Some(cached) = crate::embeddings::get_cached_embedding(text) {
            return Ok(cached);
        }

        let sem = self.get_embedding_semaphore().await;
        let _permit = sem.acquire().await.context("Failed to acquire embedding permit")?;
        
        let active = self.active_embeddings.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let mut max = self.max_concurrent_embeddings.load(std::sync::atomic::Ordering::SeqCst);
        while active > max {
            match self.max_concurrent_embeddings.compare_exchange_weak(
                max,
                active,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(actual) => max = actual,
            }
        }

        let embedder = self.embedder.clone();
        let text_str = text.to_string();
        let res = tokio::task::spawn_blocking(move || {
            if let Some(ref emp) = embedder {
                emp.embed(&text_str)
            } else {
                anyhow::bail!("No embedder configured")
            }
        }).await?;

        self.active_embeddings.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        let vec = res?;
        crate::embeddings::cache_embedding(text.to_string(), vec.clone());
        Ok(vec)
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut results = vec![None; texts.len()];
        let mut missing_indices = Vec::new();
        let mut missing_texts = Vec::new();

        for (i, text) in texts.iter().enumerate() {
            if let Some(cached) = crate::embeddings::get_cached_embedding(text) {
                results[i] = Some(cached);
            } else {
                missing_indices.push(i);
                missing_texts.push(text.clone());
            }
        }

        if !missing_texts.is_empty() {
            let sem = self.get_embedding_semaphore().await;
            let _permit = sem.acquire().await.context("Failed to acquire embedding permit")?;
            
            let active = self.active_embeddings.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            let mut max = self.max_concurrent_embeddings.load(std::sync::atomic::Ordering::SeqCst);
            while active > max {
                match self.max_concurrent_embeddings.compare_exchange_weak(
                    max,
                    active,
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                ) {
                    Ok(_) => break,
                    Err(actual) => max = actual,
                }
            }

            let embedder = self.embedder.clone();
            let missing_texts_clone = missing_texts.clone();
            let res = tokio::task::spawn_blocking(move || {
                if let Some(ref emp) = embedder {
                    emp.embed_batch(&missing_texts_clone)
                } else {
                    let is_mock = {
                        #[cfg(any(test, debug_assertions, feature = "test-mock"))]
                        {
                            std::env::var("MYTHRAX_TEST_MOCK").is_ok() || std::env::var("MYTHRAX_MOCK_LLM").is_ok()
                        }
                        #[cfg(not(any(test, debug_assertions, feature = "test-mock")))]
                        {
                            false
                        }
                    };
                    if is_mock {
                        Ok(vec![vec![0.0f32; 768]; missing_texts_clone.len()])
                    } else {
                        Err(anyhow::anyhow!("No embedding model loaded"))
                    }
                }
            }).await?;

            self.active_embeddings.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            
            let embedded = res?;
            for (idx, emb) in missing_indices.into_iter().zip(embedded) {
                crate::embeddings::cache_embedding(texts[idx].clone(), emb.clone());
                results[idx] = Some(emb);
            }
        }
        let mut final_results = Vec::with_capacity(results.len());
        for opt in results {
            match opt {
                Some(vec) => final_results.push(vec),
                None => anyhow::bail!("Embedding batch returned mismatched results or missing embedding"),
            }
        }
        Ok(final_results)
    }

    async fn get_all_wisdom_rules(&self) -> Result<Vec<WisdomRule>> {
        self.get_all_wisdom_rules_db().await
    }

    async fn get_all_wiki_nodes(&self) -> Result<Vec<WikiNode>> {
        self.get_all_wiki_nodes_db().await
    }

    async fn diagnose_error_internal(&self, stderr: &str, stdout: &str) -> Result<Option<(String, String)>> {
        self.diagnose_error_internal_db(stderr, stdout).await
    }

    async fn journal_state(&self, vault_root: &std::path::Path, session_id: Option<&str>) -> Result<()> {
        self.journal_state_db(vault_root, session_id).await
    }

    async fn reinforce_episode(&self, id: &str) -> Result<()> {
        self.reinforce_episode_db(id).await
    }

    async fn get_checkpoints(&self) -> Result<Vec<serde_json::Value>> {
        let mut response = self.db.query("SELECT * FROM checkpoint_node ORDER BY timestamp DESC;").await?;
        let records: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
        Ok(records)
    }

    async fn get_indexing_write_count(&self, vault_path: &str) -> Result<usize> {
        let writes = self.indexing_writes.lock().await;
        if let Some(&count) = writes.get(vault_path) {
            Ok(count)
        } else {
            if let Some(filename) = std::path::Path::new(vault_path).file_name().and_then(|s| s.to_str()) {
                Ok(*writes.get(filename).unwrap_or(&0))
            } else {
                Ok(0)
            }
        }
    }

    async fn get_max_concurrent_background_embeddings(&self) -> Result<usize> {
        Ok(self.max_concurrent_embeddings.load(std::sync::atomic::Ordering::SeqCst))
    }
}


pub(crate) fn load_api_key(provider: &str) -> Option<String> {
    if let Ok(home) = std::env::var("HOME") {
        let keys_path = std::path::PathBuf::from(&home).join(".mythrax/keys.json");
        if keys_path.exists()
            && let Ok(content) = std::fs::read_to_string(&keys_path)
                && let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&content)
                    && let Some(val) = map.get(provider)
                        && let Some(s) = val.as_str() {
                            return Some(s.to_string());
                        }
    }
    None
}

pub(crate) fn save_api_key(provider: &str, key: &str) -> Result<()> {
    if let Ok(home) = std::env::var("HOME") {
        let mythrax_dir = std::path::PathBuf::from(&home).join(".mythrax");
        std::fs::create_dir_all(&mythrax_dir)?;
        let keys_path = mythrax_dir.join("keys.json");
        let mut map = if keys_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&keys_path) {
                serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&content).unwrap_or_default()
            } else {
                serde_json::Map::new()
            }
        } else {
            serde_json::Map::new()
        };
        map.insert(provider.to_string(), serde_json::Value::String(key.to_string()));
        let content = serde_json::to_string_pretty(&map)?;
        std::fs::write(&keys_path, &content)?;
        
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&keys_path, std::fs::Permissions::from_mode(0o600));
        }
    }
    Ok(())
}


#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TemporalCueType {
    Preceding,
    Succeeding,
    Relative,
    Procedural,
}

pub fn parse_temporal_cues(query: &str) -> Option<(TemporalCueType, f32)> {
    let query_lower = query.to_lowercase();
    
    // 1. Procedural Check (e.g. "what did we do...")
    let mut words = query_lower.split_whitespace().map(|w| w.trim_matches(|c| c == '?' || c == '.' || c == ','));
    if let Some(first_word) = words.next() {
        let question_words = ["what", "how", "why", "which", "where", "when", "did", "have"];
        if question_words.contains(&first_word) {
            let action_words = ["do", "done", "took", "take", "taken", "happen", "run", "execute", "call", "step", "try", "attempt"];
            if words.any(|w| action_words.contains(&w)) {
                return Some((TemporalCueType::Procedural, 3.0));
            }
        }
    }

    let words_vec: Vec<String> = query_lower.split_whitespace()
        .map(|w| w.chars().filter(|c| c.is_alphanumeric()).collect())
        .collect();
    
    let deep_modifiers = ["long", "far", "much", "way"];
    let preceding = ["before", "preceding", "previously", "prior", "earlier", "ago", "last"];
    let succeeding = ["after", "following", "subsequently", "later", "next"];
    let relative = ["recent", "recently", "latest", "newest", "today", "now"];

    // 2. Deep Preceding / Succeeding Checks (window of 2)
    for window in words_vec.windows(2) {
        if deep_modifiers.contains(&window[0].as_str()) {
            if preceding.contains(&window[1].as_str()) {
                return Some((TemporalCueType::Preceding, 3.0));
            }
            if succeeding.contains(&window[1].as_str()) {
                return Some((TemporalCueType::Succeeding, 3.0));
            }
        }
    }

    // 3. Single Word Checks
    for word in &words_vec {
        if preceding.contains(&word.as_str()) {
            return Some((TemporalCueType::Preceding, 1.0));
        }
        if succeeding.contains(&word.as_str()) {
            return Some((TemporalCueType::Succeeding, 1.0));
        }
        if relative.contains(&word.as_str()) {
            return Some((TemporalCueType::Relative, 1.0));
        }
    }

    None
}

pub fn calculate_temporal_decay(
    created_at: chrono::DateTime<chrono::Utc>,
    anchor: chrono::DateTime<chrono::Utc>,
    lambda: f32,
) -> f32 {
    let delta_t_secs = (anchor.timestamp() - created_at.timestamp()) as f64;
    let delta_t_days = delta_t_secs.max(0.0) / 86400.0;
    let clamped_lambda = lambda.max(0.0) as f64;
    
    ((-clamped_lambda * delta_t_days).exp()) as f32
}

pub fn sentence_cosine_similarity_opt(
    query_tokens: &[String],
    query_tokens_set: &std::collections::HashSet<&str>,
    global_idf: &std::collections::HashMap<String, f32>,
    norm_query_sqrt: f64,
    sentence_trimmed: &str,
) -> f32 {
    let mut sentence_freq: std::collections::HashMap<String, f32> = std::collections::HashMap::with_capacity(32);
    for word in sentence_trimmed.split(|c: char| !c.is_alphanumeric() && c != '-') {
        let trimmed = word.trim();
        if trimmed.is_empty() || crate::retrieval::bm25::is_stop_word(trimmed) {
            continue;
        }
        let stemmed = crate::retrieval::bm25::stem(trimmed);
        *sentence_freq.entry(stemmed).or_insert(0.0) += 1.0;
    }

    if sentence_freq.is_empty() {
        return 0.0;
    }

    let mut dot_product: f64 = 0.0;
    let mut norm_sentence: f64 = 0.0;

    for t in query_tokens {
        let idf = global_idf.get(t).copied().unwrap_or(0.0) as f64;
        let tf = *sentence_freq.get(t).unwrap_or(&0.0) as f64;
        dot_product += tf * (idf * idf);
        
        let sentence_component = tf * idf;
        norm_sentence += sentence_component * sentence_component;
    }

    for (w, &tf) in &sentence_freq {
        if !query_tokens_set.contains(w.as_str()) {
            let tf_f64 = tf as f64;
            norm_sentence += tf_f64 * tf_f64;
        }
    }

    let norm_sentence_sqrt = norm_sentence.sqrt();
    if norm_sentence_sqrt < 1e-9 {
        return 0.0;
    }

    (dot_product / (norm_query_sqrt * norm_sentence_sqrt)) as f32
}

pub fn sentence_cosine_similarity(
    query_tokens: &[String],
    sentence: &str,
    global_idf: &std::collections::HashMap<String, f32>,
) -> f32 {
    let mut norm_query: f64 = 0.0;
    for t in query_tokens {
        let idf = global_idf.get(t).copied().unwrap_or(0.0) as f64;
        norm_query += idf * idf;
    }
    let norm_query_sqrt = norm_query.sqrt();
    if norm_query_sqrt < 1e-9 {
        return 0.0;
    }
    let query_tokens_set: std::collections::HashSet<&str> = query_tokens.iter().map(|s| s.as_str()).collect();
    let sentence_trimmed = sentence.trim();
    sentence_cosine_similarity_opt(query_tokens, &query_tokens_set, global_idf, norm_query_sqrt, sentence_trimmed)
}

pub(crate) fn get_user_prefix(session_id: &str) -> &str {
    if session_id.starts_with("answer_") {
        let mut s = session_id;
        if let Some(last_underscore_idx) = s.rfind('_') {
            let suffix = &s[last_underscore_idx + 1..];
            if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) && suffix.len() <= 3 {
                s = &s[..last_underscore_idx];
            }
        }
        s
    } else {
        if let Some(last_underscore_idx) = session_id.rfind('_') {
            let suffix = &session_id[last_underscore_idx + 1..];
            if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) && suffix.len() <= 3 {
                &session_id[..last_underscore_idx]
            } else {
                session_id
            }
        } else {
            session_id
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Entity;

    #[test]
    fn test_get_user_prefix() {
        assert_eq!(get_user_prefix("user_123"), "user");
        assert_eq!(get_user_prefix("user_123_456"), "user_123");
        assert_eq!(get_user_prefix("answer_user_123"), "answer_user");
        assert_eq!(get_user_prefix("answer_user_123_456"), "answer_user_123");
        assert_eq!(get_user_prefix("session"), "session");
        assert_eq!(get_user_prefix("answer_sess"), "answer_sess");
    }

    #[test]
    fn test_unescape_id_part() {
        assert_eq!(unescape_id_part("123"), "123");
        assert_eq!(unescape_id_part("⟨123⟩"), "123");
        assert_eq!(unescape_id_part("⟨⟨123⟩⟩"), "123");
        assert_eq!(unescape_id_part("⟨⟨abc\\-def⟩⟩"), "abc-def");
        assert_eq!(unescape_id_part("abc\\\\def"), "abc\\def");
    }

    #[tokio::test]
    async fn test_classify_query_comprehensive() {
        let backend = SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        assert_eq!(backend.classify_query_db("my next mtg").await, QueryCategory::Temporal);
        assert_eq!(backend.classify_query_db("our appts next week").await, QueryCategory::Temporal);
        assert_eq!(backend.classify_query_db("show next meeting").await, QueryCategory::Temporal);
        assert_eq!(backend.classify_query_db("my favourite lodging").await, QueryCategory::Preference);
        assert_eq!(backend.classify_query_db("my profile").await, QueryCategory::User);
        assert_eq!(backend.classify_query_db("our job description").await, QueryCategory::User);
        assert_eq!(backend.classify_query_db("who am i?").await, QueryCategory::User);
        assert_eq!(backend.classify_query_db("tell me about me").await, QueryCategory::User);
        assert_eq!(backend.classify_query_db("about our friend").await, QueryCategory::User);
        assert_eq!(backend.classify_query_db("what is job salary").await, QueryCategory::User);
    }

    #[tokio::test]
    async fn test_surreal_db_operations() {
        let backend = SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        let episode = EpisodeSave {
        created_at: None,
            title: "Test caching failure".to_string(),
            content: "Observed cache mismatch in redis client.".to_string(),
            entities: vec![Entity {
                name: "RedisClient".to_string(),
                entity_type: "class".to_string(),
                summary: "redis connection pool".to_string(),
                labels: vec!["caching".to_string()],
                scope: None,
                vault_path: None,
                embedding: None,
            }],
            scope: Some("testing".to_string()),
            vault_path: Some("test_path.md".to_string()),
            source_episode: None,
            session_id: None,
            task_id: None,
            discovery_tokens: None,
            facts: None,
            concepts: None,
            files_read: None,
            files_modified: None,
            node_type: None,
            confidence: None,
        };

        let ep_id = backend.save_episode(&episode).await.unwrap();
        assert!(ep_id.contains("episode"));

        // Call again to test the is_update update path
        let ep_id2 = backend.save_episode(&episode).await.unwrap();
        assert_eq!(ep_id, ep_id2);

        let all_eps: Vec<serde_json::Value> = backend.db.select("episode").await.unwrap();
        println!("DEBUG: All episodes in DB: {:?}", all_eps);

        let search_results = backend.search(crate::contracts::SearchParams::from_positional(
        "redis",
        Some("testing"),
        false,
        2,
        0,
        0.55,
        None,
        false,
        true,
        true,
        None,
        true,
        None,
    )).await.unwrap();
        assert_eq!(search_results.results.len(), 1);
        assert!(search_results.results[0].content.contains("redis"));

        backend.record_feedback(&ep_id, false).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_save_episodes_batch() {
        let backend = SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        let episodes = vec![
            EpisodeSave {
        created_at: None,
                title: "Batch episode 1".to_string(),
                content: "First batch item content for testing.".to_string(),
                scope: Some("batch-test".to_string()),
                vault_path: Some("batch_1.md".to_string()),
                session_id: Some("session_123".to_string()),
                ..Default::default()
            },
            EpisodeSave {
        created_at: None,
                title: "Batch episode 2".to_string(),
                content: "Second batch item content for testing.".to_string(),
                scope: Some("batch-test".to_string()),
                vault_path: Some("batch_2.md".to_string()),
                session_id: Some("session_123".to_string()),
                ..Default::default()
            },
        ];

        backend.save_episodes_batch(&episodes).await.unwrap();

        // Query to check they were inserted correctly
        let all_eps: Vec<serde_json::Value> = backend.db.select("episode").await.unwrap();
        assert_eq!(all_eps.len(), 2);

        // Verify the fields on the inserted episodes
        let ep1 = all_eps.iter().find(|e| e.get("title").and_then(|v| v.as_str()) == Some("Batch episode 1")).unwrap();
        assert_eq!(ep1.get("scope").and_then(|v| v.as_str()), Some("batch-test"));
        assert_eq!(ep1.get("vault_path").and_then(|v| v.as_str()), Some("batch_1.md"));
        assert_eq!(ep1.get("session_id").and_then(|v| v.as_str()), Some("session_123"));

        // Verify that metrics were inserted
        let all_metrics: Vec<serde_json::Value> = backend.db.select("metrics").await.unwrap();
        assert_eq!(all_metrics.len(), 2);
    }

    #[tokio::test]
    async fn test_deep_insight_graph_traversal() {
        let backend = SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        let episode = EpisodeSave {
        created_at: None,
            title: "Redis pool connection failure".to_string(),
            content: "Redis clients are dropping connections under load.".to_string(),
            entities: vec![],
            scope: Some("deep-test".to_string()),
            vault_path: None,
            source_episode: None,
            session_id: None,
            task_id: None,
            discovery_tokens: None,
            facts: None,
            concepts: None,
            files_read: None,
            files_modified: None,
            node_type: None,
            confidence: None,
        };

        let ep_id = backend.save_episode(&episode).await.unwrap();
        let ep_thing = parse_record_id(&ep_id).unwrap();

        // Create a wiki_node
        let _ = backend.db.query("
            CREATE type::record('wiki_node', 'pool_size') CONTENT {
                name: 'Redis Connection Pooling Guidelines',
                content: 'Set max connections to 50 under high concurrency environments.',
                scope: 'deep-test'
            };
        ").await.unwrap().check().unwrap();

        let wiki_thing = surrealdb::types::RecordId {
            table: "wiki_node".into(),
            key: surrealdb::types::RecordIdKey::from("pool_size".to_string()),
        };

        // Relate the episode to the wiki_node
        let _ = backend.db.query("RELATE $from -> relates_to -> $to;")
            .bind(("from", ep_thing.clone()))
            .bind(("to", wiki_thing.clone()))
            .await.unwrap()
            .check().unwrap();

        // Perform search WITH deep_insight = true
        let results_deep = backend.search(crate::contracts::SearchParams::from_positional(
        "Redis",
        Some("deep-test"),
        true,
        10,
        0,
        0.55,
        None,
        true,
        true,
        true,
        None,
        true,
        None,
    )).await.unwrap();
        assert_eq!(results_deep.results.len(), 1);
        assert!(results_deep.results[0].content.contains("dropping connections"));
        assert!(results_deep.results[0].content.contains("Redis Connection Pooling Guidelines"));
        assert!(results_deep.results[0].content.contains("Set max connections to 50"));

        // Perform search WITHOUT deep_insight = true
        let results_normal = backend.search(crate::contracts::SearchParams::from_positional(
        "failure",
        Some("deep-test"),
        false,
        10,
        0,
        0.55,
        None,
        false,
        true,
        true,
        None,
        true,
        None,
    )).await.unwrap();
        assert_eq!(results_normal.results.len(), 1);
        assert!(results_normal.results[0].content.contains("dropping connections"));
        assert!(!results_normal.results[0].content.contains("Redis Connection Pooling Guidelines"));
    }

    #[tokio::test]
    async fn test_get_memory_nodes() {
        let backend = SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        // 1. Seed an episode
        let episode = EpisodeSave {
        created_at: None,
            title: "Hydration Episode".to_string(),
            content: "Testing node hydration capabilities.".to_string(),
            entities: vec![],
            scope: Some("hydration-test".to_string()),
            vault_path: None,
            source_episode: None,
            session_id: None,
            task_id: None,
            discovery_tokens: None,
            facts: None,
            concepts: None,
            files_read: None,
            files_modified: None,
            node_type: None,
            confidence: None,
        };
        let ep_id = backend.save_episode(&episode).await.unwrap();

        // 2. Seed a wisdom rule
        let rule = WisdomRule {
            id: None,
            target_pattern: "hydration-pattern".to_string(),
            action_to_avoid: "avoiding hydration".to_string(),
            causal_explanation: "leads to dry tests".to_string(),
            prescribed_remedy: "hydrate it".to_string(),
            tier: "dynamic".to_string(),
            scope: "hydration-test".to_string(),
            vault_path: None,
            embedding: None,
            source_episodes: vec![],
            generator_name: "test".to_string(),
            similarity: None,
            utility: None,
            status: None,
            superseded_at: None,
            superseded_by: None,
            rule_type: None,
        };
        let rule_id = backend.save_wisdom_rule(&rule).await.unwrap();

        // 3. Seed a wiki node
        let node = WikiNode {
            id: None,
            name: "Hydration Guide".to_string(),
            content: "Pour water on tests.".to_string(),
            scope: "hydration-test".to_string(),
            vault_path: None,
            embedding: None,
        };
        let wiki_id = backend.save_wiki_node(&node).await.unwrap();

        // Query get_memory_nodes
        let response = backend.get_memory_nodes(&[ep_id.clone(), rule_id.clone(), wiki_id.clone()]).await.unwrap();
        
        assert_eq!(response.episodes.len(), 1);
        assert_eq!(response.episodes[0].title, "Hydration Episode");
        
        assert_eq!(response.wisdom_rules.len(), 1);
        assert_eq!(response.wisdom_rules[0].target_pattern, "hydration-pattern");

        assert_eq!(response.wiki_nodes.len(), 1);
        assert_eq!(response.wiki_nodes[0].name, "Hydration Guide");
    }

    #[tokio::test]
    async fn test_multi_tier_search_ranking() {
        let mut backend = SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();
        backend.embedder = None;
        backend.save_profile_key("retrieval.boost.person_name", "false").await.unwrap();
        backend.save_profile_key("retrieval.boost.exact_quote", "false").await.unwrap();
        backend.save_profile_key("retrieval.boost.temporal_proximity", "false").await.unwrap();
        backend.save_profile_key("retrieval.boost.keyword_overlap", "false").await.unwrap();
        backend.save_profile_key("retrieval.hybrid", "false").await.unwrap();

        // 1. Seed an episode containing 'concurrency'
        let episode = EpisodeSave {
        created_at: None,
            title: "Concurrency Episode".to_string(),
            content: "Concurrency is hard.".to_string(),
            entities: vec![],
            scope: Some("ranking-test".to_string()),
            vault_path: None,
            source_episode: None,
            session_id: None,
            task_id: None,
            node_type: None,
            ..Default::default()
        };
        let _ = backend.save_episode(&episode).await.unwrap();

        // 2. Seed a wisdom rule containing 'concurrency'
        let rule = WisdomRule {
            id: None,
            target_pattern: "Concurrency pattern".to_string(),
            action_to_avoid: "avoiding concurrency".to_string(),
            causal_explanation: "causes slow code".to_string(),
            prescribed_remedy: "use concurrency safely".to_string(),
            tier: "skills".to_string(), // Skills tier boost = 1.2
            scope: "ranking-test".to_string(),
            vault_path: None,
            embedding: None,
            source_episodes: vec![],
            generator_name: "test".to_string(),
            similarity: None,
            utility: None,
            status: None,
            superseded_at: None,
            superseded_by: None,
            rule_type: None,
        };
        let _ = backend.save_wisdom_rule(&rule).await.unwrap();

        // 3. Seed a wiki node containing 'concurrency'
        let node = WikiNode {
            id: None,
            name: "Concurrency Guide".to_string(),
            content: "Concurrency best practices.".to_string(),
            scope: "ranking-test".to_string(),
            vault_path: None,
            embedding: None,
        };
        let _ = backend.save_wiki_node(&node).await.unwrap();

        // Execute text search (query_emb will be None, similarity defaults to 1.0)
        let response = backend.search(crate::contracts::SearchParams::from_positional(
        "Concurrency",
        Some("ranking-test"),
        false,
        10,
        0,
        0.0,
        None,
        false,
        true,
        true,
        None,
        true,
        None,
    )).await.unwrap();

        println!("DEBUG RESULTS: {:?}", response.results);
        assert_eq!(response.results.len(), 3);
        
        // Assert sorting order based on tier boosts: skills (1.2) > wiki/insight (1.1) > episode (1.0)
        assert_eq!(response.results[0].tier, "skills");
        assert_eq!(response.results[0].title, "Concurrency pattern");

        assert_eq!(response.results[1].tier, "insight");
        assert_eq!(response.results[1].title, "Concurrency Guide");

        assert_eq!(response.results[2].tier, "episode");
        assert_eq!(response.results[2].title, "Concurrency Episode");
    }

    #[tokio::test]
    async fn test_graph_linked_handoffs() {
        let backend = SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        // 1. Seed distilled context nodes in STM
        let parent_id = "conv_parent";
        let subagent_id = "conv_subagent";
        
        let ep_id = backend.save_episode(&EpisodeSave {
        created_at: None,
            title: "Context Episode".to_string(),
            content: "Some context info".to_string(),
            entities: vec![],
            scope: Some("testing".to_string()),
            vault_path: None,
            source_episode: None,
            session_id: None,
            task_id: None,
            discovery_tokens: None,
            facts: None,
            concepts: None,
            files_read: None,
            files_modified: None,
            node_type: None,
            confidence: None,
        }).await.unwrap();

        backend.save_stm(parent_id, "distilled_context_nodes", &format!("[\"{}\"]", ep_id)).await.unwrap();

        // 2. Save handoff
        let handoff = HandoffSave {
            parent_conversation_id: parent_id.to_string(),
            subagent_conversation_id: subagent_id.to_string(),
            summary: "Testing handoff".to_string(),
            handoff_file_path: "some/path.md".to_string(),
            scope: Some("testing".to_string()),
            include_tool_execution: None,
        };
        let handoff_id = backend.save_handoff(&handoff).await.unwrap();

        // 3. Verify relationships
        let sql = "SELECT VALUE out FROM relates_to WHERE in = $handoff_id;";
        let mut response = backend.db.query(sql).bind(("handoff_id", parse_record_id(&handoff_id).unwrap())).await.unwrap();
        let related_outs: Vec<surrealdb::types::RecordId> = response.take(0).unwrap();
        assert_eq!(related_outs.len(), 1);
        assert_eq!(format_record_id(&related_outs[0]), ep_id);
    }

    #[tokio::test]
    async fn test_directional_graph_traversal() {
        let backend = SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        // 1. Create Episode and WikiNode (Insight)
        let ep_id = backend.save_episode(&EpisodeSave {
        created_at: None,
            title: "Child Episode".to_string(),
            content: "Contains info".to_string(),
            entities: vec![],
            scope: Some("directional-test".to_string()),
            vault_path: None,
            source_episode: None,
            session_id: None,
            task_id: None,
            node_type: None,
            ..Default::default()
        }).await.unwrap();

        let node = WikiNode {
            id: None,
            name: "Parent Insight".to_string(),
            content: "Insight summary".to_string(),
            scope: "directional-test".to_string(),
            vault_path: Some("wiki/insights/parent_insight.md".to_string()),
            embedding: None,
        };
        let node_id = backend.save_wiki_node(&node).await.unwrap();

        // Relate Episode -> relates_to -> WikiNode (Upward)
        backend.relate_nodes(&ep_id, &node_id, None, None, None).await.unwrap();

        let results_upward = backend.search(crate::contracts::SearchParams::from_positional(
        "Parent Insight",
        Some("directional-test"),
        true,
        10,
        0,
        0.85,
        None,
        false,
        true,
        true,
        None,
        true,
        None,
    )).await.unwrap();
        println!("DEBUG: results_upward: {:#?}", results_upward.results);
        assert_eq!(results_upward.results.len(), 1);
        let content_upward = &results_upward.results[0].content;
        assert!(!content_upward.contains("Child Episode"));

        // 3. Search with allow_downward = true
        let results_downward = backend.search(crate::contracts::SearchParams::from_positional(
        "Parent Insight",
        Some("directional-test"),
        true,
        10,
        0,
        0.85,
        None,
        true,
        true,
        true,
        None,
        true,
        None,
    )).await.unwrap();
        assert_eq!(results_downward.results.len(), 1);
        let content_downward = &results_downward.results[0].content;
        assert!(content_downward.contains("Child Episode"));
    }

    #[tokio::test]
    async fn test_token_budget_truncation() {
        let backend = SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        // 1. Create a Skill Rule (Priority 0)
        let skill_rule = WisdomRule {
            id: None,
            target_pattern: "Skill Pattern".to_string(),
            action_to_avoid: "Avoid this".to_string(),
            causal_explanation: "Cause".to_string(),
            prescribed_remedy: "Remedy".to_string(),
            tier: "skills".to_string(),
            scope: "budget-test".to_string(),
            vault_path: None,
            embedding: None,
            source_episodes: vec![],
            generator_name: "test".to_string(),
            similarity: None,
            utility: None,
            status: None,
            superseded_at: None,
            superseded_by: None,
            rule_type: None,
        };
        backend.save_wisdom_rule(&skill_rule).await.unwrap();

        // 2. Create an Episode (Priority 4)
        let ep = EpisodeSave {
        created_at: None,
            title: "Episode Title with Pattern".to_string(),
            content: "Episode body content".to_string(),
            entities: vec![],
            scope: Some("budget-test".to_string()),
            vault_path: None,
            source_episode: None,
            session_id: None,
            task_id: None,
            discovery_tokens: None,
            facts: None,
            concepts: None,
            files_read: None,
            files_modified: None,
            node_type: None,
            confidence: None,
        };
        backend.save_episode(&ep).await.unwrap();

        // 3. Search with a tight token budget
        let response = backend.search(crate::contracts::SearchParams::from_positional(
        "Pattern",
        Some("budget-test"),
        true,
        10,
        0,
        0.0,
        Some(20),
        false,
        true,
        true,
        None,
        true,
        None,
    )).await.unwrap();
        
        // Skill rule is kept, Episode is omitted
        assert_eq!(response.results.len(), 1);
        assert_eq!(response.results[0].tier, "skills");
        
        // Check omitted_ids
        assert!(response.omitted_ids.is_some());
        let omitted = response.omitted_ids.unwrap();
        assert_eq!(omitted.len(), 1);
        assert!(omitted[0].starts_with("episode:"));
    }

    #[tokio::test]
    async fn test_get_all_wiki_nodes() {
        let backend = SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        let node = WikiNode {
            id: None,
            name: "Test Node".to_string(),
            content: "Test Content".to_string(),
            scope: "test-scope".to_string(),
            vault_path: Some("wiki/test.md".to_string()),
            embedding: None,
        };

        let node_id = backend.save_wiki_node(&node).await.unwrap();
        assert!(node_id.starts_with("wiki_node:"));

        let all_nodes = backend.get_all_wiki_nodes().await.unwrap();
        assert_eq!(all_nodes.len(), 1);
        assert_eq!(all_nodes[0].name, "Test Node");
        assert_eq!(all_nodes[0].content, "Test Content");
        assert_eq!(all_nodes[0].scope, "test-scope");
    }

    #[tokio::test]
    async fn test_search_wisdom_compaction() {
        let backend = SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        let skill_rule = WisdomRule {
            id: None,
            target_pattern: "Avoid manual steps".to_string(),
            action_to_avoid: "doing things manually".to_string(),
            causal_explanation: "This is a very long explanation explaining why doing things manually is bad, error-prone, slow, and non-deterministic".to_string(),
            prescribed_remedy: "automate all steps".to_string(),
            tier: "skills".to_string(),
            scope: "compaction-test".to_string(),
            vault_path: None,
            embedding: None,
            source_episodes: vec![],
            generator_name: "test".to_string(),
            similarity: None,
            utility: None,
            status: None,
            superseded_at: None,
            superseded_by: None,
            rule_type: None,
        };
        backend.save_wisdom_rule(&skill_rule).await.unwrap();

        // Search with large budget - full content should include the "Why" explanation
        let res_large = backend.search(crate::contracts::SearchParams::from_positional(
        "Avoid",
        Some("compaction-test"),
        false,
        10,
        0,
        0.0,
        Some(1000),
        false,
        true,
        true,
        None,
        true,
        None,
    )).await.unwrap();
        assert_eq!(res_large.results.len(), 1);
        assert!(res_large.results[0].content.contains("**Why**:"));

        // Dynamically compute the budget needed for compacted content
        let text_compacted = format!("{}\n**Action to Avoid**: {}\n**Prescribed Remedy**: {}", 
            skill_rule.target_pattern, skill_rule.action_to_avoid, skill_rule.prescribed_remedy);
        let tokens_compacted = backend.count_text_tokens(&text_compacted);

        // Search with tight budget - should strip "**Why**:" and fit under budget
        let res_small = backend.search(crate::contracts::SearchParams::from_positional(
        "Avoid",
        Some("compaction-test"),
        false,
        10,
        0,
        0.0,
        Some(tokens_compacted + 5),
        false,
        true,
        true,
        None,
        true,
        None,
    )).await.unwrap();
        assert_eq!(res_small.results.len(), 1);
        assert!(!res_small.results[0].content.contains("**Why**:"));
        assert!(res_small.results[0].content.contains("**Action to Avoid**:"));
        assert!(res_small.results[0].content.contains("**Prescribed Remedy**:"));
    }

    #[tokio::test]
    async fn test_search_paragraph_and_truncation_compaction() {
        let backend = SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        // Save a multi-paragraph WikiNode
        let node1 = WikiNode {
            id: None,
            name: "Multi-Paragraph Node".to_string(),
            content: "First paragraph here.\n\nSecond paragraph contains a lot of additional details. It goes on and on to explain everything about performance, cognitive capabilities, memory hierarchies, and database pruning in the mythrax daemon system. We want this paragraph to be extremely long so that its token count is significantly higher than the first paragraph and the truncation suffix combined, making compaction highly effective and necessary to fit within small budgets.".to_string(),
            scope: "compaction-test".to_string(),
            vault_path: None,
            embedding: None,
        };
        backend.save_wiki_node(&node1).await.unwrap();

        // Search with large budget - full content
        let res_large = backend.search(crate::contracts::SearchParams::from_positional(
        "Multi-Paragraph",
        Some("compaction-test"),
        false,
        10,
        0,
        0.0,
        Some(1000),
        false,
        true,
        true,
        None,
        true,
        None,
    )).await.unwrap();
        assert_eq!(res_large.results.len(), 1);
        assert!(res_large.results[0].content.contains("Second paragraph"));

        // Dynamically compute the budget needed for compacted content
        let compacted_content = format!("First paragraph here.\n\n... [Truncated (Inner-Node Compaction)]");
        let text_compacted = format!("{}\n{}", node1.name, compacted_content);
        let tokens_compacted = backend.count_text_tokens(&text_compacted);

        // Search with small budget -> first paragraph + suffix
        let res_small = backend.search(crate::contracts::SearchParams::from_positional(
        "Multi-Paragraph",
        Some("compaction-test"),
        false,
        10,
        0,
        0.0,
        Some(tokens_compacted + 5),
        false,
        true,
        true,
        None,
        true,
        None,
    )).await.unwrap();
        assert_eq!(res_small.results.len(), 1);
        assert!(res_small.results[0].content.contains("First paragraph here."));
        assert!(!res_small.results[0].content.contains("Second paragraph"));
        assert!(res_small.results[0].content.contains("... [Truncated (Inner-Node Compaction)]"));

        // Save a single long paragraph WikiNode
        let node2 = WikiNode {
            id: None,
            name: "Single-Paragraph Node".to_string(),
            content: "This is a single very long paragraph without any newlines whatsoever. We want to test that the binary search character truncation fallback mechanism works perfectly when the budget is constrained. Therefore, we write a very long description with many words and spaces so that the token count of the full text is significantly larger than the budget and the truncation suffix combined.".to_string(),
            scope: "compaction-test-single-para".to_string(),
            vault_path: None,
            embedding: None,
        };
        backend.save_wiki_node(&node2).await.unwrap();

        // Dynamically compute the budget for a truncated version (e.g. half the content length)
        let half_len = node2.content.len() / 2;
        let truncated_content = format!("{}... [Truncated (Inner-Node Compaction)]", &node2.content[..half_len]);
        let text_truncated = format!("{}\n{}", node2.name, truncated_content);
        let tokens_truncated = backend.count_text_tokens(&text_truncated);

        // Search with tight budget -> character-truncated
        let res_trunc = backend.search(crate::contracts::SearchParams::from_positional(
        "Single-Paragraph",
        Some("compaction-test-single-para"),
        false,
        10,
        0,
        0.0,
        Some(tokens_truncated + 5),
        false,
        true,
        true,
        None,
        true,
        None,
    )).await.unwrap();
        assert_eq!(res_trunc.results.len(), 1);
        assert!(res_trunc.results[0].content.contains("... [Truncated (Inner-Node Compaction)]"));
        assert!(res_trunc.results[0].content.len() < node2.content.len());
    }

    #[tokio::test]
    async fn test_search_excludes_episodes_by_default() {
        let backend = SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        // Save an Episode
        let ep = EpisodeSave {
        created_at: None,
            title: "Test Episode".to_string(),
            content: "Secret content in the episode".to_string(),
            entities: vec![],
            scope: Some("exclusion-test".to_string()),
            vault_path: None,
            source_episode: None,
            session_id: None,
            task_id: None,
            node_type: None,
            ..Default::default()
        };
        backend.save_episode(&ep).await.unwrap();

        // Search with include_episodes = false (default behavior) -> should NOT find the episode
        let res_default = backend.search(crate::contracts::SearchParams::from_positional(
        "Secret",
        Some("exclusion-test"),
        false,
        10,
        0,
        0.0,
        None,
        false,
        false,
        true,
        None,
        true,
        None,
    )).await.unwrap();
        assert_eq!(res_default.results.len(), 0);

        // Search with include_episodes = true -> should find the episode
        let res_include = backend.search(crate::contracts::SearchParams::from_positional(
        "Secret",
        Some("exclusion-test"),
        false,
        10,
        0,
        0.0,
        None,
        false,
        true,
        true,
        None,
        true,
        None,
    )).await.unwrap();
        assert_eq!(res_include.results.len(), 1);
        assert_eq!(res_include.results[0].title, "Test Episode");
    }

    #[tokio::test]
    async fn test_graph_traversal_excludes_episodes() {
        let backend = SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        // Create an Episode and a WikiNode
        let ep_id = backend.save_episode(&EpisodeSave {
        created_at: None,
            title: "Child Episode".to_string(),
            content: "Contains info".to_string(),
            entities: vec![],
            scope: Some("graph-exclusion-test".to_string()),
            vault_path: None,
            source_episode: None,
            session_id: None,
            task_id: None,
            discovery_tokens: None,
            facts: None,
            concepts: None,
            files_read: None,
            files_modified: None,
            node_type: None,
            confidence: None,
        }).await.unwrap();

        let node = WikiNode {
            id: None,
            name: "Parent Insight".to_string(),
            content: "Insight summary".to_string(),
            scope: "graph-exclusion-test".to_string(),
            vault_path: None,
            embedding: None,
        };
        let node_id = backend.save_wiki_node(&node).await.unwrap();

        // Relate Episode -> relates_to -> WikiNode (Upward)
        backend.relate_nodes(&ep_id, &node_id, None, None, None).await.unwrap();

        // Search with deep_insight = true, allow_downward = true and include_episodes = true -> child episode should be traversed and included
        let res_include = backend.search(crate::contracts::SearchParams::from_positional(
        "Parent Insight",
        Some("graph-exclusion-test"),
        true,
        10,
        0,
        0.85,
        None,
        true,
        true,
        true,
        None,
        true,
        None,
    )).await.unwrap();
        assert_eq!(res_include.results.len(), 1);
        assert!(res_include.results[0].content.contains("Child Episode"));

        // Search with deep_insight = true, allow_downward = true and include_episodes = false -> child episode should NOT be traversed
        let res_exclude = backend.search(crate::contracts::SearchParams::from_positional(
        "Parent Insight",
        Some("graph-exclusion-test"),
        true,
        10,
        0,
        0.85,
        None,
        true,
        false,
        true,
        None,
        true,
        None,
    )).await.unwrap();
        assert_eq!(res_exclude.results.len(), 1);
        assert!(!res_exclude.results[0].content.contains("Child Episode"));
    }

    #[tokio::test]
    async fn test_search_excludes_artifacts_by_default() {
        let backend = SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        // 1. Create a WikiNode representing an artifact
        let artifact_node = WikiNode {
            id: None,
            name: "Artifact Node".to_string(),
            content: "Contains some secret data".to_string(),
            scope: "artifact-exclusion-test".to_string(),
            vault_path: Some("wiki/artifacts/test_artifact.md".to_string()),
            embedding: None,
        };
        backend.save_wiki_node(&artifact_node).await.unwrap();

        // 2. Create a normal WikiNode
        let normal_node = WikiNode {
            id: None,
            name: "Normal Node".to_string(),
            content: "Contains some secret data".to_string(),
            scope: "artifact-exclusion-test".to_string(),
            vault_path: Some("wiki/scope/insights/my_insight.md".to_string()),
            embedding: None,
        };
        backend.save_wiki_node(&normal_node).await.unwrap();

        // 3. Search with include_artifacts = false -> should find only Normal Node
        let res_default = backend.search(crate::contracts::SearchParams::from_positional(
        "secret",
        Some("artifact-exclusion-test"),
        false,
        10,
        0,
        0.0,
        None,
        false,
        true,
        false,
        None,
        true,
        None,
    )).await.unwrap();
        assert_eq!(res_default.results.len(), 1);
        assert_eq!(res_default.results[0].title, "Normal Node");

        // 4. Search with include_artifacts = true -> should find both
        let res_include = backend.search(crate::contracts::SearchParams::from_positional(
        "secret",
        Some("artifact-exclusion-test"),
        false,
        10,
        0,
        0.0,
        None,
        false,
        true,
        true,
        None,
        true,
        None,
    )).await.unwrap();
        assert_eq!(res_include.results.len(), 2);
        
        let titles: Vec<String> = res_include.results.iter().map(|r| r.title.clone()).collect();
        assert!(titles.contains(&"Normal Node".to_string()));
        assert!(titles.contains(&"Artifact Node".to_string()));
    }

    #[test]
    fn test_temporal_cue_parser() {
        let result = parse_temporal_cues("before that happened");
        assert_eq!(result, Some((TemporalCueType::Preceding, 1.0)));

        let result_after = parse_temporal_cues("after the event");
        assert_eq!(result_after, Some((TemporalCueType::Succeeding, 1.0)));

        let result_recent = parse_temporal_cues("recently updated");
        assert_eq!(result_recent, Some((TemporalCueType::Relative, 1.0)));

        let result_none = parse_temporal_cues("just a normal sentence");
        assert_eq!(result_none, None);

        // Procedural cue test cases (interrogative action cues)
        let result_what_did = parse_temporal_cues("What did I do before?");
        assert_eq!(result_what_did, Some((TemporalCueType::Procedural, 3.0)));

        let result_how_run = parse_temporal_cues("How did I run the script?");
        assert_eq!(result_how_run, Some((TemporalCueType::Procedural, 3.0)));

        let result_which_steps = parse_temporal_cues("Which steps did we take?");
        assert_eq!(result_which_steps, Some((TemporalCueType::Procedural, 3.0)));

        let result_taken = parse_temporal_cues("What troubleshooting steps have already been taken?");
        assert_eq!(result_taken, Some((TemporalCueType::Procedural, 3.0)));

        let result_done = parse_temporal_cues("What did we have done?");
        assert_eq!(result_done, Some((TemporalCueType::Procedural, 3.0)));

        // Strictness / False positive protection cases
        let result_why_fail = parse_temporal_cues("Why did the test fail?");
        assert_eq!(result_why_fail, None);

        let result_normal_did = parse_temporal_cues("I did run the server.");
        assert_eq!(result_normal_did, None);

        let result_what_is = parse_temporal_cues("What is the status of the server?");
        assert_eq!(result_what_is, None);
    }

    #[test]
    fn test_relative_temporal_decay() {
        let now = chrono::Utc::now();
        let one_minute_ago = now - chrono::Duration::minutes(1);
        let one_day_ago = now - chrono::Duration::days(1);
        let lambda = 0.1;

        let decay_recent = calculate_temporal_decay(one_minute_ago, now, lambda);
        let decay_old = calculate_temporal_decay(one_day_ago, now, lambda);

        assert!(decay_recent > decay_old, "Recent timestamp should decay less than older timestamp");
        assert!(decay_recent > 0.0, "Decay values must be positive");
        assert!(decay_old > 0.0, "Decay values must be positive");
    }

    #[test]
    fn test_lightweight_span_reranker() {
        let mut global_idf = std::collections::HashMap::new();
        global_idf.insert("redi".to_string(), 1.0);
        global_idf.insert("crash".to_string(), 1.0);
        global_idf.insert("server".to_string(), 0.5);

        let query_tokens_exact = crate::retrieval::bm25::tokenize("redis crash");
        let sentence_exact = "redis server did crash";
        let sim_exact = sentence_cosine_similarity(&query_tokens_exact, sentence_exact, &global_idf);
        assert!(sim_exact > 0.8, "Exact match similarity should be high, got {}", sim_exact);

        let query_tokens_unrelated = crate::retrieval::bm25::tokenize("quantum physics");
        let sentence_unrelated = "redis server did crash";
        let sim_unrelated = sentence_cosine_similarity(&query_tokens_unrelated, sentence_unrelated, &global_idf);
        assert!(sim_unrelated < 0.1, "Unrelated match similarity should be low, got {}", sim_unrelated);
    }

    #[tokio::test]
    async fn test_context_turn_injection_secure() {
        let backend = SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        let ep1 = EpisodeSave {
        created_at: None,
            title: "first step".to_string(),
            content: "This is the first step of the build".to_string(),
            entities: vec![],
            scope: Some("general".to_string()),
            vault_path: Some("wiki/scope/build1.md".to_string()),
            source_episode: None,
            session_id: Some("session-1".to_string()),
            task_id: None,
            discovery_tokens: None,
            facts: None,
            concepts: None,
            files_read: None,
            files_modified: None,
            node_type: None,
            confidence: None,
        };
        let ep1_id = backend.save_episode(&ep1).await.unwrap();

        let ep2 = EpisodeSave {
        created_at: None,
            title: "second step".to_string(),
            content: "This is the second step of the build".to_string(),
            entities: vec![],
            scope: Some("general".to_string()),
            vault_path: Some("wiki/scope/build2.md".to_string()),
            source_episode: None,
            session_id: Some("session-1".to_string()),
            task_id: None,
            discovery_tokens: None,
            facts: None,
            concepts: None,
            files_read: None,
            files_modified: None,
            node_type: None,
            confidence: None,
        };
        let ep2_id = backend.save_episode(&ep2).await.unwrap();

        let ep1_rec = parse_record_id(&ep1_id).unwrap();
        let ep2_rec = parse_record_id(&ep2_id).unwrap();
        backend.db.query("RELATE $from -> followed_by -> $to;")
            .bind(("from", ep1_rec))
            .bind(("to", ep2_rec))
            .await.unwrap().check().unwrap();

        let ep3 = EpisodeSave {
        created_at: None,
            title: "unrelated user step".to_string(),
            content: "Third step of build from a completely separate session".to_string(),
            entities: vec![],
            scope: Some("general".to_string()),
            vault_path: Some("wiki/scope/build3.md".to_string()),
            source_episode: None,
            session_id: Some("session-2".to_string()),
            task_id: None,
            discovery_tokens: None,
            facts: None,
            concepts: None,
            files_read: None,
            files_modified: None,
            node_type: None,
            confidence: None,
        };
        let ep3_id = backend.save_episode(&ep3).await.unwrap();

        let response = backend.search(crate::contracts::SearchParams::from_positional(
        "second step before",
        Some("general"),
        false,
        10,
        0,
        0.0,
        None,
        false,
        true,
        false,
        Some("session-1"),
        true,
        None,
    )).await.unwrap();

        let results = response.results;
        
        println!("DEBUG SEARCH RESULTS:");
        for r in &results {
            println!("  id={}, title='{}', content='{}', similarity={}", r.id, r.title, r.content, r.similarity);
        }

        let ep2_found = results.iter().any(|ep| ep.id == ep2_id);
        assert!(ep2_found, "Primary candidate Episode 2 should be in results");

        let ep1_found = results.iter().any(|ep| ep.id == ep1_id);
        assert!(ep1_found, "Neighbor candidate Episode 1 should be fetched via neighbor turn expansion");

        let ep3_found = results.iter().any(|ep| ep.id == ep3_id);
        assert!(!ep3_found, "Episode 3 from different session must not be returned");
    }
}

// BPE Tokenizer support functions

static GLOBAL_TOKENIZER: std::sync::OnceLock<Option<tokenizers::Tokenizer>> = std::sync::OnceLock::new();

fn get_global_tokenizer() -> Option<&'static tokenizers::Tokenizer> {
    GLOBAL_TOKENIZER.get_or_init(|| {
        if let Ok(home) = std::env::var("HOME") {
            let base_path = std::path::PathBuf::from(home).join(".mythrax/models");
            let paths = vec![
                base_path.join("llm_tokenizer.json"),
                base_path.join("tokenizer.json"),
            ];
            for path in paths {
                if path.exists() {
                    if let Ok(tok) = tokenizers::Tokenizer::from_file(&path) {
                        return Some(tok);
                    }
                }
            }
        }
        None
    }).as_ref()
}

fn estimate_bpe_tokens(text: &str) -> usize {
    let mut count = 0;
    let mut chars = text.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            if c == '\n' {
                count += 1;
            } else {
                count += 1;
            }
        } else if c.is_ascii_punctuation() {
            if c == ':' && chars.peek() == Some(&':') {
                chars.next();
                count += 1;
            } else if c == '-' && chars.peek() == Some(&'>') {
                chars.next();
                count += 1;
            } else if c == '=' && chars.peek() == Some(&'>') {
                chars.next();
                count += 1;
            } else {
                count += 1;
            }
        } else if c.is_alphanumeric() || c == '_' {
            let mut word = String::new();
            word.push(c);
            while let Some(&next_c) = chars.peek() {
                if next_c.is_alphanumeric() || next_c == '_' {
                    word.push(next_c);
                    chars.next();
                } else {
                    break;
                }
            }
            count += estimate_word_tokens(&word);
        } else {
            count += 1;
        }
    }
    count
}

fn estimate_word_tokens(word: &str) -> usize {
    if word.is_empty() {
        return 0;
    }
    
    if word.contains('_') {
        let parts: Vec<&str> = word.split('_').collect();
        let mut part_tokens = 0;
        for part in parts {
            part_tokens += estimate_word_tokens(part);
        }
        let underscores = word.chars().filter(|&c| c == '_').count();
        return part_tokens + underscores;
    }
    
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut prev_is_lower = false;
    
    for c in word.chars() {
        if c.is_uppercase() && prev_is_lower {
            if !current.is_empty() {
                parts.push(current);
                current = String::new();
            }
            prev_is_lower = false;
        } else if c.is_lowercase() {
            prev_is_lower = true;
        } else {
            prev_is_lower = false;
        }
        current.push(c);
    }
    if !current.is_empty() {
        parts.push(current);
    }
    
    if parts.len() > 1 {
        let mut part_tokens = 0;
        for part in parts {
            part_tokens += estimate_word_tokens(&part);
        }
        return part_tokens;
    }
    
    ((word.len() + 2) / 3).max(1)
}
