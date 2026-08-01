---
title: "Bug: Server Panic on Chunk Serialization in API Stream"
labels: ["bug", "agent-found"]
severity: "High"
---

## Bug Description
In `mythrax-core/src/api.rs`, the server streams LLM responses back to the client. On line 737, the code uses `.unwrap()` to serialize the response chunk:
`serde_json::to_string(&chunk).unwrap()`

If the chunk fails to serialize (e.g. due to an invalid character or deeply nested structure), `unwrap()` will panic, causing the entire single-port daemon to crash and disconnect all clients.

## File & Line Number
`mythrax-core/src/api.rs:737`

## Minimal Reproducible Scenario
1. Trigger a chunk response containing a multi-byte sequence or structure that `serde_json` fails to serialize.
2. The `to_string(&chunk)` returns `Err`.
3. The `.unwrap()` call panics, terminating the daemon process.

## Suggested Fix
Replace `.unwrap()` with a match block or `unwrap_or_else` that logs the error and gracefully skips the chunk, or returns an internal server error to the client instead of crashing the daemon.
