---
title: Panic Path: Unwrap on downcast_ref in inspect_failed_query.rs
labels: bug,agent-found
---

**File:** `mythrax-core/src/bin/inspect_failed_query.rs`
**Line:** 236

**Description:**
The code attempts to downcast the backend reference and unwraps the result on line 236:
```rust
let surreal_backend = backend.as_any().downcast_ref::<SurrealBackend>().unwrap();
```
If the backend is not `SurrealBackend`, this panics.

**Severity:** Medium

**Minimal Reproducible Scenario:**
Run the `inspect_failed_query` binary with a backend implementation that is not `SurrealBackend`.

**Suggested Fix:**
Use a match or `if let` and return an error:
```rust
let surreal_backend = backend.as_any().downcast_ref::<SurrealBackend>()
    .ok_or_else(|| anyhow::anyhow!("Backend is not SurrealBackend"))?;
```