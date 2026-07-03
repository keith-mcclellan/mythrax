---
title: "Bug: Panic on mismatched response type during token economics mutation"
labels: ["bug", "agent-found"]
---

### Vulnerability / Bug Description
In the MCP tool call handling endpoint (`mythrax-core/src/mcp_routes.rs`), the response payload `response_obj` is expected to be a JSON object so that token economics can be inserted. It invokes `response_obj.as_object_mut().unwrap().insert(...)`. If the tool responds with a JSON array or a primitive value, it panics.

### File and Line Number
- `mythrax-core/src/mcp_routes.rs`, line 2179.

### Minimal Reproducible Scenario
1. Implement or register an MCP tool that returns a JSON array or a raw boolean/string `serde_json::Value`.
2. Ensure the discovery stats trigger token tracking (i.e. `total_discovery > 0`).
3. The server crashes attempting to unwrap a non-object payload as a mutable object.

### Severity
**Medium**. Depends on third-party integrations (e.g. what an MCP server provides). Panicking the main API worker threads is a high impact risk.

### Suggested Fix
The code has been modified to gracefully check `if let Some(obj) = response_obj.as_object_mut()` before attempting to mutate.
