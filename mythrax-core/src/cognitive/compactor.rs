use crate::cognitive::synthesis::load_insights;
use crate::contracts::WikiNode;
use crate::db::StorageBackend;
use crate::llm::LLMClient;
use crate::store::MarkdownStore;
use anyhow::Result;
use std::path::Path;
use surrealdb_types::SurrealValue;

pub struct Compactor {
    llm: LLMClient,
}

impl Default for Compactor {
    fn default() -> Self {
        Self::new()
    }
}

impl Compactor {
    pub fn new() -> Self {
        Self {
            llm: LLMClient::new(),
        }
    }

    pub async fn delta_compact_checkpoints(&self, db: &dyn StorageBackend) -> Result<String> {
        let checkpoints = db.get_checkpoints().await?;
        if checkpoints.is_empty() {
            return Ok("No checkpoints found.".to_string());
        }

        let mut prompt_content = String::new();
        for (i, chk) in checkpoints.iter().enumerate() {
            let timestamp = chk
                .get("timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let project_type = chk
                .get("project_type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let exit_code = chk.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(0);
            let errors = chk
                .get("compiler_errors")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let git_diff = chk.get("git_diff").and_then(|v| v.as_str()).unwrap_or("");

            if i < 2 {
                prompt_content.push_str(&format!(
                    "### Checkpoint {} (Raw - High Fidelity)\n\
                     Timestamp: {}\n\
                     Project Type: {}\n\
                     Compiler Exit Code: {}\n\
                     Compiler/Linter Output:\n{}\n\
                     Git Diff:\n{}\n\n",
                    i + 1,
                    timestamp,
                    project_type,
                    exit_code,
                    errors,
                    git_diff
                ));
            } else {
                let compact_diff = if git_diff.len() > 200 {
                    let summary_prompt = format!(
                        "Summarize this git diff briefly (under 50 words):\n\n{}",
                        git_diff
                    );
                    self.llm
                        .completion(db, Some("You are a code summarizer."), &summary_prompt)
                        .await
                        .unwrap_or_else(|_| "Git diff summary failed".to_string())
                } else {
                    git_diff.to_string()
                };

                prompt_content.push_str(&format!(
                    "### Checkpoint {} (Compressed Summary)\n\
                     Timestamp: {}\n\
                     Project Type: {}\n\
                     Compiler Exit Code: {}\n\
                     Summary of Changes:\n{}\n\n",
                    i + 1,
                    timestamp,
                    project_type,
                    exit_code,
                    compact_diff
                ));
            }
        }

        let sys_prompt = "You are a master systems architect. Analyze the sequence of checkpoints and summarize the transitions between them, detailing how the codebase evolved, what errors were resolved, and the progression of active changes.";
        let summary = self
            .llm
            .completion(db, Some(sys_prompt), &prompt_content)
            .await?;
        Ok(summary)
    }

    pub async fn compact_scope(
        &self,
        db: &dyn StorageBackend,
        store: &MarkdownStore,
        scope: &str,
        _embedder: Option<std::sync::Arc<crate::embeddings::LocalEmbedder>>,
    ) -> Result<()> {
        let _ = auto_page_workspace_files(db).await;
        let _ = db.prune_stale_memories(&store.vault_root).await;
        let _ = self.archive_decayed_episodes(db, store).await;

        // -------------------------------------------------------------
        // Enforce 500-node procedural episode cap per scope
        // -------------------------------------------------------------
        if let Ok(mut active_procs) = db.get_episodes_by_node_type("procedural").await {
            active_procs
                .retain(|ep| ep.scope.as_deref() == Some(scope) && !ep.archived.unwrap_or(false));
            if active_procs.len() > 500 {
                active_procs.sort_by(|a, b| {
                    let time_a = a
                        .last_retrieved_at
                        .as_ref()
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|| chrono::Utc::now());
                    let time_b = b
                        .last_retrieved_at
                        .as_ref()
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|| chrono::Utc::now());
                    time_a.cmp(&time_b)
                });

                let num_to_archive = active_procs.len() - 500;
                if let Some(surreal_backend) = db
                    .as_any()
                    .downcast_ref::<crate::db::backend::SurrealBackend>()
                {
                    for i in 0..num_to_archive {
                        if let Some(ref ep_id) = active_procs[i].id {
                            let id_raw = ep_id.split(':').nth(1).unwrap_or(ep_id).to_string();
                            let archive_sql = "UPDATE type::record('episode', $id) MERGE {
                                archived: true,
                                archived_at: time::now(),
                                utility: 1.0,
                                importance: 1.0
                            };";
                            let _ = surreal_backend
                                .db
                                .query(archive_sql)
                                .bind(("id", id_raw))
                                .await;

                            if let Some(ref vp) = active_procs[i].vault_path {
                                let src_file = store.vault_root.join(vp);
                                if src_file.exists() {
                                    let archive_dir = store.vault_root.join("vault/archive");
                                    let _ = std::fs::create_dir_all(&archive_dir);
                                    let filename = std::path::Path::new(vp)
                                        .file_name()
                                        .unwrap_or_else(|| std::ffi::OsStr::new("episode.md"));
                                    let dest_file = archive_dir.join(filename);
                                    let _ = std::fs::rename(&src_file, &dest_file);
                                }
                            }
                        }
                    }
                }
            }
        }

