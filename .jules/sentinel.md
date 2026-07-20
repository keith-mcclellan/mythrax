## 2024-07-20 - SQL Injection in SurrealDB querying

**Vulnerability:** Found a SQL Injection vulnerability in `mythrax-core/src/mcp_routes/manage_handlers.rs` where the `get_node_scope` function was formatting a user-controlled table name into the SQL query via `format!("SELECT scope FROM {};", rec_id.table)`. Also the query was incorrect as it fetched from the entire table.

**Learning:** It's important to use correct record bindings in SurrealDB Rust client when querying for specific records, rather than injecting the table name.

**Prevention:** Always use `SELECT ... FROM $id` and bind the `surrealdb::types::RecordId` directly to the `id` parameter. This prevents SQL injection and correctly selects the record.
