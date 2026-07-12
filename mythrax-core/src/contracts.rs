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
    pub archived_at: Option<String>,
    pub node_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
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
    pub node_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EpisodeSaveBuilder {
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
    pub node_type: Option<String>,
    pub confidence: Option<f32>,
    pub created_at: Option<String>,
}

impl EpisodeSaveBuilder {
    pub fn new(title: String, content: String) -> Self {
        Self {
            title,
            content,
            entities: Vec::new(),
            scope: None,
            vault_path: None,
            source_episode: None,
            session_id: None,
            task_id: None,
            discovery_tokens: None,
            facts: None,
            concepts: None,
            files_read: None,
            files_modified: None,
            node_type: None,
            confidence: None,
            created_at: None,
        }
    }

    pub fn entities(mut self, entities: Vec<Entity>) -> Self {
        self.entities = entities;
        self
    }

    pub fn scope(mut self, scope: Option<String>) -> Self {
        self.scope = scope;
        self
    }

    pub fn vault_path(mut self, vault_path: Option<String>) -> Self {
        self.vault_path = vault_path;
        self
    }

    pub fn source_episode(mut self, source_episode: Option<String>) -> Self {
        self.source_episode = source_episode;
        self
    }

    pub fn session_id(mut self, session_id: Option<String>) -> Self {
        self.session_id = session_id;
        self
    }

    pub fn task_id(mut self, task_id: Option<String>) -> Self {
        self.task_id = task_id;
        self
    }

    pub fn discovery_tokens(mut self, discovery_tokens: Option<u32>) -> Self {
        self.discovery_tokens = discovery_tokens;
        self
    }

    pub fn facts(mut self, facts: Option<Vec<String>>) -> Self {
        self.facts = facts;
        self
    }

    pub fn concepts(mut self, concepts: Option<Vec<String>>) -> Self {
        self.concepts = concepts;
        self
    }

    pub fn files_read(mut self, files_read: Option<Vec<String>>) -> Self {
        self.files_read = files_read;
        self
    }

    pub fn files_modified(mut self, files_modified: Option<Vec<String>>) -> Self {
        self.files_modified = files_modified;
        self
    }

    pub fn node_type(mut self, node_type: Option<String>) -> Self {
        self.node_type = node_type;
        self
    }

    pub fn confidence(mut self, confidence: Option<f32>) -> Self {
        self.confidence = confidence;
        self
    }

    pub fn created_at(mut self, created_at: Option<String>) -> Self {
        self.created_at = created_at;
        self
    }

    pub fn build(self) -> EpisodeSave {
        EpisodeSave {
            title: self.title,
            content: self.content,
            entities: self.entities,
            scope: self.scope,
            vault_path: self.vault_path,
            source_episode: self.source_episode,
            session_id: self.session_id,
            task_id: self.task_id,
            discovery_tokens: self.discovery_tokens,
            facts: self.facts,
            concepts: self.concepts,
            files_read: self.files_read,
            files_modified: self.files_modified,
            node_type: self.node_type,
            confidence: self.confidence,
            created_at: self.created_at,
        }
    }
}

impl EpisodeSave {
    pub fn builder(title: String, content: String) -> EpisodeSaveBuilder {
        EpisodeSaveBuilder::new(title, content)
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_retrieved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue, Default)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_type: Option<String>,
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
    pub llm_post_inference_delay_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfigRequest {
    pub provider: String,
    pub duration: Option<String>,
    pub model: Option<String>,
    pub cloud_provider: Option<String>,
    pub api_key: Option<String>,
    pub llm_post_inference_delay_ms: Option<u64>,
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
    pub include_tool_execution: Option<bool>,
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
    pub include_tool_execution: Option<bool>,
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

