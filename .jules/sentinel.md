## 2024-07-04 - [Fix format SQL Injection with SurrealDB type::table()]
**Vulnerability:** A potential SQL injection vulnerability in `get_node_scope` where the table name was interpolated via `format!` into the `FROM` clause (`let sql = format!("SELECT scope FROM {};", rec_id.table);`).
**Learning:** Using `format!` to construct SQL queries in SurrealDB is dangerous when incorporating data from parsed inputs like record IDs.
**Prevention:** Use `type::table($param)` inside the query and `.bind(("param", table_string))` to safely parameterize dynamic table names in SurrealDB.
