---
labels: bug, agent-found
---
# Panic in save_episodes_batch_db on relation parsing

**File & Line:** `mythrax-core/src/db/crud_operations.rs:517-518`

**Scenario:** JSON objects in `relations` lack 'from_str' or 'to_str' keys, or the values are not strings, causing `.unwrap()` panic.

**Severity:** High

**Suggested Fix:** Use defensive parsing with `and_then` or `match` and skip/log malformed entries.