
## 2024-07-08 - [SQL Injection via Missing SurrealDB Parameterization]
**Vulnerability:** SQL Injection in `get_node_scope` (`mcp_routes.rs`) via table name. The user-provided ID was parsed and interpolated into the query using `format!("SELECT scope FROM {};", rec_id.table);`.
**Learning:** In SurrealDB Rust clients, `format!()` or string interpolation should never be used to construct queries with dynamic input. A table name derived from user input without validation is a dangerous injection vector.
**Prevention:** Always parameterize variables. For dynamic table names or record IDs, use `type::table($param)` and explicitly bind variables via `.bind(("param", value))`, or use `SELECT ... FROM $id` and bind the `surrealdb::types::RecordId` directly.