        // -------------------------------------------------------------
        // Task C.1: Near-Duplicate Episodic Merging
        // -------------------------------------------------------------
        if db
            .is_feature_enabled("compactor.enable_near_duplicate_merging", true)
            .await
        {
            let mut dedup_threshold = 0.90f32;
            if let Ok(Some(val_str)) = db.get_profile_key("compactor.dedup_threshold").await {
                if let Ok(parsed) = val_str.parse::<f32>() {
                    dedup_threshold = parsed;
                }
            }

            if let Ok(episodes) = db.get_all_episodes().await {
                let mut active_eps: Vec<crate::contracts::Episode> = episodes
                    .into_iter()
                    .filter(|ep| {
                        ep.scope.as_deref() == Some(scope)
                            && !ep.archived.unwrap_or(false)
                            && ep.embedding.is_some()
                            && ep.id.is_some()
                    })
                    .collect();

                let mut deleted_ids = std::collections::HashSet::new();

                for i in 0..active_eps.len() {
                    let id_i = active_eps[i].id.as_ref().cloned().unwrap();
                    if deleted_ids.contains(&id_i) {
                        continue;
                    }

                    for j in (i + 1)..active_eps.len() {
                        let id_j = active_eps[j].id.as_ref().cloned().unwrap();
                        if deleted_ids.contains(&id_j) {
                            continue;
                        }

                        // Check session_id and node_type
                        let same_session = active_eps[i].session_id == active_eps[j].session_id;
                        let same_node_type = active_eps[i].node_type == active_eps[j].node_type;
                        if same_session && same_node_type {
                            // Check similarity
                            let sim = cosine_similarity(
                                active_eps[i].embedding.as_ref().unwrap(),
                                active_eps[j].embedding.as_ref().unwrap(),
                            );

                            if sim >= dedup_threshold {
                                // Determine older vs newer
                                let time_i = active_eps[i]
                                    .last_retrieved_at
                                    .as_ref()
                                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                                    .map(|dt| dt.with_timezone(&chrono::Utc))
                                    .unwrap_or_else(|| chrono::Utc::now());
                                let time_j = active_eps[j]
                                    .last_retrieved_at
                                    .as_ref()
                                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                                    .map(|dt| dt.with_timezone(&chrono::Utc))
                                    .unwrap_or_else(|| chrono::Utc::now());

                                let (older_idx, newer_idx) =
                                    if time_i <= time_j { (i, j) } else { (j, i) };

                                let older = active_eps[older_idx].clone();
                                let newer = active_eps[newer_idx].clone();

                                let merged_content =
                                    format!("{}\n{}", older.content, newer.content);
                                let merged_last_retrieved_at = newer
                                    .last_retrieved_at
                                    .clone()
                                    .or(older.last_retrieved_at.clone());
                                let merged_word_count = Some(
                                    older.word_count.unwrap_or(0) + newer.word_count.unwrap_or(0),
                                );

                                let mut merged_facts = older.facts.clone().unwrap_or_default();
                                if let Some(ref new_facts) = newer.facts {
                                    for f in new_facts {
                                        if !merged_facts.contains(f) {
                                            merged_facts.push(f.clone());
                                        }
                                    }
                                }
                                let merged_facts = if merged_facts.is_empty() {
                                    None
                                } else {
                                    Some(merged_facts)
                                };

                                let mut merged_concepts =
                                    older.concepts.clone().unwrap_or_default();
                                if let Some(ref new_concepts) = newer.concepts {
                                    for c in new_concepts {
                                        if !merged_concepts.contains(c) {
                                            merged_concepts.push(c.clone());
                                        }
                                    }
                                }
                                let merged_concepts = if merged_concepts.is_empty() {
                                    None
                                } else {
                                    Some(merged_concepts)
                                };

                                let mut merged_files_read =
                                    older.files_read.clone().unwrap_or_default();
                                if let Some(ref new_files) = newer.files_read {
                                    for f in new_files {
                                        if !merged_files_read.contains(f) {
                                            merged_files_read.push(f.clone());
                                        }
                                    }
                                }
                                let merged_files_read = if merged_files_read.is_empty() {
                                    None
                                } else {
                                    Some(merged_files_read)
                                };

                                let mut merged_files_modified =
                                    older.files_modified.clone().unwrap_or_default();
                                if let Some(ref new_files) = newer.files_modified {
                                    for f in new_files {
                                        if !merged_files_modified.contains(f) {
                                            merged_files_modified.push(f.clone());
                                        }
                                    }
                                }
                                let merged_files_modified = if merged_files_modified.is_empty() {
                                    None
                                } else {
                                    Some(merged_files_modified)
                                };

                                // Update older episode in active_eps
                                active_eps[older_idx].content = merged_content.clone();
                                active_eps[older_idx].last_retrieved_at =
                                    merged_last_retrieved_at.clone();
                                active_eps[older_idx].word_count = merged_word_count;
                                active_eps[older_idx].facts = merged_facts.clone();
                                active_eps[older_idx].concepts = merged_concepts.clone();
                                active_eps[older_idx].files_read = merged_files_read.clone();
                                active_eps[older_idx].files_modified =
                                    merged_files_modified.clone();

                                let newer_id = newer.id.as_ref().unwrap();
                                deleted_ids.insert(newer_id.clone());

                                if let Some(surreal_backend) =
                                    db.as_any()
                                        .downcast_ref::<crate::db::backend::SurrealBackend>()
                                {
                                    let older_raw_id = older
                                        .id
                                        .as_ref()
                                        .unwrap()
                                        .split(':')
                                        .nth(1)
                                        .unwrap_or(older.id.as_ref().unwrap())
                                        .to_string();
                                    let query_sql = "UPDATE type::record('episode', $id) MERGE {
                                        content: $content,
                                        last_retrieved_at: $last_retrieved_at,
                                        facts: $facts,
                                        concepts: $concepts,
                                        files_read: $files_read,
                                        files_modified: $files_modified,
                                        word_count: $word_count
                                    };";
                                    let _ = surreal_backend
                                        .db
                                        .query(query_sql)
                                        .bind(("id", older_raw_id))
                                        .bind(("content", merged_content.clone()))
                                        .bind(("last_retrieved_at", merged_last_retrieved_at))
                                        .bind(("facts", merged_facts))
                                        .bind(("concepts", merged_concepts))
                                        .bind(("files_read", merged_files_read))
                                        .bind(("files_modified", merged_files_modified))
                                        .bind(("word_count", merged_word_count))
                                        .await;

                                    let older_rec = crate::db::backend::parse_record_id(
                                        older.id.as_ref().unwrap(),
                                    )
                                    .unwrap();
                                    let newer_rec = crate::db::backend::parse_record_id(
                                        newer.id.as_ref().unwrap(),
                                    )
                                    .unwrap();

                                    let mut older_count = 0;
                                    let mut newer_count = 0;
                                    let mut older_exists = false;
                                    let mut newer_exists = false;

                                    let metrics_sql = "SELECT target_id, access_count FROM metrics WHERE target_id = $older OR target_id = $newer;";
                                    if let Ok(mut resp) = surreal_backend
                                        .db
                                        .query(metrics_sql)
                                        .bind(("older", older_rec.clone()))
                                        .bind(("newer", newer_rec.clone()))
                                        .await
                                    {
                                        #[derive(serde::Deserialize, SurrealValue)]
                                        struct MetRow {
                                            target_id: surrealdb::types::RecordId,
                                            access_count: i64,
                                        }
                                        if let Ok(rows) = resp.take::<Vec<MetRow>>(0) {
                                            for r in rows {
                                                let r_target_str =
                                                    crate::db::backend::format_record_id(
                                                        &r.target_id,
                                                    );
                                                if r_target_str == *older.id.as_ref().unwrap() {
                                                    older_count = r.access_count;
                                                    older_exists = true;
                                                } else if r_target_str
                                                    == *newer.id.as_ref().unwrap()
                                                {
                                                    newer_count = r.access_count;
                                                    newer_exists = true;
                                                }
                                            }
                                        }
                                    }

                                    let sum_count = older_count + newer_count;
                                    if older_exists {
                                        let update_metric_sql = "UPDATE metrics SET access_count = $count, last_accessed = time::now() WHERE target_id = $target_id;";
                                        let _ = surreal_backend
                                            .db
                                            .query(update_metric_sql)
                                            .bind(("count", sum_count))
                                            .bind(("target_id", older_rec.clone()))
                                            .await;
                                    } else {
                                        let metrics_uuid = uuid::Uuid::new_v4().to_string();
                                        let create_sql = "CREATE type::record('metrics', $metrics_uuid) CONTENT {
                                            target_id: $target_id,
                                            utility_score: 50.0,
                                            access_count: $count,
                                            last_accessed: time::now()
                                        };";
                                        let _ = surreal_backend
                                            .db
                                            .query(create_sql)
                                            .bind(("metrics_uuid", metrics_uuid))
                                            .bind(("target_id", older_rec.clone()))
                                            .bind(("count", sum_count))
                                            .await;
                                    }

                                    if newer_exists {
                                        let delete_metric_sql =
                                            "DELETE FROM metrics WHERE target_id = $target_id;";
                                        let _ = surreal_backend
                                            .db
                                            .query(delete_metric_sql)
                                            .bind(("target_id", newer_rec))
                                            .await;
                                    }

                                    let newer_raw_id = newer
                                        .id
                                        .as_ref()
                                        .unwrap()
                                        .split(':')
                                        .nth(1)
                                        .unwrap_or(newer.id.as_ref().unwrap())
                                        .to_string();
                                    let delete_ep_sql = "DELETE type::record('episode', $id);";
                                    let _ = surreal_backend
                                        .db
                                        .query(delete_ep_sql)
                                        .bind(("id", newer_raw_id))
                                        .await;
                                }

                                if let Some(ref vp) = older.vault_path {
                                    let _ = store.write_file(vp, &merged_content);
                                }

                                if let Some(ref vp) = newer.vault_path {
                                    let path = store.vault_root.join(vp);
                                    if path.exists() {
                                        let _ = std::fs::remove_file(path);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Prune chat history exceeding 100 turns per session
        if let Some(surreal_backend) = db
            .as_any()
            .downcast_ref::<crate::db::backend::SurrealBackend>()
        {
            let sessions_res = surreal_backend
                .db
                .query("SELECT session_id FROM chat_history GROUP BY session_id;")
                .await;
            match sessions_res {
                Ok(mut resp) => {
                    #[derive(serde::Deserialize, SurrealValue)]
                    struct SessionRow {
                        session_id: String,
                    }
                    if let Ok(rows) = resp.take::<Vec<SessionRow>>(0) {
                        for row in rows {
                            let ids_res = surreal_backend.db.query("SELECT id, created_at FROM chat_history WHERE session_id = $session_id ORDER BY created_at DESC;")
                                .bind(("session_id", row.session_id.as_str()))
                                .await;
                            if let Ok(mut ids_resp) = ids_res {
                                #[derive(serde::Deserialize, SurrealValue)]
                                struct IdRow {
                                    id: surrealdb::types::RecordId,
                                }
                                let ids: Vec<IdRow> = ids_resp.take(0).unwrap_or_default();
                                if ids.len() > 100 {
                                    let to_delete = &ids[100..];
                                    for item in to_delete {
                                        let _ = surreal_backend
                                            .db
                                            .query("DELETE $id;")
                                            .bind(("id", item.id.clone()))
                                            .await;
                                    }
                                }
                            }
                        }
                    }
                }
                Err(_) => {}
            }
        }
        let insights = load_insights(&store.vault_root);
        let scope_insights: Vec<_> = insights
            .into_iter()
            .filter(|ins| ins.scope == scope)
            .collect();

        if scope_insights.is_empty() {
            return Ok(());
        }

        // 1. Load embeddings for all insights in the scope by resolving their DB record IDs
        let mut node_ids = Vec::new();
        let mut ins_by_id = std::collections::HashMap::new();

        for ins in &scope_insights {
            let rel_path = Path::new(&ins.vault_path)
                .strip_prefix(&store.vault_root)
                .unwrap_or(Path::new(&ins.vault_path))
                .to_string_lossy()
                .to_string();
            if let Ok(Some(insight_id)) = db.get_wiki_node_id_by_vault_path(&rel_path).await {
                node_ids.push(insight_id.clone());
                ins_by_id.insert(insight_id, ins.clone());
            }
        }

        let mut valid_insights = Vec::new();
        if !node_ids.is_empty() {
            if let Ok(nodes_resp) = db.get_memory_nodes(&node_ids).await {
                for node in nodes_resp.wiki_nodes {
                    if let Some(ref id) = node.id {
                        if let Some(ins) = ins_by_id.get(id) {
                            valid_insights.push((ins.clone(), id.clone(), node.embedding));
                        }
                    }
                }
            }
        }

        // 2. Prepare embeddings and run DBSCAN
        let mut embeddings = Vec::new();
        let mut dbscan_indices = Vec::new();
        let mut outlier_insights = Vec::new();

        for (ins, id, emb_opt) in &valid_insights {
            if let Some(emb) = emb_opt {
                embeddings.push(emb.as_slice());
                dbscan_indices.push((ins.clone(), id.clone()));
            } else {
                outlier_insights.push((ins.clone(), id.clone()));
            }
        }

        // Add any scope_insights that couldn't be resolved in the DB to outliers
        for ins in &scope_insights {
            let rel_path = Path::new(&ins.vault_path)
                .strip_prefix(&store.vault_root)
                .unwrap_or(Path::new(&ins.vault_path))
                .to_string_lossy()
                .to_string();
            let is_resolved = valid_insights.iter().any(|(v_ins, _, _)| {
                let v_rel = Path::new(&v_ins.vault_path)
                    .strip_prefix(&store.vault_root)
                    .unwrap_or(Path::new(&v_ins.vault_path))
                    .to_string_lossy()
                    .to_string();
                v_rel == rel_path
            });
            if !is_resolved {
                outlier_insights.push((ins.clone(), String::new()));
            }
        }

        let labels = if !embeddings.is_empty() {
            crate::cognitive::synthesis::dbscan(&embeddings, 0.10, 2)
        } else {
            Vec::new()
        };

        let mut clusters: std::collections::HashMap<
            usize,
            Vec<(crate::cognitive::synthesis::InsightNote, String)>,
        > = std::collections::HashMap::new();

        for (idx, label) in labels.into_iter().enumerate() {
            let item = dbscan_indices[idx].clone();
            if let Some(cluster_id) = label {
                clusters.entry(cluster_id).or_default().push(item);
            } else {
                outlier_insights.push(item);
            }
        }

        // 3. For each group/cluster, call the LLM to generate the compaction summary and save the WikiNode
        for (cluster_id, member_insights) in &clusters {
            let mut combined_content = String::new();
            for (ins, _) in member_insights {
                combined_content.push_str(&format!(
                    "Insight Title: {}\nInsight Body:\n{}\n\n",
                    ins.title, ins.content
                ));
            }

            // Extract anchors and clean content
            let (cleaned_content, extracted_anchors) = extract_attention_anchors(&combined_content);

            let sys_prompt = "You are an architectural compactor. Summarize the key architectural decisions, design patterns, and systemic constraints described in these insights.";
            let prompt_text = format!("Insights:\n\n{}", cleaned_content);
            let summary = self
                .llm
                .completion(db, Some(sys_prompt), &prompt_text)
                .await?;
            let summary = page_markdown_code_blocks(db, &summary).await?;

            let stm_anchors = get_active_stm_anchors(&store.vault_root);
            let mut all_anchors = extracted_anchors;
            for sa in stm_anchors {
                if !all_anchors.contains(&sa) {
                    all_anchors.push(sa);
                }
            }

            let first_title = member_insights
                .first()
                .map(|(c, _)| c.title.as_str())
                .unwrap_or("compaction");
            let slug = first_title.to_lowercase().replace([' ', '/'], "_");
            let uuid = uuid::Uuid::new_v4().to_string();
            let relative_path = format!("wiki/compaction/{}_{}_{}.md", scope, slug, &uuid[..8]);

            let mut file_content = format!(
                "---\ntype: \"compaction\"\nscope: \"{}\"\ncluster_id: {}\n---\n\n# Architectural Compaction: {}\n\n{}",
                scope, cluster_id, scope, summary
            );

            if !all_anchors.is_empty() {
                file_content.push_str("\n\n## Attention Anchors\n");
                for anchor in &all_anchors {
                    file_content.push_str(&format!("- [ANCHOR: {}]\n", anchor));
                }
            }
            store.write_file(&relative_path, &file_content)?;

            let node_contract = WikiNode {
                id: None,
                name: format!("Compaction: {} - Cluster {}", scope, cluster_id),
                content: summary.clone(),
                scope: scope.to_string(),
                vault_path: Some(relative_path.clone()),
                embedding: None,
            };
            if let Ok(compaction_id) = db.save_wiki_node(&node_contract).await {
                for (_, insight_id) in member_insights {
                    if !insight_id.is_empty() {
                        let _ = db
                            .relate_nodes(insight_id, &compaction_id, None, None, None)
                            .await;
                    }
                }
            }

            tracing::info!(
                "Compacted scope '{}' cluster {}: summary saved",
                scope,
                cluster_id
            );
        }

        // 4. Handle outlier insights by grouping them into a single miscellaneous compaction
        if !outlier_insights.is_empty() {
            let mut combined_content = String::new();
            for (ins, _) in &outlier_insights {
                combined_content.push_str(&format!(
                    "Insight Title: {}\nInsight Body:\n{}\n\n",
                    ins.title, ins.content
                ));
            }

            // Extract anchors and clean content
            let (cleaned_content, extracted_anchors) = extract_attention_anchors(&combined_content);

            let sys_prompt = "You are an architectural compactor. Summarize the key architectural decisions, design patterns, and systemic constraints described in these insights.";
            let prompt_text = format!("Insights:\n\n{}", cleaned_content);
            let summary = self
                .llm
                .completion(db, Some(sys_prompt), &prompt_text)
                .await?;
            let summary = page_markdown_code_blocks(db, &summary).await?;

            let stm_anchors = get_active_stm_anchors(&store.vault_root);
            let mut all_anchors = extracted_anchors;
            for sa in stm_anchors {
                if !all_anchors.contains(&sa) {
                    all_anchors.push(sa);
                }
            }

            let uuid = uuid::Uuid::new_v4().to_string();
            let relative_path =
                format!("wiki/compaction/{}_miscellaneous_{}.md", scope, &uuid[..8]);

            let mut file_content = format!(
                "---\ntype: \"compaction\"\nscope: \"{}\"\ncluster_id: \"miscellaneous\"\n---\n\n# Architectural Compaction: {} (Miscellaneous)\n\n{}",
                scope, scope, summary
            );

            if !all_anchors.is_empty() {
                file_content.push_str("\n\n## Attention Anchors\n");
                for anchor in &all_anchors {
                    file_content.push_str(&format!("- [ANCHOR: {}]\n", anchor));
                }
            }
            store.write_file(&relative_path, &file_content)?;

            let node_contract = WikiNode {
                id: None,
                name: format!("Compaction: {} - Miscellaneous", scope),
                content: summary.clone(),
                scope: scope.to_string(),
                vault_path: Some(relative_path.clone()),
                embedding: None,
            };
            if let Ok(compaction_id) = db.save_wiki_node(&node_contract).await {
                for (_, insight_id) in &outlier_insights {
                    if !insight_id.is_empty() {
                        let _ = db
                            .relate_nodes(insight_id, &compaction_id, None, None, None)
                            .await;
                    }
                }
            }

            tracing::info!(
                "Compacted scope '{}' outliers: miscellaneous summary saved",
                scope
            );
        }

        Ok(())
    }

    pub async fn compact_global(
        &self,
        db: &dyn StorageBackend,
        store: &MarkdownStore,
    ) -> Result<()> {
        let _ = auto_page_workspace_files(db).await;
        let _ = db.prune_stale_memories(&store.vault_root).await;
        let _ = self.archive_decayed_episodes(db, store).await;

        // Perform history pruning using SurrealDB backend if available
        if let Some(surreal_backend) = db
            .as_any()
            .downcast_ref::<crate::db::backend::SurrealBackend>()
        {
            let mut pruning_days: i64 = 30; // default 30 days
            let query_res = surreal_backend.db.query("SELECT VALUE value FROM profile WHERE key = 'compaction.history_pruning_days' LIMIT 1;").await;
            if let Ok(mut resp) = query_res {
                if let Ok(values) = resp.take::<Vec<String>>(0) {
                    if let Some(val_str) = values.first() {
                        if let Ok(days) = val_str.parse::<i64>() {
                            pruning_days = days;
                        }
                    }
                }
            }

            let threshold = chrono::Utc::now() - chrono::Duration::days(pruning_days);
            let _ = surreal_backend
                .db
                .query("DELETE wiki_node_history WHERE changed_at < type::datetime($threshold);")
                .bind(("threshold", threshold.to_rfc3339()))
                .await;
        }
        let compaction_dir = store.vault_root.join("wiki/compaction");
        if !compaction_dir.exists() {
            return Ok(());
        }

        let mut combined_compaction = String::new();
        let mut compaction_paths = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&compaction_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().map(|s| s == "md").unwrap_or(false) {
                    let path = entry.path();
                    let rel_path = path
                        .strip_prefix(&store.vault_root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string();
                    compaction_paths.push(rel_path);
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        combined_compaction.push_str(&content);
                        combined_compaction.push_str("\n\n---\n\n");
                    }
                }
            }
        }

        if combined_compaction.is_empty() {
            return Ok(());
        }

        let (cleaned_compaction, extracted_anchors) =
            extract_attention_anchors(&combined_compaction);

        let sys_prompt = "You are a master systems architect. Synthesize all the provided architectural compactions into a single, cohesive global systems synthesis document outlining overall patterns, critical rules, and systems status.";
        let prompt_text = format!("Architectural Compactions:\n\n{}", cleaned_compaction);
        let global_summary = self
            .llm
            .completion(db, Some(sys_prompt), &prompt_text)
            .await?;
        let global_summary = page_markdown_code_blocks(db, &global_summary).await?;

        let stm_anchors = get_active_stm_anchors(&store.vault_root);
        let mut all_anchors = extracted_anchors;
        for sa in stm_anchors {
            if !all_anchors.contains(&sa) {
                all_anchors.push(sa);
            }
        }

        let uuid = uuid::Uuid::new_v4().to_string();
        let relative_path = format!("wiki/general/global_compaction_{}.md", &uuid[..8]);
        let mut file_content = format!(
            "---\ntype: \"global_compaction\"\n---\n\n# Global Systems Synthesis\n\n{}",
            global_summary
        );

        if !all_anchors.is_empty() {
            file_content.push_str("\n\n## Attention Anchors\n");
            for anchor in &all_anchors {
                file_content.push_str(&format!("- [ANCHOR: {}]\n", anchor));
            }
        }
        store.write_file(&relative_path, &file_content)?;

        let node_contract = WikiNode {
            id: None,
            name: "Global Systems Synthesis".to_string(),
            content: global_summary.clone(),
            scope: "general".to_string(),
            vault_path: Some(relative_path.clone()),
            embedding: None,
        };
        if let Ok(global_compaction_id) = db.save_wiki_node(&node_contract).await {
            for comp_path in compaction_paths {
                if let Ok(Some(comp_id)) = db.get_wiki_node_id_by_vault_path(&comp_path).await {
                    let _ = db
                        .relate_nodes(&comp_id, &global_compaction_id, None, None, None)
                        .await;
                }
            }
        }

        Ok(())
    }

    async fn archive_decayed_episodes(
        &self,
        db: &dyn StorageBackend,
        store: &MarkdownStore,
    ) -> Result<()> {
        // Retrieve compaction.decay_threshold (default 0.15) from 'profile' table
        let mut decay_threshold = 0.15f32;
        if let Some(surreal_backend) = db
            .as_any()
            .downcast_ref::<crate::db::backend::SurrealBackend>()
        {
            let query_sql =
                "SELECT VALUE value FROM profile WHERE key = 'compaction.decay_threshold' LIMIT 1;";
            if let Ok(mut resp) = surreal_backend.db.query(query_sql).await {
                if let Ok(Some(val_str)) = resp.take::<Option<String>>(0) {
                    if let Ok(parsed) = val_str.parse::<f32>() {
                        decay_threshold = parsed;
                    }
                }
            }
        }

        let mut access_counts = std::collections::HashMap::new();
        if let Some(surreal_backend) = db
            .as_any()
            .downcast_ref::<crate::db::backend::SurrealBackend>()
        {
            let metrics_sql = "SELECT target_id, access_count FROM metrics;";
            if let Ok(mut resp) = surreal_backend.db.query(metrics_sql).await {
                if let Ok(rows) = resp.take::<Vec<crate::db::backend::MetricAccess>>(0) {
                    for r in rows {
                        let target_str = crate::db::backend::format_record_id(&r.target_id);
                        access_counts.insert(target_str, r.access_count);
                    }
                }
            }
        }

        let episodes = db.get_all_episodes().await?;
        let now = std::time::SystemTime::now();
        for ep in episodes {
            if ep.archived.unwrap_or(false) {
                continue;
            }
            let last_ret = if let Some(ref lr_str) = ep.last_retrieved_at {
                chrono::DateTime::parse_from_rfc3339(lr_str)
                    .map(|dt| std::time::SystemTime::from(dt))
                    .unwrap_or(now)
            } else {
                now
            };

            let is_procedural = ep.node_type.as_deref() == Some("procedural");
            let access_count = ep
                .id
                .as_ref()
                .and_then(|id| access_counts.get(id).copied())
                .unwrap_or(0);

            let t_half_type = if is_procedural { 365.0f32 } else { 30.0f32 };
            let t_half_eff = if is_procedural {
                365.0f32
            } else {
                t_half_type * (1.0f32 + 0.3f32 * ((1.0f32 + access_count as f32).log2()))
            };

            let lambda_eff = 2.0f32.ln() / t_half_eff;
            let t_secs = now
                .duration_since(last_ret)
                .unwrap_or_default()
                .as_secs_f32();
            let t_days = t_secs / 86400.0f32;
            let decay_factor = (-lambda_eff * t_days).exp();
            let utility = ep.utility.unwrap_or(50.0);
            let decayed_utility = utility * decay_factor;

            if decayed_utility < decay_threshold * 50.0 {
                let mut is_referenced = false;
                if let Some(surreal_backend) = db
                    .as_any()
                    .downcast_ref::<crate::db::backend::SurrealBackend>()
                {
                    if let Some(ref ep_id) = ep.id {
                        if let Ok(ep_rec) = crate::db::backend::parse_record_id(ep_id) {
                            let check_ref_sql = "SELECT VALUE id FROM relates_to WHERE in = $ep OR out = $ep LIMIT 1;";
                            if let Ok(mut resp) = surreal_backend
                                .db
                                .query(check_ref_sql)
                                .bind(("ep", ep_rec))
                                .await
                            {
                                if let Ok(rows) = resp.take::<Vec<surrealdb::types::RecordId>>(0) {
                                    if !rows.is_empty() {
                                        is_referenced = true;
                                    }
                                }
                            }
                        }
                    }
                }

                if is_referenced {
                    if let Some(surreal_backend) = db
                        .as_any()
                        .downcast_ref::<crate::db::backend::SurrealBackend>()
                    {
                        if let Some(ref ep_id) = ep.id {
                            let query_sql = "UPDATE type::record('episode', $id) MERGE {
                                archived: true,
                                archived_at: time::now(),
                                utility: 1.0,
                                importance: 1.0
                            };";
                            let id_raw = ep_id.split(':').nth(1).unwrap_or(ep_id).to_string();
                            let _ = surreal_backend
                                .db
                                .query(query_sql)
                                .bind(("id", id_raw))
                                .await;
                        }
                    }
                    continue;
                }

                // Move physical file to vault/archive/
                if let Some(ref vp) = ep.vault_path {
                    let src_file = store.vault_root.join(vp);
                    if src_file.exists() {
                        let archive_dir = store.vault_root.join("vault/archive");
                        let _ = std::fs::create_dir_all(&archive_dir);
                        let filename = std::path::Path::new(vp)
                            .file_name()
                            .unwrap_or_else(|| std::ffi::OsStr::new("episode.md"));
                        let dest_file = archive_dir.join(filename);
                        let _ = std::fs::rename(&src_file, &dest_file);
                    }

                    // 2. Generate high-level Raptor summary using the LLM
                    let sys_prompt = "You are a master systems summarizer. Generate a high-level, highly compressed Raptor summary of the following episode's content, preserving the essential historical trace.";
                    let prompt = format!("Episode Title: {}\nContent:\n{}", ep.title, ep.content);
                    if let Ok(summary) = self.llm.completion(db, Some(sys_prompt), &prompt).await {
                        // 3. Save as wiki Raptor summary node
                        let uuid = uuid::Uuid::new_v4().to_string();
                        let wiki_rel = format!("wiki/archive/raptor_summary_{}.md", &uuid[..8]);
                        let wiki_content = format!(
                            "---\ntype: \"raptor_summary\"\noriginal_title: \"{}\"\n---\n\n# Raptor Summary: {}\n\n{}",
                            ep.title, ep.title, summary
                        );
                        let _ = store.write_file(&wiki_rel, &wiki_content);

                        let node_contract = WikiNode {
                            id: None,
                            name: format!("Raptor Summary: {}", ep.title),
                            content: summary,
                            scope: ep.scope.clone().unwrap_or_else(|| "general".to_string()),
                            vault_path: Some(wiki_rel),
                            embedding: None,
                        };
                        let _ = db.save_wiki_node(&node_contract).await;
                    }

                    // 4. Demote the record in the database instead of deleting it (Epic 3)
                    if let Some(surreal_backend) = db
                        .as_any()
                        .downcast_ref::<crate::db::backend::SurrealBackend>()
                    {
                        let ep_id = ep
                            .id
                            .as_ref()
                            .ok_or_else(|| anyhow::anyhow!("Episode ID missing"))?;
                        let filename = std::path::Path::new(vp)
                            .file_name()
                            .unwrap_or_else(|| std::ffi::OsStr::new("episode.md"));
                        let new_vp = format!("vault/archive/{}", filename.to_string_lossy());

                        let query_sql = "UPDATE type::record('episode', $id) MERGE {
                            archived: true,
                            archived_at: time::now(),
                            utility: 1.0,
                            importance: 1.0,
                            vault_path: $new_vp
                        };";

                        let resp = surreal_backend
                            .db
                            .query(query_sql)
                            .bind(("id", ep_id.split(':').nth(1).unwrap_or(ep_id).to_string()))
                            .bind(("new_vp", new_vp))
                            .await?;
                        resp.check()?;
                    } else {
                        db.delete_by_vault_path(vp).await?;
                    }
                }
            }
        }
        Ok(())
    }
}

pub fn calculate_decay_factor(
    now: std::time::SystemTime,
    last_retrieved_at: std::time::SystemTime,
) -> f32 {
    let t_secs = now
        .duration_since(last_retrieved_at)
        .unwrap_or_default()
        .as_secs_f32();
    let t_days = t_secs / 86400.0f32;
    let lambda = 2.0f32.ln() / 30.0f32;
    (-lambda * t_days).exp()
}

pub fn should_prune_history(
    now: std::time::SystemTime,
    record_time: std::time::SystemTime,
) -> bool {
    let t_secs = now
        .duration_since(record_time)
        .unwrap_or_default()
        .as_secs();
    t_secs > 30 * 86400
}

pub fn extract_attention_anchors(text: &str) -> (String, Vec<String>) {
    let mut clean_lines = Vec::new();
    let mut anchors = Vec::new();

    for line in text.lines() {
        let line_lower = line.to_lowercase();
        if line_lower.contains("@attention-anchor") || line_lower.contains("[anchor:") {
            let mut anchor_text = line.to_string();
            if let Some(pos) = line_lower.find("@attention-anchor") {
                anchor_text = line[pos + "@attention-anchor".len()..].to_string();
            } else if let Some(pos) = line_lower.find("[anchor:") {
                anchor_text = line[pos + "[anchor:".len()..].to_string();
                if anchor_text.ends_with(']') {
                    anchor_text.pop();
                }
            }
            let trimmed = anchor_text.trim_start_matches(':').trim();
            if !trimmed.is_empty() {
                anchors.push(trimmed.to_string());
            }
        } else {
            clean_lines.push(line);
        }
    }

    (clean_lines.join("\n"), anchors)
}

fn get_active_stm_anchors(vault_root: &std::path::Path) -> Vec<String> {
    let mut anchors = Vec::new();
    let handoffs_dir = vault_root.join(".handoffs");
    if !handoffs_dir.exists() {
        return anchors;
    }

    if let Ok(entries) = std::fs::read_dir(handoffs_dir) {
        let mut stm_files = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .file_name()
                    .map_or(false, |name| name.to_string_lossy().starts_with("stm_"))
                && path.extension().map_or(false, |ext| ext == "json")
            {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        stm_files.push((path, modified));
                    }
                }
            }
        }

        stm_files.sort_by(|a, b| b.1.cmp(&a.1));

        if let Some((most_recent_path, _)) = stm_files.first() {
            if let Ok(content) = std::fs::read_to_string(most_recent_path) {
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(anchors_val) = json_val.get("_active_anchors") {
                        if let Some(arr) = anchors_val.as_array() {
                            for val in arr {
                                if let Some(s) = val.as_str() {
                                    anchors.push(s.to_string());
                                }
                            }
                        } else if let Some(s) = anchors_val.as_str() {
                            if s.starts_with('[') && s.ends_with(']') {
                                if let Ok(arr) = serde_json::from_str::<Vec<String>>(s) {
                                    anchors.extend(arr);
                                }
                            } else {
                                for part in s.split(&[',', '\n'][..]) {
                                    let trimmed = part.trim();
                                    if !trimmed.is_empty() {
                                        anchors.push(trimmed.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    anchors
}

#[allow(dead_code)]
fn scan_source_files(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let path_str = path.to_string_lossy();
            if path_str.contains("mythrax-core/src")
                || path_str.contains("mythrax-core/tests")
                || path_str.contains("mythrax-core/Cargo.toml")
            {
                continue;
            }
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name == "target"
                        || name == ".git"
                        || name == ".venv"
                        || name == ".agents"
                        || name == ".mythrax-shared"
                        || name == "vault"
                    {
                        continue;
                    }
                }
                scan_source_files(&path, files);
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext == "rs" || ext == "ts" || ext == "py" {
                        files.push(path);
                    }
                }
            }
        }
    }
}

async fn auto_page_workspace_files(_db: &dyn StorageBackend) -> Result<()> {
    Ok(())
}

pub async fn page_markdown_code_blocks(db: &dyn StorageBackend, markdown: &str) -> Result<String> {
    let Some(surreal) = db.as_any().downcast_ref::<crate::db::SurrealBackend>() else {
        return Ok(markdown.to_string());
    };

    let mut result = String::new();
    let mut in_code_block = false;
    let mut current_block = String::new();
    let mut current_lang = "";

    for line in markdown.lines() {
        if in_code_block {
            if line.trim() == "```" {
                // End of code block. Page the content
                let paged = if !current_lang.is_empty() {
                    crate::cognitive::paging::page_code_block(surreal, &current_block, current_lang)
                        .await?
                } else {
                    current_block.clone()
                };
                result.push_str(&paged);
                if !result.ends_with('\n') {
                    result.push('\n');
                }
                result.push_str("```\n");
                in_code_block = false;
                current_block.clear();
            } else {
                current_block.push_str(line);
                current_block.push_str("\n");
            }
        } else {
            if line.starts_with("```") {
                let lang = line["```".len()..].trim();
                let matched_lang = match lang {
                    "rust" | "rs" => Some("rs"),
                    "typescript" | "ts" => Some("ts"),
                    "javascript" | "js" => Some("js"),
                    "python" | "py" => Some("py"),
                    _ => None,
                };
                if let Some(l) = matched_lang {
                    in_code_block = true;
                    current_lang = l;
                    result.push_str(line);
                    result.push_str("\n");
                } else {
                    result.push_str(line);
                    result.push_str("\n");
                }
            } else {
                result.push_str(line);
                result.push_str("\n");
            }
        }
    }

    if markdown.ends_with('\n') {
        Ok(result)
    } else {
        if result.ends_with('\n') {
            result.pop();
        }
        Ok(result)
    }
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a.sqrt() * norm_b.sqrt())
    }
}
