---
labels: bug, agent-found
---
# Panic in update_llm_config_db on missing model/provider

**File & Line:** `mythrax-core/src/db/crud_operations.rs:919-920`

**Scenario:** `current_model` or `current_cloud_provider` is `None` (e.g., failed to fallback to defaults or parse existing config), causing `.unwrap()` panic.

**Severity:** Medium

**Suggested Fix:** Use `.unwrap_or_else(|| ...)` or return an error `Result` instead of `.unwrap()`.