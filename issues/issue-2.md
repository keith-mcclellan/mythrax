---
title: "Bug: Potential panic from `.unwrap()` on Result in search pipeline"
labels: ["bug", "agent-found"]
---

**File:** `mythrax-core/src/db/search_pipeline.rs`
**Line:** 1986 (`let mut keyword_candidates = parse_results(keyword_resp_res.unwrap(), false)?;`)

**Minimal Reproducible Scenario:**
If `keyword_resp_res` is an `Err` (e.g. due to a failed keyword search query or network issue), the `.unwrap()` call will panic, crashing the daemon.

**Severity:** High (Crash in search pipeline due to unhandled error).

**Suggested Fix:**
Replace `.unwrap()` with `?` or handle the `Result` gracefully: `let keyword_resp = keyword_resp_res?; let mut keyword_candidates = parse_results(keyword_resp, false)?;`
