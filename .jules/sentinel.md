## 2025-02-12 - Fix NoSQL Injection in Dynamic Table Query
**Vulnerability:** Found a NoSQL injection vulnerability in `get_node_scope` where the query string was dynamically constructed using string interpolation `format!("SELECT scope FROM {};", rec_id.table)`.
**Learning:** Even when reading metadata fields like `table` from a parsed `RecordId`, direct string concatenation in queries risks NoSQL injection if those IDs ever originate from unsanitized input.
**Prevention:** Always use parameterized queries or type-safe database API functions. In SurrealDB, use `type::table($table)` combined with `.bind(("table", value))` rather than string interpolation when querying dynamic table names.
