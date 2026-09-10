## 2025-02-18 - NoSQL Injection in SurrealDB Queries
**Vulnerability:** SurrealDB queries constructed using string formatting (e.g. `format!`) are vulnerable to NoSQL injection if they include unsanitized user inputs, such as `session_id`.
**Learning:** The SurrealDB Rust driver provides a parameterized query interface using `bind` that escapes parameters properly, but older queries manually interpolated variables directly into the query string.
**Prevention:** Always use parameterized queries (e.g., `db.query(sql).bind(("var", value))`) instead of string interpolation for variables.
