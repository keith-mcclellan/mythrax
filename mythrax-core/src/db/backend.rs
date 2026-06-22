use axum::async_trait;
use crate::contracts::{EpisodeSave, SearchResult, WisdomRule, LlmConfigResponse, LlmConfigRequest, Episode, HandoffSave};
use anyhow::{Result, Context};
use surrealdb::engine::local::{Db, Mem, RocksDb};
use surrealdb::Surreal;
use std::sync::Arc;
use uuid::Uuid;
use crate::db::schema::INIT_SCHEMA;

pub fn unescape_id_part(part: &str) -> String {
    let mut s = part.trim();
    while s.starts_with('⟨') {
        s = &s['⟨'.len_utf8()..];
    }
    while s.ends_with('⟩') {
        s = &s[..s.len() - '⟩'.len_utf8()];
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

pub fn parse_record_id(id_str: &str) -> Result<surrealdb::sql::Thing> {
    let parts: Vec<&str> = id_str.splitn(2, ':').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid Record ID format: {}", id_str);
    }
    let table = parts[0].to_string();
    let raw_id = unescape_id_part(parts[1]);
    Ok(surrealdb::sql::Thing {
        tb: table,
        id: surrealdb::sql::Id::from(raw_id),
    })
}

pub fn format_record_id(thing: &surrealdb::sql::Thing) -> String {
    let raw_id = match &thing.id {
        surrealdb::sql::Id::String(s) => unescape_id_part(s),
        other => unescape_id_part(&other.to_string()),
    };
    format!("{}:{}", thing.tb, raw_id)
}

#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn init(&self) -> Result<()>;
    async fn save_episode(&self, episode: &EpisodeSave) -> Result<String>;
    async fn save_wisdom_rule(&self, rule: &WisdomRule) -> Result<String>;
    async fn search(&self, query: &str, scope: Option<&str>, deep_insight: bool, limit: usize, offset: usize) -> Result<Vec<SearchResult>>;
    async fn get_wisdom(&self, query: &str, tier: &str, limit: usize) -> Result<Vec<WisdomRule>>;
    async fn record_feedback(&self, id: &str, success: bool) -> Result<()>;
    async fn apply_migrations(&self) -> Result<()>;
    async fn get_llm_config(&self) -> Result<LlmConfigResponse>;
    async fn update_llm_config(&self, req: &LlmConfigRequest) -> Result<()>;
    async fn get_unprocessed_episodes(&self) -> Result<Vec<Episode>>;
    async fn mark_episode_processed(&self, id: &str) -> Result<()>;
    async fn get_all_episodes(&self) -> Result<Vec<Episode>>;
    async fn save_profile_key(&self, key: &str, value: &str) -> Result<()>;
    async fn get_profile_key(&self, key: &str) -> Result<Option<String>>;
    async fn save_handoff(&self, handoff: &HandoffSave) -> Result<String>;
}

pub struct SurrealBackend {
    pub db: Surreal<Db>,
    pub embedder: Option<Arc<crate::embeddings::LocalEmbedder>>,
}

impl SurrealBackend {
    pub async fn new(url: &str) -> Result<Self> {
        let db = if url.starts_with("rocksdb://") {
            let path = url.strip_prefix("rocksdb://").unwrap();
            if let Some(parent) = std::path::Path::new(path).parent() {
                std::fs::create_dir_all(parent)?;
            }
            Surreal::new::<RocksDb>(path).await
                .context(format!("Failed to initialize SurrealDB with RocksDB at: {}", path))?
        } else {
            Surreal::new::<Mem>(()).await
                .context("Failed to initialize SurrealDB with in-memory store")?
        };
        db.use_ns("mythrax").use_db("memory").await?;

        let embedder = match crate::embeddings::LocalEmbedder::new() {
            Ok(emb) => Some(Arc::new(emb)),
            Err(e) => {
                tracing::warn!("Failed to initialize LocalEmbedder: {}. Falling back to non-embedded mode.", e);
                None
            }
        };

        Ok(Self { db, embedder })
    }

    pub async fn new_in_memory() -> Result<Self> {
        Self::new("mem://").await
    }
}

#[derive(serde::Deserialize, Debug)]
struct SearchRaw {
    id: surrealdb::sql::Thing,
    title: String,
    content: String,
    related_nodes: Option<Vec<RelatedNodeRaw>>,
}

#[derive(serde::Deserialize, Debug)]
struct RelatedNodeRaw {
    id: surrealdb::sql::Thing,
    title: Option<String>,
    name: Option<String>,
    content: Option<String>,
    summary: Option<String>,
    target_pattern: Option<String>,
    causal_explanation: Option<String>,
    action_to_avoid: Option<String>,
    prescribed_remedy: Option<String>,
}

