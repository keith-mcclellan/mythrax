---
title: Panic on Missing Output in manage_handlers
labels: bug, agent-found
---
# Bug Report

**Severity:** HIGH
**File:** `mythrax-core/src/mcp_routes/manage_handlers.rs`
**Line:** 97

## Minimal Reproducible Scenario
A realistic trigger path involves an MCP client calling the `manage_handlers` route (or relevant component) and triggering code that processes outputs. If `outputs_map` is None or if `output.name` is missing from the map, `outputs_map.unwrap().get(&output.name).unwrap()` will panic.

## Suggested Fix
Replace `.unwrap()` with a graceful error return or pattern matching (`if let Some(val) = outputs_map.and_then(|m| m.get(&output.name))`).
