use crate::contracts::{Fact, IdeaNode, IdeaStatus, PipelineConfig};
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
