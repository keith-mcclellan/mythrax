## 2025-03-09 - SurrealDB SQL Injection via String Interpolation
**Vulnerability:** SQL injection vulnerability in `get_node_scope` where table names in SurrealDB queries were being interpolated using `format!()` with user-supplied table data from parsed Record IDs.
**Learning:** Even table names derived indirectly from database records or API endpoints must be parameterized when used in SurrealDB SQL strings. The Rust `format!()` macro is unsafe for query construction.
**Prevention:** Use `type::table($param)` and pass the parameter via `.bind(("param", param_value))` when dynamically specifying table names in SurrealDB instead of relying on string formatting.
