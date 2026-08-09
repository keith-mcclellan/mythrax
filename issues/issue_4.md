---
title: Panic Risk via unwrap in Temporal Relation Parsing
labels: bug, agent-found
---

**Bug Type:** Panic Paths (unwrap/expect)
**File:** `mythrax-core/src/db/crud_operations.rs`
**Line:** 517-518
**Severity:** High
**Minimal Reproducible Scenario (Trigger):**
When parsing JSON temporal relations in the batch insert loop for episodes, the code uses `rel.get("from_str").unwrap().as_str().unwrap()`. If the LLM output or external tool input provides a malformed temporal relation object without `from_str` or `to_str` keys, or if the value is not a string, this will panic and crash the entire RAG retrieval pipeline or API daemon.

**Suggested Fix:**
Use defensive parsing instead of `.unwrap()`. For example, use `.and_then()` or `if let` blocks:
```rust
if let Some(from_uuid) = rel.get("from_str").and_then(|v| v.as_str()) {
    if let Some(to_uuid) = rel.get("to_str").and_then(|v| v.as_str()) {
        // relate logic...
    }
}
```
