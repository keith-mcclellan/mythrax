## 2026-07-11 - [SurrealDB SQL Injection]
**Vulnerability:** Found string interpolation (format!) used to create dynamic queries in surrealdb, such as `format!("SELECT scope FROM {};", rec_id.table)`.
**Learning:** In SurrealDB, string interpolation of table names can lead to SQL injection vulnerabilities. The query should use the `type::table()` function instead. `SELECT scope FROM type::table($table_name)`
**Prevention:** Use SurrealDB's type parsing or bind parameter for dynamic table selection.
