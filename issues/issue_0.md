---
title: "🛡️ Sentinel: [CRITICAL] Fix Panic in `crud_operations.rs` on relation parsing"
labels: bug, agent-found
---

🚨 Severity: CRITICAL
💡 Vulnerability: Potential panic due to `.unwrap()` on `rel.get("from_str")` and `.as_str()`.
🎯 Impact: If a user provides malformed `relations` in a request (e.g., missing `from_str` or `to_str` keys), it crashes the entire daemon process.
🔧 Fix: Use `.and_then()` and return an error if missing.
✅ Verification: Ensure the function returns an error if `from_str` or `to_str` are missing.

File: `mythrax-core/src/db/crud_operations.rs:517`