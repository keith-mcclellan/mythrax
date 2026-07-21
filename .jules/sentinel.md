## 2026-07-21 - Fix SQL Injection in SurrealDB Query
**Vulnerability:** Found a SQL injection vulnerability in `get_node_scope` (`mythrax-core/src/mcp_routes/manage_handlers.rs`) where the table name was dynamically inserted into the query using string interpolation (`format!("SELECT scope FROM {};", rec_id.table)`).
**Learning:** This occurred because of a misunderstanding of how SurrealDB allows querying specific records. String interpolation on table names or IDs allows arbitrary SQL execution if the input is unsanitized.
**Prevention:** To select a specific record by ID, use `SELECT ... FROM $id` and bind the `surrealdb::types::RecordId` directly, bypassing the need to format the table string. When dynamic tables are needed, parameterize them using `type::table($param)`.
