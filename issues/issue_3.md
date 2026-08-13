---
labels: bug, agent-found
---
## Medium Severity Issue in `mythrax-core/src/db/crud_operations.rs`

**Line Number:** 919
**Scenario:** If `current_model` or `current_cloud_provider` is not set by `existing`, this unwraps a `None`, causing a panic.
**Suggested Fix:** Use `.unwrap_or_else(|| "default_model".to_string())`
