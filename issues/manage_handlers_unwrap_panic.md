---
title: "Bug: Panic risk on outputs_map unwrap in MCP handlers"
labels: ["bug", "agent-found"]
---

### Description
In `mythrax-core/src/mcp_routes/manage_handlers.rs`, the code unwraps `outputs_map` and the result of `get()` based on a preceding boolean check. Although logically sound under current control flow, using `.unwrap()` instead of properly destructuring or matching introduces a risk of panics if the logic governing `has_val` is ever modified or refactored.

### File and Line Number
* `mythrax-core/src/mcp_routes/manage_handlers.rs`, line 70

### Minimal Reproducible Scenario
1. Provide a payload that passes the `has_val` check.
2. If future code modifies the `has_val` logic or if a race condition mutates the map (less likely in this synchronous block, but a general risk), the unwraps will panic: `let val = outputs_map.unwrap().get(&output.name).unwrap();`.

### Severity
**Low/Medium** - Brittle code that could lead to panics during future refactoring.

### Suggested Fix
Use pattern matching or `if let` to safely extract the value without unwrapping.
```rust
if let Some(map) = outputs_map {
    if let Some(val) = map.get(&output.name) {
        // ... validation logic ...
    }
}
```
