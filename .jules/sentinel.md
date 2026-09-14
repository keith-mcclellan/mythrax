## 2026-09-14 - Fix NoSQL Injection in SurrealDB Queries
**Vulnerability:** Found NoSQL injection vulnerabilities where session IDs and table names were directly formatted into SQL strings using `format!` before passing to `surreal_backend.db.query()`.
**Learning:** Always use parameterized queries with `$var` placeholders and `.bind()` instead of direct string concatenation when dealing with database queries in SurrealDB. Table names can be handled using `type::table($var)`.
**Prevention:** Use `$placeholder` in the query and chain `.bind(("placeholder", value.clone()))` for dynamic variables instead of `format!`.
