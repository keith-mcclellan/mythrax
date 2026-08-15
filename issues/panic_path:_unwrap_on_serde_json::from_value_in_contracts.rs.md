---
title: Panic Path: Unwrap on serde_json::from_value in contracts.rs
labels: bug,agent-found
---

**File:** `mythrax-core/src/contracts.rs`
**Line:** 1217

**Description:**
```rust
let parsed_facts: Vec<Fact> = serde_json::from_value(deserialized.causal_insight.unwrap()).unwrap();
```
This code expects `causal_insight` to be `Some` and expects it to perfectly deserialize into `Vec<Fact>`. If the data in the database (or from external input) is missing or malformed, this will panic the thread.

**Severity:** High

**Minimal Reproducible Scenario:**
Process an `Episode` where `causal_insight` is `None` or contains JSON that does not match `Vec<Fact>`.

**Suggested Fix:**
Use defensive parsing:
```rust
let parsed_facts: Vec<Fact> = match deserialized.causal_insight {
    Some(val) => serde_json::from_value(val).unwrap_or_default(),
    None => Vec::new(),
};
```