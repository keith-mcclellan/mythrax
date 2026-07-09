# 🛡️ Sentinel: [HIGH] Fix SQL Injection in Dynamic Table Selection

## Description

🚨 **Severity**: HIGH

💡 **Vulnerability**: String interpolation was used to construct table names in SurrealDB queries (`format!("SELECT scope FROM {};", rec_id.table)` in `mythrax-core/src/mcp_routes.rs`), which is a vector for SQL injection if the table name originates from user input or is improperly sanitized.

🎯 **Impact**: An attacker could potentially bypass the table name validation and inject arbitrary SQL commands. Even if `rec_id.table` normally comes from an internal, trusted source, defense-in-depth requires we parameterize inputs or avoid string interpolation for components in queries.

🔧 **Fix**: Modified `get_node_scope` to parameterize the query by passing the record ID directly as `$id`, replacing the string formatting. In SurrealDB, `SELECT ... FROM $id` resolves to the exact record and eliminates the need for dynamic table names.

✅ **Verification**:
- Inspected the change manually.
- Formatted `mythrax-core/src/mcp_routes.rs`.
- Ran unit tests via `MYTHRAX_TEST_MOCK=1 cargo test mcp_routes -- --test-threads=1`. Tests passed successfully.
