---
title: "Bug: Panic when matched record is missing ID in SurrealDB synthesis step"
labels: ["bug", "agent-found"]
---

## Description
In `mythrax-core/src/cognitive/synthesis.rs`, line 1259, there is an assumption that a database record returned by SurrealDB will always contain an ID:

```rust
let old_uuid = matched.id.as_ref().unwrap().strip_prefix("wisdom:").unwrap_or(matched.id.as_ref().unwrap());
```

## Reproducible Scenario
1. The memory synthesis component retrieves a `matched` object from SurrealDB.
2. Due to a malformed query, projection issues (e.g. `SELECT field1, field2` instead of `SELECT *`), or corrupted data state, the database engine returns the record without the `id` field.
3. `matched.id` is `None`.
4. `matched.id.as_ref().unwrap()` panics and brings down the synthesis daemon.

## Severity
**Medium**

## Suggested Fix
Gracefully handle cases where the `id` is not present, either by skipping the update or logging an error instead of panicking:

```rust
if let Some(id) = matched.id.as_ref() {
    let old_uuid = id.strip_prefix("wisdom:").unwrap_or(id);
    // ... proceed with update ...
} else {
    tracing::warn!("Matched record missing id, skipping superseded update.");
}
```
