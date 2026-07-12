# 🛡️ Sentinel: [CRITICAL] Fix SQL injection in `get_node_scope`

🚨 **Severity:** CRITICAL

💡 **Vulnerability:**
A SQL injection vulnerability existed in `mythrax-core/src/mcp_routes.rs` within the `get_node_scope` function. The code used string interpolation (`format!("SELECT scope FROM {};", rec_id.table)`) to construct a SurrealDB query using a table name parsed from a user-supplied ID string.

🎯 **Impact:**
An attacker could craft a malicious ID string, causing arbitrary SQL to be executed against the SurrealDB instance. Additionally, this behavior caused the query to fetch all scopes for the given table rather than restricting it to a specific record.

🔧 **Fix:**
Refactored the query to eliminate string interpolation. Changed the query to `let sql = "SELECT scope FROM $id;";` and used `.bind(("id", rec_id))` to safely bind the `RecordId`. This uses parameterization securely and fixes the logic bug by ensuring the query filters by the exact record rather than selecting all scopes for the entire table.

✅ **Verification:**
1. Code compiles and lint checks pass via `MYTHRAX_TEST_MOCK=1 cargo check`.
2. Tests pass successfully via `MYTHRAX_TEST_MOCK=1 cargo test --lib --bins`.
3. Tested to ensure no functional regressions for standard scopes.
