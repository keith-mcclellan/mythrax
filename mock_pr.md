# 🛡️ Sentinel: [CRITICAL] Fix SQL Injection and Overfetching in Node Scope Query

## Details
* **🚨 Severity:** CRITICAL
* **💡 Vulnerability:** The `get_node_scope` function in `mcp_routes.rs` used `format!("SELECT scope FROM {};", rec_id.table)` to construct a SurrealQL query. This created an SQL injection vector where a maliciously crafted record ID could execute arbitrary queries. Furthermore, it omitted the actual ID filter, resulting in querying an entire table rather than the specific intended record.
* **🎯 Impact:** An attacker could exploit this vulnerability to read unauthorized data from other tables or execute arbitrary queries within the context of the database. Even without malicious intent, it created a severe insecure direct object reference (IDOR) and data overfetching bug.
* **🔧 Fix:** Replaced string interpolation with a parameterized query fetching directly from the specific record: `SELECT scope FROM $id;`. Added code comments explaining the security enhancement.
* **✅ Verification:** Run `cd mythrax-core && MYTHRAX_TEST_MOCK=1 cargo check` and `MYTHRAX_TEST_MOCK=1 cargo test --lib --bins mcp_routes` to verify compilation and that no regressions were introduced.
