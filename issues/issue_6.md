---
title: Panic Risk via unwrap in llm_config configuration logic
labels: bug, agent-found
---

**Bug Type:** Panic Paths (unwrap/expect)
**File:** `mythrax-core/src/db/crud_operations.rs`
**Line:** 919-920
**Severity:** High
**Minimal Reproducible Scenario (Trigger):**
When saving or updating an LLM configuration, the code extracts `current_model` and `current_cloud_provider` which are `Option<String>`. At line 919-920, it uses `.unwrap()`:
```rust
let model = current_model.unwrap();
let cloud_provider = current_cloud_provider.unwrap();
```
If both the request and the existing configuration lack a model or cloud provider, and the fallback to default also fails or returns None, this will panic. While defaults are provided, complex configuration updates can sometimes bypass this, causing the daemon to crash.

**Suggested Fix:**
Ensure a default fallback instead of `unwrap()`:
```rust
let model = current_model.unwrap_or_else(|| "default_model".to_string());
let cloud_provider = current_cloud_provider.unwrap_or_else(|| "default_provider".to_string());
```
