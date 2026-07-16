---
title: "Bug: Panics in DB CRUD Operations parsing JSON relations and settings"
labels: ["bug", "agent-found"]
---

## Description
The CRUD persistence layer contains panics triggered by unexpected JSON shapes.

**File:** `mythrax-core/src/db/crud_operations.rs`
**Lines:** 439, 759

**Severity:** High (Crash / Data Loss on Batch Save)

**Minimal Reproducible Scenario:**
1. For `save_episodes_batch` (Line 439): The client or agent sends a relation object where `from_str` or `to_str` is not a string, or the keys are missing entirely.
2. `rel.get("from_str").unwrap().as_str().unwrap();` triggers a panic.
3. For `update_profile` (Line 759): When configuring an LLM with partial settings, if `current_model` somehow resolves to `None`, `let model = current_model.unwrap();` panics, preventing profile updates.

**Suggested Fix:**
Replace `unwrap` with safe JSON extraction and fallbacks:
Line 439: `let from_uuid = rel.get("from_str").and_then(|v| v.as_str());`
Line 759: `let model = current_model.unwrap_or_else(|| "default_model".to_string());`
