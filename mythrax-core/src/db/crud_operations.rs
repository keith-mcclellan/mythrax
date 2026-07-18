use crate::db::backend::{
    SurrealBackend, StorageBackend, format_record_id, record_key_to_string, unescape_id_part, parse_record_id,
    WikiNodeRaw, WisdomRaw, HandoffRaw, EpisodeRaw, ScoredEdge, PROFILE_CACHE,
    load_api_key, save_api_key, get_user_prefix,
};
use crate::contracts::{
    EpisodeSave, WisdomRule, LlmConfigResponse, LlmConfigRequest, Episode, HandoffSave, WikiNode,
    GetMemoryNodesResponse,
};
use crate::db::schema::INIT_SCHEMA;
use surrealdb_types::SurrealValue;
use anyhow::{Result, Context};
use uuid::Uuid;


impl SurrealBackend {
    pub async fn init_db(&self) -> Result<()> {
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
        let load_tuned = std::env::var("MYTHRAX_LOAD_TUNED_PARAMS")
            .map(|v| v != "false")
            .unwrap_or(true);
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
                            if let Err(e) = self.save_profile_key(&k, &val_str).await {
                                tracing::warn!("Failed to save profile key {} during initialization: {:?}", k, e);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn save_episode_db(&self, episode: &EpisodeSave) -> Result<String> {
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
                    node_type: $node_type,
                    created_at: type::datetime($created_at),
                    importance: $importance
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
                    node_type: $node_type,
                    created_at: type::datetime($created_at),
                    importance: $importance
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
        let created_at_val = episode.created_at.clone().unwrap_or_else(|| now_str.clone());

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
            .bind(("created_at", created_at_val))
            .bind(("importance", episode.importance.unwrap_or(5.0)))
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

        // Update in-memory term counts cache for search IDF acceleration
        self.record_episode_tokens_for_cache(&scope_val, &episode.content).await;

        Ok(new_ep_id)
    }

    pub async fn save_episodes_batch_db(&self, episodes: &[EpisodeSave]) -> Result<()> {
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
        let mut mapped_json_array: Vec<serde_json::Value> = Vec::with_capacity(episodes.len());
        for (i, ep) in episodes.iter().enumerate() {
            let id_str = Uuid::new_v4().to_string();
            let metrics_id_str = Uuid::new_v4().to_string();
            let embedding = embeddings.get(i).cloned().flatten();
            let word_count = crate::retrieval::bm25::tokenize(&ep.content).len() as u32;
            let last_retrieved_at = chrono::Utc::now().to_rfc3339();

            // Reconstruct followed_by relationships using local cache and STM
            if let Some(ref sess_id) = ep.session_id {
                let user_prefix = get_user_prefix(sess_id);
                let tracking_key = if let Some(ref t_id) = ep.task_id {
                    format!("_last_episode_id_{}", t_id)
                } else {
                    "_last_episode_id".to_string()
                };
                let map_key = format!("{}:{}", user_prefix, tracking_key);

                if let Some(last_ep_id) = local_last_eps.get(&map_key).cloned() {
                    let last_uuid = last_ep_id.strip_prefix("episode:").unwrap_or(&last_ep_id).to_string();
                    relations.push(serde_json::json!({
                        "from_str": last_uuid,
                        "to_str": id_str.clone(),
                    }));
                } else {
                    // Bounded check of STM database to bridge sequential batches
                    if let Ok(stm_map) = self.get_stm(user_prefix, Some(&tracking_key)).await {
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

            let created_at_val = ep.created_at.clone().unwrap_or_else(|| last_retrieved_at.clone());

            let mut ep_json = serde_json::json!({
                "id_str": id_str,
                "metrics_id_str": metrics_id_str,
                "title": ep.title,
                "content": ep.content,
                "scope": ep.scope.clone().unwrap_or_else(|| "general".to_string()),
                "vault_path": ep.vault_path.clone().unwrap_or_default(),
                "utility": 50.0f32,
                "last_retrieved_at": last_retrieved_at,
                "word_count": word_count,
                "created_at": created_at_val
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

            mapped_json_array.push(ep_json);
        }

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
                
                CREATE $ep_id CONTENT {
                    title: $ep.title,
                    content: $ep.content,
                    scope: $ep.scope,
                    vault_path: $ep.vault_path,
                    processed_in_dream: false,
                    embedding: $ep.embedding ?? none,
                    utility: $ep.utility,
                    last_retrieved_at: $ep.last_retrieved_at,
                    archived: false,
                    session_id: $ep.session_id ?? none,
                    word_count: $ep.word_count,
                    node_type: $ep.node_type,
                    created_at: type::datetime($ep.created_at)
                };
                
                CREATE $met_id CONTENT {
                    target_id: $ep_id,
                    utility_score: 50.0,
                    access_count: 0
                };
            };
            COMMIT TRANSACTION;
        "#;

        let res = self.db.query(query).bind(("episodes", mapped_json_array)).await?;
        res.check().context("SurrealDB save_episodes_batch transaction failed")?;

        // 6. Relate temporal followed_by connections
        for rel in relations {
            let from_uuid = rel.get("from_str").unwrap().as_str().unwrap();
            let to_uuid = rel.get("to_str").unwrap().as_str().unwrap();

            let from_thing = parse_record_id(&format!("episode:{}", from_uuid));
            let to_thing = parse_record_id(&format!("episode:{}", to_uuid));

            if let (Ok(from), Ok(to)) = (from_thing, to_thing) {
                let relate_query = "RELATE $from -> followed_by -> $to CONTENT { created_at: time::now() };";
                if let Err(e) = self.db.query(relate_query)
                    .bind(("from", from))
                    .bind(("to", to))
                    .await
                {
                    tracing::warn!("Failed to relate temporal followed_by in batch: {:?}", e);
                }
            }
        }

        // 7. Push last episode IDs to short_term_memory DB & local cache
        for (map_key, last_ep_id) in local_last_eps {
            if let Some(colon_idx) = map_key.find(':') {
                let user_prefix = &map_key[..colon_idx];
                let tracking_key = &map_key[colon_idx + 1..];
                if let Err(e) = self.save_stm(user_prefix, tracking_key, &last_ep_id).await {
                    tracing::warn!("Failed to save STM for {} / {}: {:?}", user_prefix, tracking_key, e);
                }
            }
        }

        // 8. Update IDF token frequencies in memory
        for ep in episodes {
            let scope_val = ep.scope.clone().unwrap_or_else(|| "general".to_string());
            self.record_episode_tokens_for_cache(&scope_val, &ep.content).await;
        }

        Ok(())
    }

    pub async fn save_wisdom_rule_db(&self, rule: &WisdomRule) -> Result<String> {
        if let Some(ref vp) = rule.vault_path {
            self.record_indexing_write(vp).await;
        }
        let mut rule_uuid = Uuid::new_v4().to_string();
        let mut is_update = false;

        if let Some(ref id_str) = rule.id {
            let id_clean = id_str.strip_prefix("wisdom:").unwrap_or(id_str);
            rule_uuid = id_clean.to_string();
        }

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
                        superseded_by: $superseded_by,
                        severity: $severity,
                        blocking: $blocking,
                        rule_type: $rule_type
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
                        superseded_by: $superseded_by,
                        severity: $severity,
                        blocking: $blocking,
                        rule_type: $rule_type
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
                    superseded_by: $superseded_by,
                    severity: $severity,
                    blocking: $blocking,
                    rule_type: $rule_type
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
            .bind(("severity", rule.severity.as_deref()))
            .bind(("blocking", rule.blocking.unwrap_or(false)))
            .bind(("rule_type", rule.rule_type.as_deref().unwrap_or("aesthetic")))
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

    pub async fn get_llm_config_db(&self) -> Result<LlmConfigResponse> {
        if self.is_client_mode() {
            return self.daemon_get("/v1/config/llm").await;
        }
        let sql = "SELECT active_provider, model, cloud_provider, is_override, expires_at, llm_post_inference_delay_ms, model_tier_mappings FROM config:settings;";
        let mut response = self.db.query(sql).await?.check().context("Get config query failed")?;
        let config_opt: Option<LlmConfigResponse> = response.take(0)?;
        let mut config = if let Some(mut c) = config_opt {
            if c.llm_post_inference_delay_ms.is_none() {
                c.llm_post_inference_delay_ms = Some(5000);
            }
            c
        } else {
            LlmConfigResponse {
                active_provider: "local".to_string(),
                cloud_provider: "gemini".to_string(),
                model: "mlx-community/Qwen3.6-35B-A3B-4bit".to_string(),
                is_override: false,
                expires_at: None,
                api_key: None,
                llm_post_inference_delay_ms: Some(5000),
                model_tier_mappings: None,
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

    pub async fn update_llm_config_db(&self, req: &LlmConfigRequest) -> Result<()> {
        if self.is_client_mode() {
            let _res: serde_json::Value = self.daemon_post("/v1/config/llm", req).await?;
            return Ok(());
        }
        let sql_select = "SELECT active_provider, model, cloud_provider, is_override, expires_at, llm_post_inference_delay_ms, model_tier_mappings FROM config:settings;";
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
        let delay = req.llm_post_inference_delay_ms
            .or(existing.as_ref().and_then(|e| e.llm_post_inference_delay_ms))
            .unwrap_or(5000);
        let mappings = req.model_tier_mappings.clone()
            .or(existing.as_ref().and_then(|e| e.model_tier_mappings.clone()));

        let sql = "
            UPSERT config:settings CONTENT {
                active_provider: $active_provider,
                model: $model,
                cloud_provider: $cloud_provider,
                is_override: true,
                expires_at: $expires_at,
                llm_post_inference_delay_ms: $llm_post_inference_delay_ms,
                model_tier_mappings: $model_tier_mappings
            };
        ";
        let _ = self.db.query(sql)
            .bind(("active_provider", req.provider.as_str()))
            .bind(("model", model.as_str()))
            .bind(("cloud_provider", cloud_provider.as_str()))
            .bind(("expires_at", expires_at.clone()))
            .bind(("llm_post_inference_delay_ms", delay))
            .bind(("model_tier_mappings", mappings))
            .await?.check().context("UPSERT config failed")?;

        let _ = crate::llm::router::reload_tier_mappings(self).await;

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

    pub async fn get_unprocessed_episodes_db(&self) -> Result<Vec<Episode>> {
        let sql = "SELECT * FROM episode WHERE processed_in_dream = false;";
        let mut response = self.db.query(sql).await?.check().context("Query unprocessed episodes failed")?;
        let raw_episodes: Vec<EpisodeRaw> = response.take(0)?;
        let episodes = raw_episodes.into_iter().map(Episode::from).collect();
        Ok(episodes)
    }

    pub async fn mark_episode_processed_db(&self, id: &str) -> Result<()> {
        let thing_id = parse_record_id(id)?;

        let sql = "UPDATE $id SET processed_in_dream = true;";
        let _ = self.db.query(sql).bind(("id", thing_id)).await?.check().context("Mark episode processed failed")?;
        Ok(())
    }

    pub async fn get_all_episodes_db(&self) -> Result<Vec<Episode>> {
        let sql = "SELECT * FROM episode;";
        let mut response = self.db.query(sql).await?.check().context("Query all episodes failed")?;
        let raw_episodes: Vec<EpisodeRaw> = response.take(0)?;
        let episodes = raw_episodes.into_iter().map(Episode::from).collect();
        Ok(episodes)
    }

    pub async fn get_episodes_by_node_type_db(&self, node_type: &str) -> Result<Vec<Episode>> {
        let sql = "SELECT * FROM episode WHERE node_type = $node_type;";
        let mut response = self.db.query(sql)
            .bind(("node_type", node_type))
            .await?.check().context("Query episodes by node type failed")?;
        let raw_episodes: Vec<EpisodeRaw> = response.take(0)?;
        let episodes = raw_episodes.into_iter().map(Episode::from).collect();
        Ok(episodes)
    }

    pub async fn is_feature_enabled_db(&self, feature_key: &str, default: bool) -> bool {
        match self.get_profile_key(feature_key).await {
            Ok(Some(val_str)) => val_str.parse::<bool>().unwrap_or(default),
            _ => default,
        }
    }

    pub async fn save_profile_key_db(&self, key: &str, value: &str) -> Result<()> {
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

        let cache = PROFILE_CACHE.get_or_init(|| std::sync::RwLock::new(std::collections::HashMap::new()));
        if let Ok(mut write_guard) = cache.write() {
            write_guard.insert(key.to_string(), Some(value.to_string()));
        }

        Ok(())
    }

    pub async fn get_profile_key_db(&self, key: &str) -> Result<Option<String>> {
        let cache = PROFILE_CACHE.get_or_init(|| std::sync::RwLock::new(std::collections::HashMap::new()));
        if let Ok(read_guard) = cache.read() {
            if let Some(val) = read_guard.get(key) {
                return Ok(val.clone());
            }
        }

        #[derive(serde::Deserialize, surrealdb_types::SurrealValue)]
        struct ProfileRaw {
            value: String,
        }
        let sql = "SELECT `value` FROM `profile` WHERE id = type::record('profile', $key);";
        let mut response = self.db.query(sql)
            .bind(("key", key))
            .await?.check().context("SELECT profile failed")?;
        let res: Vec<ProfileRaw> = response.take(0)?;
        let val = res.first().map(|r| r.value.clone());

        if let Ok(mut write_guard) = cache.write() {
            write_guard.insert(key.to_string(), val.clone());
        }

        Ok(val)
    }

    pub async fn save_handoff_db(&self, handoff: &HandoffSave) -> Result<String> {
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

    pub async fn save_wiki_node_db(&self, node: &WikiNode) -> Result<String> {
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
                    embedding: $embedding,
                    metacognitive_confidence: $metacognitive_confidence,
                    node_type: $node_type
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
                    embedding: $embedding,
                    metacognitive_confidence: $metacognitive_confidence,
                    node_type: $node_type
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
            .bind(("metacognitive_confidence", node.metacognitive_confidence))
            .bind(("node_type", node.node_type.as_deref().unwrap_or("insight")))
            .await?;

        response.check().context("SurrealDB save_wiki_node transaction failed")?;

        Ok(format!("wiki_node:{}", node_uuid))
    }

    pub async fn relate_nodes_db(
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

    pub async fn relate_followed_by_db(&self, from_id: &str, to_id: &str) -> Result<()> {
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

    pub async fn invalidate_edge_db(
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

    pub async fn query_edges_as_of_db(
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

    pub async fn get_related_node_ids_db(&self, from_id: &str) -> Result<Vec<String>> {
        let from_thing = parse_record_id(from_id)?;
        let sql = "SELECT VALUE out FROM relates_to WHERE in = $from;";
        let mut response = self.db.query(sql).bind(("from", from_thing)).await?;
        let ids: Vec<surrealdb::types::RecordId> = response.take(0)?;
        Ok(ids.into_iter().map(|thing| format_record_id(&thing)).collect())
    }

    pub async fn get_wiki_node_id_by_vault_path_db(&self, vault_path: &str) -> Result<Option<String>> {
        let sql = "SELECT VALUE id FROM wiki_node WHERE vault_path = $vault_path LIMIT 1;";
        let mut response = self.db.query(sql).bind(("vault_path", vault_path)).await?;
        let ids: Option<surrealdb::types::RecordId> = response.take(0)?;
        Ok(ids.map(|thing| format_record_id(&thing)))
    }

    pub async fn get_active_scopes_db(&self) -> Result<Vec<String>> {
        let sql = "SELECT VALUE scope FROM episode GROUP BY scope;";
        let mut response = self.db.query(sql).await?;
        let mut scopes: Vec<String> = response.take(0)?;
        if !scopes.contains(&"general".to_string()) {
            scopes.push("general".to_string());
        }
        Ok(scopes)
    }

    pub async fn delete_by_vault_path_db(&self, vault_path: &str) -> Result<()> {
        let sql1 = "DELETE FROM episode WHERE vault_path = $vault_path;";
        let sql2 = "DELETE FROM wisdom WHERE vault_path = $vault_path;";
        let sql3 = "DELETE FROM wiki_node WHERE vault_path = $vault_path;";
        
        self.db.query(sql1).bind(("vault_path", vault_path)).await?.check()?;
        self.db.query(sql2).bind(("vault_path", vault_path)).await?.check()?;
        self.db.query(sql3).bind(("vault_path", vault_path)).await?.check()?;
        Ok(())
    }

    pub async fn delete_wiki_node_db(&self, name: &str, scope: &str) -> Result<()> {
        let sql = "DELETE FROM wiki_node WHERE name = $name AND scope = $scope;";
        self.db.query(sql)
            .bind(("name", name))
            .bind(("scope", scope))
            .await?
            .check()?;
        Ok(())
    }

        pub async fn save_stm_db(&self, session_id: &str, key: &str, value: &str) -> Result<()> {
        if std::env::var("MYTHRAX_BENCH").is_ok() {
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
            return Ok(());
        }
        let mut final_key = key.to_string();
        let mut expires_at: Option<chrono::DateTime<chrono::Utc>> = None;

        if key.starts_with("broadcast:") {
            let parts: Vec<&str> = key.split(':').collect();
            let ttl = if parts.len() == 3 {
                final_key = format!("broadcast:{}", parts[1]);
                parts[2].parse::<i64>().unwrap_or(300)
            } else {
                300
            };
            expires_at = Some(chrono::Utc::now() + chrono::Duration::seconds(ttl));
        }

        let sql = "
            BEGIN TRANSACTION;
            UPSERT type::record('short_term_memory', [$session_id, $key]) CONTENT {
                session_id: $session_id,
                key: $key,
                value: $value,
                updated_at: time::now(),
                expires_at: $expires_at
            };
            COMMIT TRANSACTION;
        ";
        self.db.query(sql)
            .bind(("session_id", session_id))
            .bind(("key", final_key.as_str()))
            .bind(("value", value))
            .bind(("expires_at", expires_at))
            .await?.check()?;

        // Dual-write to local JSON file unless running a benchmark
        if std::env::var("MYTHRAX_BENCH").is_err() {
            crate::store::save_stm_file(session_id, &final_key, value)?;
        }
        Ok(())
    }

    pub async fn get_stm_db(&self, session_id: &str, key: Option<&str>) -> Result<std::collections::HashMap<String, String>> {
        if std::env::var("MYTHRAX_BENCH").is_ok() {
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
                return Ok(map);
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
                return Ok(map);
            }
        }
        if let Some(k) = key {
            let (sql, is_broadcast) = if k.starts_with("broadcast:") {
                ("SELECT VALUE value FROM short_term_memory WHERE key = $key AND (expires_at = NONE OR expires_at > time::now()) ORDER BY updated_at DESC LIMIT 1;", true)
            } else {
                ("SELECT VALUE value FROM short_term_memory WHERE session_id = $session_id AND key = $key AND (expires_at = NONE OR expires_at > time::now()) LIMIT 1;", false)
            };

            let mut query = self.db.query(sql);
            if !is_broadcast {
                query = query.bind(("session_id", session_id));
            }
            let mut response = query.bind(("key", k)).await?.check()?;
            let value: Option<String> = response.take(0)?;
            let mut map = std::collections::HashMap::new();
            if let Some(v) = value {
                map.insert(k.to_string(), v);
            }
            Ok(map)
        } else {
            // Fetch session specific non-expired STM keys
            let sql_sess = "SELECT key, value FROM short_term_memory WHERE session_id = $session_id AND (expires_at = NONE OR expires_at > time::now());";
            let mut response = self.db.query(sql_sess)
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

            // Fetch all non-expired broadcast keys and merge
            let sql_broad = "SELECT key, value, updated_at FROM short_term_memory WHERE string::starts_with(key, 'broadcast:') AND (expires_at = NONE OR expires_at > time::now()) ORDER BY updated_at ASC;";
            let mut broad_response = self.db.query(sql_broad).await?.check()?;
            let broad_records: Vec<StmRecord> = broad_response.take(0)?;
            for r in broad_records {
                map.insert(r.key, r.value);
            }

            Ok(map)
        }
    }

    pub async fn clear_stm_db(&self, session_id: &str) -> Result<()> {
        let sql = "DELETE FROM short_term_memory WHERE session_id = $session_id;";
        self.db.query(sql)
            .bind(("session_id", session_id))
            .await?.check()?;

        // Delete local JSON file
        crate::store::delete_stm_file(session_id)?;
        Ok(())
    }

    pub async fn update_handoff_status_db(&self, id: &str, status: &str) -> Result<()> {
        let thing_id = parse_record_id(id)?;
        let sql = "UPDATE $id SET status = $status;";
        self.db.query(sql)
            .bind(("id", thing_id))
            .bind(("status", status))
            .await?.check()?;
        Ok(())
    }

    pub async fn delete_stale_handoffs_db(&self, pruning_days: i64) -> Result<()> {
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

    pub async fn prune_stale_memories_db(&self, vault_root: &std::path::Path) -> Result<()> {
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

    pub async fn get_memory_nodes_db(&self, node_ids: &[String]) -> Result<GetMemoryNodesResponse> {
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
                        episodes.push(Episode::from(raw));
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
                        wiki_nodes.push(raw.into_wiki_node());
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

    pub async fn get_all_wisdom_rules_db(&self) -> Result<Vec<WisdomRule>> {
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
            if let Some(id_str) = &w.id {
                if let Ok(thing) = parse_record_id(id_str) {
                    w.id = Some(format_record_id(&thing));
                }
            }
        }
        Ok(rules)
    }

    pub async fn get_all_wiki_nodes_db(&self) -> Result<Vec<WikiNode>> {
        let sql = "SELECT * FROM wiki_node;";
        let mut response = self.db.query(sql).await?.check().context("Get all wiki nodes query failed")?;
        let raws: Vec<WikiNodeRaw> = response.take(0)?;
        let nodes: Vec<WikiNode> = raws.into_iter().map(|r| r.into_wiki_node()).collect();
        Ok(nodes)
    }

    pub async fn query_symbolic_scored_db(
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

    pub async fn query_symbolic_db(&self, node_id: &str, relation: Option<&str>, max_depth: Option<usize>) -> Result<Vec<String>> {
        let hits = self.query_symbolic_scored(node_id, relation, max_depth, None).await?;
        Ok(hits.into_iter().map(|h| h.node_id).collect())
    }

    pub async fn save_thought_node_db(&self, thought: &crate::contracts::ThoughtNode) -> Result<String> {
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

    pub async fn journal_state_db(&self, vault_root: &std::path::Path, session_id: Option<&str>) -> Result<()> {
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

    pub async fn reinforce_episode_db(&self, id: &str) -> Result<()> {
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
            #[derive(serde::Deserialize, surrealdb_types::SurrealValue, Debug)]
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
        }
        Ok(())
    }

    pub async fn record_feedback_db(&self, id: &str, success: bool) -> Result<()> {
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

    pub async fn diagnose_error_internal_db(&self, stderr: &str, stdout: &str) -> Result<Option<(String, String)>> {
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
                    SELECT causal_explanation, prescribed_remedy, vector::similarity::cosine(embedding, $query_embedding) AS similarity FROM wisdom
                    WHERE status != 'superseded' AND (embedding <|1, 10|> $query_embedding);
                ";
                let res = self.db.query(sql).bind(("query_embedding", q_vec.clone())).await?;
                let mut res = res.check()?;
                #[derive(serde::Deserialize, Debug, SurrealValue)]
                struct WisdomVectorRaw {
                    causal_explanation: String,
                    prescribed_remedy: String,
                    similarity: Option<f32>,
                }
                let rules: Vec<WisdomVectorRaw> = res.take(0)?;
                let mut best_match = None;
                let mut best_similarity = 0.0_f32;

                for r in rules {
                    let sim = r.similarity.unwrap_or(0.0);
                    if sim > best_similarity {
                        best_similarity = sim;
                        best_match = Some(r);
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
}
