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
        let insights = load_insights(&store.vault_root);
        let scope_insights: Vec<_> = insights
            .into_iter()
            .filter(|ins| ins.scope == scope)
            .collect();

        if scope_insights.is_empty() {
            return Ok(());
        }

        for (chunk_idx, chunk) in scope_insights.chunks(5).enumerate() {
            let mut combined_content = String::new();
            for ins in chunk {
                combined_content.push_str(&format!("Insight Title: {}\nInsight Body:\n{}\n\n", ins.title, ins.content));
            }

            let sys_prompt = "You are an architectural compactor. Summarize the key architectural decisions, design patterns, and systemic constraints described in these insights.";
            let prompt_text = format!("Insights:\n\n{}", combined_content);
            let summary = self.llm.completion(db, Some(sys_prompt), &prompt_text).await?;

            let first_title = chunk.first().map(|c| c.title.as_str()).unwrap_or("compaction");
            let slug = first_title.to_lowercase().replace([' ', '/'], "_");
            let uuid = uuid::Uuid::new_v4().to_string();
            let relative_path = format!("wiki/compaction/{}_{}_{}.md", scope, slug, &uuid[..8]);
            
            let file_content = format!(
                "---\ntype: \"compaction\"\nscope: \"{}\"\nchunk_index: {}\n---\n\n# Architectural Compaction: {}\n\n{}",
                scope,
                chunk_idx,
                scope,
                summary
            );
            store.write_file(&relative_path, &file_content)?;

            let node_contract = WikiNode {
                id: None,
                name: format!("Compaction: {} - Chunk {}", scope, chunk_idx),
                content: summary.clone(),
                scope: scope.to_string(),
                vault_path: Some(relative_path.clone()),
                embedding: None,
            };
            if let Ok(compaction_id) = db.save_wiki_node(&node_contract).await {
                for ins in chunk {
                    let rel_path = Path::new(&ins.vault_path)
                        .strip_prefix(&store.vault_root)
                        .unwrap_or(Path::new(&ins.vault_path))
                        .to_string_lossy()
                        .to_string();
                    if let Ok(Some(insight_id)) = db.get_wiki_node_id_by_vault_path(&rel_path).await {
                        let _ = db.relate_nodes(&insight_id, &compaction_id).await;
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn compact_global(
        &self,
        db: &dyn StorageBackend,
        store: &MarkdownStore,
    ) -> Result<()> {
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
