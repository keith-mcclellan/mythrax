## 2024-05-24 - [SQL Injection Fix]
**Vulnerability:** SQL Injection in get_node_scope in mythrax-core/src/mcp_routes.rs
**Learning:** String interpolation was used to dynamically insert a table name derived from a user-provided string directly into a SurrealQL query `format!("SELECT scope FROM {};", rec_id.table)`. This bypasses query parametrization protections and exposes the application to SQL injection attacks if `parse_record_id` does not strictly validate the input format. Moreover, it led to a logical bug where the query operated on the entire table rather than the specific record.
**Prevention:** Always use parameterized queries for dynamic values, including IDs. In SurrealDB, a record ID can be bound directly to a parameter (e.g., `SELECT ... FROM $id`).
