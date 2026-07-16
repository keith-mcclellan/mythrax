---
title: "Bug: Panic path in API SSE response streaming"
labels: ["bug", "agent-found"]
---

## Description
A blind unwrap during SSE chunk serialization could cause the API axum task to crash.

**File:** `mythrax-core/src/api.rs`
**Line:** 552

**Severity:** Medium (Task Crash)

**Minimal Reproducible Scenario:**
1. A streaming generation request is made (`stream: true`).
2. The AI model returns a response string containing extremely bizarre characters or a deeply nested JSON structure (if it was an object) that somehow causes `serde_json::to_string` to fail.
3. The server blindly calls `serde_json::to_string(&chunk).unwrap()`.
4. The unwrap panics, dropping the axum task and abruptly closing the connection without an HTTP error response.

**Suggested Fix:**
Handle the error gracefully and return a 500 error, or use `unwrap_or_else` to fallback to an empty string or error chunk.
