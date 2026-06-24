use crate::db::StorageBackend;
use crate::llm::LLMClient;
use crate::store::MarkdownStore;
use crate::cognitive::synthesis::load_insights;
use crate::contracts::WikiNode;
use std::path::Path;
use anyhow::Result;

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
            let timestamp = chk.get("timestamp").and_then(|v| v.as_str()).unwrap_or("unknown");
            let project_type = chk.get("project_type").and_then(|v| v.as_str()).unwrap_or("unknown");
            let exit_code = chk.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(0);
            let errors = chk.get("compiler_errors").and_then(|v| v.as_str()).unwrap_or("");
            let git_diff = chk.get("git_diff").and_then(|v| v.as_str()).unwrap_or("");

            if i < 2 {
                prompt_content.push_str(&format!(
                    "### Checkpoint {} (Raw - High Fidelity)\n\
                     Timestamp: {}\n\
                     Project Type: {}\n\
                     Compiler Exit Code: {}\n\
                     Compiler/Linter Output:\n{}\n\
                     Git Diff:\n{}\n\n",
                    i + 1, timestamp, project_type, exit_code, errors, git_diff
                ));
            } else {
                let compact_diff = if git_diff.len() > 200 {
                    let summary_prompt = format!("Summarize this git diff briefly (under 50 words):\n\n{}", git_diff);
                    self.llm.completion(db, Some("You are a code summarizer."), &summary_prompt).await.unwrap_or_else(|_| "Git diff summary failed".to_string())
                } else {
                    git_diff.to_string()
                };

                prompt_content.push_str(&format!(
                    "### Checkpoint {} (Compressed Summary)\n\
                     Timestamp: {}\n\
                     Project Type: {}\n\
                     Compiler Exit Code: {}\n\
                     Summary of Changes:\n{}\n\n",
                    i + 1, timestamp, project_type, exit_code, compact_diff
                ));
            }
        }

        let sys_prompt = "You are a master systems architect. Analyze the sequence of checkpoints and summarize the transitions between them, detailing how the codebase evolved, what errors were resolved, and the progression of active changes.";
        let summary = self.llm.completion(db, Some(sys_prompt), &prompt_content).await?;
        Ok(summary)
    }

    pub async fn compact_scope(
        &self,
        db: &dyn StorageBackend,
        store: &MarkdownStore,
        scope: &str,
    ) -> Result<()> {
        let _ = db.prune_stale_memories(&store.vault_root).await;
        let _ = self.archive_decayed_episodes(db, store).await;
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

        let mut clusters: std::collections::HashMap<usize, Vec<(crate::cognitive::synthesis::InsightNote, String)>> = std::collections::HashMap::new();

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
                combined_content.push_str(&format!("Insight Title: {}\nInsight Body:\n{}\n\n", ins.title, ins.content));
            }

            // Extract anchors and clean content
            let (cleaned_content, extracted_anchors) = extract_attention_anchors(&combined_content);

            let sys_prompt = "You are an architectural compactor. Summarize the key architectural decisions, design patterns, and systemic constraints described in these insights.";
            let prompt_text = format!("Insights:\n\n{}", cleaned_content);
            let summary = self.llm.completion(db, Some(sys_prompt), &prompt_text).await?;

            let stm_anchors = get_active_stm_anchors(&store.vault_root);
            let mut all_anchors = extracted_anchors;
            for sa in stm_anchors {
                if !all_anchors.contains(&sa) {
                    all_anchors.push(sa);
                }
            }

            let first_title = member_insights.first().map(|(c, _)| c.title.as_str()).unwrap_or("compaction");
            let slug = first_title.to_lowercase().replace([' ', '/'], "_");
            let uuid = uuid::Uuid::new_v4().to_string();
            let relative_path = format!("wiki/compaction/{}_{}_{}.md", scope, slug, &uuid[..8]);

            let mut file_content = format!(
                "---\ntype: \"compaction\"\nscope: \"{}\"\ncluster_id: {}\n---\n\n# Architectural Compaction: {}\n\n{}",
                scope,
                cluster_id,
                scope,
                summary
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
                        let _ = db.relate_nodes(insight_id, &compaction_id).await;
                    }
                }
            }

            tracing::info!("Compacted scope '{}' cluster {}: summary saved", scope, cluster_id);
        }

        // 4. Handle outlier insights by grouping them into a single miscellaneous compaction
        if !outlier_insights.is_empty() {
            let mut combined_content = String::new();
            for (ins, _) in &outlier_insights {
                combined_content.push_str(&format!("Insight Title: {}\nInsight Body:\n{}\n\n", ins.title, ins.content));
            }

            // Extract anchors and clean content
            let (cleaned_content, extracted_anchors) = extract_attention_anchors(&combined_content);

            let sys_prompt = "You are an architectural compactor. Summarize the key architectural decisions, design patterns, and systemic constraints described in these insights.";
            let prompt_text = format!("Insights:\n\n{}", cleaned_content);
            let summary = self.llm.completion(db, Some(sys_prompt), &prompt_text).await?;

            let stm_anchors = get_active_stm_anchors(&store.vault_root);
            let mut all_anchors = extracted_anchors;
            for sa in stm_anchors {
                if !all_anchors.contains(&sa) {
                    all_anchors.push(sa);
                }
            }

            let uuid = uuid::Uuid::new_v4().to_string();
            let relative_path = format!("wiki/compaction/{}_miscellaneous_{}.md", scope, &uuid[..8]);

            let mut file_content = format!(
                "---\ntype: \"compaction\"\nscope: \"{}\"\ncluster_id: \"miscellaneous\"\n---\n\n# Architectural Compaction: {} (Miscellaneous)\n\n{}",
                scope,
                scope,
                summary
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
                        let _ = db.relate_nodes(insight_id, &compaction_id).await;
                    }
                }
            }

            tracing::info!("Compacted scope '{}' outliers: miscellaneous summary saved", scope);
        }

        Ok(())
    }

    pub async fn compact_global(
        &self,
        db: &dyn StorageBackend,
        store: &MarkdownStore,
    ) -> Result<()> {
        let _ = db.prune_stale_memories(&store.vault_root).await;
        let _ = self.archive_decayed_episodes(db, store).await;
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
                    let rel_path = path.strip_prefix(&store.vault_root)
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

        let (cleaned_compaction, extracted_anchors) = extract_attention_anchors(&combined_compaction);

        let sys_prompt = "You are a master systems architect. Synthesize all the provided architectural compactions into a single, cohesive global systems synthesis document outlining overall patterns, critical rules, and systems status.";
        let prompt_text = format!("Architectural Compactions:\n\n{}", cleaned_compaction);
        let global_summary = self.llm.completion(db, Some(sys_prompt), &prompt_text).await?;

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
                    let _ = db.relate_nodes(&comp_id, &global_compaction_id).await;
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
        let episodes = db.get_all_episodes().await?;
        for ep in episodes {
            let utility = ep.utility.unwrap_or(50.0);
            if utility < 5.0 {
                // 1. Move physical file to vault/archive/
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

                    // 4. Delete the active record from database
                    db.delete_by_vault_path(vp).await?;
                }
            }
        }
        Ok(())
    }
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
                && path.file_name().map_or(false, |name| name.to_string_lossy().starts_with("stm_"))
                && path.extension().map_or(false, |ext| ext == "json") {
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
