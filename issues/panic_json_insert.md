---
title: Panic on JSON insert in manage_handlers
labels: bug, agent-found
---
# Bug Report

**Severity:** HIGH
**File:** `mythrax-core/src/mcp_routes/manage_handlers.rs`
**Line:** 2506

## Minimal Reproducible Scenario
A realistic trigger path involves an MCP client calling the route handled by `manage_handlers`. The line `response_obj.as_object_mut().unwrap().insert(...)` assumes `response_obj` is always a JSON Object. If the API ever generates or parses `response_obj` as a JSON string, array, or null instead (e.g. from an upstream LLM formatting failure or a client sending malformed data), this will panic the server process.

## Suggested Fix
Replace `.unwrap()` with a graceful type check: `if let Some(obj) = response_obj.as_object_mut() { obj.insert(...); }`.
