pub mod backend;
pub mod schema;

pub use backend::{StorageBackend, SurrealBackend, GLOBAL_BACKEND};
pub use backend::parse_record_id;
#[allow(unused_imports)] // used in tests/test_stm.rs
pub use backend::record_key_to_string;

