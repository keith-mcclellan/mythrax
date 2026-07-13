## 2024-07-13 - [Fix SQL injection in record lookup]
**Vulnerability:** Found string-interpolated query construction using `rec_id.table` rather than parameterized ID selection in `get_node_scope` (`format!("SELECT scope FROM {};", rec_id.table)`).
**Learning:** In SurrealDB, using string interpolation for table names and querying the whole table instead of targeting the specific record ID introduces SQL injection risks and causes logical errors.
**Prevention:** Always use parameterized queries and target records using `$id` (e.g. `SELECT ... FROM $id`) when fetching a specific record. Never use `format!()` to inject parts of the table or ID dynamically.
