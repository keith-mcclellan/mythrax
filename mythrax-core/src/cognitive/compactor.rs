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

    pub async fn compact_scope(
        &self,
        db: &dyn StorageBackend,
        store: &MarkdownStore,
        scope: &str,
    ) -> Result<()> {
        let _ = db.prune_stale_memories(&store.vault_root).await;
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

            let sys_prompt = "You are an architectural compactor. Summarize the key architectural decisions, design patterns, and systemic constraints described in these insights.";
            let prompt_text = format!("Insights:\n\n{}", combined_content);
            let summary = self.llm.completion(db, Some(sys_prompt), &prompt_text).await?;

            let first_title = member_insights.first().map(|(c, _)| c.title.as_str()).unwrap_or("compaction");
            let slug = first_title.to_lowercase().replace([' ', '/'], "_");
            let uuid = uuid::Uuid::new_v4().to_string();
            let relative_path = format!("wiki/compaction/{}_{}_{}.md", scope, slug, &uuid[..8]);

            let file_content = format!(
                "---\ntype: \"compaction\"\nscope: \"{}\"\ncluster_id: {}\n---\n\n# Architectural Compaction: {}\n\n{}",
                scope,
                cluster_id,
                scope,
                summary
            );
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

            let sys_prompt = "You are an architectural compactor. Summarize the key architectural decisions, design patterns, and systemic constraints described in these insights.";
            let prompt_text = format!("Insights:\n\n{}", combined_content);
            let summary = self.llm.completion(db, Some(sys_prompt), &prompt_text).await?;

            let uuid = uuid::Uuid::new_v4().to_string();
            let relative_path = format!("wiki/compaction/{}_miscellaneous_{}.md", scope, &uuid[..8]);

            let file_content = format!(
                "---\ntype: \"compaction\"\nscope: \"{}\"\ncluster_id: \"miscellaneous\"\n---\n\n# Architectural Compaction: {} (Miscellaneous)\n\n{}",
                scope,
                scope,
                summary
            );
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

        let sys_prompt = "You are a master systems architect. Synthesize all the provided architectural compactions into a single, cohesive global systems synthesis document outlining overall patterns, critical rules, and systems status.";
        let prompt_text = format!("Architectural Compactions:\n\n{}", combined_compaction);
        let global_summary = self.llm.completion(db, Some(sys_prompt), &prompt_text).await?;

        let uuid = uuid::Uuid::new_v4().to_string();
        let relative_path = format!("wiki/general/global_compaction_{}.md", &uuid[..8]);
        let file_content = format!(
            "---\ntype: \"global_compaction\"\n---\n\n# Global Systems Synthesis\n\n{}",
            global_summary
        );
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
}