#[async_trait]
impl StorageBackend for SurrealBackend {
    async fn init(&self) -> Result<()> {
        self.db.query(INIT_SCHEMA).await?
            .check().context("Applying schemas failed")?;

        // Initialize default configuration if config:settings does not exist
        let check_sql = "SELECT * FROM config:settings;";
        let mut response = self.db.query(check_sql).await?.check().context("Check config failed")?;
        let config_opt: Option<LlmConfigResponse> = response.take(0)?;
        if config_opt.is_none() {
            let insert_sql = "
                CREATE config:settings CONTENT {
                    active_provider: 'local',
                    model: 'mlx-community/gemma-4-26b-a4b-it-4bit',
                    cloud_provider: 'gemini',
                    is_override: false,
                    expires_at: NONE
                };
            ";
            self.db.query(insert_sql).await?.check().context("Insert default config failed")?;
        }

        Ok(())
    }

    async fn save_episode(&self, episode: &EpisodeSave) -> Result<String> {
        let mut ep_uuid = Uuid::new_v4().to_string();
        let mut is_update = false;

        if let Some(ref vp) = episode.vault_path {
            let check_query = "SELECT VALUE id FROM episode WHERE vault_path = $vault_path LIMIT 1;";
            let mut response = self.db.query(check_query).bind(("vault_path", vp)).await?;
            let ids: Option<surrealdb::sql::Thing> = response.take(0)?;
            if let Some(thing) = ids {
                ep_uuid = match &thing.id {
                    surrealdb::sql::Id::String(s) => unescape_id_part(s),
                    other => unescape_id_part(&other.to_string()),
                };
                is_update = true;
            }
        }

        let query_str = if is_update {
            "
                BEGIN TRANSACTION;
                LET $ep = type::thing('episode', $ep_uuid);
                UPDATE $ep MERGE {
                    title: $title,
                    content: $content,
                    scope: $target_scope,
                    vault_path: $vault_path,
                    processed_in_dream: false,
                    embedding: $embedding
                };
                DELETE FROM mentions WHERE in = $ep;
                COMMIT TRANSACTION;
            "
        } else {
            "
                BEGIN TRANSACTION;
                LET $ep = type::thing('episode', $ep_uuid);
                LET $met = type::thing('metrics', $metrics_uuid);
                
                CREATE $ep CONTENT {
                    title: $title,
                    content: $content,
                    scope: $target_scope,
                    vault_path: $vault_path,
                    processed_in_dream: false,
                    embedding: $embedding
                };
                
                CREATE $met CONTENT {
                    target_id: $ep,
                    utility_score: 1.0,
                    access_count: 0
                };
                
                COMMIT TRANSACTION;
            "
        };

        let metrics_uuid = Uuid::new_v4().to_string();
        let scope_val = episode.scope.clone().unwrap_or_else(|| "general".to_string());
        let vp_val = episode.vault_path.clone().unwrap_or_else(|| "".to_string());

        let embedding_val = if let Some(ref embedder) = self.embedder {
            let text_to_embed = format!("{}: {}", episode.title, episode.content);
            match embedder.embed(&text_to_embed) {
                Ok(vec) => Some(vec),
                Err(e) => {
                    tracing::warn!("Embedding generation failed in save_episode: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let response = self.db.query(query_str)
            .bind(("ep_uuid", &ep_uuid))
            .bind(("metrics_uuid", &metrics_uuid))
            .bind(("title", &episode.title))
            .bind(("content", &episode.content))
            .bind(("target_scope", &scope_val))
            .bind(("vault_path", &vp_val))
            .bind(("embedding", &embedding_val))
            .await?;

        println!("DEBUG: save_episode query response: {:#?}", response);
        response.check().context("SurrealDB save_episode transaction failed")?;

        for entity in &episode.entities {
            let entity_query = "
                BEGIN TRANSACTION;
                LET $ent_id = type::thing('entity', $name);
                INSERT INTO entity (id, name, entity_type, summary, labels, scope)
                VALUES ($ent_id, $name, $entity_type, $summary, $labels, $target_scope)
                ON DUPLICATE KEY UPDATE
                    summary = $summary,
                    labels = $labels,
                    scope = $target_scope;
                
                -- Relate episode to entity
                LET $ep = type::thing('episode', $ep_uuid);
                RELATE $ep -> mentions -> $ent_id CONTENT {
                    created_at: time::now()
                };
                COMMIT TRANSACTION;
            ";
            let _ = self.db.query(entity_query)
                .bind(("name", &entity.name))
                .bind(("entity_type", &entity.entity_type))
                .bind(("summary", &entity.summary))
                .bind(("labels", &entity.labels))
                .bind(("target_scope", &scope_val))
                .bind(("ep_uuid", &ep_uuid))
                .await?
                .check().context("Entity relation query failed")?;
        }

        Ok(format!("episode:{}", ep_uuid))
    }

    async fn save_wisdom_rule(&self, rule: &WisdomRule) -> Result<String> {
        let mut rule_uuid = Uuid::new_v4().to_string();
        let mut is_update = false;

        if let Some(ref vp) = rule.vault_path {
            let check_query = "SELECT VALUE id FROM wisdom WHERE vault_path = $vault_path LIMIT 1;";
            let mut response = self.db.query(check_query).bind(("vault_path", vp)).await?;
            let ids: Option<surrealdb::sql::Thing> = response.take(0)?;
            if let Some(thing) = ids {
                rule_uuid = match &thing.id {
                    surrealdb::sql::Id::String(s) => unescape_id_part(s),
                    other => unescape_id_part(&other.to_string()),
                };
                is_update = true;
            }
        }

        let query_str = if is_update {
            "
                BEGIN TRANSACTION;
                LET $rule = type::thing('wisdom', $rule_uuid);
                UPDATE $rule MERGE {
                    target_pattern: $target_pattern,
                    action_to_avoid: $action_to_avoid,
                    causal_explanation: $causal_explanation,
                    prescribed_remedy: $prescribed_remedy,
                    tier: $tier,
                    scope: $target_scope,
                    vault_path: $vault_path,
                    source_episodes: $source_episodes,
                    generator_name: $generator_name
                };
                COMMIT TRANSACTION;
            "
        } else {
            "
                BEGIN TRANSACTION;
                LET $rule = type::thing('wisdom', $rule_uuid);
                LET $met = type::thing('metrics', $metrics_uuid);
                
                CREATE $rule CONTENT {
                    target_pattern: $target_pattern,
                    action_to_avoid: $action_to_avoid,
                    causal_explanation: $causal_explanation,
                    prescribed_remedy: $prescribed_remedy,
                    tier: $tier,
                    scope: $target_scope,
                    vault_path: $vault_path,
                    source_episodes: $source_episodes,
                    generator_name: $generator_name
                };
                
                CREATE $met CONTENT {
                    target_id: $rule,
                    utility_score: 1.0,
                    access_count: 0
                };
                
                COMMIT TRANSACTION;
            "
        };

        let metrics_uuid = Uuid::new_v4().to_string();
        let vp_val = rule.vault_path.clone().unwrap_or_else(|| "".to_string());

        let _ = self.db.query(query_str)
            .bind(("rule_uuid", &rule_uuid))
            .bind(("metrics_uuid", &metrics_uuid))
            .bind(("target_pattern", &rule.target_pattern))
            .bind(("action_to_avoid", &rule.action_to_avoid))
            .bind(("causal_explanation", &rule.causal_explanation))
            .bind(("prescribed_remedy", &rule.prescribed_remedy))
            .bind(("tier", &rule.tier))
            .bind(("target_scope", &rule.scope))
            .bind(("vault_path", &vp_val))
            .bind(("source_episodes", &rule.source_episodes))
            .bind(("generator_name", &rule.generator_name))
            .await?
            .check().context("SurrealDB save_wisdom_rule transaction failed")?;

        Ok(format!("wisdom:{}", rule_uuid))
    }

    async fn search(&self, query: &str, scope: Option<&str>, deep_insight: bool, limit: usize, offset: usize) -> Result<Vec<SearchResult>> {
        let scope_val = scope.map(|s| s.to_string());
        
        let sql = if deep_insight {
            "
            SELECT id, title, content, 
                   <->(relates_to, mentions)<->(episode, entity, wiki_node, wisdom, hypothesis_node, handoff).* AS related_nodes
            FROM episode 
            WHERE (string::contains(title, $query) OR string::contains(content, $query)) 
              AND (scope = $target_scope OR $target_scope = NONE)
            LIMIT $limit START $offset;
            "
        } else {
            "
            SELECT id, title, content
            FROM episode 
            WHERE (string::contains(title, $query) OR string::contains(content, $query)) 
              AND (scope = $target_scope OR $target_scope = NONE)
            LIMIT $limit START $offset;
            "
        };

        let mut response = self.db.query(sql)
            .bind(("query", query))
            .bind(("target_scope", scope_val))
            .bind(("limit", limit))
            .bind(("offset", offset))
            .await?
            .check().context("Search query failed")?;

        let episodes: Vec<SearchRaw> = response.take(0)?;
        let mut results = Vec::new();

        for ep in episodes {
            let mut content = ep.content.clone();
            
            if deep_insight {
                if let Some(ref related) = ep.related_nodes {
                    if !related.is_empty() {
                        content.push_str("\n\n---\n### Related Context\n");
                        for node in related {
                            let table = &node.id.tb;
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
                        content = content.trim_end().to_string();
                    }
                }
            }

            results.push(SearchResult {
                id: format_record_id(&ep.id),
                title: ep.title,
                content,
                similarity: 1.0,
                utility: 1.0,
                tier: "Standard".to_string(),
                embedding: None,
                vault_path: None,
                source_episode: None,
            });
        }

        Ok(results)
    }

    async fn get_wisdom(&self, query: &str, tier: &str, limit: usize) -> Result<Vec<WisdomRule>> {
        let sql = "
            SELECT * FROM wisdom 
            WHERE tier = $tier AND (string::contains(target_pattern, $query) OR string::contains(causal_explanation, $query))
            LIMIT $limit;
        ";
        let mut response = self.db.query(sql)
            .bind(("tier", tier))
            .bind(("query", query))
            .bind(("limit", limit))
            .await?
            .check().context("Get wisdom query failed")?;

        let wisdom: Vec<WisdomRule> = response.take(0)?;
        let normalized_wisdom = wisdom.into_iter().map(|mut w| {
            if let Some(ref id_str) = w.id {
                if let Ok(thing) = parse_record_id(&id_str) {
                    w.id = Some(format_record_id(&thing));
                }
            }
            w
        }).collect();
        Ok(normalized_wisdom)
    }

    async fn record_feedback(&self, id: &str, success: bool) -> Result<()> {
        let thing_id = parse_record_id(id)?;
        
        let fetch_sql = "SELECT VALUE utility_score FROM metrics WHERE target_id = $target_id LIMIT 1;";
        let mut response = self.db.query(fetch_sql).bind(("target_id", &thing_id)).await?.check().context("Fetch metrics query failed")?;
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
            .bind(("target_id", &thing_id))
            .await?
            .check().context("Update metrics query failed")?;
        
        Ok(())
    }

    async fn apply_migrations(&self) -> Result<()> {
        Ok(())
    }

    async fn get_llm_config(&self) -> Result<LlmConfigResponse> {
        let sql = "SELECT active_provider, model, cloud_provider, is_override, expires_at FROM config:settings;";
        let mut response = self.db.query(sql).await?.check().context("Get config query failed")?;
        let config_opt: Option<LlmConfigResponse> = response.take(0)?;
        let mut config = if let Some(c) = config_opt {
            c
        } else {
            LlmConfigResponse {
                active_provider: "local".to_string(),
                cloud_provider: "gemini".to_string(),
                model: "mlx-community/gemma-4-26b-a4b-it-4bit".to_string(),
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
        let sql_select = "SELECT active_provider, model, cloud_provider, is_override, expires_at FROM config:settings;";
        let mut select_res = self.db.query(sql_select).await?.check().context("Get config query failed")?;
        let existing: Option<LlmConfigResponse> = select_res.take(0)?;

        let mut current_model = req.model.clone();
        let mut current_cloud_provider = req.cloud_provider.clone();

        if current_model.is_none() || current_cloud_provider.is_none() {
            let (default_model, default_cloud_provider) = if req.provider == "local" {
                ("mlx-community/gemma-4-26b-a4b-it-4bit".to_string(), "gemini".to_string())
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

        let expires_at = if req.duration.as_deref() != Some("permanent") {
            Some("2026-06-21T23:59:59Z".to_string())
        } else {
            None
        };

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
            .bind(("active_provider", &req.provider))
            .bind(("model", &model))
            .bind(("cloud_provider", &cloud_provider))
            .bind(("expires_at", &expires_at))
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
        #[derive(serde::Deserialize)]
        struct EpisodeRaw {
            id: surrealdb::sql::Thing,
            title: String,
            content: String,
            source: Option<String>,
            scope: Option<String>,
            vault_path: Option<String>,
            embedding: Option<Vec<f32>>,
            processed_in_dream: Option<bool>,
            source_episode: Option<surrealdb::sql::Thing>,
        }

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
        #[derive(serde::Deserialize)]
        struct EpisodeRaw {
            id: surrealdb::sql::Thing,
            title: String,
            content: String,
            source: Option<String>,
            scope: Option<String>,
            vault_path: Option<String>,
            embedding: Option<Vec<f32>>,
            processed_in_dream: Option<bool>,
            source_episode: Option<surrealdb::sql::Thing>,
        }

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
        }).collect();
        Ok(episodes)
    }

    async fn save_profile_key(&self, key: &str, value: &str) -> Result<()> {
        let sql = "
            UPSERT type::thing('profile', $key) CONTENT {
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
        #[derive(serde::Deserialize)]
        struct ProfileRaw {
            value: String,
        }
        let sql = "SELECT value FROM type::thing('profile', $key);";
        let mut response = self.db.query(sql)
            .bind(("key", key))
            .await?.check().context("SELECT profile failed")?;
        let res: Option<ProfileRaw> = response.take(0)?;
        Ok(res.map(|r| r.value))
    }

    async fn save_handoff(&self, handoff: &HandoffSave) -> Result<String> {
        let id_str = format!("⟨{}⟩", Uuid::new_v4());
        let query = "
            CREATE type::thing('handoff', $id) CONTENT {
                parent_conversation_id: $parent,
                subagent_conversation_id: $subagent,
                summary: $summary,
                handoff_file_path: $path,
                scope: $target_scope
            };
        ";
        self.db.query(query)
            .bind(("id", id_str.clone()))
            .bind(("parent", &handoff.parent_conversation_id))
            .bind(("subagent", &handoff.subagent_conversation_id))
            .bind(("summary", &handoff.summary))
            .bind(("path", &handoff.handoff_file_path))
            .bind(("target_scope", handoff.scope.as_deref().unwrap_or("general")))
            .await?.check()?;
        Ok(format!("handoff:{}", id_str))
    }
}

fn load_api_key(provider: &str) -> Option<String> {
    if let Ok(home) = std::env::var("HOME") {
        let keys_path = std::path::PathBuf::from(&home).join(".mythrax/keys.json");
        if keys_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&keys_path) {
                if let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&content) {
                    if let Some(val) = map.get(provider) {
                        if let Some(s) = val.as_str() {
                            return Some(s.to_string());
                        }
                    }
                }
            }
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
        };

        let ep_id = backend.save_episode(&episode).await.unwrap();
        assert!(ep_id.contains("episode"));

        // Call again to test the is_update update path
        let ep_id2 = backend.save_episode(&episode).await.unwrap();
        assert_eq!(ep_id, ep_id2);

        let all_eps: Vec<serde_json::Value> = backend.db.select("episode").await.unwrap();
        println!("DEBUG: All episodes in DB: {:?}", all_eps);

        let search_results = backend.search("redis", Some("testing"), false, 2, 0).await.unwrap();
        assert_eq!(search_results.len(), 1);
        assert!(search_results[0].content.contains("redis"));

        backend.record_feedback(&ep_id, false).await.unwrap();
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
        };

        let ep_id = backend.save_episode(&episode).await.unwrap();
        let ep_thing = parse_record_id(&ep_id).unwrap();

        // Create a wiki_node
        let _ = backend.db.query("
            CREATE type::thing('wiki_node', 'pool_size') CONTENT {
                name: 'Redis Connection Pooling Guidelines',
                content: 'Set max connections to 50 under high concurrency environments.',
                scope: 'deep-test'
            };
        ").await.unwrap().check().unwrap();

        let wiki_thing = surrealdb::sql::Thing {
            tb: "wiki_node".to_string(),
            id: surrealdb::sql::Id::from("pool_size".to_string()),
        };

        // Relate the episode to the wiki_node
        let _ = backend.db.query("RELATE $from -> relates_to -> $to;")
            .bind(("from", &ep_thing))
            .bind(("to", &wiki_thing))
            .await.unwrap()
            .check().unwrap();

        // Perform search WITH deep_insight = true
        let results_deep = backend.search("Redis", Some("deep-test"), true, 10, 0).await.unwrap();
        assert_eq!(results_deep.len(), 1);
        assert!(results_deep[0].content.contains("dropping connections"));
        assert!(results_deep[0].content.contains("Redis Connection Pooling Guidelines"));
        assert!(results_deep[0].content.contains("Set max connections to 50"));

        // Perform search WITHOUT deep_insight = true
        let results_normal = backend.search("Redis", Some("deep-test"), false, 10, 0).await.unwrap();
        assert_eq!(results_normal.len(), 1);
        assert!(results_normal[0].content.contains("dropping connections"));
        assert!(!results_normal[0].content.contains("Redis Connection Pooling Guidelines"));
    }
}
