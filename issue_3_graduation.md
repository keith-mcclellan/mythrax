---
title: "Bug: Panic path in Wisdom Rule Graduation Pipeline when ID is missing"
labels: ["bug", "agent-found"]
---

## Description
During the background graduation pipeline for memory rules, a missing ID field will cause an immediate panic.

**File:** `mythrax-core/src/db/graduation_pipeline.rs`
**Line:** 83

**Severity:** Medium (Background Task Crash)

**Minimal Reproducible Scenario:**
1. An unexpected state or direct DB manipulation leaves a Wisdom record with a missing or malformed `id` field.
2. The graduation pipeline runs `SELECT * FROM wisdom...`.
3. The loop evaluates: `let id_raw = rule.id.as_ref().unwrap().split(':').nth(1)...`.
4. The `unwrap()` on `rule.id.as_ref()` panics because `id` is `None`.
5. The background loop crashes.

**Suggested Fix:**
Safely unwrap using `match` or `if let`. Since an ID is required to update the record in the database, `continue` to the next rule if `id` is `None`:
`let id_raw = match rule.id.as_ref() { Some(id) => id.split(':').nth(1).unwrap_or(id).to_string(), None => continue, };`
