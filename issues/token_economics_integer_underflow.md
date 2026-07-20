---
title: "Bug: Integer underflow/overflow risk in token_economics calculation"
labels: ["bug", "agent-found"]
---

### Description
In `mythrax-core/src/mcp_routes/manage_handlers.rs`, the `token_economics` savings percent is calculated as `((savings as f64 / total_discovery as f64) * 100.0).round() as u32`. If `total_read` is greater than `total_discovery`, `savings` becomes a negative `i32`. When cast to `f64` and eventually cast back to an unsigned integer `u32`, this negative value will overflow, resulting in a massively incorrect (wrapped) percentage value.

### File and Line Number
* `mythrax-core/src/mcp_routes/manage_handlers.rs`, line 1545

### Minimal Reproducible Scenario
1. Trigger a token economics calculation where the agent configuration or caching causes `total_read` to exceed `total_discovery` (e.g., `total_discovery = 1000`, `total_read = 1500`).
2. `savings` becomes `-500`.
3. `-500 as f64 / 1000.0 * 100.0` yields `-50.0`.
4. `-50.0.round() as u32` results in an integer underflow, wrapping around to `4294967246`.
5. The `token_economics` payload returns a broken metric.

### Severity
**Medium** - Produces wildly incorrect metrics that could break downstream agent scaffolding evaluations.

### Suggested Fix
Use a signed integer for `savings_percent` (e.g., `i32`) or clamp it to 0 before casting to `u32`.
```rust
let savings_percent = if total_discovery > 0 {
    (((savings as f64 / total_discovery as f64) * 100.0).round() as i32).max(0) as u32
} else {
    0
};
```
