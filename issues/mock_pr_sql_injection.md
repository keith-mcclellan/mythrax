# 🛡️ Sentinel: [CRITICAL] Fix SQL injection in get_node_scope

## Description
🚨 **Severity:** CRITICAL
💡 **Vulnerability:** A SQL injection vulnerability was found in the `get_node_scope` function. The table name was directly interpolated into the query string, which could allow malicious actors to inject arbitrary SQL commands.
🎯 **Impact:** Exploiting this vulnerability could allow unauthorized access to or modification of the database, leading to potential data breaches or corruption.
🔧 **Fix:** Replaced string interpolation with parameterized bindings, executing the query on the specific record ID (`SELECT scope FROM $id;`) safely bound to the database.
✅ **Verification:** Verified that tests pass via `MYTHRAX_TEST_MOCK=1 cargo test --lib mcp_routes` and `cargo check --lib`. No external interface behavior changed.
