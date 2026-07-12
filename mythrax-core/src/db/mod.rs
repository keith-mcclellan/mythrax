pub mod backend;
pub mod schema;
pub mod query_classification;
pub mod crud_operations;
pub mod forge_pipeline;
pub mod search_pipeline;
pub mod cognitive_tasks;

pub use backend::{StorageBackend, SurrealBackend, GLOBAL_BACKEND, EpisodeRaw};
pub use backend::parse_record_id;
#[allow(unused_imports)] // used in tests/test_stm.rs
pub use backend::record_key_to_string;
pub use cognitive_tasks::{CognitiveTask, CognitiveTaskType, ExpectedFormat, Priority, TaskStatus};


