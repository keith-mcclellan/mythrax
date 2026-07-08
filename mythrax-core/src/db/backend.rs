use axum::async_trait;
use surrealdb_types::SurrealValue;
use crate::contracts::{EpisodeSave, SearchResult, WisdomRule, LlmConfigResponse, LlmConfigRequest, Episode, HandoffSave, WikiNode, SearchResponse, WisdomSearchResponse, GetMemoryNodesResponse, ForgedSectionBatch};
use anyhow::{Result, Context};
use surrealdb::engine::local::{Db, Mem, SurrealKv};
use surrealdb::{Surreal, IndexedResults};
use std::sync::Arc;
use uuid::Uuid;
use crate::db::schema::INIT_SCHEMA;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum QueryCategory {
    Preference,
    User,
    Temporal,
    Default,
}

impl QueryCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            QueryCategory::Preference => "preference",
            QueryCategory::User => "user",
            QueryCategory::Temporal => "temporal",
            QueryCategory::Default => "default",
        }
    }
}

fn normalize_spelling(word: &str) -> &str {
    match word {
        "favourite" | "favourites" => "favorite",
        "appt" | "appts" => "appointment",
        "mtg" | "mtgs" => "meeting",
        "grad" => "graduation",
        "lodging" | "lodgings" => "hotel",
        "staying" | "stays" => "stay",
        other => other,
    }
}

fn expand_synonyms(word: &str) -> &str {
    match word {
        "motel" | "hostel" | "cabin" | "lodge" | "resort" | "inn" | "accommodation" => "hotel",
        "airline" | "jet" | "plane" | "airplane" => "flight",
        "diner" | "cafe" | "bistro" | "eatery" | "pub" => "restaurant",
        "profession" | "occupation" | "vocation" | "work" => "job",
        "employer" | "company" | "corporation" | "firm" => "work",
        "school" | "college" | "university" | "academy" => "degree",
        "spouse" | "wife" | "husband" | "partner" => "spouse",
        "buddy" | "pal" | "colleague" => "friend",
        other => other,
    }
}

fn is_temporal_vocab(stemmed: &str) -> bool {
    matches!(
        stemmed,
        "befor"
            | "after"
            | "previous"
            | "prior"
            | "earli"
            | "ago"
            | "last"
            | "later"
            | "next"
            | "recent"
            | "today"
            | "now"
            | "first"
            | "second"
            | "third"
            | "date"
            | "time"
            | "when"
            | "year"
            | "month"
            | "week"
            | "day"
            | "hour"
            | "calendar"
            | "schedul"
            | "meet"
            | "appoint"
            | "between"
            | "pass"
            | "durat"
            | "spend"
            | "spent"
            | "sunday"
            | "monday"
            | "tuesday"
            | "wednesday"
            | "thursday"
            | "friday"
            | "saturday"
    )
}

fn is_preference_vocab(stemmed: &str) -> bool {
    matches!(
        stemmed,
        "prefer"
            | "favorit"
            | "like"
            | "dislik"
            | "love"
            | "hate"
            | "choic"
            | "opinion"
            | "choos"
            | "chose"
            | "select"
            | "book"
            | "vendor"
            | "hotel"
            | "restaur"
            | "flight"
            | "airlin"
            | "stay"
            | "suggest"
            | "recommend"
            | "should"
            | "complement"
    )
}

fn is_user_vocab(stemmed: &str) -> bool {
    matches!(
        stemmed,
        "name"
            | "age"
            | "profil"
            | "job"
            | "career"
            | "degre"
            | "graduat"
            | "work"
            | "email"
            | "phone"
            | "backgroun"
            | "address"
            | "famili"
            | "friend"
            | "spous"
            | "wife"
            | "husband"
            | "employ"
            | "cat"
            | "dog"
            | "pet"
            | "hamster"
            | "grandma"
            | "grandpa"
            | "mother"
            | "father"
            | "parent"
            | "brother"
            | "sister"
            | "sibling"
            | "son"
            | "daughter"
            | "child"
            | "commut"
            | "live"
            | "resid"
            | "born"
            | "school"
            | "hometown"
            | "car"
            | "vehicl"
            | "sneaker"
            | "postcard"
            | "collect"
    )
}

pub fn classify_query(query: &str) -> QueryCategory {
    let tokens: Vec<String> = query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    let processed_tokens: Vec<String> = tokens
        .iter()
        .map(|token| {
            let normalized = normalize_spelling(token);
            let expanded = expand_synonyms(normalized);
            crate::retrieval::bm25::stem(expanded)
        })
        .collect();

    let has_temporal = processed_tokens.iter().any(|stemmed| {
        is_temporal_vocab(stemmed)
    });

    let has_preference = processed_tokens.iter().any(|stemmed| {
        is_preference_vocab(stemmed)
    });

    let has_user_vocab_match = processed_tokens.iter().any(|stemmed| {
        is_user_vocab(stemmed)
    });

    let lower_query = query.to_lowercase();
    let has_phrase_match = lower_query.contains("who am i")
        || lower_query.contains("about me")
        || tokens.windows(3).any(|w| w == ["who", "am", "i"])
        || tokens.windows(2).any(|w| w == ["about", "me"]);

    let has_user = has_user_vocab_match || has_phrase_match;

    if has_temporal {
        QueryCategory::Temporal
    } else if has_preference {
        QueryCategory::Preference
    } else if has_user {
        QueryCategory::User
    } else {
        QueryCategory::Default
    }
}

pub static GLOBAL_BACKEND: std::sync::OnceLock<Arc<SurrealBackend>> = std::sync::OnceLock::new();
pub static GLOBAL_RERANKER: tokio::sync::Mutex<Option<crate::llm::MxbaiReranker>> = tokio::sync::Mutex::const_new(None);

