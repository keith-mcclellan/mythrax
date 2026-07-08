#![allow(async_fn_in_trait)]
#![recursion_limit = "512"]

pub mod api;
pub mod cli;
pub mod contracts;
pub mod db;
pub mod embeddings;
pub mod secret_filter;
pub mod store;
pub mod wal;
pub mod vault;
pub mod auth;
pub mod verify;
pub mod llm;
pub mod cognitive;
pub mod mcp;
pub mod mcp_routes;
pub mod daemon;
pub mod hooks;
pub mod retrieval;

pub mod bench;



