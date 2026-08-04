---
title: "🛡️ Sentinel: [CRITICAL] Fix Panic in `search_pipeline.rs` on failed keyword query"
labels: bug, agent-found
---

🚨 Severity: CRITICAL
💡 Vulnerability: Potential panic due to `.unwrap()` on `keyword_resp_res` which is a `Result`.
🎯 Impact: If the database query for keyword search fails (e.g. database disconnect, lock timeout), `keyword_resp_res` evaluates to `Err`, and `.unwrap()` panics the thread processing the search.
🔧 Fix: Use the `?` operator or `.unwrap_or_default()` to handle errors safely.
✅ Verification: The API should return a 500 or fallback gracefully instead of panicking.

File: `mythrax-core/src/db/search_pipeline.rs:1986`