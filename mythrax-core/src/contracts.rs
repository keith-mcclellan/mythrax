use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub name: String,
    pub entity_type: String, // "class" | "concept" | "technology" | "file" | "pattern"
    pub summary: String,
    pub labels: Vec<String>,
    pub scope: Option<String>,
    pub vault_path: Option<String>,
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: Option<String>,
    pub title: String,
    pub content: String,
    pub source: Option<String>,
    pub scope: Option<String>,
    pub vault_path: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub processed_in_dream: Option<bool>,
    pub source_episode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeSave {
    pub title: String,
    pub content: String,
    pub entities: Vec<Entity>,
    pub scope: Option<String>,
    pub vault_path: Option<String>,
    pub source_episode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub content: String,
    pub similarity: f32,
    pub utility: f32,
    pub tier: String,
    pub embedding: Option<Vec<f32>>,
    pub vault_path: Option<String>,
    pub source_episode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WisdomRule {
    pub id: Option<String>,
    pub target_pattern: String,
    pub action_to_avoid: String,
    pub causal_explanation: String,
    pub prescribed_remedy: String,
    pub tier: String, // "pinned" | "permanent" | "dynamic"
    pub scope: String,
    pub vault_path: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub source_episodes: Vec<String>,
    pub generator_name: String,
    pub similarity: Option<f32>,
    pub utility: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    pub id: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfigResponse {
    pub active_provider: String,
    pub cloud_provider: String,
    pub model: String,
    pub is_override: bool,
    pub expires_at: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfigRequest {
    pub provider: String,
    pub duration: Option<String>,
    pub model: Option<String>,
    pub cloud_provider: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HypothesisNode {
    pub node_id: String,
    pub parent_id: Option<String>,
    pub children_ids: Vec<String>,
    pub depth: i32,
    pub hypothesis: String,
    pub status: String,
    pub score: Option<f32>,
    pub result: Option<String>,
    pub insight: Option<String>,
    pub code_ref: Option<String>,
    pub code_changes: Option<std::collections::HashMap<String, String>>,
    pub scope: Option<String>,
    pub vault_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffSave {
    pub parent_conversation_id: String,
    pub subagent_conversation_id: String,
    pub summary: String,
    pub handoff_file_path: String,
    pub scope: Option<String>,
}


