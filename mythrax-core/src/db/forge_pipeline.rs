use crate::db::backend::{SurrealBackend, record_key_to_string, unescape_id_part, StorageBackend};
use crate::contracts::ForgedSectionBatch;
use anyhow::Result;
use uuid::Uuid;

impl SurrealBackend {
    pub async fn save_forged_section_db(&self, batch: &ForgedSectionBatch) -> Result<()> {
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
}
