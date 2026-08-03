# 🛡️ Sentinel: [HIGH] Fix panic in API gateway JSON serialization

**Labels:** `bug`, `agent-found`

🚨 Severity: HIGH
💡 Vulnerability: Use of `.unwrap()` on `serde_json::to_string` and `serde_json::to_vec` in API routes (`mythrax-core/src/api.rs`).
🎯 Impact: A serialization failure will panic the entire daemon process due to the unwrap, causing a denial of service (DoS) for all active sessions.
🔧 Fix: Replace `.unwrap()` with proper error handling, logging the error and returning a `500 Internal Server Error` response.
✅ Verification: The API gateway gracefully handles invalid payloads or serialization failures without crashing the process.

**Minimal Reproducible Scenario:**
Send a malformed payload or an extremely deeply nested JSON object that fails serialization on the SSE stream in the chat completions route.

**File and Line Number:**
`mythrax-core/src/api.rs` lines 793, 960, 1016, 1034, 1056, 1115, 1152

**Estimated Effort:** Medium
