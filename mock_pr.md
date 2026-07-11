# 🛡️ Sentinel: [CRITICAL] Fix SQL Injection in SurrealDB Query

## Description
🚨 **Severity:** CRITICAL
💡 **Vulnerability:** Found string interpolation (`format!`) used to create a dynamic table query in SurrealDB (`format!("SELECT scope FROM {};", rec_id.table)`) within `mcp_routes.rs:get_node_scope`.
🎯 **Impact:** Using string interpolation for dynamic table names can lead to SQL injection vulnerabilities, allowing an attacker to manipulate the table being queried or potentially execute arbitrary queries if the record ID input is maliciously crafted.
🔧 **Fix:** Modified the query to use `$id` and bound the `surrealdb::types::RecordId` directly (`let sql = "SELECT VALUE scope FROM $id;";`), which ensures parameterization and prevents SQL injection.
✅ **Verification:** Verified compilation using `cargo check` and confirmed the fix properly queries the scope using SurrealDB parameterization.

## Changes
- Modified `get_node_scope` in `mythrax-core/src/mcp_routes.rs` to securely bind `$id` instead of interpolating `rec_id.table`.
