---
labels: bug, agent-found
---
# Panic in save_episodes_batch_db on JSON object cast

**File & Line:** `mythrax-core/src/db/crud_operations.rs:437`

**Scenario:** `ep_json.as_object_mut()` returns `None` if `ep_json` is not a JSON object, triggering `.unwrap()` panic. This can happen if the JSON serialization of an episode unexpectedly results in a primitive or array.

**Severity:** High

**Suggested Fix:** Use `if let Some(ep_obj) = ep_json.as_object_mut() { ... } else { return Err(...) }`.