
## 2024-05-24 - [CRITICAL] SQL Injection in Dynamic Table Queries
**Vulnerability:** A SQL injection vulnerability was found in `mythrax-core/src/mcp_routes.rs:2558` in `get_node_scope` where the table part of a parsed `RecordId` was interpolated directly into a SurrealDB query string using `format!("SELECT scope FROM {};", rec_id.table)`.
**Learning:** This codebase uses SurrealDB v3, and dynamically building table names with string interpolation is an anti-pattern as it bypasses parameterization and allows for SQL injection when dealing with dynamic `table` variables.
**Prevention:** Always parameterize dynamic table names in SurrealDB queries. Do not use string interpolation (e.g., `format!()`) to inject table names. Instead, use the specialized parameterization pattern: `type::table($param)` and pass the dynamic table name via `.bind(("param", table_name))`.
