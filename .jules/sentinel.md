## 2025-09-06 - Prevent NoSQL Injection in vault_handlers.rs
**Vulnerability:** A NoSQL injection vulnerability exists in `src/mcp_routes/vault_handlers.rs` where the session ID was interpolated directly into a SurrealDB query string using `format!`.
**Learning:** This approach bypasses parameterized query protection, making the application vulnerable to injection attacks if the session ID is maliciously crafted.
**Prevention:** Always use parameterized queries with `$param` syntax and bind the arguments via `.bind(("param", value))` when passing variables into SurrealDB queries.
