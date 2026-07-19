#![allow(async_fn_in_trait)]
#![recursion_limit = "512"]

pub mod api;
pub mod cli;
pub mod contracts;
pub mod db;
pub mod embeddings;
pub mod secret_filter;
pub mod store;
pub mod math;
pub mod vault;
pub mod auth;
pub mod verify;
pub mod llm;
pub mod cognitive;
pub mod mcp;
pub mod mcp_routes;
pub mod daemon;
pub mod hooks;
pub mod parser;
pub mod retrieval;
pub mod bench;

pub fn is_test_mock() -> bool {
    std::env::var("MYTHRAX_TEST_MOCK").map(|v| v == "1" || v == "true" || v == "yes").unwrap_or(false)
        || std::env::var("MYTHRAX_MOCK_LLM").map(|v| v == "1" || v == "true" || v == "yes").unwrap_or(false)
        || std::env::var("MYTHRAX_TEST_MOCK").is_ok()
        || std::env::var("MYTHRAX_MOCK_LLM").is_ok()
}




