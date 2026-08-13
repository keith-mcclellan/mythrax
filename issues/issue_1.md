---
labels: bug, agent-found
---
## High Severity Issue in `mythrax-core/src/db/crud_operations.rs`

**Line Number:** 437
**Scenario:** When `ep_json` is not an object (e.g. array or null), calling `.as_object_mut().unwrap()` panics.
**Suggested Fix:** Use `let Some(ep_obj) = ep_json.as_object_mut() else { continue; };`
