---
title: "Bug: Panic risk on json object mut unwrap in manage_handlers.rs"
labels: ["bug", "agent-found"]
---

### Description
In `mythrax-core/src/mcp_routes/manage_handlers.rs`, the code unwraps a JSON object (`response_obj.as_object_mut().unwrap()`) to insert a `token_economics` field. If `response_obj` is dynamically constructed elsewhere and happens to not be a JSON Object (e.g. it becomes a scalar, array, or null due to upstream changes or unexpected inputs), this unwrap will panic and crash the route handler.

### File and Line Number
* `mythrax-core/src/mcp_routes/manage_handlers.rs`, line 1549

### Minimal Reproducible Scenario
1. Trigger the route hitting the `token_economics` insert path.
2. If `response_obj` evaluates to anything other than a JSON Object `serde_json::Value::Object`, `as_object_mut()` returns `None`.
3. The `.unwrap()` call immediately panics.

### Severity
**Low/Medium** - Unsafe assumption about a generic JSON `Value`.

### Suggested Fix
Use pattern matching or an `if let` binding to safely mutate the object.
```rust
if let Some(obj) = response_obj.as_object_mut() {
    obj.insert(
        "token_economics".to_string(),
        json!({
            // ...
        })
    );
}
```
