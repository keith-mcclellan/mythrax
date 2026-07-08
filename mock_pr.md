# 🛡️ Sentinel: [CRITICAL] Fix SQL Injection in SurrealDB Query

## Description
🚨 **Severity:** CRITICAL
💡 **Vulnerability:** A SQL injection vulnerability was found in the `get_node_scope` function (`mythrax-core/src/mcp_routes.rs`). The query dynamically interpolated a table name derived from user-provided record IDs using `format!("SELECT scope FROM {};", rec_id.table);`. This allows a malicious actor to manipulate the query execution path. Additionally, the original query failed to actually scope the result to the specific record ID, returning a seemingly random entry from the target table.
🎯 **Impact:** If exploited, this could allow an attacker to bypass intended database isolation scopes, read unauthorized tables, and execute arbitrary read commands against SurrealDB.
🔧 **Fix:** Refactored the query to avoid string interpolation entirely. Utilized SurrealQL's record ID resolution capabilities by issuing `SELECT scope FROM $id LIMIT 1;` and securely binding the parsed `$id` record directly via `backend.db.query(sql).bind(("id", rec_id))`.
✅ **Verification:**
- Validated via `cargo check` and `MYTHRAX_TEST_MOCK=1 cargo test --lib --bins`.
- SQL injection is no longer possible as no string interpolation is done against user parameters.
