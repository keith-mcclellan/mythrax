---
labels: bug, agent-found
---
# Bug: Empty result due to IN $bindings without RecordId Cast
**File:** mythrax-core/src/db/crud_operations.rs:2026, mythrax-core/src/db/search_pipeline.rs:2321
**Scenario:** Passing a native Rust `Vec<String>` to a SurrealDB `IN $ids` clause (e.g. `WHERE id IN $ep_ids;`) silently fails to match because SurrealDB evaluates them as strings instead of native record types.
**Severity:** High (Silent failure/false positives)
**Fix:** Map the string IDs to `surrealdb::types::RecordId` instances (e.g. `Vec<surrealdb::types::RecordId>`) before binding them to the query parameters.