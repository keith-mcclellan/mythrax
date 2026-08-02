#![allow(async_fn_in_trait)]
#![recursion_limit = "512"]

pub mod api;
pub mod auth;
pub mod bench;
pub mod cli;
pub mod cognitive;
pub mod config;
pub mod contracts;
pub mod daemon;
pub mod db;
pub mod embeddings;
pub mod hooks;
pub mod llm;
pub mod math;
pub mod mcp;
pub mod mcp_routes;
pub mod parser;
pub mod retrieval;
pub mod secret_filter;
pub mod store;
pub mod vault;
pub mod verify;

pub fn is_test_mock() -> bool {
    let check_var = |name: &str| -> bool {
        match std::env::var(name) {
            Ok(v) => v == "1" || v == "true" || v == "yes",
            Err(_) => false,
        }
    };
    check_var("MYTHRAX_TEST_MOCK") || check_var("MYTHRAX_MOCK_LLM")
}
