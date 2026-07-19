Title: 🛡️ Sentinel: [CRITICAL] Fix SQL injection in SurrealDB query

🚨 Severity: CRITICAL
💡 Vulnerability: SQL injection vulnerability in `get_node_scope` due to using string formatting (`format!`) for the table name instead of a parameterized query for the record ID.
🎯 Impact: An attacker could potentially supply a malicious string that breaks out of the table name context, leading to arbitrary SQL execution against the SurrealDB backend.
🔧 Fix: Replaced `format!("SELECT scope FROM {};", rec_id.table)` with a parameterized query `SELECT scope FROM $id;` and properly bound the record ID.
✅ Verification: Run `cargo test` and manually inspect `mythrax-core/src/mcp_routes/manage_handlers.rs` to confirm no string formatting is used in the query construction.
