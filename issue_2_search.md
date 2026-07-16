---
title: "Bug: Panic path in Hybrid Search Pipeline due to blind unwrap"
labels: ["bug", "agent-found"]
---

## Description
A `None` value unwrapped blindly causes a panic in the search pipeline if keyword search fails but hybrid search was requested.

**File:** `mythrax-core/src/db/search_pipeline.rs`
**Line:** 1512, 1741

**Severity:** Medium/High

**Minimal Reproducible Scenario:**
1. Configure `search.fts_cap` to a valid number.
2. Ensure the system enters the `is_hybrid` branch (e.g., query embedding is successful).
3. The keyword search (`keyword_resp_res`) fails (e.g., due to a temporary DB timeout or FTS index build error), returning `None`.
4. `parse_results(keyword_resp_res.unwrap(), false)?` is evaluated.
5. The unwrap panics, causing the entire search request to fail ungracefully rather than falling back to pure vector search or returning a graceful API error.
6. Similarly, at line 1741, if `session_id` is somehow `None`, `c.session_id.as_ref().unwrap()` panics.

**Suggested Fix:**
Use `ok_or_else` to map `keyword_resp_res` to a proper `Result` and propagate it using `?`, or fall back to an empty result set if graceful degradation is preferred. For `session_id`, use `if let Some(sess) = c.session_id.as_ref()`.
