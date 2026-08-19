---
title: "Bug: Potential panic from `.unwrap()` on Option in search pipeline"
labels: ["bug", "agent-found"]
---

**File:** `mythrax-core/src/db/search_pipeline.rs`
**Line:** 2244 (`let sess = c.session_id.as_ref().unwrap();`)

**Minimal Reproducible Scenario:**
If a context object `c` has a `None` value for `session_id` during the search pipeline processing, calling `.unwrap()` on it will panic.

**Severity:** High (Crash in search pipeline due to unhandled None).

**Suggested Fix:**
Check if `session_id` is `Some` before unwrapping, or return an error/default value if it is `None`. For example: `let sess = c.session_id.as_ref().ok_or_else(|| anyhow!("Missing session ID"))?;`
