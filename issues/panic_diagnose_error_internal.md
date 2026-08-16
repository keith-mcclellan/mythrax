---
labels: bug, agent-found
---
# Panic in diagnose_error_internal_db on regex capture

**File & Line:** `mythrax-core/src/db/crud_operations.rs:3081, 3083`

**Scenario:** `caps.get(1)` returns `None` if the regex matches but somehow lacks capture group 1, causing `.unwrap()` panic.

**Severity:** Medium

**Suggested Fix:** Use `if let Some(cap) = caps.get(1) { ... }`.