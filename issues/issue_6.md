---
labels: bug, agent-found
---
## Medium Severity Issue in `mythrax-core/src/db/search_pipeline.rs`

**Line Number:** 2244
**Scenario:** If `c.session_id` is `None`, `.unwrap()` will panic.
**Suggested Fix:** Use `let Some(sess) = c.session_id.as_ref() else { return false; };`
