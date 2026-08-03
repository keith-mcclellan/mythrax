use serde::{Deserialize, Serialize};
use surrealdb_types::SurrealValue;

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct SessionRecordDto {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct AnchorRowDto {
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct EpisodeQueryResultDto {
    pub id: surrealdb::types::RecordId,
    pub title: String,
    pub content: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