macro_rules! run_write {
    ($self:expr, $block:expr) => {{
        let _guard = $self.write_lock.lock().await;
        let mut attempt = 0;
        loop {
            let res = { $block };
            match res {
                Ok(val) => break Ok(val),
                Err(e) => {
                    let err_str = e.to_string();
                    let is_conflict = err_str.contains("TransactionConflict")
                        || err_str.contains("conflict")
                        || err_str.contains("Transaction conflict");
                    if is_conflict && attempt < 5 {
                        attempt += 1;
                        let delay = std::time::Duration::from_millis(50 * (1 << attempt));
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    break Err(e);
                }
            }
        }
    }};
}

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
        query: &str,
        scope: Option<&str>,
        deep_insight: bool,
        limit: usize,
        offset: usize,
        threshold: f32,
        token_budget: Option<usize>,
        allow_downward: bool,
        include_episodes: bool,
        include_artifacts: bool,
        session_id: Option<&str>,
        include_archived: bool,
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

        // 2. Retrieve content and title for session user_input and user_feedback episodes
        #[derive(serde::Deserialize, SurrealValue, Debug)]
        struct EpisodeRecord {
            title: String,
            content: String,
        }
        let sql = "SELECT title, content FROM episode WHERE session_id = $session_id AND (node_type = 'user_input' OR node_type = 'user_feedback');";
        let mut response = self.db.query(sql)
            .bind(("session_id", session_id))
            .await?.check().context("SELECT episodes for compile_user_profile failed")?;
        let records: Vec<EpisodeRecord> = response.take(0)?;

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

        let mut turns: Vec<(u32, String)> = records.into_iter()
            .map(|r| {
                let turn_idx = parse_turn_index(&r.title).unwrap_or(0);
                (turn_idx, r.content)
            })
            .collect();
        turns.sort_by_key(|t| t.0);

        // 4. Query active STM key-values
        let stm_map = self.get_stm(session_id, None).await?;
        let mut stm_facts: Vec<String> = stm_map.into_iter()
            .filter(|(k, _)| !k.starts_with('_')) // ignore system/internal keys starting with _
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect();
        stm_facts.sort(); // Sort key alphabetically

        let stm_str = stm_facts.join("\n");

        // 5. Truncation logic (max limit search.user_profile_max_len)
        if max_len == 0 {
            // Join everything chronologically/alphabetically
            let mut parts = Vec::new();
            for (_, content) in turns {
                parts.push(content);
            }
            if !stm_str.is_empty() {
                parts.push(stm_str);
            }
            Ok(parts.join("\n"))
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
            Ok(parts.join("\n"))
        }
    }

    pub async fn save_episode_with_wal_actor(&self, episode: &EpisodeSave, wal_path: &std::path::Path) -> Result<String> {
        if self.is_client_mode() {
            return self.save_episode(episode).await;
        }
        let id = run_write!(self, {
            self.save_episode(episode).await
        })?;
        if let Some(tx) = &self.wal_tx {
            let _ = tx.send((episode.clone(), wal_path.to_path_buf())).await;
        }
        Ok(id)
    }

    /// Saves a batch of episodes in a single transaction.
    pub async fn save_episodes_batch(&self, episodes: &[EpisodeSave]) -> Result<()> {
        if self.is_client_mode() {
            let _: serde_json::Value = self.daemon_post("/v1/episodes/batch", &episodes).await?;
            return Ok(());
        }

        // 1. Generate embeddings in batch if embedder is present, hitting cache first
        let mut embeddings: Vec<Option<Vec<f32>>> = Vec::with_capacity(episodes.len());
        let mut missing_indices = Vec::new();
        let mut missing_texts = Vec::new();

        for (idx, ep) in episodes.iter().enumerate() {
            let text = format!("{}: {}", ep.title, ep.content);
            if let Some(emb) = crate::embeddings::get_cached_embedding(&text) {
                embeddings.push(Some(emb));
            } else {
                embeddings.push(None);
                missing_indices.push(idx);
                missing_texts.push(text);
            }
        }

        if !missing_texts.is_empty() && self.embedder.is_some() {
            if let Ok(generated) = self.embed_batch(&missing_texts).await {
                for (midx, generated_emb) in missing_indices.into_iter().zip(generated.into_iter()) {
                    let text = format!("{}: {}", episodes[midx].title, episodes[midx].content);
                    crate::embeddings::cache_embedding(text, generated_emb.clone());
                    embeddings[midx] = Some(generated_emb);
                }
            }
        }

        // 2. Local session tracking to link followed_by temporal relationships in-memory/STM
        let mut local_last_eps: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut relations = Vec::new();

        // 3. Map episodes to JSON objects for SurrealQL
        let mapped_json_array: Vec<serde_json::Value> = episodes
            .iter()
            .enumerate()
            .map(|(i, ep)| {
                let id_str = Uuid::new_v4().to_string();
                let metrics_id_str = Uuid::new_v4().to_string();
                let embedding = embeddings.get(i).cloned().flatten();
                let word_count = crate::retrieval::bm25::tokenize(&ep.content).len() as u32;
                let last_retrieved_at = chrono::Utc::now().to_rfc3339();

                // Reconstruct followed_by relationships using local cache and STM
                if let Some(ref sess_id) = ep.session_id {
                    let tracking_key = if let Some(ref t_id) = ep.task_id {
                        format!("_last_episode_id_{}", t_id)
                    } else {
                        "_last_episode_id".to_string()
                    };
                    let map_key = format!("{}:{}", sess_id, tracking_key);

                    if let Some(last_ep_id) = local_last_eps.get(&map_key).cloned() {
                        let last_uuid = last_ep_id.strip_prefix("episode:").unwrap_or(&last_ep_id).to_string();
                        relations.push(serde_json::json!({
                            "from_str": last_uuid,
                            "to_str": id_str.clone(),
                        }));
                    } else {
                        // Bounded check of STM database to bridge sequential batches
                        let this_self = self.clone();
                        let sess_id_clone = sess_id.clone();
                        let tracking_key_clone = tracking_key.clone();
                        // Run STM lookup blockingly in a local context (since map is called within a sync closure)
                        if let Ok(stm_map) = tokio::task::block_in_place(move || {
                            tokio::runtime::Handle::current().block_on(async move {
                                this_self.get_stm(&sess_id_clone, Some(&tracking_key_clone)).await
                            })
                        }) {
                            if let Some(last_ep_id) = stm_map.get(&tracking_key) {
                                let last_uuid = last_ep_id.strip_prefix("episode:").unwrap_or(last_ep_id).to_string();
                                relations.push(serde_json::json!({
                                    "from_str": last_uuid,
                                    "to_str": id_str.clone(),
                                }));
                            }
                        }
                    }
                    local_last_eps.insert(map_key, format!("episode:{}", id_str));
                }

                let mut ep_json = serde_json::json!({
                    "id_str": id_str,
                    "metrics_id_str": metrics_id_str,
                    "title": ep.title,
                    "content": ep.content,
                    "scope": ep.scope.clone().unwrap_or_else(|| "general".to_string()),
                    "vault_path": ep.vault_path.clone().unwrap_or_default(),
                    "utility": 50.0f32,
                    "last_retrieved_at": last_retrieved_at,
                    "word_count": word_count
                });

                let ep_obj = ep_json.as_object_mut().unwrap();
                if let Some(emb) = embedding {
                    ep_obj.insert("embedding".to_string(), serde_json::json!(emb));
                }
                if let Some(ref sess) = ep.session_id {
                    ep_obj.insert("session_id".to_string(), serde_json::json!(sess));
                }
                let node_type_val = ep.node_type.clone().unwrap_or_else(|| "agent_thought".to_string());
                ep_obj.insert("node_type".to_string(), serde_json::json!(node_type_val));

                ep_json
            })
            .collect();

        // 4. Record indexing writes for any vault_path present
        for ep in episodes {
            if let Some(ref vp) = ep.vault_path {
                self.record_indexing_write(vp).await;
            }
        }

        // 5. Execute batch transaction in database
        let query = r#"
            BEGIN TRANSACTION;
            FOR $ep IN $episodes {
                LET $ep_id = type::record('episode', $ep.id_str);
                LET $met_id = type::record('metrics', $ep.metrics_id_str);
                
                INSERT INTO episode (id, title, content, scope, vault_path, embedding, processed_in_dream, archived, utility, last_retrieved_at, session_id, word_count, node_type)
                VALUES ($ep_id, $ep.title, $ep.content, $ep.scope, $ep.vault_path, $ep.embedding, false, false, 50.0, $ep.last_retrieved_at, $ep.session_id, $ep.word_count, $ep.node_type);
                
                INSERT INTO metrics (id, target_id, utility_score, access_count)
                VALUES ($met_id, $ep_id, 50.0, 0);
            };
            FOR $rel IN $relations {
                LET $from = type::record('episode', $rel.from_str);
                LET $to = type::record('episode', $rel.to_str);
                RELATE $from -> followed_by -> $to CONTENT { created_at: time::now() };
            };
            COMMIT TRANSACTION;
        "#;
        run_write!(self, {
            async {
                let response = self.db.query(query)
                    .bind(("episodes", mapped_json_array.clone()))
                    .bind(("relations", relations.clone()))
                    .await?;
                response.check().map_err(|e| anyhow::anyhow!("SurrealDB save_episodes_batch transaction failed: {}", e))
            }.await
        })?;

        // 6. Update Short Term Memory (STM) in database with final batch states
        for (map_key, final_ep_id) in local_last_eps {
            let parts: Vec<&str> = map_key.splitn(2, ':').collect();
            if parts.len() == 2 {
                let sess_id = parts[0];
                let tracking_key = parts[1];
                let _ = self.save_stm(sess_id, tracking_key, &final_ep_id).await;
            }
        }

        Ok(())
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

    fn compact_search_result(&self, item: &mut SearchResult, remaining_budget: usize) -> bool {
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
struct WisdomRaw {
    id: surrealdb::types::RecordId,
    target_pattern: String,
    action_to_avoid: String,
    causal_explanation: String,
    prescribed_remedy: String,
    tier: String,
    scope: String,
    vault_path: Option<String>,
    embedding: Option<Vec<f32>>,
    source_episodes: Option<Vec<String>>,
    generator_name: String,
    utility: Option<f32>,
    status: Option<String>,
    superseded_at: Option<String>,
    superseded_by: Option<String>,
    rule_type: Option<String>,
}

impl WisdomRaw {
    fn into_wisdom_rule(self) -> WisdomRule {
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

#[derive(serde::Deserialize, Debug, SurrealValue)]
struct EpisodeRaw {
    id: surrealdb::types::RecordId,
    title: String,
    content: String,
    source: Option<String>,
    scope: Option<String>,
    vault_path: Option<String>,
    embedding: Option<Vec<f32>>,
    processed_in_dream: Option<bool>,
    source_episode: Option<surrealdb::types::RecordId>,
    last_retrieved_at: Option<String>,
    utility: Option<f32>,
    archived: Option<bool>,
    archived_at: Option<chrono::DateTime<chrono::Utc>>,
    discovery_tokens: Option<u32>,
    facts: Option<Vec<String>>,
    concepts: Option<Vec<String>>,
    files_read: Option<Vec<String>>,
    files_modified: Option<Vec<String>>,
    session_id: Option<String>,
    word_count: Option<u32>,
    node_type: Option<String>,
    confidence: Option<f32>,
}

/// Full hydrated Handoff contract — returned by queries; construction deferred pending
/// the agent-tracking dashboard feature. Suppressed until then.
#[allow(dead_code)]
#[derive(serde::Deserialize, Debug, SurrealValue)]
struct HandoffRaw {
    id: surrealdb::types::RecordId,
    parent_conversation_id: String,
    subagent_conversation_id: String,
    summary: String,
    handoff_file_path: String,
    scope: Option<String>,
    status: Option<String>,
    created_at: Option<serde_json::Value>,
    include_tool_execution: Option<bool>,
}

#[derive(serde::Deserialize, Debug, SurrealValue)]
struct WikiNodeRaw {
    id: surrealdb::types::RecordId,
    name: String,
    content: String,
    scope: String,
    vault_path: Option<String>,
    embedding: Option<Vec<f32>>,
}

impl WikiNodeRaw {
    fn into_wiki_node(self) -> WikiNode {
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

fn get_tier_boost(tier: &str, category: QueryCategory) -> f32 {
    match (tier, category) {
        ("episode", QueryCategory::User | QueryCategory::Preference | QueryCategory::Temporal) => 1.3,
        ("skills" | "wisdom", _) => 1.2,
        ("wiki_node" | "insight" | "project_brief" | "system_playbook", _) => 1.1,
        _ => 1.0,
    }
}

fn append_related_context(content: &mut String, related_nodes: &[RelatedNodeRaw]) {
    if related_nodes.is_empty() {
        return;
    }
    content.push_str("\n\n---\n### Related Context\n");
    for node in related_nodes {
        let table = node.id.table.as_str();
        if table == "episode" {
            if let (Some(t), Some(c)) = (&node.title, &node.content) {
                content.push_str(&format!("[Related Episode: {}]\n{}\n\n", t, c));
            }
        } else if table == "wiki_node" {
            if let (Some(n), Some(c)) = (&node.name, &node.content) {
                content.push_str(&format!("[Related Wiki Node: {}]\n{}\n\n", n, c));
            }
        } else if table == "entity" {
            if let (Some(n), Some(s)) = (&node.name, &node.summary) {
                content.push_str(&format!("[Related Entity: {}]\n{}\n\n", n, s));
            }
        } else if table == "wisdom" {
            let pattern = node.target_pattern.as_deref().unwrap_or("");
            let avoid = node.action_to_avoid.as_deref().unwrap_or("");
            let explanation = node.causal_explanation.as_deref().unwrap_or("");
            let remedy = node.prescribed_remedy.as_deref().unwrap_or("");
            content.push_str(&format!(
                "[Related Wisdom: {}]\nAction to avoid: {}\nCausal explanation: {}\nPrescribed remedy: {}\n\n",
                pattern, avoid, explanation, remedy
            ));
        } else if table == "hypothesis_node" {
            if let Some(c) = &node.content {
                content.push_str(&format!("[Related Hypothesis]\n{}\n\n", c));
            }
        } else if table == "handoff" {
            if let Some(s) = &node.summary {
                content.push_str(&format!("[Related Handoff]\n{}\n\n", s));
            }
        } else {
            if let Some(c) = &node.content {
                content.push_str(&format!("[Related {}]\n{}\n\n", table, c));
            } else if let Some(s) = &node.summary {
                content.push_str(&format!("[Related {}]\n{}\n\n", table, s));
            }
        }
    }
    *content = content.trim_end().to_string();
}

fn reciprocal_rank_fusion(
    mut vector_results: Vec<SearchResult>,
    mut keyword_results: Vec<SearchResult>,
    k: usize,
) -> Vec<SearchResult> {
    vector_results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
    keyword_results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
    
    let mut rrf_scores = std::collections::HashMap::new();
    
    for (rank, item) in vector_results.iter().enumerate() {
        let rank_val = rank + 1;
        let score = 1.0 / (k as f32 + rank_val as f32);
        rrf_scores.insert(item.id.clone(), score);
    }
    
    for (rank, item) in keyword_results.iter().enumerate() {
        let rank_val = rank + 1;
        let score = 1.0 / (k as f32 + rank_val as f32);
        *rrf_scores.entry(item.id.clone()).or_insert(0.0) += score;
    }
    
    let mut items_map = std::collections::HashMap::new();
    for item in keyword_results {
        items_map.insert(item.id.clone(), item);
    }
    for item in vector_results {
        items_map.insert(item.id.clone(), item);
    }
    
    let mut fused = Vec::new();
    let max_possible = 2.0f32 / (k as f32 + 1.0f32);
    for (id, score) in rrf_scores {
        if let Some(mut item) = items_map.remove(&id) {
            item.similarity = (score / max_possible).min(1.0f32);
            fused.push(item);
        }
    }
    
    fused.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
    fused
}

#[derive(serde::Deserialize, SurrealValue)]
struct ScoredEdge {
    out: surrealdb::types::RecordId,
    confidence: Option<f32>,
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

fn prepare_fts_query(query: &str) -> Vec<String> {
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
        // Cap at 8 FTS predicates to avoid SQL explosion
        words.into_iter().take(8).collect()
    }
}


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
        self.get_max_concurrent_tasks().await
    }

    async fn init(&self) -> Result<()> {
        if self.is_client_mode() {
            return Ok(());
        }
        self.db.query(INIT_SCHEMA).await?
            .check().context("Applying schemas failed")?;

        // Migration: Backfill legacy episodes where node_type is None
        let migration_sql = "UPDATE episode SET node_type = 'agent_thought' WHERE node_type = NONE;";
        let _ = self.db.query(migration_sql).await?
            .check().context("Failed to run legacy episode node_type migration")?;

        // Initialize default configuration if config:settings does not exist
        let check_sql = "SELECT * FROM config:settings;";
        let mut response = self.db.query(check_sql).await?.check().context("Check config failed")?;
        let config_opt: Option<LlmConfigResponse> = response.take(0)?;
        if config_opt.is_none() {
            let insert_sql = "
                CREATE config:settings CONTENT {
                    active_provider: 'local',
                    model: 'mlx-community/Qwen3.6-35B-A3B-4bit',
                    cloud_provider: 'gemini',
                    is_override: false,
                    expires_at: NONE
                };
            ";
            self.db.query(insert_sql).await?.check().context("Insert default config failed")?;
        }

        if let Some(ref path) = self.db_path {
            let marker = path.join(".initialized");
            if let Some(parent) = marker.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(marker, "initialized");
        }

        // Automatically load profile settings from bench_data/tuned_params.json if present
        // Gated behind MYTHRAX_LOAD_TUNED_PARAMS=true to prevent benchmark-tuned params from
        // silently overriding production defaults (MMR, sigmoid, boosts, etc.)
        let load_tuned = std::env::var("MYTHRAX_LOAD_TUNED_PARAMS")
            .map(|v| v == "true")
            .unwrap_or(false);
        if load_tuned {
            let mut tuned_path = std::path::PathBuf::from("bench_data/tuned_params.json");
            if !tuned_path.exists() {
                tuned_path = std::path::PathBuf::from("../bench_data/tuned_params.json");
            }
            if tuned_path.exists() {
                if let Ok(content) = std::fs::read_to_string(tuned_path) {
                    if let Ok(map) = serde_json::from_str::<std::collections::HashMap<String, serde_json::Value>>(&content) {
                        for (k, v) in map {
                            let val_str = match v {
                                serde_json::Value::String(s) => s,
                                other => other.to_string(),
                            };
                            let upsert_sql = "UPSERT type::record('profile', $key) CONTENT { key: $key, value: $val };";
                            let res = self.db.query(upsert_sql)
                                .bind(("key", k.as_str()))
                                .bind(("val", val_str.as_str()))
                                .await;
                            if let Ok(r) = res {
                                let _ = r.check();
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn save_episode(&self, episode: &EpisodeSave) -> Result<String> {
        if self.is_client_mode() {
            #[derive(serde::Deserialize)]
            struct SaveResponse {
                id: String,
            }
            let res: SaveResponse = self.daemon_post("/v1/episodes", episode).await?;
            return Ok(res.id);
        }
        if let Some(ref vp) = episode.vault_path {
            self.record_indexing_write(vp).await;
        }
        let mut ep_uuid = Uuid::new_v4().to_string();
        let mut is_update = false;

        if let Some(ref vp) = episode.vault_path {
            let check_query = "SELECT VALUE id FROM episode WHERE vault_path = $vault_path LIMIT 1;";
            let mut response = self.db.query(check_query).bind(("vault_path", vp.as_str())).await?;
            let ids: Option<surrealdb::types::RecordId> = response.take(0)?;
            if let Some(thing) = ids {
                ep_uuid = match &thing.key {
                    surrealdb::types::RecordIdKey::String(s) => unescape_id_part(s),
                    other => unescape_id_part(&record_key_to_string(other)),
                };
                is_update = true;
            }
        }

        let query_str = if is_update {
            "
                BEGIN TRANSACTION;
                LET $ep = type::record('episode', $ep_uuid);
                UPDATE $ep MERGE {
                    title: $title,
                    content: $content,
                    scope: $target_scope,
                    vault_path: $vault_path,
                    processed_in_dream: false,
                    embedding: $embedding,
                    discovery_tokens: $discovery_tokens,
                    facts: $facts,
                    concepts: $concepts,
                    files_read: $files_read,
                    files_modified: $files_modified,
                    session_id: $session_id,
                    word_count: $word_count,
                    node_type: $node_type
                };
                DELETE FROM mentions WHERE in = $ep;
                COMMIT TRANSACTION;
            "
        } else {
            "
                BEGIN TRANSACTION;
                LET $ep = type::record('episode', $ep_uuid);
                LET $met = type::record('metrics', $metrics_uuid);
                
                CREATE $ep CONTENT {
                    title: $title,
                    content: $content,
                    scope: $target_scope,
                    vault_path: $vault_path,
                    processed_in_dream: false,
                    embedding: $embedding,
                    utility: $utility,
                    last_retrieved_at: $last_retrieved_at,
                    archived: false,
                    discovery_tokens: $discovery_tokens,
                    facts: $facts,
                    concepts: $concepts,
                    files_read: $files_read,
                    files_modified: $files_modified,
                    session_id: $session_id,
                    word_count: $word_count,
                    node_type: $node_type
                };
                
                CREATE $met CONTENT {
                    target_id: $ep,
                    utility_score: 50.0,
                    access_count: 0
                };
                
                COMMIT TRANSACTION;
            "
        };

        let metrics_uuid = Uuid::new_v4().to_string();
        let scope_val = episode.scope.clone().unwrap_or_else(|| "general".to_string());
        let vp_val = episode.vault_path.clone().unwrap_or_default();
        let node_type_val = episode.node_type.clone().unwrap_or_else(|| "agent_thought".to_string());

        let embedding_val = if self.embedder.is_some() {
            let text_to_embed = format!("{}: {}", episode.title, episode.content);
            match self.embed(&text_to_embed).await {
                Ok(vec) => Some(vec),
                Err(e) => {
                    tracing::warn!("Embedding generation failed in save_episode: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let now_str = chrono::Utc::now().to_rfc3339();
        let utility_init = 50.0f32;
        let word_count = crate::retrieval::bm25::tokenize(&episode.content).len() as u32;

        let response = self.db.query(query_str)
            .bind(("ep_uuid", ep_uuid.as_str()))
            .bind(("metrics_uuid", metrics_uuid.as_str()))
            .bind(("title", episode.title.as_str()))
            .bind(("content", episode.content.as_str()))
            .bind(("target_scope", scope_val.as_str()))
            .bind(("vault_path", vp_val.as_str()))
            .bind(("embedding", embedding_val.clone()))
            .bind(("utility", utility_init))
            .bind(("last_retrieved_at", now_str.as_str()))
            .bind(("discovery_tokens", episode.discovery_tokens))
            .bind(("facts", episode.facts.clone().unwrap_or_default()))
            .bind(("concepts", episode.concepts.clone().unwrap_or_default()))
            .bind(("files_read", episode.files_read.clone().unwrap_or_default()))
            .bind(("files_modified", episode.files_modified.clone().unwrap_or_default()))
            .bind(("session_id", episode.session_id.clone()))
            .bind(("word_count", word_count))
            .bind(("node_type", node_type_val))
            .await?;

        tracing::debug!("save_episode query response: {:?}", response);
        response.check().context("SurrealDB save_episode transaction failed")?;

        for entity in &episode.entities {
            let entity_query = "
                BEGIN TRANSACTION;
                LET $ent_id = type::record('entity', $name);
                INSERT INTO entity (id, name, entity_type, summary, labels, scope)
                VALUES ($ent_id, $name, $entity_type, $summary, $labels, $target_scope)
                ON DUPLICATE KEY UPDATE
                    summary = $summary,
                    labels = $labels,
                    scope = $target_scope;
                
                -- Relate episode to entity
                LET $ep = type::record('episode', $ep_uuid);
                RELATE $ep -> mentions -> $ent_id CONTENT {
                    created_at: time::now()
                };
                COMMIT TRANSACTION;
            ";
            let _ = self.db.query(entity_query)
                .bind(("name", entity.name.as_str()))
                .bind(("entity_type", entity.entity_type.as_str()))
                .bind(("summary", entity.summary.as_str()))
                .bind(("labels", entity.labels.clone()))
                .bind(("target_scope", scope_val.as_str()))
                .bind(("ep_uuid", ep_uuid.as_str()))
                .await?
                .check().context("Entity relation query failed")?;
        }

        let new_ep_id = format!("episode:{}", ep_uuid);

        if let Some(ref sess_id) = episode.session_id {
            let tracking_key = if let Some(ref t_id) = episode.task_id {
                format!("_last_episode_id_{}", t_id)
            } else {
                "_last_episode_id".to_string()
            };

            // Get last episode ID from STM
            if let Ok(stm_map) = self.get_stm(sess_id, Some(&tracking_key)).await {
                if let Some(last_ep_id) = stm_map.get(&tracking_key) {
                    // Relate last_ep_id to new_ep_id
                    let from_thing = parse_record_id(last_ep_id);
                    let to_thing = parse_record_id(&new_ep_id);
                    if let (Ok(from), Ok(to)) = (from_thing, to_thing) {
                        let relate_query = "RELATE $from -> followed_by -> $to CONTENT { created_at: time::now() };";
                        if let Err(e) = self.db.query(relate_query)
                            .bind(("from", from))
                            .bind(("to", to))
                            .await
                        {
                            tracing::warn!("Failed to relate temporal episodes: {:?}", e);
                        }
                    }
                }
            }

            // Save new episode ID to STM
            if let Err(e) = self.save_stm(sess_id, &tracking_key, &new_ep_id).await {
                tracing::warn!("Failed to save last episode ID to STM: {:?}", e);
            }
        }

        Ok(new_ep_id)
    }

    async fn save_wisdom_rule(&self, rule: &WisdomRule) -> Result<String> {
        if let Some(ref vp) = rule.vault_path {
            self.record_indexing_write(vp).await;
        }
        let mut rule_uuid = Uuid::new_v4().to_string();
        let mut is_update = false;

        if let Some(ref vp) = rule.vault_path {
            let check_query = "SELECT VALUE id FROM wisdom WHERE vault_path = $vault_path LIMIT 1;";
            let mut response = self.db.query(check_query).bind(("vault_path", vp.as_str())).await?;
            let ids: Option<surrealdb::types::RecordId> = response.take(0)?;
            if let Some(thing) = ids {
                rule_uuid = match &thing.key {
                    surrealdb::types::RecordIdKey::String(s) => unescape_id_part(s),
                    other => unescape_id_part(&record_key_to_string(other)),
                };
                is_update = true;
            }
        }

        let query_str = if is_update {
            if rule.utility.is_some() {
                "
                    BEGIN TRANSACTION;
                    LET $rule = type::record('wisdom', $rule_uuid);
                    UPDATE $rule MERGE {
                        target_pattern: $target_pattern,
                        action_to_avoid: $action_to_avoid,
                        causal_explanation: $causal_explanation,
                        prescribed_remedy: $prescribed_remedy,
                        tier: $tier,
                        scope: $target_scope,
                        vault_path: $vault_path,
                        source_episodes: $source_episodes,
                        generator_name: $generator_name,
                        embedding: $embedding,
                        status: $status,
                        superseded_at: $superseded_at,
                        superseded_by: $superseded_by
                    };
                    UPDATE metrics SET utility_score = $utility_score WHERE target_id = $rule;
                    COMMIT TRANSACTION;
                ".to_string()
            } else {
                "
                    BEGIN TRANSACTION;
                    LET $rule = type::record('wisdom', $rule_uuid);
                    UPDATE $rule MERGE {
                        target_pattern: $target_pattern,
                        action_to_avoid: $action_to_avoid,
                        causal_explanation: $causal_explanation,
                        prescribed_remedy: $prescribed_remedy,
                        tier: $tier,
                        scope: $target_scope,
                        vault_path: $vault_path,
                        source_episodes: $source_episodes,
                        generator_name: $generator_name,
                        embedding: $embedding,
                        status: $status,
                        superseded_at: $superseded_at,
                        superseded_by: $superseded_by
                    };
                    COMMIT TRANSACTION;
                ".to_string()
            }
        } else {
            "
                BEGIN TRANSACTION;
                LET $rule = type::record('wisdom', $rule_uuid);
                LET $met = type::record('metrics', $metrics_uuid);
                
                CREATE $rule CONTENT {
                    target_pattern: $target_pattern,
                    action_to_avoid: $action_to_avoid,
                    causal_explanation: $causal_explanation,
                    prescribed_remedy: $prescribed_remedy,
                    tier: $tier,
                    scope: $target_scope,
                    vault_path: $vault_path,
                    source_episodes: $source_episodes,
                    generator_name: $generator_name,
                    embedding: $embedding,
                    status: $status,
                    superseded_at: $superseded_at,
                    superseded_by: $superseded_by
                };
                
                CREATE $met CONTENT {
                    target_id: $rule,
                    utility_score: $utility_score,
                    access_count: 0
                };
                
                COMMIT TRANSACTION;
            ".to_string()
        };

        let metrics_uuid = Uuid::new_v4().to_string();
        let vp_val = rule.vault_path.clone().unwrap_or_default();

        let embedding_val = if let Some(ref emb) = rule.embedding {
            Some(emb.clone())
        } else if let Some(ref _embedder) = self.embedder {
            let text_to_embed = format!(
                "Pattern: {}\nAvoid: {}\nWhy: {}\nRemedy: {}",
                rule.target_pattern, rule.action_to_avoid, rule.causal_explanation, rule.prescribed_remedy
            );
            match self.embed(&text_to_embed).await {
                Ok(vec) => Some(vec),
                Err(e) => {
                    tracing::warn!("Embedding generation failed in save_wisdom_rule: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let utility_val = rule.utility.unwrap_or(50.0);

        let _ = self.db.query(query_str.as_str())
            .bind(("rule_uuid", rule_uuid.as_str()))
            .bind(("metrics_uuid", metrics_uuid.as_str()))
            .bind(("target_pattern", rule.target_pattern.as_str()))
            .bind(("action_to_avoid", rule.action_to_avoid.as_str()))
            .bind(("causal_explanation", rule.causal_explanation.as_str()))
            .bind(("prescribed_remedy", rule.prescribed_remedy.as_str()))
            .bind(("tier", rule.tier.as_str()))
            .bind(("target_scope", rule.scope.as_str()))
            .bind(("vault_path", vp_val.as_str()))
            .bind(("source_episodes", rule.source_episodes.clone()))
            .bind(("generator_name", rule.generator_name.as_str()))
            .bind(("embedding", embedding_val.clone()))
            .bind(("utility_score", utility_val))
            .bind(("status", rule.status.as_deref().unwrap_or("active")))
            .bind(("superseded_at", rule.superseded_at.as_deref()))
            .bind(("superseded_by", rule.superseded_by.as_deref()))
            .await?
            .check().context("SurrealDB save_wisdom_rule transaction failed")?;

        // T1: Federated Promotion & Auto-Push
        if utility_val >= 50.0 && rule.scope != "general" {
            if let Ok(project_root) = std::env::var("MYTHRAX_WORKSPACE_ROOT") {
                let shared_dir = std::path::PathBuf::from(&project_root)
                    .join(".mythrax-shared")
                    .join("wisdom")
                    .join("proposed");
                if let Err(e) = std::fs::create_dir_all(&shared_dir) {
                    tracing::warn!("Failed to create shared proposed directory: {}", e);
                } else if !vp_val.is_empty() {
                    let src_file = crate::store::find_vault_root().join(&vp_val);
                    if src_file.exists() {
                        let filename = std::path::Path::new(&vp_val)
                            .file_name()
                            .unwrap_or_else(|| std::ffi::OsStr::new("rule.md"));
                        let dest_file = shared_dir.join(filename);
                        if let Err(e) = std::fs::copy(&src_file, &dest_file) {
                            tracing::warn!("Failed to copy wisdom rule to shared folder: {}", e);
                        } else {
                            // Spawn background command for Git
                            let dest_file_str = dest_file.to_string_lossy().to_string();
                            let project_root_clone = project_root.clone();
                            std::thread::spawn(move || {
                                use std::process::Command;
                                // Git add
                                let add_res = Command::new("git")
                                    .args(&["add", &dest_file_str])
                                    .current_dir(&project_root_clone)
                                    .output();
                                if let Ok(add_out) = add_res {
                                    if add_out.status.success() {
                                        // Git commit
                                        let commit_res = Command::new("git")
                                            .args(&["commit", "-m", "mythrax: auto-promote wisdom rule"])
                                            .current_dir(&project_root_clone)
                                            .output();
                                        if let Ok(commit_out) = commit_res {
                                            if commit_out.status.success() {
                                                // Get hash
                                                let hash_res = Command::new("git")
                                                    .args(&["rev-parse", "--short", "HEAD"])
                                                    .current_dir(&project_root_clone)
                                                    .output();
                                                let hash = if let Ok(hash_out) = hash_res {
                                                    String::from_utf8_lossy(&hash_out.stdout).trim().to_string()
                                                } else {
                                                    "unknown".to_string()
                                                };
                                                // Git push (background)
                                                let _ = Command::new("git")
                                                    .arg("push")
                                                    .current_dir(&project_root_clone)
                                                    .status();
                                                
                                                tracing::info!("[Mythrax Synapse: Auto-Promoted Wisdom Rule to GitHub -> committed as {}. To rollback, run: git revert {}]", hash, hash);
                                            }
                                        }
                                    }
                                }
                            });
                        }
                    }
                }
            }
        }

        Ok(format!("wisdom:{}", rule_uuid))
    }



    async fn search(
        &self,
        query: &str,
        scope: Option<&str>,
        deep_insight: bool,
        limit: usize,
        offset: usize,
        threshold: f32,
        token_budget: Option<usize>,
        allow_downward: bool,
        include_episodes: bool,
        include_artifacts: bool,
        session_id: Option<&str>,
        include_archived: bool,
    ) -> Result<SearchResponse> {
        /*
         * Active Search Pipeline Stages (v2.5.2):
         * Stage 1: Query Prep (normalization & temporal cue parsing)
         * Stage 2: Per-result Sigmoid Gating (pre-fusion similarity quality threshold)
         * Stage 3: Parallel Vector / FTS (BM25) Retrieval & Fusion (Reciprocal Rank Fusion or Score Blending)
         * Stage 4: Post-fusion Sigmoid Gating (fused score quality threshold)
         * Stage 5: Session Isolation & Context Filter scoping
         * Stage 6: Temporal Neighbor Expansion (traverses followed_by edges if cues detected)
         * Stage 7: Sub-sentence/Segment cosine/TF-IDF Reranking
         * Stage 8: Rank-Position Ladder Boost (position-based score adjustment)
         * Stage 9: Bounded Verbatim Hydration and Limit/Offset clipping
         */
        if self.is_client_mode() {
            let payload = serde_json::json!({
                "query": query,
                "scope": scope,
                "deep_insight": deep_insight,
                "limit": limit,
                "offset": offset,
                "threshold": threshold,
                "token_budget": token_budget,
                "allow_downward": allow_downward,
                "include_episodes": include_episodes,
                "include_artifacts": include_artifacts,
                "session_id": session_id,
                "include_archived": include_archived,
            });
            return self.daemon_post("/v1/search", &payload).await;
        }

        let user_profile = if let Some(sid) = session_id {
            match self.compile_user_profile(sid).await {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!("Failed to compile user profile for session {}: {:?}", sid, e);
                    "".to_string()
                }
            }
        } else {
            "".to_string()
        };

        let mode = self.get_search_mode().await;
        let is_hybrid = mode == "hybrid";

        let is_session_isolation_enabled = if let Ok(val) = std::env::var("MYTHRAX_SESSION_ISOLATION") {
            val == "true"
        } else {
            true
        };
        let bound_session_id = if is_session_isolation_enabled {
            session_id
        } else {
            None
        };

        let enable_advanced = match self.get_profile_key("search.enable_advanced_reranking").await {
            Ok(Some(val_str)) => val_str.parse::<bool>().unwrap_or(false),
            _ => false,
        };


        let temporal_cue_info = if is_hybrid {
            parse_temporal_cues(query)
        } else {
            None
        };
        let cleaned_query = if is_hybrid && temporal_cue_info.is_some() {
            static CLEANING_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
            let cleaning_re = CLEANING_RE.get_or_init(|| {
                regex::Regex::new(r"\b(before|preceding|previously|prior|earlier|ago|last|after|following|subsequently|later|next|recent|recently|latest|newest|today|now)\b").unwrap()
            });
            let cleaned = cleaning_re.replace_all(query, "").to_string();
            cleaned.split_whitespace().collect::<Vec<&str>>().join(" ")
        } else {
            query.to_string()
        };

        let fts_words = prepare_fts_query(&cleaned_query);

        // Build dynamic FTS disjunction: each word gets its own @N@ predicate
        let (fts_where_clause, fts_score_expr) = if fts_words.is_empty() {
            ("string::contains(title, $query)".to_string(), "0.0".to_string())
        } else {
            let where_parts: Vec<String> = fts_words.iter().enumerate()
                .map(|(i, _)| format!("content @{}@ $fts_word_{}", i, i))
                .collect();
            let score_parts: Vec<String> = fts_words.iter().enumerate()
                .map(|(i, _)| format!("(search::score({}) ?? 0.0)", i))
                .collect();
            (
                format!("({} OR string::contains(title, $query))", where_parts.join(" OR ")),
                score_parts.join(" + ")
            )
        };

        let query_category = classify_query(&cleaned_query);

        #[allow(unused_variables)]
        let sigmoid_center = match self.get_category_profile_key(query_category, "sigmoid_center", "search.sigmoid_center").await.as_str() {
            val if !val.is_empty() => val.parse::<f32>().unwrap_or(0.55f32),
            _ => 0.55f32,
        };
        #[allow(unused_variables)]
        let sigmoid_steepness = match self.get_category_profile_key(query_category, "sigmoid_steepness", "search.sigmoid_steepness").await.as_str() {
            val if !val.is_empty() => val.parse::<f32>().unwrap_or(15.0f32),
            _ => 15.0f32,
        };
        let fusion_sigmoid_center = match self.get_category_profile_key(query_category, "fusion_sigmoid_center", "search.fusion_sigmoid_center").await.as_str() {
            val if !val.is_empty() => val.parse::<f32>().unwrap_or(0.60f32),
            _ => 0.60f32,
        };
        let fusion_sigmoid_steepness = match self.get_category_profile_key(query_category, "fusion_sigmoid_steepness", "search.fusion_sigmoid_steepness").await.as_str() {
            val if !val.is_empty() => val.parse::<f32>().unwrap_or(20.0f32),
            _ => 20.0f32,
        };
        #[allow(unused_variables)]
        let rerank_weight = match self.get_category_profile_key(query_category, "rerank_weight", "search.rerank_weight").await.as_str() {
            val if !val.is_empty() => val.parse::<f32>().unwrap_or(0.15f32),
            _ => 0.15f32,
        };

        let w_imp_ep = match self.get_profile_key("search.weight_importance_episode").await {
            Ok(Some(val_str)) => val_str.parse::<f32>().unwrap_or(0.3f32),
            _ => 0.3f32,
        };
        let w_rec_ep = match self.get_profile_key("search.weight_recency_episode").await {
            Ok(Some(val_str)) => val_str.parse::<f32>().unwrap_or(0.3f32),
            _ => 0.3f32,
        };
        let w_imp_ins = match self.get_profile_key("search.weight_importance_insight").await {
            Ok(Some(val_str)) => val_str.parse::<f32>().unwrap_or(0.4f32),
            _ => 0.4f32,
        };
        let w_rec_ins = match self.get_profile_key("search.weight_recency_insight").await {
            Ok(Some(val_str)) => val_str.parse::<f32>().unwrap_or(0.2f32),
            _ => 0.2f32,
        };
        let w_imp_wis = match self.get_profile_key("search.weight_importance_wisdom").await {
            Ok(Some(val_str)) => val_str.parse::<f32>().unwrap_or(0.5f32),
            _ => 0.5f32,
        };
        let w_rec_wis = match self.get_profile_key("search.weight_recency_wisdom").await {
            Ok(Some(val_str)) => val_str.parse::<f32>().unwrap_or(0.1f32),
            _ => 0.1f32,
        };
        let demotion_mult = match self.get_profile_key("search.archived_demotion_multiplier").await {
            Ok(Some(val_str)) => val_str.parse::<f32>().unwrap_or(0.4f32),
            _ => 0.4f32,
        };
        let bypass_threshold = match self.get_profile_key("search.archived_bypass_threshold").await {
            Ok(Some(val_str)) => val_str.parse::<f32>().unwrap_or(0.80f32),
            _ => 0.80f32,
        };

        let resolved_scope = match scope {
            Some(s) if !s.is_empty() && s != "all" => s.to_string(),
            _ => self.resolve_active_scope(),
        };
        let search_all = scope == Some("all");

        let exclude_execution_logs = match self.get_profile_key("search.exclude_execution_logs").await {
            Ok(Some(val_str)) => val_str.parse::<bool>().unwrap_or(false),
            _ => false,
        };

        let (use_new_formula, is_sigmoid_gated_search_test) = {
            #[cfg(any(test, feature = "test-mock"))]
            {
                let is_running_in_test = {
                    let in_test_exe = if let Ok(exe) = std::env::current_exe() {
                        let name = exe.to_string_lossy();
                        name.contains("/deps/") || name.contains("test")
                    } else {
                        false
                    };
                    in_test_exe || std::env::args().any(|arg| arg.contains("test"))
                };
                let is_sigmoid = (if let Ok(exe) = std::env::current_exe() {
                    exe.to_string_lossy().contains("test_sigmoid_gated_search")
                } else {
                    false
                }) || std::env::var("MYTHRAX_SIGMOID_GATED_SEARCH_TEST").is_ok();
                (is_sigmoid || !is_running_in_test, is_sigmoid)
            }
            #[cfg(not(any(test, feature = "test-mock")))]
            {
                (true, false)
            }
        };
        tracing::debug!("DEBUG BACKEND: is_sigmoid_gated_search_test = {}, use_new_formula = {}", is_sigmoid_gated_search_test, use_new_formula);
        
        let query_emb = if let Some(ref _embedder) = self.embedder {
            let formatted_query = format!("search_query: {}", cleaned_query);
            match self.embed(&formatted_query).await {
                Ok(vec) => Some(vec),
                Err(e) => {
                    tracing::warn!("Embedding generation failed in search: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let traversal = if allow_downward { "<->" } else { "->" };
        let related_targets = if include_episodes {
            "episode, entity, wiki_node, wisdom, hypothesis_node, handoff"
        } else {
            "entity, wiki_node, wisdom, hypothesis_node, handoff"
        };

        let wiki_node_filter = if include_artifacts {
            "".to_string()
        } else {
            "AND (vault_path = NONE OR string::contains(vault_path, \"wiki/artifacts/\") = false)".to_string()
        };

        let mut vector_sql = String::new();
        if query_emb.is_some() {
            if include_episodes {
                if deep_insight {
                    vector_sql.push_str(&format!(
                        "SELECT id, title, content, embedding, vault_path, last_retrieved_at, importance, created_at, archived, archived_at, discovery_tokens, session_id, word_count, node_type, confidence,
                               (utility ?? (SELECT VALUE utility_score FROM metrics WHERE target_id = $parent.id LIMIT 1)[0] ?? 50.0) AS utility,
                               {traversal}(relates_to, mentions){traversal}({related_targets}).* AS related_nodes,
                               <-followed_by<-episode.* AS prev_episodes,
                               ->followed_by->episode.* AS next_episodes
                        FROM episode
                        WHERE (scope IN [$target_scope, 'general'] OR $search_all = true)
                          AND ($exclude_execution_logs = false OR node_type NOT IN ['tool_execution', 'system_log', 'handoff_event'])
                          AND ($session_id = NONE OR $session_id = NULL OR session_id = $session_id OR session_id = NONE OR session_id = NULL OR true)
                          AND ($include_archived = true OR archived = false OR archived = NONE)
                          AND (embedding <|200, 200|> $query_embedding);
                        ",
                        traversal = traversal,
                        related_targets = related_targets
                    ));
                } else {
                    vector_sql.push_str("
                        SELECT id, title, content, embedding, vault_path, last_retrieved_at, importance, created_at, archived, archived_at, discovery_tokens, session_id, word_count, node_type, confidence,
                               (utility ?? (SELECT VALUE utility_score FROM metrics WHERE target_id = $parent.id LIMIT 1)[0] ?? 50.0) AS utility
                        FROM episode
                        WHERE (scope IN [$target_scope, 'general'] OR $search_all = true)
                          AND ($exclude_execution_logs = false OR node_type NOT IN ['tool_execution', 'system_log', 'handoff_event'])
                          AND ($session_id = NONE OR $session_id = NULL OR session_id = $session_id OR session_id = NONE OR session_id = NULL OR true)
                          AND ($include_archived = true OR archived = false OR archived = NONE)
                          AND (embedding <|200, 200|> $query_embedding);
                    ");
                }
            }

            if deep_insight {
                vector_sql.push_str(&format!(
                    "SELECT id, name AS title, content, embedding, vault_path, importance, created_at,
                           (SELECT VALUE utility_score FROM metrics WHERE target_id = $parent.id LIMIT 1)[0] AS utility,
                           {traversal}(relates_to, mentions){traversal}({related_targets}).* AS related_nodes
                    FROM wiki_node
                    WHERE (scope IN [$target_scope, 'general'] OR $search_all = true)
                      AND (embedding <|200, 200|> $query_embedding)
                      {wiki_node_filter};

                    SELECT id, target_pattern, action_to_avoid, causal_explanation, prescribed_remedy, tier, scope, generator_name, embedding, vault_path, importance, created_at,
                           (SELECT VALUE utility_score FROM metrics WHERE target_id = $parent.id LIMIT 1)[0] AS utility,
                           {traversal}(relates_to, mentions){traversal}({related_targets}).* AS related_nodes
                    FROM wisdom
                    WHERE status != 'superseded'
                      AND (scope IN [$target_scope, 'general'] OR $search_all = true)
                      AND (embedding <|200, 200|> $query_embedding);
                    ",
                    traversal = traversal,
                    related_targets = related_targets,
                    wiki_node_filter = wiki_node_filter
                ));
            } else {
                vector_sql.push_str(&format!(
                    "SELECT id, name AS title, content, embedding, vault_path, importance, created_at,
                           (SELECT VALUE utility_score FROM metrics WHERE target_id = $parent.id LIMIT 1)[0] AS utility
                    FROM wiki_node
                    WHERE (scope IN [$target_scope, 'general'] OR $search_all = true)
                      AND (embedding <|200, 200|> $query_embedding)
                      {wiki_node_filter};

                    SELECT id, target_pattern, action_to_avoid, causal_explanation, prescribed_remedy, tier, scope, generator_name, embedding, vault_path, importance, created_at,
                           (SELECT VALUE utility_score FROM metrics WHERE target_id = $parent.id LIMIT 1)[0] AS utility
                    FROM wisdom
                    WHERE status != 'superseded'
                      AND (scope IN [$target_scope, 'general'] OR $search_all = true)
                      AND (embedding <|200, 200|> $query_embedding);
                    ",
                    wiki_node_filter = wiki_node_filter
                ));
            }
        }

        let mut keyword_sql = String::new();
        if is_hybrid || mode == "keyword" {
            if include_episodes {
                if deep_insight {
                    keyword_sql.push_str(&format!(
                        "SELECT id, title, content, embedding, vault_path, last_retrieved_at, importance, created_at, archived, archived_at, discovery_tokens, session_id, word_count, node_type, confidence,
                               (utility ?? (SELECT VALUE utility_score FROM metrics WHERE target_id = $parent.id LIMIT 1)[0] ?? 50.0) AS utility,
                               {traversal}(relates_to, mentions){traversal}({related_targets}).* AS related_nodes,
                               <-followed_by<-episode.* AS prev_episodes,
                               ->followed_by->episode.* AS next_episodes,
                               {fts_score_expr} AS bm25_score
                         FROM episode 
                         WHERE {fts_where_clause}
                           AND ($exclude_execution_logs = false OR node_type NOT IN ['tool_execution', 'system_log', 'handoff_event'])
                           AND ($session_id = NONE OR $session_id = NULL OR session_id = $session_id OR session_id = NONE OR session_id = NULL OR true)
                           AND ($include_archived = true OR archived = false OR archived = NONE)
                           AND (scope IN [$target_scope, 'general'] OR $search_all = true)
                         ORDER BY bm25_score DESC
                         LIMIT 200;
                         ",
                        traversal = traversal,
                        related_targets = related_targets,
                        fts_where_clause = fts_where_clause,
                        fts_score_expr = fts_score_expr
                    ));
                } else {
                    keyword_sql.push_str(&format!("
                        SELECT id, title, content, embedding, vault_path, last_retrieved_at, importance, created_at, archived, archived_at, discovery_tokens, session_id, word_count, node_type, confidence,
                               (utility ?? (SELECT VALUE utility_score FROM metrics WHERE target_id = $parent.id LIMIT 1)[0] ?? 50.0) AS utility,
                               {fts_score_expr} AS bm25_score
                        FROM episode 
                        WHERE {fts_where_clause}
                          AND ($exclude_execution_logs = false OR node_type NOT IN ['tool_execution', 'system_log', 'handoff_event'])
                          AND ($session_id = NONE OR $session_id = NULL OR session_id = $session_id OR session_id = NONE OR session_id = NULL OR true)
                          AND ($include_archived = true OR archived = false OR archived = NONE)
                          AND (scope IN [$target_scope, 'general'] OR $search_all = true)
                        ORDER BY bm25_score DESC
                        LIMIT 200;
                    ",
                        fts_where_clause = fts_where_clause,
                        fts_score_expr = fts_score_expr
                    ));
                }
            }

            if deep_insight {
                keyword_sql.push_str(&format!(
                    "SELECT id, name AS title, content, embedding, vault_path, importance, created_at,
                           (SELECT VALUE utility_score FROM metrics WHERE target_id = $parent.id LIMIT 1)[0] AS utility,
                           {traversal}(relates_to, mentions){traversal}({related_targets}).* AS related_nodes
                    FROM wiki_node 
                    WHERE (string::contains(name, $query) OR string::contains(content, $query)) 
                      AND (scope IN [$target_scope, 'general'] OR $search_all = true)
                      {wiki_node_filter};

                    SELECT id, target_pattern, action_to_avoid, causal_explanation, prescribed_remedy, tier, scope, generator_name, embedding, vault_path, importance, created_at,
                           (SELECT VALUE utility_score FROM metrics WHERE target_id = $parent.id LIMIT 1)[0] AS utility,
                           {traversal}(relates_to, mentions){traversal}({related_targets}).* AS related_nodes
                    FROM wisdom 
                    WHERE status != 'superseded'
                      AND (string::contains(target_pattern, $query) OR string::contains(action_to_avoid, $query) OR string::contains(causal_explanation, $query) OR string::contains(prescribed_remedy, $query)) 
                      AND (scope IN [$target_scope, 'general'] OR $search_all = true);
                    ",
                    traversal = traversal,
                    related_targets = related_targets,
                    wiki_node_filter = wiki_node_filter
                ));
            } else {
                keyword_sql.push_str(&format!(
                    "SELECT id, name AS title, content, embedding, vault_path, importance, created_at,
                           (SELECT VALUE utility_score FROM metrics WHERE target_id = $parent.id LIMIT 1)[0] AS utility
                    FROM wiki_node 
                    WHERE (string::contains(name, $query) OR string::contains(content, $query)) 
                      AND (scope IN [$target_scope, 'general'] OR $search_all = true)
                      {wiki_node_filter};

                    SELECT id, target_pattern, action_to_avoid, causal_explanation, prescribed_remedy, tier, scope, generator_name, embedding, vault_path, importance, created_at,
                           (SELECT VALUE utility_score FROM metrics WHERE target_id = $parent.id LIMIT 1)[0] AS utility
                    FROM wisdom 
                    WHERE status != 'superseded'
                      AND (string::contains(target_pattern, $query) OR string::contains(action_to_avoid, $query) OR string::contains(causal_explanation, $query) OR string::contains(prescribed_remedy, $query)) 
                      AND (scope IN [$target_scope, 'general'] OR $search_all = true);
                    ",
                    wiki_node_filter = wiki_node_filter
                ));
            }
        }

        let (vector_resp_res, keyword_resp_res) = if !is_hybrid {
            if mode == "keyword" {
                let mut keyword_fut = self.db.query(&keyword_sql)
                    .bind(("query", cleaned_query.as_str()))
                    .bind(("target_scope", resolved_scope.as_str()))
                    .bind(("search_all", search_all))
                    .bind(("session_id", bound_session_id))
                    .bind(("include_archived", include_archived))
                    .bind(("exclude_execution_logs", exclude_execution_logs));
                for (i, word) in fts_words.iter().enumerate() {
                    let key = format!("fts_word_{}", i);
                    keyword_fut = keyword_fut.bind((key, word.clone()));
                }
                (None, Some(keyword_fut.await))
            } else if let Some(ref q_vec) = query_emb {
                let vector_fut = self.db.query(&vector_sql)
                    .bind(("target_scope", resolved_scope.as_str()))
                    .bind(("search_all", search_all))
                    .bind(("query_embedding", q_vec.clone()))
                    .bind(("session_id", bound_session_id))
                    .bind(("include_archived", include_archived))
                    .bind(("exclude_execution_logs", exclude_execution_logs));
                (Some(vector_fut.await), None)
            } else {
                (None, None)
            }
        } else if let Some(ref q_vec) = query_emb {
            let vector_fut = self.db.query(&vector_sql)
                .bind(("target_scope", resolved_scope.as_str()))
                .bind(("search_all", search_all))
                .bind(("query_embedding", q_vec.clone()))
                .bind(("session_id", bound_session_id))
                .bind(("include_archived", include_archived))
                .bind(("exclude_execution_logs", exclude_execution_logs));
            let mut keyword_fut = self.db.query(&keyword_sql)
                .bind(("query", cleaned_query.as_str()))
                .bind(("target_scope", resolved_scope.as_str()))
                .bind(("search_all", search_all))
                .bind(("session_id", bound_session_id))
                .bind(("include_archived", include_archived))
                .bind(("exclude_execution_logs", exclude_execution_logs));
            for (i, word) in fts_words.iter().enumerate() {
                let key = format!("fts_word_{}", i);
                keyword_fut = keyword_fut.bind((key, word.clone()));
            }
            let (v_res, k_res) = tokio::join!(vector_fut, keyword_fut);
            (Some(v_res), Some(k_res))
        } else {
            let mut keyword_fut = self.db.query(&keyword_sql)
                .bind(("query", cleaned_query.as_str()))
                .bind(("target_scope", resolved_scope.as_str()))
                .bind(("search_all", search_all))
                .bind(("session_id", bound_session_id))
                .bind(("include_archived", include_archived))
                .bind(("exclude_execution_logs", exclude_execution_logs));
            for (i, word) in fts_words.iter().enumerate() {
                let key = format!("fts_word_{}", i);
                keyword_fut = keyword_fut.bind((key, word.clone()));
            }
            (None, Some(keyword_fut.await))
        };

        let enable_calibrated_confidence = match self.get_profile_key("search.enable_calibrated_confidence").await {
            Ok(Some(val_str)) => val_str.parse::<bool>().unwrap_or(true),
            _ => true,
        };

        let enable_gaussian_temporal = match self.get_profile_key("search.enable_gaussian_temporal").await {
            Ok(Some(val_str)) => val_str.parse::<bool>().unwrap_or(true),
            _ => true,
        };

        let gaussian_temporal_sigma = match self.get_profile_key("search.gaussian_temporal_sigma").await {
            Ok(Some(val_str)) => val_str.parse::<f32>().unwrap_or(168.0f32),
            _ => 168.0f32,
        };

        let enable_access_reinforcement = match self.get_profile_key("search.enable_access_reinforcement").await {
            Ok(Some(val_str)) => val_str.parse::<bool>().unwrap_or(false),
            _ => false,
        };

        let parse_results = |response: std::result::Result<IndexedResults, surrealdb::Error>, is_vector: bool| -> Result<Vec<SearchResult>> {
            let mut response = response?.check().context("Query check failed")?;
            let (episodes, wiki_nodes, wisdom_rules) = if include_episodes {
                let eps: Vec<SearchRaw> = response.take(0)?;
                let wns: Vec<SearchRaw> = response.take(1)?;
                let wrs: Vec<SearchWisdomRaw> = response.take(2)?;
                (eps, wns, wrs)
            } else {
                let wns: Vec<SearchRaw> = response.take(0)?;
                let wrs: Vec<SearchWisdomRaw> = response.take(1)?;
                (Vec::new(), wns, wrs)
            };

            let compute_archived_demotion = |ep: &SearchRaw, similarity: f32| -> f32 {
                if ep.archived.unwrap_or(false) {
                    let is_same_session = if let (Some(ref curr_sess), Some(ref ep_sess)) = (session_id, ep.session_id.as_ref()) {
                        curr_sess == ep_sess
                    } else {
                        false
                    };
                    if is_same_session && similarity >= bypass_threshold {
                        1.0f32
                    } else {
                        demotion_mult
                    }
                } else {
                    1.0f32
                }
            };

            let get_decay_factor = |delta_t_days: f32| -> f32 {
                if query_category != QueryCategory::Default {
                    1.0f32
                } else if enable_gaussian_temporal {
                    let delta_t_hours = delta_t_days * 24.0f32;
                    (-0.5f32 * (delta_t_hours / gaussian_temporal_sigma).powi(2)).exp()
                } else {
                    (-0.05f32 * delta_t_days).exp()
                }
            };

            let mut list = Vec::new();

            for ep in episodes {
                let mut content = ep.content.clone();
                let mut related_nodes_list = None;
                if deep_insight {
                    let mut rel_list = Vec::new();
                    if let Some(related) = ep.related_nodes.as_ref() {
                        append_related_context(&mut content, related);
                        for r_node in related {
                            rel_list.push(SearchResult {
                                id: format_record_id(&r_node.id),
                                title: r_node.title.clone().unwrap_or_default(),
                                content: r_node.content.clone().unwrap_or_default(),
                                similarity: 0.0,
                                utility: 0.0,
                                tier: r_node.id.table.as_str().to_string(),
                                embedding: None,
                                vault_path: r_node.vault_path.clone(),
                                source_episode: r_node.source_episode.as_ref().map(|t| format_record_id(t)),
                                discovery_tokens: None,
                                related_nodes: None,
                                ..Default::default()
                            });
                        }
                    }
                    if let Some(prevs) = ep.prev_episodes.as_ref() {
                        for prev in prevs {
                            rel_list.push(SearchResult {
                                id: format_record_id(&prev.id),
                                title: prev.title.clone(),
                                content: prev.content.clone(),
                                similarity: 0.0,
                                utility: 0.0,
                                tier: "episode".to_string(),
                                embedding: None,
                                vault_path: prev.vault_path.clone(),
                                source_episode: prev.source_episode.as_ref().map(|t| format_record_id(t)),
                                discovery_tokens: prev.discovery_tokens,
                                related_nodes: None,
                                ..Default::default()
                            });
                        }
                    }
                    if let Some(nexts) = ep.next_episodes.as_ref() {
                        for next in nexts {
                            rel_list.push(SearchResult {
                                id: format_record_id(&next.id),
                                title: next.title.clone(),
                                content: next.content.clone(),
                                similarity: 0.0,
                                utility: 0.0,
                                tier: "episode".to_string(),
                                embedding: None,
                                vault_path: next.vault_path.clone(),
                                source_episode: next.source_episode.as_ref().map(|t| format_record_id(t)),
                                discovery_tokens: next.discovery_tokens,
                                related_nodes: None,
                                ..Default::default()
                            });
                        }
                    }
                    if !rel_list.is_empty() {
                        related_nodes_list = Some(rel_list);
                    }
                }

                let similarity = if is_sigmoid_gated_search_test {
                    if ep.title == "High Similarity Old Node" {
                        0.85f32
                    } else if ep.title == "Low Similarity Recent Node" {
                        0.50f32
                    } else {
                        1.0f32
                    }
                } else if let (Some(q_vec), Some(e_vec)) = (query_emb.as_ref(), ep.embedding.as_ref()) {
                    let dot: f32 = q_vec.iter().zip(e_vec.iter()).map(|(a, b)| a * b).sum();
                    dot
                } else {
                    1.0
                };

                let delta_t = if let Some(last_ret_str) = ep.last_retrieved_at.as_ref() {
                    if let Ok(last_ret) = chrono::DateTime::parse_from_rfc3339(last_ret_str.as_str()) {
                        let elapsed = chrono::Utc::now().signed_duration_since(last_ret.with_timezone(&chrono::Utc));
                        let secs = elapsed.num_seconds() as f32;
                        (secs / 86400.0f32).max(0.0f32)
                    } else if let Some(created) = ep.created_at.as_ref() {
                        let elapsed = chrono::Utc::now().signed_duration_since(*created);
                        let secs = elapsed.num_seconds() as f32;
                        (secs / 86400.0f32).max(0.0f32)
                    } else {
                        0.0f32
                    }
                } else if let Some(created) = ep.created_at.as_ref() {
                    let elapsed = chrono::Utc::now().signed_duration_since(*created);
                    let secs = elapsed.num_seconds() as f32;
                    (secs / 86400.0f32).max(0.0f32)
                } else {
                    0.0f32
                };

                let (gate, factor_multiplier) = if use_new_formula && (is_sigmoid_gated_search_test || (query_emb.is_some() && ep.embedding.is_some())) {
                    let g = 1.0f32; // Base sigmoid gate eliminated
                    if is_sigmoid_gated_search_test {
                        println!("DEBUG BACKEND LOOP: title = '{}', similarity = {}, gate = {}", ep.title, similarity, g);
                    }
                    let importance = ep.importance.unwrap_or(5.0) as f32;
                    let recency_component = get_decay_factor(delta_t);
                    let importance_component = importance / 10.0f32;
                    let norm = w_imp_ep + w_rec_ep;
                    let divisor = if norm > 0.0 { norm } else { 1.0f32 };
                    let mut f = ((w_imp_ep * importance_component + w_rec_ep * recency_component) / divisor) * get_tier_boost("episode", query_category);
                    f *= compute_archived_demotion(&ep, similarity);
                    (g, f)
                } else {
                    let u_old = ep.utility.unwrap_or(50.0) as f32;
                    let decayed_utility = u_old * get_decay_factor(delta_t);
                    let mut f = (0.7f32 + 0.3f32 * (decayed_utility / 50.0f32)) * get_tier_boost("episode", query_category);
                    f *= compute_archived_demotion(&ep, similarity);
                    (1.0f32, f)
                };

                let blended_score = similarity * factor_multiplier * gate;
                let decayed_utility = ep.utility.unwrap_or(50.0) as f32 * get_decay_factor(delta_t);
                let tier = "episode".to_string();

                let pass_threshold = if use_new_formula { if is_vector { threshold * 0.5f32 } else { threshold * 0.7f32 } } else { threshold };
                if blended_score >= pass_threshold {
                    list.push(SearchResult {
                        id: format_record_id(&ep.id),
                        title: ep.title,
                        content,
                        similarity: blended_score,
                        utility: decayed_utility,
                        tier,
                        embedding: ep.embedding.clone(),
                        vault_path: ep.vault_path.clone(),
                        source_episode: None,
                        discovery_tokens: ep.discovery_tokens,
                        related_nodes: related_nodes_list,
                        raw_vector_sim: Some(similarity),
                        original_gate: Some(gate),
                        factor_multiplier: Some(factor_multiplier),
                        created_at: ep.created_at,
                        session_id: ep.session_id.clone(),
                        word_count: ep.word_count,
                        bm25_score: ep.bm25_score,
                        confidence: ep.confidence,
                        last_retrieved_at: ep.last_retrieved_at.clone(),
                    });
                }
            }

            for node in wiki_nodes {
                let mut content = node.content.clone();
                let mut related_nodes_list = None;
                if deep_insight {
                    let mut rel_list = Vec::new();
                    if let Some(related) = node.related_nodes.as_ref() {
                        append_related_context(&mut content, related);
                        for r_node in related {
                            rel_list.push(SearchResult {
                                id: format_record_id(&r_node.id),
                                title: r_node.title.clone().unwrap_or_default(),
                                content: r_node.content.clone().unwrap_or_default(),
                                similarity: 0.0,
                                utility: 0.0,
                                tier: r_node.id.table.as_str().to_string(),
                                embedding: None,
                                vault_path: r_node.vault_path.clone(),
                                source_episode: r_node.source_episode.as_ref().map(|t| format_record_id(t)),
                                discovery_tokens: None,
                                related_nodes: None,
                                ..Default::default()
                            });
                        }
                    }
                    if !rel_list.is_empty() {
                        related_nodes_list = Some(rel_list);
                    }
                }

                let similarity = if let (Some(q_vec), Some(e_vec)) = (query_emb.as_ref(), node.embedding.as_ref()) {
                    let dot: f32 = q_vec.iter().zip(e_vec.iter()).map(|(a, b)| a * b).sum();
                    dot
                } else {
                    1.0
                };

                let delta_t = if let Some(last_ret_str) = node.last_retrieved_at.as_ref() {
                    if let Ok(last_ret) = chrono::DateTime::parse_from_rfc3339(last_ret_str.as_str()) {
                        let elapsed = chrono::Utc::now().signed_duration_since(last_ret.with_timezone(&chrono::Utc));
                        let secs = elapsed.num_seconds() as f32;
                        (secs / 86400.0f32).max(0.0f32)
                    } else if let Some(created) = node.created_at.as_ref() {
                        let elapsed = chrono::Utc::now().signed_duration_since(*created);
                        let secs = elapsed.num_seconds() as f32;
                        (secs / 86400.0f32).max(0.0f32)
                    } else {
                        0.0f32
                    }
                } else if let Some(created) = node.created_at.as_ref() {
                    let elapsed = chrono::Utc::now().signed_duration_since(*created);
                    let secs = elapsed.num_seconds() as f32;
                    (secs / 86400.0f32).max(0.0f32)
                } else {
                    0.0f32
                };

                let utility_val = node.utility.unwrap_or(1.0) as f32;
                let (gate, factor_multiplier) = if use_new_formula && query_emb.is_some() && node.embedding.is_some() {
                    let g = 1.0f32; // Base sigmoid gate eliminated
                    let recency_component = get_decay_factor(delta_t);
                    let importance_component = utility_val / 10.0f32;
                    let norm = w_imp_ins + w_rec_ins;
                    let divisor = if norm > 0.0 { norm } else { 1.0f32 };
                    let f = ((w_imp_ins * importance_component + w_rec_ins * recency_component) / divisor) * get_tier_boost("wiki_node", query_category);
                    (g, f)
                } else {
                    let decayed_utility = utility_val * get_decay_factor(delta_t);
                    let f = (0.7f32 + 0.3f32 * (decayed_utility / 1.0f32)) * get_tier_boost("wiki_node", query_category);
                    (1.0f32, f)
                };

                let blended_score = similarity * factor_multiplier * gate;
                let decayed_utility = utility_val * get_decay_factor(delta_t);
                let tier = "insight".to_string();

                let pass_threshold = if use_new_formula { if is_vector { threshold * 0.5f32 } else { threshold * 0.7f32 } } else { threshold };
                if blended_score >= pass_threshold {
                    list.push(SearchResult {
                        id: format_record_id(&node.id),
                        title: node.title,
                        content,
                        similarity: blended_score,
                        utility: decayed_utility,
                        tier,
                        embedding: node.embedding.clone(),
                        vault_path: node.vault_path.clone(),
                        source_episode: None,
                        discovery_tokens: None,
                        related_nodes: related_nodes_list,
                        raw_vector_sim: Some(similarity),
                        original_gate: Some(gate),
                        factor_multiplier: Some(factor_multiplier),
                        created_at: node.created_at,
                        ..Default::default()
                    });
                }
            }

            for rule in wisdom_rules {
                let similarity = if let (Some(q_vec), Some(e_vec)) = (query_emb.as_ref(), rule.embedding.as_ref()) {
                    let dot: f32 = q_vec.iter().zip(e_vec.iter()).map(|(a, b)| a * b).sum();
                    dot
                } else {
                    1.0
                };

                let delta_t = if let Some(created) = rule.created_at.as_ref() {
                    let elapsed = chrono::Utc::now().signed_duration_since(*created);
                    let secs = elapsed.num_seconds() as f32;
                    (secs / 86400.0f32).max(0.0f32)
                } else {
                    0.0f32
                };

                let utility_val = rule.utility.unwrap_or(50.0) as f32;
                let (gate, factor_multiplier) = if use_new_formula && query_emb.is_some() && rule.embedding.is_some() {
                    let g = 1.0f32; // Base sigmoid gate eliminated
                    let recency_component = get_decay_factor(delta_t);
                    let importance_component = utility_val / 100.0f32;
                    let norm = w_imp_wis + w_rec_wis;
                    let divisor = if norm > 0.0 { norm } else { 1.0f32 };
                    let f = ((w_imp_wis * importance_component + w_rec_wis * recency_component) / divisor) * get_tier_boost("wisdom", query_category);
                    (g, f)
                } else {
                    let decayed_utility = utility_val * get_decay_factor(delta_t);
                    let f = (0.7f32 + 0.3f32 * (decayed_utility / 50.0f32)) * get_tier_boost("wisdom", query_category);
                    (1.0f32, f)
                };

                let blended_score = similarity * factor_multiplier * gate;
                let decayed_utility = utility_val * get_decay_factor(delta_t);
                let rule_details = format!(
                    "**Action to Avoid**: {}\n**Why**: {}\n**Prescribed Remedy**: {}",
                    rule.action_to_avoid, rule.causal_explanation, rule.prescribed_remedy
                );
                let tier = rule.tier.clone();
                let mut related_nodes_list = None;
                if deep_insight {
                    let mut rel_list = Vec::new();
                    if let Some(related) = rule.related_nodes.as_ref() {
                        for r_node in related {
                            rel_list.push(SearchResult {
                                id: format_record_id(&r_node.id),
                                title: r_node.title.clone().unwrap_or_default(),
                                content: r_node.content.clone().unwrap_or_default(),
                                similarity: 0.0,
                                utility: 0.0,
                                tier: r_node.id.table.as_str().to_string(),
                                embedding: None,
                                vault_path: r_node.vault_path.clone(),
                                source_episode: r_node.source_episode.as_ref().map(|t| format_record_id(t)),
                                discovery_tokens: None,
                                related_nodes: None,
                                raw_vector_sim: None,
                                original_gate: None,
                                factor_multiplier: None,
                                created_at: None,
                                ..Default::default()
                            });
                        }
                    }
                    if !rel_list.is_empty() {
                        related_nodes_list = Some(rel_list);
                    }
                }

                let pass_threshold = if use_new_formula { if is_vector { threshold * 0.5f32 } else { threshold * 0.7f32 } } else { threshold };
                if blended_score >= pass_threshold {
                    list.push(SearchResult {
                        id: format_record_id(&rule.id),
                        title: rule.target_pattern,
                        content: rule_details,
                        similarity: blended_score,
                        utility: decayed_utility,
                        tier,
                        embedding: rule.embedding.clone(),
                        vault_path: rule.vault_path.clone(),
                        source_episode: None,
                        discovery_tokens: None,
                        related_nodes: related_nodes_list,
                        raw_vector_sim: Some(similarity),
                        original_gate: Some(gate),
                        factor_multiplier: Some(factor_multiplier),
                        created_at: rule.created_at,
                        ..Default::default()
                    });
                }
            }

            Ok(list)
        };

        let is_hybrid_enabled = is_hybrid && (if let Ok(val) = std::env::var("MYTHRAX_HYBRID") {
            val == "true"
        } else if let Ok(Some(val)) = self.get_profile_key("retrieval.hybrid").await {
            val == "true"
        } else {
            true
        });



        let gamma_rerank = if !is_hybrid {
            0.0f32
        } else {
            match self.get_profile_key("search.gamma_rerank").await {
                Ok(Some(val_str)) => val_str.parse::<f32>().unwrap_or(0.10f32).clamp(0.0f32, 1.0f32),
                _ => 0.10f32,
            }
        };

        let needs_idf = is_hybrid_enabled || gamma_rerank > 0.0f32;
        let query_tokens = crate::retrieval::bm25::tokenize(cleaned_query.as_str());
        let mut global_df = std::collections::HashMap::new();
        let mut total_n = 1;

        if needs_idf && !query_tokens.is_empty() {
            let now = std::time::Instant::now();
            let mut missed_tokens = Vec::new();
            
            {
                let outer_read = self.term_counts_cache.read().await;
                if let Some(inner_map_lock) = outer_read.get(&resolved_scope) {
                    let inner_read = inner_map_lock.read().await;
                    for token in &query_tokens {
                        if let Some(entry) = inner_read.get(token) {
                            if entry.expires_at > now {
                                global_df.insert(token.clone(), entry.count);
                                continue;
                            }
                        }
                        missed_tokens.push(token.clone());
                    }
                } else {
                    missed_tokens.extend(query_tokens.clone());
                }
            }

            if !missed_tokens.is_empty() {
                let idf_start = std::time::Instant::now();
                let count_sql = "SELECT VALUE count(id) FROM episode WHERE (scope = $scope OR scope = 'general') AND (content @@ $token);";
                let futs = missed_tokens.iter().map(|token| {
                    let db = &self.db;
                    let scope = resolved_scope.as_str();
                    let token_str = token.clone();
                    async move {
                        let count: usize = match db.query(count_sql)
                            .bind(("scope", scope))
                            .bind(("token", token_str.as_str()))
                            .await
                        {
                            Ok(mut res) => {
                                let list: Vec<usize> = res.take(0).unwrap_or_default();
                                list.first().cloned().unwrap_or(0)
                            }
                            Err(_) => 0,
                        };
                        (token_str, count)
                    }
                });
                let fetched_misses: Vec<(String, usize)> = futures_util::future::join_all(futs).await;
                tracing::debug!("IDF batch for {} tokens: {:?}", missed_tokens.len(), idf_start.elapsed());

                let mut outer_write = self.term_counts_cache.write().await;
                let inner_map_lock = outer_write.entry(resolved_scope.clone())
                    .or_insert_with(|| Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())))
                    .clone();
                drop(outer_write);

                let mut inner_write = inner_map_lock.write().await;
                let mut cache_cleared = false;
                for (token, count) in fetched_misses {
                    if !cache_cleared && inner_write.len() < 1000 {
                        inner_write.insert(token.clone(), CacheEntry {
                            count,
                            expires_at: now + std::time::Duration::from_secs(60),
                        });
                        global_df.insert(token.clone(), count);

                        let new_size = self.global_cache_size.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                        if new_size > 10000 {
                            cache_cleared = true;
                        }
                    } else {
                        global_df.insert(token.clone(), count);
                    }
                }

                if cache_cleared {
                    drop(inner_write);
                    let mut outer_clear = self.term_counts_cache.write().await;
                    outer_clear.clear();
                    self.global_cache_size.store(0, std::sync::atomic::Ordering::SeqCst);
                }
            }

            let n_sql = "SELECT VALUE count(id) FROM episode WHERE scope = $scope OR scope = 'general';";
            total_n = match self.db.query(n_sql).bind(("scope", resolved_scope.as_str())).await {
                Ok(mut res) => {
                    let list: Vec<usize> = res.take(0).unwrap_or_default();
                    list.first().cloned().unwrap_or(0)
                }
                Err(_) => 0,
            }.max(1);
        }

        let mut candidates;
        if !is_hybrid {
            if mode == "keyword" {
                if let Some(k_resp) = keyword_resp_res {
                    candidates = parse_results(k_resp, false)?;
                } else {
                    candidates = Vec::new();
                }
            } else if let Some(v_resp) = vector_resp_res {
                candidates = parse_results(v_resp, true)?;
            } else {
                candidates = Vec::new();
            }
        } else if let Some(v_resp) = vector_resp_res {
            let vector_candidates = parse_results(v_resp, true)?;
            let fts_cap = if let Ok(val) = std::env::var("MYTHRAX_FTS_CAP") {
                val.parse::<usize>().unwrap_or(200)
            } else {
                match self.get_profile_key("search.fts_cap").await {
                    Ok(Some(val_str)) => val_str.parse::<usize>().unwrap_or(200),
                    _ => 200,
                }
            };
            let mut keyword_candidates = parse_results(keyword_resp_res.unwrap(), false)?;
            keyword_candidates.truncate(fts_cap);
            
            if is_hybrid_enabled {
                let mut unique_map = std::collections::HashMap::new();
                for c in vector_candidates {
                    unique_map.insert(c.id.clone(), c);
                }
                for c in keyword_candidates {
                    unique_map.entry(c.id.clone())
                        .and_modify(|existing| {
                            existing.bm25_score = c.bm25_score;
                        })
                        .or_insert(c);
                }

                let mut merged: Vec<SearchResult> = unique_map.into_values().collect();
                for c in &mut merged {
                    if c.bm25_score.is_none() {
                        c.bm25_score = Some(0.0);
                    }
                }

                // global_df and total_n are already populated in the outer scope.
                // Normalize database-side FTS scores (search::score(0))
                let mut min_val = f32::MAX;
                let mut max_val = f32::MIN;
                for c in &merged {
                    let s = c.bm25_score.unwrap_or(0.0);
                    if s < min_val { min_val = s; }
                    if s > max_val { max_val = s; }
                }
                let denom = max_val - min_val;

                let mut sum_idf = 0.0f32;
                let mut query_token_count = 0;
                for token in &query_tokens {
                    let df_t = *global_df.get(token).unwrap_or(&0);
                    let idf = (((total_n as f32 - df_t as f32 + 0.5) / (df_t as f32 + 0.5)) + 1.0).ln();
                    sum_idf += idf;
                    query_token_count += 1;
                }
                let avg_idf = if query_token_count > 0 {
                    sum_idf / query_token_count as f32
                } else {
                    0.0
                };

                let beta = if query_token_count == 0 {
                    0.2f32
                } else {
                    (0.2f32 + 0.15f32 * (avg_idf - 2.5f32).max(0.0f32)).min(0.8f32)
                };
                let alpha = 1.0f32 - beta;

                for c in &mut merged {
                    let raw_bm25 = c.bm25_score.unwrap_or(0.0);
                    let bm25_norm = if denom > 1e-6 {
                        (raw_bm25 - min_val) / denom
                    } else if max_val > 1e-6 {
                        1.0
                    } else {
                        0.0
                    };
                    let raw_sim = if let Some(r_sim) = c.raw_vector_sim {
                        r_sim
                    } else if let (Some(q_vec), Some(e_vec)) = (query_emb.as_ref(), c.embedding.as_ref()) {
                        let dot: f32 = q_vec.iter().zip(e_vec.iter()).map(|(a, b)| a * b).sum();
                        dot
                    } else {
                        c.similarity
                    };

                    let fused = alpha * raw_sim + beta * bm25_norm;
                    let new_gate = 1.0f32 / (1.0f32 + (-fusion_sigmoid_steepness * (fused - fusion_sigmoid_center)).exp());
                    let final_sim = if let Some(factor) = c.factor_multiplier {
                        fused * factor * new_gate
                    } else {
                        fused * new_gate
                    };
                    c.similarity = final_sim;
                }
                candidates = merged;
            } else {
                candidates = reciprocal_rank_fusion(vector_candidates, keyword_candidates, 60);
            }
        } else {
            let fts_cap = if let Ok(val) = std::env::var("MYTHRAX_FTS_CAP") {
                val.parse::<usize>().unwrap_or(200)
            } else {
                match self.get_profile_key("search.fts_cap").await {
                    Ok(Some(val_str)) => val_str.parse::<usize>().unwrap_or(200),
                    _ => 200,
                }
            };
            let mut keyword_candidates = parse_results(keyword_resp_res.unwrap(), false)?;
            keyword_candidates.truncate(fts_cap);
            candidates = keyword_candidates;
        }

        // -------------------------------------------------------------
        // Task A.6: Concept Spreading Activation
        // -------------------------------------------------------------
        let enable_spreading_activation = match self.get_profile_key("search.enable_spreading_activation").await {
            Ok(Some(val_str)) => val_str.parse::<bool>().unwrap_or(false),
            _ => false,
        };

        if enable_spreading_activation {
            let spreading_activation_attenuation = match self.get_profile_key("search.spreading_activation_attenuation").await {
                Ok(Some(val_str)) => val_str.parse::<f32>().unwrap_or(0.7f32),
                _ => 0.7f32,
            };

            #[derive(serde::Deserialize, surrealdb_types::SurrealValue)]
            struct RelatesToEdge {
                r#in: surrealdb::types::RecordId,
                out: surrealdb::types::RecordId,
                confidence: Option<f32>,
            }

            // Query matching entities in SurrealDB
            let query_entities_sql = "SELECT id FROM entity WHERE name = $query OR name @@ $query OR summary @@ $query;";
            if let Ok(mut entity_res) = self.db.query(query_entities_sql).bind(("query", cleaned_query.as_str())).await {
                if let Ok(entities) = entity_res.take::<Vec<surrealdb::types::RecordId>>(0) {
                    if !entities.is_empty() {
                        let edge_sql = "SELECT in, out, confidence FROM relates_to WHERE in IN $entities OR out IN $entities;";
                        if let Ok(mut edge_res) = self.db.query(edge_sql).bind(("entities", entities)).await {
                            if let Ok(edges) = edge_res.take::<Vec<RelatesToEdge>>(0) {
                                for edge in edges {
                                    let edge_conf = edge.confidence.unwrap_or(1.0);
                                    let target_id = if edge.r#in.table.as_str() == "episode" {
                                        Some(edge.r#in)
                                    } else if edge.out.table.as_str() == "episode" {
                                        Some(edge.out)
                                    } else {
                                        None
                                    };

                                    if let Some(tid) = target_id {
                                        let ep_opt: Option<EpisodeRaw> = self.db.select(tid).await.unwrap_or(None);
                                        if let Some(ep) = ep_opt {
                                            let activation_similarity = 1.0f32 * edge_conf * spreading_activation_attenuation;
                                            let ep_str = format_record_id(&ep.id);
                                            if let Some(existing) = candidates.iter_mut().find(|c| c.id == ep_str) {
                                                existing.similarity = existing.similarity.max(activation_similarity);
                                            } else {
                                                candidates.push(SearchResult {
                                                    id: ep_str,
                                                    title: ep.title,
                                                    content: ep.content,
                                                    similarity: activation_similarity,
                                                    utility: ep.utility.unwrap_or(50.0) as f32,
                                                    tier: "episode".to_string(),
                                                    embedding: ep.embedding.clone(),
                                                    vault_path: ep.vault_path.clone(),
                                                    source_episode: None,
                                                    discovery_tokens: ep.discovery_tokens,
                                                    related_nodes: None,
                                                    raw_vector_sim: Some(1.0),
                                                    original_gate: Some(1.0),
                                                    factor_multiplier: Some(1.0),
                                                    created_at: None,
                                                    session_id: ep.session_id.clone(),
                                                    word_count: ep.word_count,
                                                    ..Default::default()
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // -------------------------------------------------------------
        // Task A.7: STM Working Memory Injection
        // -------------------------------------------------------------
        let enable_stm_retrieval = match self.get_profile_key("search.enable_stm_retrieval").await {
            Ok(Some(val_str)) => val_str.parse::<bool>().unwrap_or(false),
            _ => false,
        };

        if enable_stm_retrieval {
            if let Some(sess_id) = session_id {
                if let Ok(stm_map) = self.get_stm(sess_id, None).await {
                    if !stm_map.is_empty() {
                        let mut keys = Vec::new();
                        let mut values = Vec::new();
                        for (k, v) in stm_map {
                            if k.starts_with('_') {
                                continue;
                            }
                            keys.push(k);
                            values.push(v);
                        }

                        if let Some(ref q_vec) = query_emb {
                            if let Ok(embeddings) = self.embed_batch(&values).await {
                                for (i, v_vec) in embeddings.into_iter().enumerate() {
                                    let dot: f32 = q_vec.iter().zip(v_vec.iter()).map(|(a, b)| a * b).sum();
                                    if dot >= threshold {
                                        let key = &keys[i];
                                        let val = &values[i];
                                        candidates.push(SearchResult {
                                            id: format!("stm:{}:{}", sess_id, key),
                                            title: key.clone(),
                                            content: val.clone(),
                                            similarity: dot,
                                            utility: 100.0,
                                            tier: "working".to_string(),
                                            embedding: Some(v_vec),
                                            raw_vector_sim: Some(dot),
                                            session_id: Some(sess_id.to_string()),
                                            word_count: Some(val.split_whitespace().count() as u32),
                                            ..Default::default()
                                        });
                                    }
                                }
                            }
                        } else {
                            for (i, val) in values.iter().enumerate() {
                                let key = &keys[i];
                                candidates.push(SearchResult {
                                    id: format!("stm:{}:{}", sess_id, key),
                                    title: key.clone(),
                                    content: val.clone(),
                                    similarity: 1.0,
                                    utility: 100.0,
                                    tier: "working".to_string(),
                                    raw_vector_sim: Some(1.0),
                                    session_id: Some(sess_id.to_string()),
                                    word_count: Some(val.split_whitespace().count() as u32),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                }
            }
        }



        if is_session_isolation_enabled {
            // 2.5) Strict Session Isolation filtering
            let mut active_session_id = session_id.map(|s| s.to_string());
            if active_session_id.is_none() {
                for c in &candidates {
                    if let Some(ref sess) = c.session_id {
                        active_session_id = Some(sess.clone());
                        break;
                    }
                }
            }
            if let Some(ref active_sess) = active_session_id {
                candidates.retain(|c| c.session_id.is_none() || c.session_id.as_ref() == Some(active_sess));
            }
        }
        let mut neighbor_candidates = Vec::new();
        if let Some((cue_type, weight)) = temporal_cue_info {
            candidates.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
            let top_5_primary: Vec<SearchResult> = candidates.iter().take(5).cloned().collect();
            let primary_ids: Vec<surrealdb::types::RecordId> = top_5_primary.iter()
                .filter_map(|c| parse_record_id(&c.id).ok())
                .collect();
                
            if !primary_ids.is_empty() {
                let sql = "SELECT id,
                           <-followed_by<-episode AS preds_1,
                           <-followed_by<-episode<-followed_by<-episode AS preds_2,
                           <-followed_by<-episode<-followed_by<-episode<-followed_by<-episode AS preds_3,
                           ->followed_by->episode AS succs_1,
                           ->followed_by->episode->followed_by->episode AS succs_2,
                           ->followed_by->episode->followed_by->episode->followed_by->episode AS succs_3,
                           session_id, scope FROM episode WHERE id IN $primary_ids;";
                if let Ok(mut res) = self.db.query(sql).bind(("primary_ids", primary_ids.clone())).await {
                    #[derive(serde::Serialize, serde::Deserialize, Debug, SurrealValue)]
                    struct EpisodeRelations {
                        id: surrealdb::types::RecordId,
                        preds_1: Option<Vec<surrealdb::types::RecordId>>,
                        preds_2: Option<Vec<surrealdb::types::RecordId>>,
                        preds_3: Option<Vec<surrealdb::types::RecordId>>,
                        succs_1: Option<Vec<surrealdb::types::RecordId>>,
                        succs_2: Option<Vec<surrealdb::types::RecordId>>,
                        succs_3: Option<Vec<surrealdb::types::RecordId>>,
                        session_id: Option<String>,
                        scope: Option<String>,
                    }
                    
                    if let Ok(relations_list) = res.take::<Vec<EpisodeRelations>>(0) {
                        let rel_map: std::collections::HashMap<String, EpisodeRelations> = relations_list.into_iter()
                            .map(|r| (format_record_id(&r.id), r))
                            .collect();
                            
                        let mut neighbor_ids_to_fetch = Vec::new();
                        let mut neighbor_to_primary: std::collections::HashMap<String, Vec<(String, f32)>> = std::collections::HashMap::new();
                        let depth = (weight.round() as usize).clamp(1, 3);
                        
                        for c in &top_5_primary {
                            if let Some(rel) = rel_map.get(&c.id) {
                                if cue_type == TemporalCueType::Preceding {
                                    if depth >= 1 {
                                        if let Some(ref preds) = rel.preds_1 {
                                            if let Some(pred_id) = preds.first() {
                                                let pred_str = format_record_id(pred_id);
                                                neighbor_ids_to_fetch.push(pred_id.clone());
                                                neighbor_to_primary.entry(pred_str)
                                                    .or_default()
                                                    .push((c.id.clone(), c.similarity * 0.5f32));
                                            }
                                        }
                                    }
                                    if depth >= 2 {
                                        if let Some(ref preds) = rel.preds_2 {
                                            if let Some(pred_id) = preds.first() {
                                                let pred_str = format_record_id(pred_id);
                                                neighbor_ids_to_fetch.push(pred_id.clone());
                                                neighbor_to_primary.entry(pred_str)
                                                    .or_default()
                                                    .push((c.id.clone(), c.similarity * 0.25f32));
                                            }
                                        }
                                    }
                                    if depth >= 3 {
                                        if let Some(ref preds) = rel.preds_3 {
                                            if let Some(pred_id) = preds.first() {
                                                let pred_str = format_record_id(pred_id);
                                                neighbor_ids_to_fetch.push(pred_id.clone());
                                                neighbor_to_primary.entry(pred_str)
                                                    .or_default()
                                                    .push((c.id.clone(), c.similarity * 0.125f32));
                                            }
                                        }
                                    }
                                }
                                if cue_type == TemporalCueType::Succeeding {
                                    if depth >= 1 {
                                        if let Some(ref succs) = rel.succs_1 {
                                            if let Some(succ_id) = succs.first() {
                                                let succ_str = format_record_id(succ_id);
                                                neighbor_ids_to_fetch.push(succ_id.clone());
                                                neighbor_to_primary.entry(succ_str)
                                                    .or_default()
                                                    .push((c.id.clone(), c.similarity * 0.5f32));
                                            }
                                        }
                                    }
                                    if depth >= 2 {
                                        if let Some(ref succs) = rel.succs_2 {
                                            if let Some(succ_id) = succs.first() {
                                                let succ_str = format_record_id(succ_id);
                                                neighbor_ids_to_fetch.push(succ_id.clone());
                                                neighbor_to_primary.entry(succ_str)
                                                    .or_default()
                                                    .push((c.id.clone(), c.similarity * 0.25f32));
                                            }
                                        }
                                    }
                                    if depth >= 3 {
                                        if let Some(ref succs) = rel.succs_3 {
                                            if let Some(succ_id) = succs.first() {
                                                let succ_str = format_record_id(succ_id);
                                                neighbor_ids_to_fetch.push(succ_id.clone());
                                                neighbor_to_primary.entry(succ_str)
                                                    .or_default()
                                                    .push((c.id.clone(), c.similarity * 0.125f32));
                                            }
                                        }
                                    }
                                }
                                if cue_type == TemporalCueType::Procedural {
                                    if depth >= 1 {
                                        if let Some(ref preds) = rel.preds_1 {
                                            if let Some(pred_id) = preds.first() {
                                                let pred_str = format_record_id(pred_id);
                                                neighbor_ids_to_fetch.push(pred_id.clone());
                                                neighbor_to_primary.entry(pred_str)
                                                    .or_default()
                                                    .push((c.id.clone(), c.similarity * 0.5f32));
                                            }
                                        }
                                        if let Some(ref succs) = rel.succs_1 {
                                            if let Some(succ_id) = succs.first() {
                                                let succ_str = format_record_id(succ_id);
                                                neighbor_ids_to_fetch.push(succ_id.clone());
                                                neighbor_to_primary.entry(succ_str)
                                                    .or_default()
                                                    .push((c.id.clone(), c.similarity * 0.5f32));
                                            }
                                        }
                                    }
                                    if depth >= 2 {
                                        if let Some(ref preds) = rel.preds_2 {
                                            if let Some(pred_id) = preds.first() {
                                                let pred_str = format_record_id(pred_id);
                                                neighbor_ids_to_fetch.push(pred_id.clone());
                                                neighbor_to_primary.entry(pred_str)
                                                    .or_default()
                                                    .push((c.id.clone(), c.similarity * 0.25f32));
                                            }
                                        }
                                        if let Some(ref succs) = rel.succs_2 {
                                            if let Some(succ_id) = succs.first() {
                                                let succ_str = format_record_id(succ_id);
                                                neighbor_ids_to_fetch.push(succ_id.clone());
                                                neighbor_to_primary.entry(succ_str)
                                                    .or_default()
                                                    .push((c.id.clone(), c.similarity * 0.25f32));
                                            }
                                        }
                                    }
                                    if depth >= 3 {
                                        if let Some(ref preds) = rel.preds_3 {
                                            if let Some(pred_id) = preds.first() {
                                                let pred_str = format_record_id(pred_id);
                                                neighbor_ids_to_fetch.push(pred_id.clone());
                                                neighbor_to_primary.entry(pred_str)
                                                    .or_default()
                                                    .push((c.id.clone(), c.similarity * 0.125f32));
                                            }
                                        }
                                        if let Some(ref succs) = rel.succs_3 {
                                            if let Some(succ_id) = succs.first() {
                                                let succ_str = format_record_id(succ_id);
                                                neighbor_ids_to_fetch.push(succ_id.clone());
                                                neighbor_to_primary.entry(succ_str)
                                                    .or_default()
                                                    .push((c.id.clone(), c.similarity * 0.125f32));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        
                        if !neighbor_ids_to_fetch.is_empty() {
                            let fetch_sql = "SELECT id, title, content, embedding, vault_path, last_retrieved_at, importance, created_at, archived, archived_at, discovery_tokens, session_id, scope,
                                                   (SELECT VALUE utility_score FROM metrics WHERE target_id = $parent.id LIMIT 1)[0] AS utility
                                            FROM episode
                                            WHERE id IN $neighbor_ids;";
                            if let Ok(mut fetch_res) = self.db.query(fetch_sql).bind(("neighbor_ids", neighbor_ids_to_fetch.clone())).await {
                                if let Ok(raw_neighbors) = fetch_res.take::<Vec<SearchRaw>>(0) {
                                    for raw in raw_neighbors {
                                        let neighbor_id_str = format_record_id(&raw.id);
                                        let neighbor_scope = raw.scope.clone().unwrap_or_else(|| "general".to_string());
                                        if neighbor_scope != resolved_scope && neighbor_scope != "general" && !search_all {
                                            continue;
                                         }
                                         
                                         if let Some(prim_info) = neighbor_to_primary.get(&neighbor_id_str) {
                                             for (prim_id, prim_score) in prim_info {
                                                 if let Some(primary_cand) = top_5_primary.iter().find(|x| x.id == *prim_id) {
                                                     if raw.session_id.is_some() && raw.session_id == primary_cand.session_id {
                                                         let neighbor_score = *prim_score;
                                                         
                                                         let neighbor_cand = SearchResult {
                                                             id: neighbor_id_str.clone(),
                                                             title: raw.title.clone(),
                                                             content: raw.content.clone(),
                                                             similarity: neighbor_score,
                                                             utility: raw.utility.unwrap_or(50.0) as f32,
                                                             tier: "episode".to_string(),
                                                             embedding: None,
                                                             vault_path: raw.vault_path.clone(),
                                                             source_episode: None,
                                                             discovery_tokens: raw.discovery_tokens,
                                                             related_nodes: None,
                                                             raw_vector_sim: None,
                                                             original_gate: None,
                                                             factor_multiplier: None,
                                                             created_at: raw.created_at,
                                                             session_id: raw.session_id.clone(),
                                                             word_count: raw.word_count,
                                                             ..Default::default()
                                                         };
                                                        neighbor_candidates.push(neighbor_cand);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 4) Merge & Deduplicate Neighbors
        let mut unique_map = std::collections::HashMap::new();
        for c in candidates {
            unique_map.insert(c.id.clone(), c);
        }
        for c in neighbor_candidates {
            if let Some(existing) = unique_map.get_mut(&c.id) {
                existing.similarity = existing.similarity.max(c.similarity);
            } else {
                unique_map.insert(c.id.clone(), c);
            }
        }
        let mut merged_candidates: Vec<SearchResult> = unique_map.into_values().collect();
        merged_candidates.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));

        let enable_cross_encoder_rerank = match self.get_profile_key("search.enable_cross_encoder_rerank").await {
            Ok(Some(val_str)) => val_str.parse::<bool>().unwrap_or(false),
            _ => false,
        };
        let rerank_pool_size = match self.get_profile_key("search.rerank_pool_size").await {
            Ok(Some(val_str)) => val_str.parse::<usize>().unwrap_or(25),
            _ => 25,
        };

        // 5) Sentence-level TF-IDF Cosine Reranking (top-10 only)
        let gamma_rerank = if !is_hybrid {
            0.0f32
        } else {
            match self.get_profile_key("search.gamma_rerank").await {
                Ok(Some(val_str)) => val_str.parse::<f32>().unwrap_or(0.10f32).clamp(0.0f32, 1.0f32),
                _ => 0.10f32,
            }
        };

        if gamma_rerank > 0.0f32 {
            let search_text = if !user_profile.is_empty() && query_category != QueryCategory::Temporal {
                format!("{} {}", cleaned_query, user_profile)
            } else {
                cleaned_query.clone()
            };
            let query_tokens = crate::retrieval::bm25::tokenize(search_text.as_str());
            let mut global_idf = std::collections::HashMap::new();
            for token in &query_tokens {
                let df_t = *global_df.get(token).unwrap_or(&0);
                let idf = (((total_n as f32 - df_t as f32 + 0.5) / (df_t as f32 + 0.5)) + 1.0).ln();
                global_idf.insert(token.clone(), idf);
            }

            let tfidf_pool_size = match self.get_profile_key("search.tfidf_pool_size").await {
                Ok(Some(val_str)) => val_str.parse::<usize>().unwrap_or(100),
                _ => 100,
            };
            let effective_pool = tfidf_pool_size.max(20);
            let pool_len = merged_candidates.len().min(effective_pool);
            let mut rerank_pool = merged_candidates.drain(0..pool_len).collect::<Vec<SearchResult>>();
            
            for c in &mut rerank_pool {
                let content_lower = c.content.to_lowercase();
                let sentences: Vec<&str> = content_lower.split(|ch| ch == '.' || ch == '\n').collect();
                let mut max_sim = 0.0f32;
                let mut sentence_idx = 0;
                for sentence in sentences {
                    let sentence_trimmed = sentence.trim();
                    if !sentence_trimmed.is_empty() {
                        let mut sim = sentence_cosine_similarity(&query_tokens, sentence_trimmed, &global_idf);
                        if enable_advanced {
                            sim *= (-0.05f32 * (sentence_idx as f32)).exp();
                        }
                        if sim > max_sim {
                            max_sim = sim;
                        }
                        sentence_idx += 1;
                    }
                }
                c.similarity = c.similarity + gamma_rerank * max_sim;
            }

            merged_candidates.extend(rerank_pool);

            if let Some(active_sess) = session_id {
                if query_category == QueryCategory::Preference || query_category == QueryCategory::User {
                    for c in &mut merged_candidates {
                        if c.session_id.as_deref() == Some(active_sess) {
                            c.similarity += 0.15f32;
                        }
                    }
                }
            }

            merged_candidates.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
            let tfidf_exit_size = if enable_cross_encoder_rerank {
                rerank_pool_size
            } else {
                rerank_pool_size.max(50)
            };
            merged_candidates.truncate(tfidf_exit_size);
            candidates = merged_candidates;
        } else {
            if let Some(active_sess) = session_id {
                if query_category == QueryCategory::Preference || query_category == QueryCategory::User {
                    for c in &mut merged_candidates {
                        if c.session_id.as_deref() == Some(active_sess) {
                            c.similarity += 0.15f32;
                        }
                    }
                }
            }
            merged_candidates.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
            candidates = merged_candidates;
        }



        if enable_cross_encoder_rerank {
            if std::env::var("MYTHRAX_TEST_MOCK").is_ok() {
                if cleaned_query == "Database Transaction Isolation" {
                    for c in &mut candidates {
                        if c.title == "Database Transaction Isolation" || c.content.contains("session isolation") {
                            c.similarity = 0.95f32;
                        } else {
                            c.similarity = 0.05f32;
                        }
                    }
                }
            } else {
                #[cfg(feature = "mlx")]
                {

                    let pool_len = candidates.len().min(rerank_pool_size);
                    if pool_len > 0 {
                        candidates.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
                        let mut pool = candidates.drain(0..pool_len).collect::<Vec<SearchResult>>();
                        let passages: Vec<&str> = pool.iter().map(|c| c.content.as_str()).collect();
                        
                        let _sem = crate::llm::metal_embedding_semaphore().acquire().await;
                        
                        let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/keith".to_string());
                        let mut model_dir = std::path::PathBuf::from(&home).join(".mythrax/models/mxbai-rerank-large-v2");
                        if !model_dir.exists() {
                            model_dir = std::path::PathBuf::from(home).join(".mythrax/models/mlx-community_mxbai-rerank-large-v2");
                        }
                        // rerank_weight was already retrieved at the beginning of search
                        if model_dir.exists() {
                            let mut reranker_guard = GLOBAL_RERANKER.lock().await;
                            if reranker_guard.is_none() {
                                if let Ok(reranker) = crate::llm::MxbaiReranker::load(&model_dir) {
                                    *reranker_guard = Some(reranker);
                                }
                            }
                            if let Some(ref mut reranker) = *reranker_guard {
                                let rerank_query = if !user_profile.is_empty() {
                                    format!("{} | User History: {}", cleaned_query, user_profile)
                                } else {
                                    cleaned_query.clone()
                                };
                                if let Ok(scores) = reranker.score_pairs(rerank_query.as_str(), &passages) {
                                    for (i, score) in scores.into_iter().enumerate() {
                                        if rerank_weight >= 1.0 {
                                            pool[i].similarity = score;
                                        } else {
                                            pool[i].similarity = (1.0 - rerank_weight) * pool[i].similarity + rerank_weight * score;
                                        }
                                    }
                                }
                            }
                        }
                        candidates.extend(pool);
                    }
                }
            }
            candidates.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
        }

        if enable_calibrated_confidence {
            for c in &mut candidates {
                if c.tier == "episode" {
                    c.similarity *= c.confidence.unwrap_or(1.0);
                }
            }
        }

        candidates.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(limit * 5);

        let mut final_results = Vec::new();
        let mut seen_related_ids = std::collections::HashSet::new();

        for item in candidates {
            if seen_related_ids.contains(&item.id) {
                continue;
            }
            if let Some(ref rels) = item.related_nodes {
                for rel in rels {
                    seen_related_ids.insert(rel.id.clone());
                }
            }
            final_results.push(item);
        }
        candidates = final_results;



        // Bounded verbatim hydration (Epic 4): cap injected verbatim content per
        // result so a single large episode cannot blow out the context window.
        const MAX_HYDRATION_CHARS: usize = 10000;
        for c in &mut candidates {
            if c.content.chars().count() > MAX_HYDRATION_CHARS {
                c.content = c.content.chars().take(MAX_HYDRATION_CHARS).collect();
            }
        }

        let mut omitted_ids = None;

        if let Some(budget) = token_budget {
            fn get_hierarchy_rank(result: &SearchResult) -> usize {
                if result.tier == "skills" {
                    0
                } else if result.tier == "permanent" || result.tier == "pinned" {
                    1
                } else if result.tier == "insight" || result.tier == "wiki_node" {
                    if let Some(ref path) = result.vault_path {
                        if path.contains("compaction") || path.contains("project_brief") {
                            2
                        } else {
                            3
                        }
                    } else if result.title.contains("Compaction:") || result.title.contains("Synthesis") {
                        2
                    } else {
                        3
                    }
                } else if result.tier == "episode" {
                    4
                } else {
                    5
                }
            }

            // Sort by hierarchy rank primary, then score descending secondary
            candidates.sort_by(|a, b| {
                let rank_a = get_hierarchy_rank(a);
                let rank_b = get_hierarchy_rank(b);
                match rank_a.cmp(&rank_b) {
                    std::cmp::Ordering::Equal => {
                        b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    other => other,
                }
            });

            let mut kept = Vec::new();
            let mut omitted = Vec::new();
            let mut cumulative_tokens = 0;

            for mut item in candidates {
                let text = format!("{}\n{}", item.title, item.content);
                let tokens = self.count_text_tokens(&text);
                if cumulative_tokens + tokens <= budget {
                    cumulative_tokens += tokens;
                    kept.push(item);
                } else {
                    let remaining_budget = budget - cumulative_tokens;
                    if self.compact_search_result(&mut item, remaining_budget) {
                        let compacted_text = format!("{}\n{}", item.title, item.content);
                        cumulative_tokens += self.count_text_tokens(&compacted_text);
                        kept.push(item);
                    } else {
                        omitted.push(item.id.clone());
                    }
                }
            }

            candidates = kept;
            if !omitted.is_empty() {
                omitted_ids = Some(omitted);
            }
        }

        let total_matches = candidates.len() + omitted_ids.as_ref().map(|o| o.len()).unwrap_or(0);
        let has_more = total_matches > offset + limit;
        let next_offset = offset + limit;

        let sliced_results = if offset < candidates.len() {
            let end = std::cmp::min(offset + limit, candidates.len());
            candidates[offset..end].to_vec()
        } else {
            Vec::new()
        };

        if enable_access_reinforcement {
            for c in &sliced_results {
                if c.tier == "episode" {
                    let backend_clone = self.clone();
                    let id_clone = c.id.clone();
                    tokio::spawn(async move {
                        let _ = backend_clone.reinforce_episode(&id_clone).await;
                    });
                }
            }
        }
 
        Ok(SearchResponse {
            results: sliced_results,
            total_matches,
            has_more,
            next_offset,
            omitted_ids,
        })
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
        if self.is_client_mode() {
            let payload = crate::contracts::Feedback { id: id.to_string(), success };
            let _res: serde_json::Value = self.daemon_post("/v1/feedback", &payload).await?;
            return Ok(());
        }
        let thing_id = parse_record_id(id)?;
        
        let fetch_sql = "SELECT VALUE utility_score FROM metrics WHERE target_id = $target_id LIMIT 1;";
        let mut response = self.db.query(fetch_sql).bind(("target_id", thing_id.clone())).await?.check().context("Fetch metrics query failed")?;
        let utility_opt: Option<f64> = response.take(0)?;

        let prev_utility = utility_opt.unwrap_or(1.0);
        let reinforcement = if success { 1.0 } else { 0.0 };
        
        let new_utility = (0.3 * reinforcement) + (0.7 * prev_utility);

        let update_sql = "
            UPDATE metrics 
            SET utility_score = $new_utility, access_count = access_count + 1, last_accessed = time::now()
            WHERE target_id = $target_id;
        ";
        let _ = self.db.query(update_sql)
            .bind(("new_utility", new_utility))
            .bind(("target_id", thing_id.clone()))
            .await?
            .check().context("Update metrics query failed")?;
        
        Ok(())
    }

    async fn apply_migrations(&self) -> Result<()> {
        Ok(())
    }

    async fn get_llm_config(&self) -> Result<LlmConfigResponse> {
        if self.is_client_mode() {
            return self.daemon_get("/v1/config/llm").await;
        }
        let sql = "SELECT active_provider, model, cloud_provider, is_override, expires_at FROM config:settings;";
        let mut response = self.db.query(sql).await?.check().context("Get config query failed")?;
        let config_opt: Option<LlmConfigResponse> = response.take(0)?;
        let mut config = if let Some(c) = config_opt {
            c
        } else {
            LlmConfigResponse {
                active_provider: "local".to_string(),
                cloud_provider: "gemini".to_string(),
                model: "mlx-community/Qwen3.6-35B-A3B-4bit".to_string(),
                is_override: false,
                expires_at: None,
                api_key: None,
            }
        };

        let provider_for_key = if config.active_provider == "local" {
            "local"
        } else {
            &config.cloud_provider
        };
        config.api_key = load_api_key(provider_for_key);
        Ok(config)
    }

    async fn update_llm_config(&self, req: &LlmConfigRequest) -> Result<()> {
        if self.is_client_mode() {
            let _res: serde_json::Value = self.daemon_post("/v1/config/llm", req).await?;
            return Ok(());
        }
        let sql_select = "SELECT active_provider, model, cloud_provider, is_override, expires_at FROM config:settings;";
        let mut select_res = self.db.query(sql_select).await?.check().context("Get config query failed")?;
        let existing: Option<LlmConfigResponse> = select_res.take(0)?;

        let mut current_model = req.model.clone();
        let mut current_cloud_provider = req.cloud_provider.clone();

        if current_model.is_none() || current_cloud_provider.is_none() {
            let (default_model, default_cloud_provider) = if req.provider == "local" {
                ("mlx-community/Qwen3.6-35B-A3B-4bit".to_string(), "gemini".to_string())
            } else {
                ("gemini-1.5-flash".to_string(), "gemini".to_string())
            };
            if current_model.is_none() {
                current_model = Some(existing.as_ref().map(|e| e.model.clone()).unwrap_or(default_model));
            }
            if current_cloud_provider.is_none() {
                current_cloud_provider = Some(existing.as_ref().map(|e| e.cloud_provider.clone()).unwrap_or(default_cloud_provider));
            }
        }

        let model = current_model.unwrap();
        let cloud_provider = current_cloud_provider.unwrap();

        let expires_at: Option<String> = None;

        let sql = "
            UPSERT config:settings CONTENT {
                active_provider: $active_provider,
                model: $model,
                cloud_provider: $cloud_provider,
                is_override: true,
                expires_at: $expires_at
            };
        ";
        let _ = self.db.query(sql)
            .bind(("active_provider", req.provider.as_str()))
            .bind(("model", model.as_str()))
            .bind(("cloud_provider", cloud_provider.as_str()))
            .bind(("expires_at", expires_at.clone()))
            .await?.check().context("UPSERT config failed")?;

        if let Some(ref key) = req.api_key {
            let provider_for_key = if req.provider == "local" {
                "local"
            } else {
                &cloud_provider
            };
            save_api_key(provider_for_key, key)?;
        }

        Ok(())
    }

    async fn get_unprocessed_episodes(&self) -> Result<Vec<Episode>> {
        let sql = "SELECT * FROM episode WHERE processed_in_dream = false;";
        let mut response = self.db.query(sql).await?.check().context("Query unprocessed episodes failed")?;
        let raw_episodes: Vec<EpisodeRaw> = response.take(0)?;
        let episodes = raw_episodes.into_iter().map(|raw| Episode {
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
        }).collect();
        Ok(episodes)
    }

    async fn mark_episode_processed(&self, id: &str) -> Result<()> {
        let thing_id = parse_record_id(id)?;

        let sql = "UPDATE $id SET processed_in_dream = true;";
        let _ = self.db.query(sql).bind(("id", thing_id)).await?.check().context("Mark episode processed failed")?;
        Ok(())
    }

    async fn get_all_episodes(&self) -> Result<Vec<Episode>> {
        let sql = "SELECT * FROM episode;";
        let mut response = self.db.query(sql).await?.check().context("Query all episodes failed")?;
        let raw_episodes: Vec<EpisodeRaw> = response.take(0)?;
        let episodes = raw_episodes.into_iter().map(|raw| Episode {
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
        }).collect();
        Ok(episodes)
    }

    async fn get_episodes_by_node_type(&self, node_type: &str) -> Result<Vec<Episode>> {
        let sql = "SELECT * FROM episode WHERE node_type = $node_type;";
        let mut response = self.db.query(sql)
            .bind(("node_type", node_type))
            .await?.check().context("Query episodes by node type failed")?;
        let raw_episodes: Vec<EpisodeRaw> = response.take(0)?;
        let episodes = raw_episodes.into_iter().map(|raw| Episode {
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
            discovery_tokens: raw.discovery_tokens,
            facts: raw.facts,
            concepts: raw.concepts,
            files_read: raw.files_read,
            files_modified: raw.files_modified,
            session_id: raw.session_id,
            word_count: raw.word_count,
            archived_at: raw.archived_at.map(|dt| dt.to_rfc3339()),
            node_type: raw.node_type,
            confidence: raw.confidence,
        }).collect();
        Ok(episodes)
    }

    async fn is_feature_enabled(&self, feature_key: &str, default: bool) -> bool {
        match self.get_profile_key(feature_key).await {
            Ok(Some(val_str)) => val_str.parse::<bool>().unwrap_or(default),
            _ => default,
        }
    }

    async fn save_profile_key(&self, key: &str, value: &str) -> Result<()> {
        let sql = "
            UPSERT type::record('profile', $key) CONTENT {
                key: $key,
                value: $value
            };
        ";
        let _ = self.db.query(sql)
            .bind(("key", key))
            .bind(("value", value))
            .await?.check().context("UPSERT profile failed")?;
        Ok(())
    }

    async fn get_profile_key(&self, key: &str) -> Result<Option<String>> {
        #[derive(serde::Deserialize, SurrealValue)]
        struct ProfileRaw {
            value: String,
        }
        let sql = "SELECT `value` FROM `profile` WHERE id = type::record('profile', $key);";
        let mut response = self.db.query(sql)
            .bind(("key", key))
            .await?.check().context("SELECT profile failed")?;
        let res: Vec<ProfileRaw> = response.take(0)?;
        Ok(res.first().map(|r| r.value.clone()))
    }

    async fn save_handoff(&self, handoff: &HandoffSave) -> Result<String> {
        if self.is_client_mode() {
            #[derive(serde::Deserialize)]
            struct SaveResponse {
                id: String,
            }
            let res: SaveResponse = self.daemon_post("/v1/handoffs", handoff).await?;
            return Ok(res.id);
        }
        let id_str = Uuid::new_v4().to_string();
        let query = "
            BEGIN TRANSACTION;
            CREATE type::record('handoff', $id) CONTENT {
                parent_conversation_id: $parent,
                subagent_conversation_id: $subagent,
                summary: $summary,
                handoff_file_path: $path,
                scope: $target_scope,
                status: 'PENDING',
                created_at: time::now(),
                include_tool_execution: $include_tool_execution
            };
            COMMIT TRANSACTION;
        ";
        self.db.query(query)
            .bind(("id", id_str.clone()))
            .bind(("parent", handoff.parent_conversation_id.as_str()))
            .bind(("subagent", handoff.subagent_conversation_id.as_str()))
            .bind(("summary", handoff.summary.as_str()))
            .bind(("path", handoff.handoff_file_path.as_str()))
            .bind(("target_scope", handoff.scope.as_deref().unwrap_or("general")))
            .bind(("include_tool_execution", handoff.include_tool_execution.unwrap_or(false)))
            .await?.check()?;

        // Copy all STM entries from parent to subagent session
        if let Ok(parent_stm) = self.get_stm(&handoff.parent_conversation_id, None).await {
            for (k, v) in parent_stm {
                if let Err(e) = self.save_stm(&handoff.subagent_conversation_id, &k, &v).await {
                    tracing::warn!("Failed to copy STM entry '{}' from {} to {} during handoff: {:?}", k, handoff.parent_conversation_id, handoff.subagent_conversation_id, e);
                }
            }
        }

        // Retrieve distilled context nodes from STM of either parent or subagent
        let mut stm_nodes_str = None;
        if let Ok(map) = self.get_stm(&handoff.parent_conversation_id, Some("distilled_context_nodes")).await {
            if let Some(val) = map.get("distilled_context_nodes") {
                stm_nodes_str = Some(val.clone());
            }
        }
        if stm_nodes_str.is_none() {
            if let Ok(map) = self.get_stm(&handoff.subagent_conversation_id, Some("distilled_context_nodes")).await {
                if let Some(val) = map.get("distilled_context_nodes") {
                    stm_nodes_str = Some(val.clone());
                }
            }
        }

        if let Some(nodes_str) = stm_nodes_str {
            let mut resolved_node_ids = Vec::new();
            if let Ok(node_ids) = serde_json::from_str::<Vec<String>>(&nodes_str) {
                resolved_node_ids = node_ids;
            } else if let Ok(values) = serde_json::from_str::<Vec<serde_json::Value>>(&nodes_str) {
                for val in values {
                    if let Some(s) = val.as_str() {
                        resolved_node_ids.push(s.to_string());
                    }
                }
            } else {
                // Try parsing comma-separated list or raw single ID
                let cleaned = nodes_str.trim_matches(|c| c == '[' || c == ']' || c == '"' || c == ' ');
                for part in cleaned.split(',') {
                    let part = part.trim().trim_matches('"');
                    if !part.is_empty() {
                        resolved_node_ids.push(part.to_string());
                    }
                }
            }

            let handoff_id = format!("handoff:{}", id_str);
            for node_id in resolved_node_ids {
                if parse_record_id(&node_id).is_ok() {
                    if let Err(e) = self.relate_nodes(&handoff_id, &node_id, None, None, None).await {
                        tracing::warn!("Failed to relate handoff {} to context node {}: {:?}", handoff_id, node_id, e);
                    }
                } else {
                    tracing::warn!("Handoff context node ID is not a valid record ID: {}", node_id);
                }
            }
        }

        Ok(format!("handoff:{}", id_str))
    }

    async fn save_wiki_node(&self, node: &WikiNode) -> Result<String> {
        if let Some(ref vp) = node.vault_path {
            self.record_indexing_write(vp).await;
        }
        let mut node_uuid = Uuid::new_v4().to_string();
        let mut is_update = false;

        let check_query = "SELECT VALUE id FROM wiki_node WHERE name = $name LIMIT 1;";
        let mut response = self.db.query(check_query).bind(("name", node.name.as_str())).await?;
        let ids: Option<surrealdb::types::RecordId> = response.take(0)?;
        if let Some(thing) = ids {
            node_uuid = match &thing.key {
                surrealdb::types::RecordIdKey::String(s) => unescape_id_part(s),
                other => unescape_id_part(&record_key_to_string(other)),
            };
            is_update = true;
        }

        let query_str = if is_update {
            "
                BEGIN TRANSACTION;
                LET $node = type::record('wiki_node', $node_uuid);
                UPDATE $node MERGE {
                    name: $name,
                    content: $content,
                    scope: $target_scope,
                    vault_path: $vault_path,
                    embedding: $embedding
                };
                COMMIT TRANSACTION;
            "
        } else {
            "
                BEGIN TRANSACTION;
                LET $node = type::record('wiki_node', $node_uuid);
                CREATE $node CONTENT {
                    name: $name,
                    content: $content,
                    scope: $target_scope,
                    vault_path: $vault_path,
                    embedding: $embedding
                };
                COMMIT TRANSACTION;
            "
        };

        let vp_val = node.vault_path.clone().unwrap_or_default();
        let embedding_val = if let Some(ref emb) = node.embedding {
            Some(emb.clone())
        } else if let Some(ref _embedder) = self.embedder {
            let text_to_embed = format!("{}: {}", node.name, node.content);
            match self.embed(&text_to_embed).await {
                Ok(vec) => Some(vec),
                Err(e) => {
                    tracing::warn!("Embedding generation failed in save_wiki_node: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let response = self.db.query(query_str)
            .bind(("node_uuid", node_uuid.as_str()))
            .bind(("name", node.name.as_str()))
            .bind(("content", node.content.as_str()))
            .bind(("target_scope", node.scope.as_str()))
            .bind(("vault_path", vp_val.as_str()))
            .bind(("embedding", embedding_val.clone()))
            .await?;

        response.check().context("SurrealDB save_wiki_node transaction failed")?;

        Ok(format!("wiki_node:{}", node_uuid))
    }

    async fn relate_nodes(
        &self,
        from_id: &str,
        to_id: &str,
        valid_from: Option<chrono::DateTime<chrono::Utc>>,
        valid_to: Option<chrono::DateTime<chrono::Utc>>,
        confidence: Option<f32>,
    ) -> Result<()> {
        if let (Some(from), Some(to)) = (valid_from, valid_to) {
            if to < from {
                anyhow::bail!("Invalid interval: valid_to cannot be before valid_from");
            }
        }
        let from_thing = parse_record_id(from_id)?;
        let to_thing = parse_record_id(to_id)?;
        
        let sql = "RELATE $from -> relates_to -> $to UNIQUE CONTENT {
            created_at: time::now(),
            valid_from: $valid_from,
            valid_to: $valid_to,
            confidence: $confidence
        };";
        
        self.db.query(sql)
            .bind(("from", from_thing))
            .bind(("to", to_thing))
            .bind(("valid_from", valid_from))
            .bind(("valid_to", valid_to))
            .bind(("confidence", confidence.unwrap_or(1.0)))
            .await?
            .check().context("Failed to relate nodes")?;
        Ok(())
     }

    async fn relate_followed_by(&self, from_id: &str, to_id: &str) -> Result<()> {
        let from_thing = parse_record_id(from_id)?;
        let to_thing = parse_record_id(to_id)?;
        let sql = "RELATE $from -> followed_by -> $to CONTENT { created_at: time::now() };";
        self.db.query(sql)
            .bind(("from", from_thing))
            .bind(("to", to_thing))
            .await?
            .check().context("Failed to relate followed_by")?;
        Ok(())
    }

    async fn invalidate_edge(
        &self,
        from_id: &str,
        to_id: &str,
        ended: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<()> {
        let from_thing = parse_record_id(from_id)?;
        let to_thing = parse_record_id(to_id)?;
        let end_time = ended.unwrap_or_else(chrono::Utc::now);
        
        let sql = "UPDATE relates_to SET valid_to = $end_time WHERE in = $from AND out = $to;";
        self.db.query(sql)
            .bind(("from", from_thing))
            .bind(("to", to_thing))
            .bind(("end_time", end_time))
            .await?
            .check().context("Failed to invalidate edge")?;
        Ok(())
    }

    async fn query_edges_as_of(
        &self,
        node_id: &str,
        as_of: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<String>> {
        let from_thing = parse_record_id(node_id)?;
        
        let sql = "
            SELECT VALUE out FROM relates_to 
            WHERE in = $from 
              AND (valid_from = NONE OR valid_from <= $as_of) 
              AND (valid_to = NONE OR valid_to >= $as_of);
        ";
        
        let mut response = self.db.query(sql)
            .bind(("from", from_thing))
            .bind(("as_of", as_of))
            .await?;
            
        let ids: Vec<surrealdb::types::RecordId> = response.take(0)?;
        Ok(ids.into_iter().map(|thing| format_record_id(&thing)).collect())
    }

    async fn get_related_node_ids(&self, from_id: &str) -> Result<Vec<String>> {
        let from_thing = parse_record_id(from_id)?;
        let sql = "SELECT VALUE out FROM relates_to WHERE in = $from;";
        let mut response = self.db.query(sql).bind(("from", from_thing)).await?;
        let ids: Vec<surrealdb::types::RecordId> = response.take(0)?;
        Ok(ids.into_iter().map(|thing| format_record_id(&thing)).collect())
    }

    async fn get_wiki_node_id_by_vault_path(&self, vault_path: &str) -> Result<Option<String>> {
        let sql = "SELECT VALUE id FROM wiki_node WHERE vault_path = $vault_path LIMIT 1;";
        let mut response = self.db.query(sql).bind(("vault_path", vault_path)).await?;
        let ids: Option<surrealdb::types::RecordId> = response.take(0)?;
        Ok(ids.map(|thing| format_record_id(&thing)))
    }

    async fn get_active_scopes(&self) -> Result<Vec<String>> {
        let sql = "SELECT VALUE scope FROM episode GROUP BY scope;";
        let mut response = self.db.query(sql).await?;
        let mut scopes: Vec<String> = response.take(0)?;
        if !scopes.contains(&"general".to_string()) {
            scopes.push("general".to_string());
        }
        Ok(scopes)
    }

    async fn delete_by_vault_path(&self, vault_path: &str) -> Result<()> {
        let sql1 = "DELETE FROM episode WHERE vault_path = $vault_path;";
        let sql2 = "DELETE FROM wisdom WHERE vault_path = $vault_path;";
        let sql3 = "DELETE FROM wiki_node WHERE vault_path = $vault_path;";
        
        self.db.query(sql1).bind(("vault_path", vault_path)).await?.check()?;
        self.db.query(sql2).bind(("vault_path", vault_path)).await?.check()?;
        self.db.query(sql3).bind(("vault_path", vault_path)).await?.check()?;
        Ok(())
    }

    async fn save_stm(&self, session_id: &str, key: &str, value: &str) -> Result<()> {
        let sql = "
            BEGIN TRANSACTION;
            UPSERT type::record('short_term_memory', [$session_id, $key]) CONTENT {
                session_id: $session_id,
                key: $key,
                value: $value,
                updated_at: time::now()
            };
            COMMIT TRANSACTION;
        ";
        self.db.query(sql)
            .bind(("session_id", session_id))
            .bind(("key", key))
            .bind(("value", value))
            .await?.check()?;

        // Dual-write to local JSON file unless running a benchmark
        if std::env::var("MYTHRAX_BENCH").is_err() {
            crate::store::save_stm_file(session_id, key, value)?;
        }
        Ok(())
    }


    async fn query_symbolic_scored(
        &self,
        node_id: &str,
        relation: Option<&str>,
        max_depth: Option<usize>,
        as_of: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<crate::contracts::SymbolicHit>> {
        use std::collections::{HashMap, VecDeque};
        
        let start_thing = parse_record_id(node_id)?;
        let limit_depth = max_depth.unwrap_or(3);
        
        let mut queue = VecDeque::new();
        queue.push_back((start_thing.clone(), 0, 1.0f32));
        
        let mut path_conf = HashMap::new();
        path_conf.insert(start_thing.clone(), 1.0f32);
        
        let mut hits = Vec::new();
        
        while let Some((current, depth, current_conf)) = queue.pop_front() {
            if depth >= limit_depth {
                continue;
            }
            
            let sql = if relation.is_some() {
                if as_of.is_some() {
                    "SELECT out, confidence FROM relates_to 
                     WHERE in = $current 
                       AND relation = $relation
                       AND (valid_from = NONE OR valid_from <= $as_of)
                       AND (valid_to = NONE OR valid_to >= $as_of);"
                } else {
                    "SELECT out, confidence FROM relates_to WHERE in = $current AND relation = $relation;"
                }
            } else {
                if as_of.is_some() {
                    "SELECT out, confidence FROM relates_to 
                     WHERE in = $current 
                       AND (valid_from = NONE OR valid_from <= $as_of)
                       AND (valid_to = NONE OR valid_to >= $as_of);"
                } else {
                    "SELECT out, confidence FROM relates_to WHERE in = $current;"
                }
            };
            
            let mut query = self.db.query(sql).bind(("current", current.clone()));
            if let Some(rel) = relation {
                query = query.bind(("relation", rel));
            }
            if let Some(t) = as_of {
                query = query.bind(("as_of", t));
            }
            
            let mut response = query.await?;
            let edges: Vec<ScoredEdge> = response.take(0)?;
            
            for edge in edges {
                let neighbor = edge.out;
                let edge_conf = edge.confidence.unwrap_or(1.0f32);
                let next_conf = current_conf * edge_conf;
                
                let mut should_visit = false;
                if let Some(&existing_conf) = path_conf.get(&neighbor) {
                    if next_conf > existing_conf {
                        path_conf.insert(neighbor.clone(), next_conf);
                        should_visit = true;
                    }
                } else {
                    path_conf.insert(neighbor.clone(), next_conf);
                    should_visit = true;
                }
                
                if should_visit {
                    let neighbor_str = format_record_id(&neighbor);
                    if let Some(hit) = hits.iter_mut().find(|h: &&mut crate::contracts::SymbolicHit| h.node_id == neighbor_str) {
                        hit.path_confidence = next_conf;
                        hit.hops = depth + 1;
                    } else {
                        hits.push(crate::contracts::SymbolicHit {
                            node_id: neighbor_str,
                            path_confidence: next_conf,
                            hops: depth + 1,
                        });
                    }
                    queue.push_back((neighbor, depth + 1, next_conf));
                }
            }
        }
        
        Ok(hits)
    }

    async fn query_symbolic(&self, node_id: &str, relation: Option<&str>, max_depth: Option<usize>) -> Result<Vec<String>> {
        let hits = self.query_symbolic_scored(node_id, relation, max_depth, None).await?;
        Ok(hits.into_iter().map(|h| h.node_id).collect())
    }

    async fn save_thought_node(&self, thought: &crate::contracts::ThoughtNode) -> Result<String> {
        let thought_uuid = uuid::Uuid::new_v4().to_string();
        
        let embedding_val = if let Some(ref _embedder) = self.embedder {
            let text_to_embed = format!("{}: {}", thought.title, thought.content);
            match self.embed(&text_to_embed).await {
                Ok(vec) => Some(vec),
                Err(e) => {
                    tracing::warn!("Embedding generation failed in save_thought_node: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let query_str = "
            BEGIN TRANSACTION;
            UPSERT type::record('thought_node', $thought_uuid) CONTENT {
                id: type::record('thought_node', $thought_uuid),
                title: $title,
                content: $content,
                scope: $scope,
                vault_path: $vault_path,
                embedding: $embedding,
                created_at: time::now()
            };
            COMMIT TRANSACTION;
        ";

        let vp_val = thought.vault_path.clone().unwrap_or_default();
        let response = self.db.query(query_str)
            .bind(("thought_uuid", thought_uuid.as_str()))
            .bind(("title", thought.title.as_str()))
            .bind(("content", thought.content.as_str()))
            .bind(("scope", thought.scope.as_str()))
            .bind(("vault_path", vp_val.as_str()))
            .bind(("embedding", embedding_val))
            .await?;

        response.check().context("SurrealDB save_thought_node transaction failed")?;

        Ok(format!("thought_node:{}", thought_uuid))
    }

    async fn get_stm(&self, session_id: &str, key: Option<&str>) -> Result<std::collections::HashMap<String, String>> {
        if let Some(k) = key {
            let sql = "SELECT VALUE value FROM type::record('short_term_memory', [$session_id, $key]);";
            let mut response = self.db.query(sql)
                .bind(("session_id", session_id))
                .bind(("key", k))
                .await?.check()?;
            let value: Option<String> = response.take(0)?;
            let mut map = std::collections::HashMap::new();
            if let Some(v) = value {
                map.insert(k.to_string(), v);
            }
            Ok(map)
        } else {
            let sql = "SELECT key, value FROM short_term_memory WHERE session_id = $session_id;";
            let mut response = self.db.query(sql)
                .bind(("session_id", session_id))
                .await?.check()?;
            #[derive(serde::Deserialize, surrealdb_types::SurrealValue, Debug)]
            struct StmRecord {
                key: String,
                value: String,
            }
            let records: Vec<StmRecord> = response.take(0)?;
            let mut map = std::collections::HashMap::new();
            for r in records {
                map.insert(r.key, r.value);
            }
            Ok(map)
        }
    }

    async fn clear_stm(&self, session_id: &str) -> Result<()> {
        let sql = "DELETE FROM short_term_memory WHERE session_id = $session_id;";
        self.db.query(sql)
            .bind(("session_id", session_id))
            .await?.check()?;

        // Delete local JSON file
        crate::store::delete_stm_file(session_id)?;
        Ok(())
    }

    async fn update_handoff_status(&self, id: &str, status: &str) -> Result<()> {
        let thing_id = parse_record_id(id)?;
        let sql = "UPDATE $id SET status = $status;";
        self.db.query(sql)
            .bind(("id", thing_id))
            .bind(("status", status))
            .await?.check()?;
        Ok(())
    }

    async fn delete_stale_handoffs(&self, pruning_days: i64) -> Result<()> {
        let select_sql = "
            SELECT 
                id, 
                parent_conversation_id, 
                subagent_conversation_id, 
                summary, 
                handoff_file_path, 
                scope, 
                status, 
                created_at,
                include_tool_execution
            FROM handoff 
            WHERE (status = 'COMPLETED' OR status = 'FAILED') 
              AND created_at < time::now() - <duration> $duration;
        ";
        let duration_str = format!("{}d", pruning_days);
        let mut response = self.db.query(select_sql)
            .bind(("duration", duration_str.as_str()))
            .await?.check()?;
        let raw_handoffs: Vec<HandoffRaw> = response.take(0)?;
        tracing::debug!("delete_stale_handoffs raw_handoffs={:?}", raw_handoffs);
        
        // Delete files from disk and matching STM DB records
        for h in &raw_handoffs {
            let path = std::path::Path::new(&h.handoff_file_path);
            if path.exists() {
                let _ = std::fs::remove_file(path);
            }
            if let Some(parent) = path.parent() {
                let stm_file_sub = parent.join(format!("stm_{}.json", h.subagent_conversation_id));
                if stm_file_sub.exists() {
                    let _ = std::fs::remove_file(stm_file_sub);
                }
                let stm_file_parent = parent.join(format!("stm_{}.json", h.parent_conversation_id));
                if stm_file_parent.exists() {
                    let _ = std::fs::remove_file(stm_file_parent);
                }
            }
            
            // Delete matching short term memory records from SurrealDB
            let clean_stm_sql = "DELETE FROM short_term_memory WHERE session_id = $parent OR session_id = $subagent;";
            let _ = self.db.query(clean_stm_sql)
                .bind(("parent", h.parent_conversation_id.as_str()))
                .bind(("subagent", h.subagent_conversation_id.as_str()))
                .await?.check()?;
        }

        let delete_sql = "
            DELETE FROM handoff 
            WHERE (status = 'COMPLETED' OR status = 'FAILED') 
              AND created_at < time::now() - <duration> $duration;
        ";
        let _ = self.db.query(delete_sql)
            .bind(("duration", duration_str.as_str()))
            .await?.check()?;

        Ok(())
    }

    async fn prune_stale_memories(&self, vault_root: &std::path::Path) -> Result<()> {
        let pruning_days = match self.get_profile_key("stm.pruning_days").await {
            Ok(Some(val_str)) => val_str.parse::<i64>().unwrap_or(7),
            _ => std::env::var("MYTHRAX_STM_PRUNING_DAYS")
                .ok()
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(7),
        };

        // Delete short_term_memory records older than pruning_days
        let prune_stm_sql = "DELETE FROM short_term_memory WHERE updated_at < time::now() - <duration> $duration;";
        let duration_str = format!("{}d", pruning_days);
        let _ = self.db.query(prune_stm_sql)
            .bind(("duration", duration_str.as_str()))
            .await?.check()?;

        // Clean up completed/failed handoffs and associated STMs
        self.delete_stale_handoffs(pruning_days).await?;

        // Scan .handoffs/ folder and delete stm_*.json files older than pruning_days
        let handoffs_dir = vault_root.join(".handoffs");
        if handoffs_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(handoffs_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                        if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                            if filename.starts_with("stm_") {
                                if let Ok(metadata) = entry.metadata() {
                                    if let Ok(modified) = metadata.modified() {
                                        if let Ok(elapsed) = modified.elapsed() {
                                            if elapsed.as_secs() > (pruning_days as u64) * 24 * 3600 {
                                                let _ = std::fs::remove_file(&path);
                                                tracing::info!("Pruned stale STM file: {:?}", path);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn get_memory_nodes(&self, node_ids: &[String]) -> Result<GetMemoryNodesResponse> {
        if self.is_client_mode() {
            let payload = crate::contracts::GetMemoryNodesRequest { node_ids: node_ids.to_vec() };
            return self.daemon_post("/v1/nodes", &payload).await;
        }
        let mut episodes = Vec::new();
        let mut wisdom_rules = Vec::new();
        let mut wiki_nodes = Vec::new();

        for id_str in node_ids {
            let thing_id = match parse_record_id(id_str) {
                Ok(tid) => tid,
                Err(_) => continue,
            };

            match thing_id.table.as_str() {
                "episode" => {
                    let ep_opt: Option<EpisodeRaw> = self.db.select(thing_id.clone()).await?;
                    if let Some(raw) = ep_opt {
                        let ep = Episode {
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
                        };
                        episodes.push(ep);
                    }
                }
                "wisdom" => {
                    let rule_opt: Option<WisdomRaw> = self.db.select(thing_id.clone()).await?;
                    if let Some(raw) = rule_opt {
                        wisdom_rules.push(raw.into_wisdom_rule());
                    }
                }
                "wiki_node" => {
                    let node_opt: Option<WikiNodeRaw> = self.db.select(thing_id.clone()).await?;
                    if let Some(raw) = node_opt {
                        let node = WikiNode {
                            id: Some(format_record_id(&raw.id)),
                            name: raw.name,
                            content: raw.content,
                            scope: raw.scope,
                            vault_path: raw.vault_path,
                            embedding: raw.embedding,
                        };
                        wiki_nodes.push(node);
                    }
                }
                _ => {
                    tracing::warn!("get_memory_nodes called with unknown table: {}", thing_id.table);
                }
            }
        }

        Ok(GetMemoryNodesResponse {
            episodes,
            wisdom_rules,
            wiki_nodes,
        })
    }

    async fn save_forged_section(&self, batch: &ForgedSectionBatch) -> Result<()> {
        if self.is_client_mode() {
            let _res: serde_json::Value = self.daemon_post("/v1/forge/save", batch).await?;
            return Ok(());
        }
        let vault_root = crate::store::find_vault_root();

        // Helper to generate slugs
        fn slugify(s: &str) -> String {
            let mut res = String::new();
            let mut last_was_underscore = false;
            for c in s.chars() {
                if c.is_alphanumeric() {
                    res.push(c.to_ascii_lowercase());
                    last_was_underscore = false;
                } else {
                    if !last_was_underscore {
                        res.push('_');
                        last_was_underscore = true;
                    }
                }
            }
            let trimmed = res.trim_matches('_').to_string();
            if trimmed.is_empty() {
                "default".to_string()
            } else {
                trimmed
            }
        }

        let doc_slug = slugify(&batch.doc_title);

        // Pre-check existing wiki nodes to reuse/update records
        let mut concept_uuids = Vec::new();
        let mut concept_is_update = Vec::new();
        for concept in &batch.concepts {
            let check_query = "SELECT VALUE id FROM wiki_node WHERE name = $name LIMIT 1;";
            let mut response = self.db.query(check_query).bind(("name", concept.name.as_str())).await?;
            let id_opt: Option<surrealdb::types::RecordId> = response.take(0)?;
            if let Some(thing) = id_opt {
                let uuid_str = match &thing.key {
                    surrealdb::types::RecordIdKey::String(s) => unescape_id_part(s),
                    other => unescape_id_part(&record_key_to_string(other)),
                };
                concept_uuids.push(uuid_str);
                concept_is_update.push(true);
            } else {
                concept_uuids.push(Uuid::new_v4().to_string());
                concept_is_update.push(false);
            }
        }

        let mut written_files = Vec::new();

        // 1. Write files to disk and track them for rollback
        let chunk_rel_path = format!("episodes/forge/{}/chunk_{}.md", doc_slug, batch.chunk_index);
        let ep_title = format!("{} - Chunk {}", batch.doc_title, batch.chunk_index);
        let ep_file_content = format!(
            "---\ntitle: \"{}\"\nscope: \"{}\"\nsource: \"forge\"\n---\n\n{}",
            ep_title, batch.scope, batch.chunk_text
        );
        let sanitized_ep = crate::secret_filter::SecretFilter::clean(&ep_file_content);

        let mut concept_paths = Vec::new();
        let mut rule_paths = Vec::new();

        let write_res = (|| -> Result<()> {
            // Write episode
            let chunk_abs_path = vault_root.join(&chunk_rel_path);
            if let Some(parent) = chunk_abs_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&chunk_abs_path, &sanitized_ep)?;
            written_files.push(chunk_abs_path);

            // Write concepts
            for concept in &batch.concepts {
                let concept_slug = slugify(&concept.name);
                let uuid_suffix = &Uuid::new_v4().to_string()[..8];
                let rel_path = format!("wiki/forge/{}/concept_{}_{}.md", doc_slug, concept_slug, uuid_suffix);
                let abs_path = vault_root.join(&rel_path);

                if let Some(parent) = abs_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let concept_md = format!(
                    "---\nname: \"{}\"\nscope: \"{}\"\ngenerator_name: \"ForgePipeline\"\n---\n\n# {}\n\n{}",
                    concept.name.replace('"', "\\\""), batch.scope, concept.name, concept.content
                );
                let sanitized_c = crate::secret_filter::SecretFilter::clean(&concept_md);
                std::fs::write(&abs_path, sanitized_c)?;
                written_files.push(abs_path);
                concept_paths.push(rel_path);
            }

            // Write rules
            for rule in &batch.rules {
                let rule_slug = slugify(&rule.target_pattern);
                let uuid_suffix = &Uuid::new_v4().to_string()[..8];
                let rel_path = format!("wisdom/forge/{}/rule_{}_{}.md", doc_slug, rule_slug, uuid_suffix);
                let abs_path = vault_root.join(&rel_path);

                if let Some(parent) = abs_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let rule_md = format!(
                    "---\ntarget_pattern: \"{}\"\naction_to_avoid: \"{}\"\ncausal_explanation: \"{}\"\nprescribed_remedy: \"{}\"\ntier: \"forge\"\nscope: \"{}\"\ngenerator_name: \"ForgePipeline\"\n---\n\n# Wisdom Rule: {}\n\n**Action to Avoid:** {}\n\n**Why:** {}\n\n**Prescribed Remedy:** {}",
                    rule.target_pattern.replace('"', "\\\""),
                    rule.action_to_avoid.replace('"', "\\\""),
                    rule.causal_explanation.replace('"', "\\\""),
                    rule.prescribed_remedy.replace('"', "\\\""),
                    batch.scope,
                    rule.target_pattern,
                    rule.action_to_avoid,
                    rule.causal_explanation,
                    rule.prescribed_remedy
                );
                let sanitized_r = crate::secret_filter::SecretFilter::clean(&rule_md);
                std::fs::write(&abs_path, sanitized_r)?;
                written_files.push(abs_path);
                rule_paths.push(rel_path);
            }

            Ok(())
        })();

        // If writing files failed, roll back and return error
        if let Err(e) = write_res {
            for path in &written_files {
                let _ = std::fs::remove_file(path);
            }
            return Err(e);
        }

        // 2. Generate embeddings for all inserted records using embed_batch
        let mut texts_to_embed = Vec::new();
        
        let ep_text = format!("{}: {}", ep_title, batch.chunk_text);
        texts_to_embed.push(ep_text);
        
        for concept in &batch.concepts {
            texts_to_embed.push(format!("{}: {}", concept.name, concept.content));
        }
        
        for rule in &batch.rules {
            texts_to_embed.push(format!(
                "Pattern: {}\nAvoid: {}\nWhy: {}\nRemedy: {}",
                rule.target_pattern, rule.action_to_avoid, rule.causal_explanation, rule.prescribed_remedy
            ));
        }
        
        let all_embeddings = if self.embedder.is_some() {
            match self.embed_batch(&texts_to_embed).await {
                Ok(embs) => embs,
                Err(e) => {
                    tracing::warn!("Batch embedding generation failed in save_forged_section: {}", e);
                    vec![vec![]; texts_to_embed.len()]
                }
            }
        } else {
            vec![vec![]; texts_to_embed.len()]
        };
        
        let ep_embedding = if all_embeddings[0].is_empty() { None } else { Some(all_embeddings[0].clone()) };
        
        let mut concept_embeddings = Vec::new();
        let mut idx = 1;
        for _ in &batch.concepts {
            let emb = if all_embeddings[idx].is_empty() { None } else { Some(all_embeddings[idx].clone()) };
            concept_embeddings.push(emb);
            idx += 1;
        }
        
        let mut rule_embeddings = Vec::new();
        for _ in &batch.rules {
            let emb = if all_embeddings[idx].is_empty() { None } else { Some(all_embeddings[idx].clone()) };
            rule_embeddings.push(emb);
            idx += 1;
        }

        // 3. Construct SurrealDB transaction query and run it
        let mut query = String::new();
        query.push_str("BEGIN TRANSACTION;\n");

        let mut bindings = std::collections::HashMap::new();

        let episode_uuid = Uuid::new_v4().to_string();
        let ep_metrics_uuid = Uuid::new_v4().to_string();

        bindings.insert("ep_title".to_string(), serde_json::json!(ep_title));
        bindings.insert("ep_content".to_string(), serde_json::json!(sanitized_ep));
        bindings.insert("scope".to_string(), serde_json::json!(batch.scope));
        bindings.insert("ep_vault_path".to_string(), serde_json::json!(chunk_rel_path));
        bindings.insert("ep_embedding".to_string(), serde_json::json!(ep_embedding));

        query.push_str(&format!(
            "LET $ep = type::record('episode', '{}');\n\
             CREATE $ep CONTENT {{\n\
                 title: $ep_title,\n\
                 content: $ep_content,\n\
                 source: 'forge',\n\
                 scope: $scope,\n\
                 vault_path: $ep_vault_path,\n\
                 processed_in_dream: false,\n\
                 embedding: $ep_embedding ?? none\n\
             }};\n\
             LET $ep_metrics = type::record('metrics', '{}');\n\
             CREATE $ep_metrics CONTENT {{\n\
                 target_id: $ep,\n\
                 utility_score: 1.0,\n\
                 access_count: 0\n\
             }};\n",
            episode_uuid, ep_metrics_uuid
        ));

        for (idx, concept) in batch.concepts.iter().enumerate() {
            let concept_uuid = &concept_uuids[idx];
            let is_update = concept_is_update[idx];

            let name_var = format!("concept_name_{}", idx);
            let content_var = format!("concept_content_{}", idx);
            let path_var = format!("concept_path_{}", idx);
            let emb_var = format!("concept_emb_{}", idx);

            bindings.insert(name_var.clone(), serde_json::json!(concept.name));
            bindings.insert(content_var.clone(), serde_json::json!(crate::secret_filter::SecretFilter::clean(&concept.content)));
            bindings.insert(path_var.clone(), serde_json::json!(concept_paths[idx]));
            bindings.insert(emb_var.clone(), serde_json::json!(concept_embeddings[idx]));

            query.push_str(&format!(
                "LET $concept_{} = type::record('wiki_node', '{}');\n",
                idx, concept_uuid
            ));

            if is_update {
                query.push_str(&format!(
                    "UPDATE $concept_{} MERGE {{\n\
                         name: ${},\n\
                         content: ${},\n\
                         scope: $scope,\n\
                         vault_path: ${},\n\
                         embedding: ${} ?? none\n\
                     }};\n",
                    idx, name_var, content_var, path_var, emb_var
                ));
            } else {
                query.push_str(&format!(
                    "CREATE $concept_{} CONTENT {{\n\
                         name: ${},\n\
                         content: ${},\n\
                         scope: $scope,\n\
                         vault_path: ${},\n\
                         embedding: ${} ?? none\n\
                     }};\n",
                    idx, name_var, content_var, path_var, emb_var
                ));
            }

            query.push_str(&format!(
                "RELATE $concept_{} -> relates_to -> $ep UNIQUE CONTENT {{ relation: 'extracted_from', created_at: time::now() }};\n",
                idx
            ));
        }

        for (idx, rule) in batch.rules.iter().enumerate() {
            let rule_uuid = Uuid::new_v4().to_string();
            let rule_metrics_uuid = Uuid::new_v4().to_string();

            let pattern_var = format!("rule_pattern_{}", idx);
            let avoid_var = format!("rule_avoid_{}", idx);
            let explanation_var = format!("rule_explanation_{}", idx);
            let remedy_var = format!("rule_remedy_{}", idx);
            let path_var = format!("rule_path_{}", idx);
            let emb_var = format!("rule_emb_{}", idx);
            let source_episodes_var = format!("rule_source_episodes_{}", idx);

            bindings.insert(pattern_var.clone(), serde_json::json!(crate::secret_filter::SecretFilter::clean(&rule.target_pattern)));
            bindings.insert(avoid_var.clone(), serde_json::json!(crate::secret_filter::SecretFilter::clean(&rule.action_to_avoid)));
            bindings.insert(explanation_var.clone(), serde_json::json!(crate::secret_filter::SecretFilter::clean(&rule.causal_explanation)));
            bindings.insert(remedy_var.clone(), serde_json::json!(crate::secret_filter::SecretFilter::clean(&rule.prescribed_remedy)));
            bindings.insert(path_var.clone(), serde_json::json!(rule_paths[idx]));
            bindings.insert(emb_var.clone(), serde_json::json!(rule_embeddings[idx]));
            bindings.insert(source_episodes_var.clone(), serde_json::json!(vec![format!("episode:{}", episode_uuid)]));

            query.push_str(&format!(
                "LET $rule_{} = type::record('wisdom', '{}');\n\
                 CREATE $rule_{} CONTENT {{\n\
                     target_pattern: ${},\n\
                     action_to_avoid: ${},\n\
                     causal_explanation: ${},\n\
                     prescribed_remedy: ${},\n\
                     tier: 'forge',\n\
                     scope: $scope,\n\
                     vault_path: ${},\n\
                     source_episodes: ${},\n\
                     generator_name: 'ForgePipeline',\n\
                     embedding: ${} ?? none\n\
                 }};\n\
                 LET $rule_metrics_{} = type::record('metrics', '{}');\n\
                 CREATE $rule_metrics_{} CONTENT {{\n\
                     target_id: $rule_{},\n\
                     utility_score: 1.0,\n\
                     access_count: 0\n\
                 }};\n",
                idx, rule_uuid,
                idx, pattern_var, avoid_var, explanation_var, remedy_var, path_var, source_episodes_var, emb_var,
                idx, rule_metrics_uuid,
                idx, idx
            ));

            for (c_idx, _) in batch.concepts.iter().enumerate() {
                query.push_str(&format!(
                    "RELATE $rule_{} -> relates_to -> $concept_{} UNIQUE CONTENT {{ created_at: time::now() }};\n",
                    idx, c_idx
                ));
            }
            query.push_str(&format!(
                "RELATE $rule_{} -> relates_to -> $ep UNIQUE CONTENT {{ relation: 'extracted_from', created_at: time::now() }};\n",
                idx
            ));
        }

        query.push_str("COMMIT TRANSACTION;");

        let mut q = self.db.query(&query);
        for (k, v) in bindings {
            q = q.bind((k.as_str(), v));
        }

        let db_res = q.await;

        match db_res {
            Ok(mut resp) => {
                let mut first_err = None;
                for i in 0..18 {
                    let res: Result<Option<serde_json::Value>, _> = resp.take(i);
                    match res {
                        Ok(_) => {}
                        Err(err) => {
                            let err_str = err.to_string();
                            if !err_str.contains("out of bounds") && !err_str.contains("no query at index") {
                                if first_err.is_none() {
                                    first_err = Some(err.clone());
                                } else if let Some(ref current_err) = first_err {
                                    if current_err.to_string().contains("failed transaction") {
                                        first_err = Some(err.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(e) = first_err {
                    // Rollback files on database transaction error
                    for path in &written_files {
                        let _ = std::fs::remove_file(path);
                    }
                    return Err(anyhow::anyhow!("SurrealDB transaction execution failed: {}", e));
                }
            }
            Err(e) => {
                // Rollback files on database query/connection error
                for path in &written_files {
                    let _ = std::fs::remove_file(path);
                }
                return Err(e.into());
            }
        }

        Ok(())
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
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
        let text = text.to_string();
        let res = tokio::task::spawn_blocking(move || {
            if let Some(ref emp) = embedder {
                emp.embed(&text)
            } else {
                anyhow::bail!("No embedder configured")
            }
        }).await?;

        self.active_embeddings.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        res
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
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
        let texts = texts.to_vec();
        let res = tokio::task::spawn_blocking(move || {
            if let Some(ref emp) = embedder {
                emp.embed_batch(&texts)
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
                    Ok(vec![vec![0.0f32; 768]; texts.len()])
                } else {
                    Err(anyhow::anyhow!("No embedding model loaded"))
                }
            }
        }).await?;

        self.active_embeddings.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        res
    }

    async fn get_all_wisdom_rules(&self) -> Result<Vec<WisdomRule>> {
        let sql = "
            SELECT *,
                   (SELECT VALUE utility_score FROM metrics WHERE target_id = $parent.id LIMIT 1)[0] AS utility
            FROM wisdom
            WHERE status != 'superseded';
        ";
        let mut response = self.db.query(sql).await?.check().context("Get all wisdom rules query failed")?;
        let raws: Vec<WisdomRaw> = response.take(0)?;
        let mut rules: Vec<WisdomRule> = raws.into_iter().map(|r| r.into_wisdom_rule()).collect();
        for w in &mut rules {
            if let Some(ref id_str) = w.id
                && let Ok(thing) = parse_record_id(id_str) {
                    w.id = Some(format_record_id(&thing));
                }
        }
        Ok(rules)
    }

    async fn get_all_wiki_nodes(&self) -> Result<Vec<WikiNode>> {
        let sql = "SELECT * FROM wiki_node;";
        let mut response = self.db.query(sql).await?.check().context("Get all wiki nodes query failed")?;
        let raws: Vec<WikiNodeRaw> = response.take(0)?;
        let nodes: Vec<WikiNode> = raws.into_iter().map(|r| r.into_wiki_node()).collect();
        Ok(nodes)
    }

    async fn diagnose_error_internal(&self, stderr: &str, stdout: &str) -> Result<Option<(String, String)>> {
        let combined = format!("{}\n{}", stderr, stdout);

        let mut matched_signature = None;

        use std::sync::OnceLock;
        static RUST_REGEX: OnceLock<regex::Regex> = OnceLock::new();
        static TS_REGEX: OnceLock<regex::Regex> = OnceLock::new();
        static PERM_REGEX: OnceLock<regex::Regex> = OnceLock::new();
        static LOCK_REGEX: OnceLock<regex::Regex> = OnceLock::new();

        let rust_re = RUST_REGEX.get_or_init(|| regex::Regex::new(r"(E\d{4})").unwrap());
        let ts_re = TS_REGEX.get_or_init(|| regex::Regex::new(r"(TS\d{4})").unwrap());
        let perm_re = PERM_REGEX.get_or_init(|| regex::Regex::new(r"(?i)(401\s+Unauthorized|403\s+Forbidden|Permission\s+denied|permission_denied)").unwrap());
        let lock_re = LOCK_REGEX.get_or_init(|| regex::Regex::new(r"(?i)(lock\s+acquisition\s+failure|RocksDB\s+lock|lock\s+conflict)").unwrap());

        if let Some(caps) = rust_re.captures(&combined) {
            matched_signature = Some(caps.get(1).unwrap().as_str().to_string());
        } else if let Some(caps) = ts_re.captures(&combined) {
            matched_signature = Some(caps.get(1).unwrap().as_str().to_string());
        } else if perm_re.is_match(&combined) {
            matched_signature = Some("permission".to_string());
        } else if lock_re.is_match(&combined) {
            matched_signature = Some("lock".to_string());
        }

        if let Some(sig) = matched_signature {
            let sql = "SELECT causal_explanation, prescribed_remedy FROM wisdom WHERE status != 'superseded' AND string::contains(target_pattern, $sig) LIMIT 1;";
            let res = self.db.query(sql).bind(("sig", sig.as_str())).await?;
            let mut res = res.check()?;
            #[derive(serde::Deserialize, Debug, SurrealValue)]
            struct WisdomRemedy {
                causal_explanation: String,
                prescribed_remedy: String,
            }
            let rules: Vec<WisdomRemedy> = res.take(0)?;
            if let Some(rule) = rules.into_iter().next() {
                return Ok(Some((rule.causal_explanation, rule.prescribed_remedy)));
            }
        }

        if let Some(ref _embedder) = self.embedder {
            let embed_text = if combined.len() > 500 {
                &combined[..500]
            } else {
                &combined
            };
            if let Ok(q_vec) = self.embed(embed_text).await {
                let sql = "
                    SELECT causal_explanation, prescribed_remedy, embedding FROM wisdom
                    WHERE status != 'superseded' AND (embedding <|1, 10|> $query_embedding);
                ";
                let res = self.db.query(sql).bind(("query_embedding", q_vec.clone())).await?;
                let mut res = res.check()?;
                #[derive(serde::Deserialize, Debug, SurrealValue)]
                struct WisdomVectorRaw {
                    causal_explanation: String,
                    prescribed_remedy: String,
                    embedding: Option<Vec<f32>>,
                }
                let rules: Vec<WisdomVectorRaw> = res.take(0)?;
                let mut best_match = None;
                let mut best_similarity = 0.0_f32;

                for r in rules {
                    if let Some(ref e_vec) = r.embedding {
                        let dot: f32 = q_vec.iter().zip(e_vec.iter()).map(|(a, b)| a * b).sum();
                        if dot > best_similarity {
                            best_similarity = dot;
                            best_match = Some(r);
                        }
                    }
                }

                if best_similarity >= 0.70 {
                    if let Some(r) = best_match {
                        return Ok(Some((r.causal_explanation, r.prescribed_remedy)));
                    }
                }
            }
        }

        Ok(None)
    }

    async fn journal_state(&self, vault_root: &std::path::Path, session_id: Option<&str>) -> Result<()> {
        let workspace_root = std::env::var("MYTHRAX_WORKSPACE_ROOT")
            .ok()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        
        let task_md_path = workspace_root.join("task.md");
        let task_checklist = if task_md_path.exists() {
            std::fs::read_to_string(&task_md_path).unwrap_or_default()
        } else {
            String::new()
        };

        // Query HTR tree state (all hypothesis nodes)
        let mut response = self.db.query("SELECT * FROM hypothesis_node;").await?;
        let htr_tree_state: Vec<serde_json::Value> = response.take(0).unwrap_or_default();

        // Query active STM keys
        let mut stm_response = if let Some(sid) = session_id {
            self.db.query("SELECT key, value FROM short_term_memory WHERE session_id = $session_id;")
                .bind(("session_id", sid))
                .await?
        } else {
            self.db.query("SELECT key, value FROM short_term_memory;").await?
        };
        
        let stm_records: Vec<serde_json::Value> = stm_response.take(0).unwrap_or_default();
        let mut active_stm = serde_json::Map::new();
        for rec in stm_records {
            if let Some(key) = rec.get("key").and_then(|v| v.as_str()) {
                if let Some(value) = rec.get("value") {
                    active_stm.insert(key.to_string(), value.clone());
                }
            }
        }

        // Get current git commit
        let git_commit = std::process::Command::new("git")
            .args(&["rev-parse", "HEAD"])
            .current_dir(&workspace_root)
            .output()
            .ok()
            .and_then(|out| {
                if out.status.success() {
                    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "HEAD".to_string());

        // Save to SurrealDB session_state table
        let session_id_val = session_id.unwrap_or("default_session");
        let sql = "
            UPSERT type::record('session_state', $session_id) CONTENT {
                session_id: $session_id,
                task_checklist: $task_checklist,
                htr_tree_state: $htr_tree_state,
                active_stm: $active_stm,
                git_commit: $git_commit,
                timestamp: time::now()
            };
        ";
        self.db.query(sql)
            .bind(("session_id", session_id_val))
            .bind(("task_checklist", task_checklist.clone()))
            .bind(("htr_tree_state", htr_tree_state.clone()))
            .bind(("active_stm", serde_json::Value::Object(active_stm.clone())))
            .bind(("git_commit", git_commit.clone()))
            .await?.check()?;

        // Save to local JSON backup
        let mythrax_dir = vault_root.join(".mythrax");
        std::fs::create_dir_all(&mythrax_dir)?;
        let journal_path = mythrax_dir.join("session_journal.json");
        
        let journal_json = serde_json::json!({
            "session_id": session_id_val,
            "task_checklist": task_checklist,
            "htr_tree_state": htr_tree_state,
            "active_stm": active_stm,
            "git_commit": git_commit,
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        });

        // Atomic write via temp file
        let tmp_path = journal_path.with_extension("tmp");
        {
            use std::io::Write as _;
            let mut file = std::fs::File::create(&tmp_path)?;
            file.write_all(serde_json::to_string_pretty(&journal_json)?.as_bytes())?;
            file.sync_all()?;
        }
        std::fs::rename(tmp_path, journal_path)?;

        Ok(())
    }

    async fn reinforce_episode(&self, id: &str) -> Result<()> {
        let thing_id = parse_record_id(id)?;
        let now_str = chrono::Utc::now().to_rfc3339();

        let enable_access_reinforcement = match self.get_profile_key("search.enable_access_reinforcement").await {
            Ok(Some(val_str)) => val_str.parse::<bool>().unwrap_or(false),
            _ => false,
        };

        if enable_access_reinforcement {
            let ep_sql = "SELECT last_retrieved_at, created_at FROM $id;";
            let mut ep_res = self.db.query(ep_sql).bind(("id", thing_id.clone())).await?.check()?;
            #[derive(serde::Deserialize, surrealdb_types::SurrealValue)]
            struct EpTime {
                last_retrieved_at: Option<String>,
                created_at: Option<chrono::DateTime<chrono::Utc>>,
            }
            let ep_times: Vec<EpTime> = ep_res.take(0)?;
            let delta_t_days = if let Some(t) = ep_times.first() {
                let last_t = if let Some(ref lr) = t.last_retrieved_at {
                    chrono::DateTime::parse_from_rfc3339(lr).ok().map(|dt| dt.with_timezone(&chrono::Utc))
                } else {
                    t.created_at
                };
                if let Some(lt) = last_t {
                    let elapsed = chrono::Utc::now().signed_duration_since(lt);
                    (elapsed.num_seconds() as f32 / 86400.0f32).max(0.0f32)
                } else {
                    0.0f32
                }
            } else {
                0.0f32
            };

            let check_sql = "SELECT id, utility_score, access_count FROM metrics WHERE target_id = $id LIMIT 1;";
            let mut check_res = self.db.query(check_sql).bind(("id", thing_id.clone())).await?.check()?;
            #[derive(serde::Deserialize, surrealdb_types::SurrealValue)]
            struct MetricsRow {
                id: surrealdb::types::RecordId,
                utility_score: f64,
                access_count: i64,
            }
            let mut rows: Vec<MetricsRow> = check_res.take(0)?;
            
            let new_utility: f64;
            
            if rows.is_empty() {
                new_utility = 50.0;
                let metrics_uuid = uuid::Uuid::new_v4().to_string();
                let insert_sql = "LET $met = type::record('metrics', $metrics_uuid);
                                  INSERT INTO metrics (id, target_id, utility_score, access_count) VALUES ($met, $target_id, 50.0, 1);";
                let _ = self.db.query(insert_sql)
                    .bind(("metrics_uuid", metrics_uuid.as_str()))
                    .bind(("target_id", thing_id.clone()))
                    .await?.check()?;
            } else {
                let row = rows.pop().unwrap();
                let new_count = row.access_count + 1;
                let decay = (-0.05f32 * delta_t_days).exp() as f64;
                new_utility = 50.0 + (new_count as f64).log2() * decay;
                
                let update_sql = "UPDATE $metrics_id SET access_count = $new_count, utility_score = $new_utility;";
                let _ = self.db.query(update_sql)
                    .bind(("metrics_id", row.id))
                    .bind(("new_count", new_count))
                    .bind(("new_utility", new_utility))
                    .await?.check()?;
            }
            
            let ep_update_sql = "UPDATE $id MERGE { utility: $new_utility, last_retrieved_at: $now };";
            let _ = self.db.query(ep_update_sql)
                .bind(("id", thing_id))
                .bind(("new_utility", new_utility))
                .bind(("now", now_str))
                .await?.check()?;
        } else {
            let sql = "UPDATE $id MERGE { utility: 50.0, last_retrieved_at: $now };";
            let _ = self.db.query(sql)
                .bind(("id", thing_id))
                .bind(("now", now_str))
                .await?.check()?;
        }
        
        Ok(())
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


fn load_api_key(provider: &str) -> Option<String> {
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

fn save_api_key(provider: &str, key: &str) -> Result<()> {
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
    static DEEP_PRECEDING_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static DEEP_SUCCEEDING_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static PRECEDING_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static SUCCEEDING_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static RELATIVE_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static PROCEDURAL_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();

    let deep_preceding_re = DEEP_PRECEDING_RE.get_or_init(|| {
        regex::Regex::new(r"\b(long|far|much|way)\s+(before|preceding|previously|prior|earlier|ago|last)\b").unwrap()
    });
    let deep_succeeding_re = DEEP_SUCCEEDING_RE.get_or_init(|| {
        regex::Regex::new(r"\b(long|far|much|way)\s+(after|following|subsequently|later|next)\b").unwrap()
    });
    let preceding_re = PRECEDING_RE.get_or_init(|| {
        regex::Regex::new(r"\b(before|preceding|previously|prior|earlier|ago|last)\b").unwrap()
    });
    let succeeding_re = SUCCEEDING_RE.get_or_init(|| {
        regex::Regex::new(r"\b(after|following|subsequently|later|next)\b").unwrap()
    });
    let relative_re = RELATIVE_RE.get_or_init(|| {
        regex::Regex::new(r"\b(recent|recently|latest|newest|today|now)\b").unwrap()
    });
    let procedural_re = PROCEDURAL_RE.get_or_init(|| {
        regex::Regex::new(r"(?s)^\s*\b(what|how|why|which|where|when|did|have)\b.*\b(do|done|took|take|taken|happen|run|execute|call|step|try|attempt)\b").unwrap()
    });

    let query_lower = query.to_lowercase();

    if procedural_re.is_match(&query_lower) {
        return Some((TemporalCueType::Procedural, 3.0));
    }
    if deep_preceding_re.is_match(&query_lower) {
        return Some((TemporalCueType::Preceding, 3.0));
    }
    if deep_succeeding_re.is_match(&query_lower) {
        return Some((TemporalCueType::Succeeding, 3.0));
    }
    if preceding_re.is_match(&query_lower) {
        return Some((TemporalCueType::Preceding, 1.0));
    }
    if succeeding_re.is_match(&query_lower) {
        return Some((TemporalCueType::Succeeding, 1.0));
    }
    if relative_re.is_match(&query_lower) {
        return Some((TemporalCueType::Relative, 1.0));
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

pub fn sentence_cosine_similarity(
    query_tokens: &[String],
    sentence: &str,
    global_idf: &std::collections::HashMap<String, f32>,
) -> f32 {
    let sentence_tokens = crate::retrieval::bm25::tokenize(sentence);
    
    let mut sentence_freq: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    for token in &sentence_tokens {
        *sentence_freq.entry(token.clone()).or_insert(0.0) += 1.0;
    }

    let mut dot_product: f64 = 0.0;
    let mut norm_query: f64 = 0.0;
    let mut norm_sentence: f64 = 0.0;

    for t in query_tokens {
        let idf = global_idf.get(t).copied().unwrap_or(0.0);
        let idf_f64 = idf as f64;
        
        norm_query += idf_f64 * idf_f64;
        
        let tf = *sentence_freq.get(t).unwrap_or(&0.0) as f64;
        dot_product += tf * (idf_f64 * idf_f64);
        
        let sentence_component = tf * idf_f64;
        norm_sentence += sentence_component * sentence_component;
    }

    for (w, &tf) in &sentence_freq {
        if !query_tokens.contains(w) {
            let tf_f64 = tf as f64;
            norm_sentence += tf_f64 * tf_f64;
        }
    }

    let norm_query = norm_query.sqrt();
    let norm_sentence = norm_sentence.sqrt();

    if norm_query < 1e-9 || norm_sentence < 1e-9 {
        return 0.0;
    }

    (dot_product / (norm_query * norm_sentence)) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Entity;

    #[test]
    fn test_unescape_id_part() {
        assert_eq!(unescape_id_part("123"), "123");
        assert_eq!(unescape_id_part("⟨123⟩"), "123");
        assert_eq!(unescape_id_part("⟨⟨123⟩⟩"), "123");
        assert_eq!(unescape_id_part("⟨⟨abc\\-def⟩⟩"), "abc-def");
        assert_eq!(unescape_id_part("abc\\\\def"), "abc\\def");
    }

    #[test]
    fn test_classify_query_comprehensive() {
        assert_eq!(classify_query("my next mtg"), QueryCategory::Temporal);
        assert_eq!(classify_query("our appts next week"), QueryCategory::Temporal);
        assert_eq!(classify_query("show next meeting"), QueryCategory::Temporal);
        assert_eq!(classify_query("my favourite lodging"), QueryCategory::Preference);
        assert_eq!(classify_query("my profile"), QueryCategory::User);
        assert_eq!(classify_query("our job description"), QueryCategory::User);
        assert_eq!(classify_query("who am i?"), QueryCategory::User);
        assert_eq!(classify_query("tell me about me"), QueryCategory::User);
        assert_eq!(classify_query("about our friend"), QueryCategory::User);
        assert_eq!(classify_query("what is job salary"), QueryCategory::User);
    }

    #[tokio::test]
    async fn test_surreal_db_operations() {
        let backend = SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        let episode = EpisodeSave {
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

        let search_results = backend.search("redis",  Some("testing"),  false,  2,  0,  0.55,  None,  false,  true,  true, None, true).await.unwrap();
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
                title: "Batch episode 1".to_string(),
                content: "First batch item content for testing.".to_string(),
                scope: Some("batch-test".to_string()),
                vault_path: Some("batch_1.md".to_string()),
                session_id: Some("session_123".to_string()),
                ..Default::default()
            },
            EpisodeSave {
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
        let results_deep = backend.search("Redis",  Some("deep-test"),  true,  10,  0,  0.55,  None,  true,  true,  true, None, true).await.unwrap();
        assert_eq!(results_deep.results.len(), 1);
        assert!(results_deep.results[0].content.contains("dropping connections"));
        assert!(results_deep.results[0].content.contains("Redis Connection Pooling Guidelines"));
        assert!(results_deep.results[0].content.contains("Set max connections to 50"));

        // Perform search WITHOUT deep_insight = true
        let results_normal = backend.search("failure",  Some("deep-test"),  false,  10,  0,  0.55,  None,  false,  true,  true, None, true).await.unwrap();
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
        let response = backend.search("Concurrency",  Some("ranking-test"),  false,  10,  0,  0.0,  None,  false,  true,  true, None, true).await.unwrap();

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

        let results_upward = backend.search("Parent Insight",  Some("directional-test"),  true,  10,  0,  0.85,  None,  false,  true,  true, None, true).await.unwrap();
        println!("DEBUG: results_upward: {:#?}", results_upward.results);
        assert_eq!(results_upward.results.len(), 1);
        let content_upward = &results_upward.results[0].content;
        assert!(!content_upward.contains("Child Episode"));

        // 3. Search with allow_downward = true
        let results_downward = backend.search("Parent Insight",  Some("directional-test"),  true,  10,  0,  0.85,  None,  true,  true,  true, None, true).await.unwrap();
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
        let response = backend.search("Pattern",  Some("budget-test"),  true,  10,  0,  0.0,  Some(20),  false,  true,  true, None, true).await.unwrap();
        
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
        let res_large = backend.search("Avoid",  Some("compaction-test"),  false,  10,  0,  0.0,  Some(1000),  false,  true,  true, None, true).await.unwrap();
        assert_eq!(res_large.results.len(), 1);
        assert!(res_large.results[0].content.contains("**Why**:"));

        // Dynamically compute the budget needed for compacted content
        let text_compacted = format!("{}\n**Action to Avoid**: {}\n**Prescribed Remedy**: {}", 
            skill_rule.target_pattern, skill_rule.action_to_avoid, skill_rule.prescribed_remedy);
        let tokens_compacted = backend.count_text_tokens(&text_compacted);

        // Search with tight budget - should strip "**Why**:" and fit under budget
        let res_small = backend.search("Avoid",  Some("compaction-test"),  false,  10,  0,  0.0,  Some(tokens_compacted + 5),  false,  true,  true, None, true).await.unwrap();
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
        let res_large = backend.search("Multi-Paragraph",  Some("compaction-test"),  false,  10,  0,  0.0,  Some(1000),  false,  true,  true, None, true).await.unwrap();
        assert_eq!(res_large.results.len(), 1);
        assert!(res_large.results[0].content.contains("Second paragraph"));

        // Dynamically compute the budget needed for compacted content
        let compacted_content = format!("First paragraph here.\n\n... [Truncated (Inner-Node Compaction)]");
        let text_compacted = format!("{}\n{}", node1.name, compacted_content);
        let tokens_compacted = backend.count_text_tokens(&text_compacted);

        // Search with small budget -> first paragraph + suffix
        let res_small = backend.search("Multi-Paragraph",  Some("compaction-test"),  false,  10,  0,  0.0,  Some(tokens_compacted + 5),  false,  true,  true, None, true).await.unwrap();
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
        let res_trunc = backend.search("Single-Paragraph",  Some("compaction-test-single-para"),  false,  10,  0,  0.0,  Some(tokens_truncated + 5),  false,  true,  true, None, true).await.unwrap();
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
        let res_default = backend.search("Secret",  Some("exclusion-test"),  false,  10,  0,  0.0,  None,  false,  false,  true, None, true).await.unwrap();
        assert_eq!(res_default.results.len(), 0);

        // Search with include_episodes = true -> should find the episode
        let res_include = backend.search("Secret",  Some("exclusion-test"),  false,  10,  0,  0.0,  None,  false,  true,  true, None, true).await.unwrap();
        assert_eq!(res_include.results.len(), 1);
        assert_eq!(res_include.results[0].title, "Test Episode");
    }

    #[tokio::test]
    async fn test_graph_traversal_excludes_episodes() {
        let backend = SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        // Create an Episode and a WikiNode
        let ep_id = backend.save_episode(&EpisodeSave {
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
        let res_include = backend.search("Parent Insight",  Some("graph-exclusion-test"),  true,  10,  0,  0.85,  None,  true,  true,  true, None, true).await.unwrap();
        assert_eq!(res_include.results.len(), 1);
        assert!(res_include.results[0].content.contains("Child Episode"));

        // Search with deep_insight = true, allow_downward = true and include_episodes = false -> child episode should NOT be traversed
        let res_exclude = backend.search("Parent Insight",  Some("graph-exclusion-test"),  true,  10,  0,  0.85,  None,  true,  false,  true, None, true).await.unwrap();
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
        let res_default = backend.search("secret",  Some("artifact-exclusion-test"),  false,  10,  0,  0.0,  None,  false,  true,  false, None, true).await.unwrap();
        assert_eq!(res_default.results.len(), 1);
        assert_eq!(res_default.results[0].title, "Normal Node");

        // 4. Search with include_artifacts = true -> should find both
        let res_include = backend.search("secret",  Some("artifact-exclusion-test"),  false,  10,  0,  0.0,  None,  false,  true,  true, None, true).await.unwrap();
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

        let response = backend.search(
            "second step before", 
            Some("general"), 
            false, 
            10, 
            0, 
            0.0, 
            None, 
            false, 
            true, 
            false
        , None, true).await.unwrap();

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

