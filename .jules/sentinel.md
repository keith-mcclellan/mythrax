## 2024-07-19 - SQL Injection in SurrealDB Queries via String Formatting
**Vulnerability:** A SQL injection vulnerability was found in `get_node_scope` where the record's table name was injected into a SurrealDB query via `format!("SELECT scope FROM {};", rec_id.table);`.
**Learning:** String interpolation for dynamically constructing table names or IDs allows arbitrary SQL execution. Although an ID was being bound to the query execution, it wasn't being used in the target string.
**Prevention:** Always use parameterized `$id` variables when selecting specific records: `SELECT scope FROM $id;` and bind the `surrealdb::types::RecordId` directly.
