---
title: Panic risk in mcp_routes/manage_handlers parsing logic
labels: ["bug", "agent-found"]
---
## Bug Description
**File and Line Number:** `mythrax-core/src/mcp_routes/manage_handlers.rs:97`

**Minimal Reproducible Scenario:** When processing outputs from an external action in `handle_manage_htr`, the code assumes `outputs_map` and the specific `output.name` exist: `let val = outputs_map.unwrap().get(&output.name).unwrap();`. Since `outputs_map` is parsed from external input `args.get("outputs")`, if it's missing or if `get(&output.name)` returns `None` despite `has_val` evaluating to true (due to logic bugs or concurrent modifications), this will panic. External inputs must never be trusted with `.unwrap()`.

**Severity:** High (Crash due to external payload format)

**Suggested Fix:** Use defensive parsing with `if let Some(map) = outputs_map` and `if let Some(val) = map.get(&output.name)`. Return an appropriate HTTP error response if values are missing or malformed.
