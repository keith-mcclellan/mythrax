## 2025-02-14 - Prevent SQL Injection via SurrealDB Parameterization
**Vulnerability:** A SQL injection vulnerability was found in `get_node_scope` where the SurrealDB table name from user input was directly inserted into a SQL query string via `format!("SELECT scope FROM {};", rec_id.table);`.
**Learning:** Using `format!` or other string interpolation for SurrealDB queries with user-controlled input, including table names derived from `RecordId`, is unsafe and risks SQL injection or logic bugs querying the wrong target.
**Prevention:** Always use parameterized queries for all inputs, including IDs. In SurrealDB with Rust, use `backend.db.query("SELECT ... FROM $id").bind(("id", rec_id))` to safely bind the `surrealdb::types::RecordId` directly to the target `$id` parameter.
