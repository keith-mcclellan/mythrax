## 2024-05-24 - SQL Injection in Dynamic Table Selection
**Vulnerability:** String interpolation used to construct table names in SurrealDB queries (`format!("SELECT scope FROM {};", rec_id.table)`).
**Learning:** `RecordId.table` can potentially contain unescaped characters depending on how it's parsed, making string interpolation a vector for SQL injection.
**Prevention:** Use `$id` to bind the full `RecordId` directly, or use `type::table($table_name)` for parameterized dynamic table selection in SurrealDB.
