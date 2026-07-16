---
title: "Bug: Panic on Malformed Record IDs in Compactor Loop"
labels: ["bug", "agent-found"]
---

# Panic on Malformed Record IDs in Compactor Loop

**File:** `mythrax-core/src/cognitive/compactor.rs`
**Line:** 278, 279

## Description
In `mythrax-core/src/cognitive/compactor.rs`, the background episode compactor extracts the record IDs of two similar episodes (`older.id` and `newer.id`) and parses them using `crate::db::backend::parse_record_id`.
`parse_record_id` strictly requires the ID string to contain a colon (e.g., `table:id`), or it returns an `Err`.
On line 278, the code blindly calls `.unwrap()` on the result:
```rust
let older_rec = crate::db::backend::parse_record_id(older.id.as_ref().unwrap()).unwrap();
```
If any agent or API user injects an episode with a malformed string ID (e.g., just the uuid without the `episode:` prefix), this background loop will permanently panic and crash the daemon when attempting to merge them.

## Minimal Reproducible Scenario
1. An agent or client directly calls the API/tool to save an episode node but sets the ID to `"abc-123"` instead of `"episode:abc-123"`.
2. The agent creates a second similar episode with ID `"episode:def-456"`.
3. The background compactor process awakens and detects these two episodes share the same session, node type, and have high similarity.
4. The compactor reaches line 278 and invokes `parse_record_id("abc-123")`.
5. `parse_record_id` fails because there is no colon, returning an `Err`.
6. `.unwrap()` triggers a panic, tearing down the daemon.

## Severity
**High**. Agents have direct write access to context creation. A single malformed ID can cause a permanent, unrecoverable denial of service for the cognitive background task.

## Suggested Fix
Gracefully handle the `Result` by logging an error and skipping the merge iteration.
```rust
let older_rec = match crate::db::backend::parse_record_id(older.id.as_ref().unwrap()) {
    Ok(rec) => rec,
    Err(e) => {
        log::error!("Invalid older ID in compactor: {}", e);
        continue;
    }
};
```
