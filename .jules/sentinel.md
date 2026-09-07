## 2024-07-26 - [CRITICAL] Fix NoSQL Injection in vault_handlers.rs
**Vulnerability:** The `cleanup_stale_sessions` function constructed NoSQL queries using `format!` and directly interpolated the `session_id` string into the `DELETE` query, allowing for injection attacks.
**Learning:** Developers sometimes use `format!` out of habit for string building, forgetting that database query APIs (like SurrealDB's) require parameterized `.bind()` calls to prevent injection when handling user or untrusted data.
**Prevention:** Always use `$` placeholders and `.bind()` method on `surreal_backend.db.query()` instead of string interpolation or concatenation when passing arguments to database queries.
