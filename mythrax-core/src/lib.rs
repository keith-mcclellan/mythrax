#![allow(async_fn_in_trait)]
#![recursion_limit = "512"]

pub mod api;
pub mod auth;
pub mod cli;
pub mod cognitive;
pub mod contracts;
pub mod daemon;
pub mod db;
pub mod embeddings;
pub mod hooks;
pub mod llm;
pub mod mcp;
pub mod mcp_routes;
pub mod retrieval;
pub mod secret_filter;
pub mod store;
pub mod vault;
pub mod verify;
pub mod wal;

#[cfg(any(test, feature = "bench"))]
pub mod bench;
