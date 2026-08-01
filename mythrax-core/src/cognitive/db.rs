use crate::contracts::{Fact, IdeaNode, IdeaStatus, PipelineConfig, WikiNode};
use crate::db::StorageBackend;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use surrealdb_types::SurrealValue;

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct RefinementLog {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub idea_node_id: String,
    pub fact_id: String,
    pub action: String, // "support", "contradict", "irrelevant"
    pub previous_confidence: f32,
    pub new_confidence: f32,
    pub reasoning: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn save_fact(backend: &dyn StorageBackend, fact: &Fact) -> Result<String> {
    backend.save_fact(fact).await
}

pub async fn get_fact(backend: &dyn StorageBackend, id: &str) -> Result<Option<Fact>> {
    backend.get_fact(id).await
}

pub async fn get_facts_by_scope(backend: &dyn StorageBackend, scope: &str) -> Result<Vec<Fact>> {
    backend.get_facts_by_scope(scope).await
}

pub async fn get_unassociated_facts(backend: &dyn StorageBackend, scope: &str) -> Result<Vec<Fact>> {
    backend.get_unassociated_facts(scope).await
}

pub async fn save_idea_node(backend: &dyn StorageBackend, idea: &IdeaNode) -> Result<String> {
    backend.save_idea_node(idea).await
}

pub async fn get_idea_node(backend: &dyn StorageBackend, id: &str) -> Result<Option<IdeaNode>> {
    backend.get_idea_node(id).await
}

pub async fn get_idea_nodes_by_scope(backend: &dyn StorageBackend, scope: &str) -> Result<Vec<IdeaNode>> {
    backend.get_idea_nodes_by_scope(scope).await
}

pub async fn get_validated_idea_nodes(
    backend: &dyn StorageBackend,
    scope: &str,
    min_confidence: f32,
) -> Result<Vec<IdeaNode>> {
    let nodes = backend.get_idea_nodes_by_scope(scope).await?;
    Ok(nodes
        .into_iter()
        .filter(|n| n.status == IdeaStatus::Validated && n.confidence >= min_confidence)
        .collect())
}

pub async fn get_pruned_idea_nodes(
    backend: &dyn StorageBackend,
    scope: &str,
    max_confidence: f32,
) -> Result<Vec<IdeaNode>> {
    let mut all_pruned = Vec::new();
    if let Ok(nodes) = backend.get_idea_nodes_by_scope(scope).await {
        all_pruned.extend(
            nodes
                .into_iter()
                .filter(|n| n.status == IdeaStatus::Pruned || n.confidence <= max_confidence),
        );
    }
    if scope != "general" {
        if let Ok(gen_nodes) = backend.get_idea_nodes_by_scope("general").await {
            all_pruned.extend(
                gen_nodes
                    .into_iter()
                    .filter(|n| n.status == IdeaStatus::Pruned || n.confidence <= max_confidence),
            );
        }
    }
    Ok(all_pruned)
}

pub async fn delete_fact(backend: &dyn StorageBackend, id: &str) -> Result<()> {
    backend.delete_fact(id).await
}

pub async fn delete_idea_node(backend: &dyn StorageBackend, id: &str) -> Result<()> {
    backend.delete_idea_node(id).await
}

pub async fn get_pipeline_config(backend: &dyn StorageBackend) -> Result<PipelineConfig> {
    if let Ok(Some(cfg)) = backend.get_pipeline_config().await {
        Ok(cfg)
    } else {
        Ok(PipelineConfig::default())
    }
}

pub async fn save_code_symbol(
    backend: &dyn StorageBackend,
    symbol: &crate::contracts::CodeSymbol,
) -> Result<String> {
    let slug_name = format!("{}_{}", symbol.file_slug, symbol.name);

    // Mirror AST CodeSymbol into physical Obsidian vault file
    let rel_ast_path = format!("reference/ast/{}_{}_ast.md", symbol.file_slug, symbol.name);
    let root = crate::store::find_vault_root();
    let full_ast_path = root.join(&rel_ast_path);
    if let Some(parent) = full_ast_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let doc = format!(
        "---\ntitle: \"AST: {}\"\nscope: \"{}\"\nnode_type: \"ast_symbol\"\n---\n\n# AST: {}\n\n**Symbol:** `{}` ({})\n**File:** `{}` (L{}-L{})\n**Signature:** `{}`\n\n**Doc Comment:**\n{}\n",
        symbol.name,
        symbol.scope,
        symbol.name,
        symbol.name,
        symbol.symbol_type,
        symbol.file_path,
        symbol.start_line,
        symbol.end_line,
        symbol.signature,
        symbol.doc_comment.as_deref().unwrap_or("None")
    );
    let _ = std::fs::write(&full_ast_path, &doc);

    // Mirror to database via WikiNode so it's indexed regardless of backend type
    let ast_node = WikiNode {
        id: None,
        name: format!("ast/{}", slug_name),
        content: doc,
        scope: symbol.scope.clone(),
        vault_path: Some(rel_ast_path),
        node_type: Some("ast_symbol".to_string()),
        item_type: Some("ast_symbol".to_string()),
        ..Default::default()
    };
    let _ = backend.save_wiki_node(&ast_node).await;

    if let Some(surreal) = backend.as_any().downcast_ref::<crate::db::SurrealBackend>() {
        let query = "
            UPSERT type::record('code_symbol', $slug_name) CONTENT {
                name: $name,
                symbol_type: $symbol_type,
                file_path: $file_path,
                file_slug: $file_slug,
                start_line: $start_line,
                end_line: $end_line,
                signature: $signature,
                doc_comment: $doc_comment,
                call_graph: $call_graph,
                scope: $scope,
                embedding: $embedding,
                created_at: time::now()
            };
        ";
        let mut resp = surreal
            .db
            .query(query)
            .bind(("slug_name", slug_name.as_str()))
            .bind(("name", symbol.name.as_str()))
            .bind(("symbol_type", symbol.symbol_type.as_str()))
            .bind(("file_path", symbol.file_path.as_str()))
            .bind(("file_slug", symbol.file_slug.as_str()))
            .bind(("start_line", symbol.start_line as i64))
            .bind(("end_line", symbol.end_line as i64))
            .bind(("signature", symbol.signature.as_str()))
            .bind(("doc_comment", symbol.doc_comment.as_deref()))
            .bind(("call_graph", symbol.call_graph.clone()))
            .bind(("scope", symbol.scope.as_str()))
            .bind(("embedding", symbol.embedding.clone()))
            .await?;
        let raw: Option<crate::contracts::CodeSymbol> = resp.take(0)?;
        Ok(raw.and_then(|r| r.id).unwrap_or_else(|| slug_name))
    } else {
        Ok(slug_name)
    }
}

pub async fn save_subagent_worktree(
    backend: &dyn StorageBackend,
    worktree: &crate::contracts::SubagentWorktree,
) -> Result<String> {
    if let Some(surreal) = backend.as_any().downcast_ref::<crate::db::SurrealBackend>() {
        let query = "
            UPSERT type::record('subagent_worktree', $subagent_id) CONTENT {
                subagent_id: $subagent_id,
                worktree_path: $worktree_path,
                base_branch: $base_branch,
                feature_branch: $feature_branch,
                created_at: time::now(),
                status: $status
            };
        ";
        let mut resp = surreal
            .db
            .query(query)
            .bind(("subagent_id", worktree.subagent_id.as_str()))
            .bind(("worktree_path", worktree.worktree_path.as_str()))
            .bind(("base_branch", worktree.base_branch.as_str()))
            .bind(("feature_branch", worktree.feature_branch.as_str()))
            .bind(("status", worktree.status.as_str()))
            .await?;
        let raw: Option<crate::contracts::SubagentWorktree> = resp.take(0)?;
        Ok(raw.and_then(|r| r.id).unwrap_or_else(|| worktree.subagent_id.clone()))
    } else {
        Ok(worktree.subagent_id.clone())
    }
}

pub async fn save_pipeline_config(
    backend: &dyn StorageBackend,
    config: &PipelineConfig,
) -> Result<()> {
    backend.save_pipeline_config(config).await
}

pub async fn queue_cognitive_task(
    backend: &dyn StorageBackend,
    task_type: &str,
    payload: &str,
    scope: &str,
) -> Result<String> {
    let task = crate::contracts::CognitiveTask {
        id: None,
        task_type: task_type.to_string(),
        payload: payload.to_string(),
        scope: scope.to_string(),
        status: "pending".to_string(),
        created_at: Some(chrono::Utc::now()),
    };
    if let Some(surreal) = backend.as_any().downcast_ref::<crate::db::SurrealBackend>() {
        let task_id = uuid::Uuid::new_v4().to_string();
        let record_id = crate::db::parse_record_id(&format!("cognitive_task:{}", task_id))?;
        let mut save = task.clone();
        save.id = None;
        let _res: Option<crate::contracts::CognitiveTask> = surreal.db.upsert(record_id).content(save).await?;
        Ok(task_id)
    } else {
        Ok(uuid::Uuid::new_v4().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_refinement_log_struct() {
        let log = RefinementLog {
            id: Some("log_1".to_string()),
            idea_node_id: "idea_1".to_string(),
            fact_id: "fact_1".to_string(),
            action: "support".to_string(),
            previous_confidence: 0.50,
            new_confidence: 0.65,
            reasoning: "Fact confirms claim".to_string(),
            created_at: None,
        };
        assert_eq!(log.action, "support");
        assert_eq!(log.new_confidence, 0.65);
    }
}
