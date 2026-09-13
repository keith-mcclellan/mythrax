## 2024-05-24 - [Fix NoSQL injection in session cleanup]
**Vulnerability:** NoSQL injection vulnerability found in `src/mcp_routes/vault_handlers.rs` where user-supplied `session_id` was being directly interpolated into a SurrealDB query string using `format!()`.
**Learning:** In Rust/SurrealDB, directly formatting strings for queries bypasses parameterization, allowing arbitrary query execution. `session_id`s could be maliciously crafted to delete additional records or bypass constraints.
**Prevention:** Always use parameterized queries with `$placeholder` variables and the `.bind()` method for SurrealDB queries, rather than standard string formatting.
