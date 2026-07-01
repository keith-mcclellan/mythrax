
## 2024-05-18 - [SQL Injection] Unsafe Interpolation in SurrealDB Query
**Vulnerability:** A SQL injection vulnerability was found in `mythrax-core/src/mcp_routes.rs` where the `table` field of a `RecordId` was directly interpolated into a query string (`format!("SELECT scope FROM {};", rec_id.table)`) instead of using a parameterized approach.
**Learning:** Even though SurrealDB has robust parameterized querying for most values, dynamic table names can be dangerous if directly formatted. The rust driver allows binding table names by casting strings via `type::table($param)`.
**Prevention:** Always use parameterized queries for dynamic table references with `type::table($var)` rather than interpolating strings with `format!()` macro.
