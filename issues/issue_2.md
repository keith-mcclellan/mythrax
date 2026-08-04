---
title: "🛡️ Sentinel: [HIGH] Fix Panic in `crud_operations.rs` on missing model provider"
labels: bug, agent-found
---

🚨 Severity: HIGH
💡 Vulnerability: Potential panic due to `.unwrap()` on `current_model` and `current_cloud_provider` which are `Option` types.
🎯 Impact: If a record is missing model/provider defaults, the database update crashes.
🔧 Fix: Use `.unwrap_or_else` with fallback strings like "default".
✅ Verification: Insert a record missing these fields and ensure it processes without panicking.

File: `mythrax-core/src/db/crud_operations.rs:919`