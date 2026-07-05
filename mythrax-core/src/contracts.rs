use serde::{Deserialize, Serialize};
use surrealdb_types::SurrealValue;

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct ChatMessage {
    pub id: Option<String>,
    pub session_id: String,
    pub role: String, // "user" or "assistant"
    pub content: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Entity {
    pub name: String,
    pub entity_type: String, // "class" | "concept" | "technology" | "file" | "pattern"
    pub summary: String,
    pub labels: Vec<String>,
    pub scope: Option<String>,
    pub vault_path: Option<String>,
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue, Default)]
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
    pub last_retrieved_at: Option<String>,
    pub utility: Option<f32>,
    pub archived: Option<bool>,
    pub discovery_tokens: Option<u32>,
    pub facts: Option<Vec<String>>,
    pub concepts: Option<Vec<String>>,
    pub files_read: Option<Vec<String>>,
    pub files_modified: Option<Vec<String>>,
    pub session_id: Option<String>,
    pub word_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EpisodeSave {
    pub title: String,
    pub content: String,
    pub entities: Vec<Entity>,
    pub scope: Option<String>,
    pub vault_path: Option<String>,
    pub source_episode: Option<String>,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub discovery_tokens: Option<u32>,
    pub facts: Option<Vec<String>>,
    pub concepts: Option<Vec<String>>,
    pub files_read: Option<Vec<String>>,
    pub files_modified: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub content: String,
    pub similarity: f32,
    pub utility: f32,
    pub tier: String,
    #[serde(default, skip_serializing)]
    pub embedding: Option<Vec<f32>>,
    pub vault_path: Option<String>,
    pub source_episode: Option<String>,
    pub discovery_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_nodes: Option<Vec<SearchResult>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_vector_sim: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_gate: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factor_multiplier: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub word_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bm25_score: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
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
    pub status: Option<String>,
    pub superseded_at: Option<String>,
    pub superseded_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    pub id: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
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

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
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

/// Full hydrated Handoff — reserved for agent-tracking API; construction deferred.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct Handoff {
    pub id: Option<String>,
    pub parent_conversation_id: String,
    pub subagent_conversation_id: String,
    pub summary: String,
    pub handoff_file_path: String,
    pub scope: Option<String>,
    pub status: Option<String>,
    pub created_at: Option<String>,
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct WikiNode {
    pub id: Option<String>,
    pub name: String,
    pub content: String,
    pub scope: String,
    pub vault_path: Option<String>,
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub total_matches: usize,
    pub has_more: bool,
    pub next_offset: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub omitted_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WisdomSearchResponse {
    pub results: Vec<WisdomRule>,
    pub total_matches: usize,
    pub has_more: bool,
    pub next_offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetMemoryNodesRequest {
    pub node_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetMemoryNodesResponse {
    pub episodes: Vec<Episode>,
    pub wisdom_rules: Vec<WisdomRule>,
    pub wiki_nodes: Vec<WikiNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgedConcept {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgedRule {
    pub target_pattern: String,
    pub action_to_avoid: String,
    pub causal_explanation: String,
    pub prescribed_remedy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgedSectionBatch {
    pub doc_title: String,
    pub scope: String,
    pub chunk_index: usize,
    pub chunk_text: String,
    pub concepts: Vec<ForgedConcept>,
    pub rules: Vec<ForgedRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct BeliefState {
    pub id: Option<String>,
    pub session_id: String,
    pub tasks_todo: Vec<String>,
    pub hypotheses_tested: Vec<String>,
    pub confidence_score: f32,
    pub uncertainty_areas: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct ThoughtNode {
    pub id: Option<String>,
    pub title: String,
    pub content: String,
    pub scope: String,
    pub vault_path: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexRow {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub similarity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedHookInput {
    pub session_id: String,
    pub transcript_path: Option<String>,
    pub query: Option<String>,
    pub workspace_path: Option<String>,
    pub stop_hook_active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    #[serde(rename = "continue")]
    pub continue_: bool,
    pub suppress_output: bool,
    pub exit_code: i32,
    pub injected: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SymbolicHit {
    pub node_id: String,
    pub path_confidence: f32,
    pub hops: usize,
}
