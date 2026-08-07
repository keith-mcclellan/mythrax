---
title: Panic vulnerability via .unwrap() on rel.get("from_str") and .as_str() in crud_operations.rs
labels: bug, agent-found
---

**File:** `mythrax-core/src/db/crud_operations.rs`
**Line:** 517, 518

**Minimal Reproducible Scenario:**
In temporal expansion graph traversals or when processing relations, if a relation object (`rel`) is malformed and does not contain the `from_str` or `to_str` keys, or if the value is not a string, the `.unwrap()` calls will panic. This could occur if the underlying data in the database gets corrupted, or if an adversarial or malformed temporal relation is injected via an API endpoint.

**Severity:** High (can cause Denial of Service by crashing the application on processing malformed relations).

**Suggested Fix:**
Gracefully handle missing keys or invalid types instead of unwrapping.

```rust
let from_uuid = match rel.get("from_str").and_then(|v| v.as_str()) {
    Some(id) => id,
    None => {
        // Log warning or skip
        continue;
    }
};
let to_uuid = match rel.get("to_str").and_then(|v| v.as_str()) {
    Some(id) => id,
    None => {
        continue;
    }
};
```
