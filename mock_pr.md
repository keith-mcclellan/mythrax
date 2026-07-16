# 🛡️ Sentinel: [CRITICAL] Fix SQL injection in record fetching

## 🚨 Severity
CRITICAL

## 💡 Vulnerability
In `mythrax-core/src/mcp_routes/manage_handlers.rs`, the `get_node_scope` function was using string formatting to construct a SQL query: `let sql = format!("SELECT scope FROM {};", rec_id.table);`. This directly interpolated the table name part of the parsed record ID into the query string, which is a classic SQL injection vector. The intended parameter binding `bind(("id", rec_id))` was present but completely useless because `$id` was never used in the query.

## 🎯 Impact
A malicious user could craft a specially formatted record ID (e.g. `table_name_with_injection:id_value`) to execute arbitrary SQL commands against the database, potentially reading or destroying sensitive data or dropping tables.

## 🔧 Fix
Switched the query to use proper parameterization for the record ID: `let sql = "SELECT scope FROM $id;";`. SurrealDB supports binding the entire `RecordId` directly to the parameterized query, resolving both the interpolation vulnerability and the unused binding.

## ✅ Verification
1. Run `cargo check` to ensure the project compiles.
2. Run `MYTHRAX_TEST_MOCK=1 cargo test --lib mcp_routes::manage_handlers -- --test-threads=1` to ensure tests pass and the code functions properly.
