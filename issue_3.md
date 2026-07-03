---
title: "Bug: Panic on missing UUID during wisdom rule DB updates"
labels: ["bug", "agent-found"]
---

### Vulnerability / Bug Description
In the synthesis stage, when the system attempts to supersede an old wisdom rule with a new consolidated rule, it uses `matched.id.as_ref().unwrap()`. `matched.id` is an `Option<String>`, meaning the database query could have returned a record missing an `id` field, causing an immediate panic.

### File and Line Number
- `mythrax-core/src/cognitive/synthesis.rs`, line 1259.

### Minimal Reproducible Scenario
1. Dream consolidation successfully merges rules and calls `save_wisdom_rule`.
2. The initial fetched rule (`matched`) has its `id` stripped or failed to deserialize correctly into the struct (e.g., memory corruption, missing ID field during migration).
3. The process encounters `.unwrap()` and panics the background task.

### Severity
**Low-Medium**. A missing `id` on a fetched database record suggests deeper data corruption, but a crash loop during dreaming compaction (which could prevent the database from stabilizing) makes it an important stability fix.

### Suggested Fix
The code has been updated to use safe `if let Some(matched_id) = &matched.id { ... }` blocks rather than panicking.
