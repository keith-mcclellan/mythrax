---
title: Minor Unwraps in regex match handling
labels: bug, agent-found
---

**Bug Type:** Panic Paths (unwrap/expect)
**File:** `mythrax-core/src/db/crud_operations.rs`
**Line:** 3081, 3083
**Severity:** Low
**Minimal Reproducible Scenario (Trigger):**
When parsing stderr/stdout to match error signatures, the code uses `caps.get(1).unwrap()`. Since the regex clearly has one capture group `(E\d{4})` and `(TS\d{4})`, `caps.get(1)` will theoretically always be present when there is a match. However, using `unwrap()` is still technically a panic path if the regex is modified later without updating the capture group extraction.

**Suggested Fix:**
Use `if let Some(m) = caps.get(1) { matched_signature = Some(m.as_str().to_string()); }`.
