---
title: "Unsafe Unwrap on Database Record Optional ID"
labels: ["bug", "agent-found"]
---

## Description
When updating merged wisdom rules, the compactor accesses `matched.id` assuming that any fetched database rule intrinsically contains an ID string. It then eagerly calls `.unwrap()`.

## Location
`mythrax-core/src/cognitive/synthesis.rs`, line 1259

## Minimal Reproducible Scenario
If a user upgrades their SurrealDB schema without a correct migration, or a third-party process inserts a rule incorrectly such that the `id` property isn't explicitly string-mapped, `matched.id` (which is an `Option<String>`) evaluates to `None`.
The compactor logic executes:
```rust
let old_uuid = matched.id.as_ref().unwrap().strip_prefix("wisdom:").unwrap_or(matched.id.as_ref().unwrap());
```
This forces a `unwrap()` on `None`, abruptly terminating the process and corrupting the state of active memory operations via an unhandled panic.

## Severity
Medium (State corruption risk from hard crash in an asynchronous context).

## Suggested Fix
Pattern match the ID safely, continuing or logging if it happens to be missing:

```rust
if let Some(matched_id) = &matched.id {
    let old_uuid = matched_id.strip_prefix("wisdom:").unwrap_or(matched_id);
    let new_uuid = saved_id.strip_prefix("wisdom:").unwrap_or(&saved_id);
    // Proceed with SurrealDB record update
} else {
    log::error!("Cannot supersede wisdom record because matched.id is missing.");
}
```
