---
labels: bug, agent-found
---
## High Severity Issue in `mythrax-core/src/db/crud_operations.rs`

**Line Number:** 517
**Scenario:** When `relations` contains JSON without `from_str` or `to_str` keys, or they aren't strings, the unwraps panic.
**Suggested Fix:** Use `let from_uuid = match rel.get("from_str").and_then(|v| v.as_str()) { Some(s) => s, None => continue, };`
