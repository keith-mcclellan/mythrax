## 2025-02-28 - [SQL Injection in Vault Cleanup]
**Vulnerability:** Found a SQL injection vulnerability in `mythrax-core/src/mcp_routes/vault_handlers.rs` where user-controlled string variables (like session ID) were directly interpolated into a SurrealDB query string using `format!`.
**Learning:** `format!` interpolation in SurrealDB `query()` calls bypasses query parameterization and creates a direct path for NoSQL/SQL injection, allowing arbitrary command execution within the database context.
**Prevention:** Always use parameterized queries with `$` placeholders (e.g., `$session_id`) inside the query string, followed by `.bind(("session_id", value.clone()))` on the query object.
