---
title: "Bug: Panic in `compactor.rs` due to unhandled missing ID or embedding"
labels: ["bug", "agent-found"]
severity: "High"
---

## Description
The background episode compactor thread contains multiple unsafe `.unwrap()` calls when processing episodes. If an episode was saved without an `id` or without an `embedding`, the thread will panic, silently crashing the compactor system.

## File and Line Numbers
- `mythrax-core/src/cognitive/compactor.rs:159`: `let id_i = active_eps[i].id.as_ref().cloned().unwrap();`
- `mythrax-core/src/cognitive/compactor.rs:176`: `active_eps[i].embedding.as_ref().unwrap()`

## Minimal Reproducible Scenario
1. Initialize the SurrealDB backend.
2. Manually insert or mock an `Episode` record into the active set with `embedding: None` (or `id: None`).
3. Trigger `compact_global()` or `compact_scope()`.
4. The background process iterates over `active_eps` and calls `.unwrap()` on the missing option, causing a thread panic.

## Blast Radius
The compactor thread dies, leaving memory unbounded over time, leading to memory exhaustion or degraded LLM performance due to context bloat.

## Suggested Fix
Replace `.unwrap()` with proper `match` or `if let` blocks. If an episode lacks an ID or embedding, it should be skipped in the similarity comparison loop, or explicitly logged as a malformed record and ignored.
```rust
let id_i = match active_eps[i].id.as_ref() {
    Some(id) => id.clone(),
    None => continue,
};
```