# 🛡️ Sentinel: [CRITICAL] Fix SQL injection in user query

🚨 Severity: CRITICAL
💡 Vulnerability: SQL injection vulnerability in `get_node_scope` due to unvalidated input in `parse_record_id` being directly formatted into a query string (`let sql = format!("SELECT scope FROM {};", rec_id.table);`).
🎯 Impact: Attackers could inject arbitrary table names or SQL commands by manipulating the ID input (e.g., using a maliciously crafted ID like `users; DROP TABLE users;:123`), leading to unauthorized data access, modification, or deletion within SurrealDB.
🔧 Fix: Replaced string interpolation with a parameterized query directly selecting from the record ID (`let sql = "SELECT scope FROM $id;";` and binding `$id` to `rec_id`).
✅ Verification: Ensure the unit tests for data access run correctly and try fetching scopes with normal Record IDs to verify they still behave as expected without failing.
