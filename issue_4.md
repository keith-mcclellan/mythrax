---
title: "Bug: Missing Test Coverage for Critical Math and Sync Utilities"
labels: ["bug", "agent-found"]
---

# Missing Test Coverage for Critical Utility Functions

**File:** Multiple (`mythrax-core/src/db/backend.rs`, `mythrax-core/src/vault/watcher.rs`, etc.)

## Description
During a routine scan of public functions with no accompanying unit tests (`test_<func_name>` or any usage within a `mod tests`), several critical functions were identified that represent test coverage gaps. If these functions are modified or regress, the failure will occur at runtime rather than during CI.

Functions missing test coverage include:
- `parse_record_id` and `format_record_id` in `mythrax-core/src/db/backend.rs`. These are heavily used throughout the database layer and in the cognitive compactor for resolving SurrealDB record IDs.
- `ignore_hash` and `is_hash_ignored` in `mythrax-core/src/vault/watcher.rs`. These functions are responsible for maintaining the bi-directional sync loop prevention cache.
- `chunk_transcript` and `extract_decisions` in `mythrax-core/src/vault/distillation.rs`. These are core components for creating distilled wiki nodes from raw tool invocations.

## Minimal Reproducible Scenario
1. Modify `parse_record_id` or `format_record_id` to handle prefixes incorrectly (e.g., assuming `table:id` without checking length).
2. Run `cargo test`.
3. The tests will pass because these utility functions are not explicitly exercised by dedicated unit tests.
4. The system will fail in production when syncing or compacting nodes.

## Severity
**Medium**. This represents technical debt that increases the likelihood of shipping regressions in core utilities.

## Suggested Fix
1. Write specific unit tests targeting `parse_record_id`, `format_record_id`, and `record_key_to_string` in `mythrax-core/src/db/backend.rs` to validate edge cases (missing colons, multiple colons, etc.).
2. Add tests in `mythrax-core/src/vault/watcher.rs` for `ignore_hash` and `is_hash_ignored` to ensure hashes are properly stored and evicted.
3. Write parser unit tests for `chunk_transcript` using a mock dialogue history.
