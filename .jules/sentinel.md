## 2024-09-11 - NoSQL Injection via String Interpolation in SurrealDB Queries
**Vulnerability:** SurrealDB NoSQL injection in `vault_handlers.rs` where user-controlled session IDs were directly interpolated into delete queries using `format!()`.
**Learning:** Even internal or seemingly safe IDs (like session IDs) must never be interpolated directly into raw query strings, as an attacker controlling the ID could terminate the string literal and append malicious statements (e.g., `; DROP TABLE...`).
**Prevention:** Always use parameterized queries with `$` placeholders and `.bind()` to bind owned values, ensuring data is treated as parameters rather than executable query syntax.
