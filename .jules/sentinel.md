## 2024-05-18 - Fix NoSQL Injection in vault_handlers
**Vulnerability:** Found a NoSQL injection in `mythrax-core/src/mcp_routes/vault_handlers.rs` where `session_id` was directly formatted into a `DELETE` query string.
**Learning:** Even internal or admin routes must use parameterized queries. The application uses `format!` heavily for query construction instead of `$var` parameter binding, which is dangerous.
**Prevention:** Always use `$` placeholders (e.g. `$session_id`) and `.bind(("var_name", value))` when constructing database queries with dynamic input instead of `format!`.
