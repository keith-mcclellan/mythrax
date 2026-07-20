# 🛡️ Sentinel: [CRITICAL] Fix SQL injection in get_node_scope

🚨 **Severity:** CRITICAL
💡 **Vulnerability:** The `get_node_scope` function in `mythrax-core/src/mcp_routes/manage_handlers.rs` was directly interpolating a parsed table name (`rec_id.table`) into a SQL query using `format!("SELECT scope FROM {};", ...)`, which is a SQL injection vector. Additionally, the query was improperly selecting from the whole table instead of a specific record.
🎯 **Impact:** An attacker could potentially inject malicious SQL into the record ID which could result in arbitrary query execution or database exploitation.
🔧 **Fix:** Refactored the query to `let sql = "SELECT scope FROM $id;";` and bound the `RecordId` directly as a parameter. This uses SurrealDB's safe parameter binding and resolves the injection risk.
✅ **Verification:** Run `MYTHRAX_TEST_MOCK=1 cargo check` in the `mythrax-core` folder to ensure compilation succeeds, and run tests via `MYTHRAX_TEST_MOCK=1 cargo test mcp_routes -- --test-threads=1`.
