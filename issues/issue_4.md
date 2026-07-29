---
title: Integer underflow when calculating savings percentage
labels: bug, agent-found
severity: Medium
---

**File/Line:** `mythrax-core/src/mcp_routes/manage_handlers.rs` : 2198

**Minimal Reproducible Scenario:**
The logic `savings as f64` occurs after `let savings = (total_discovery as i32) - (total_read as i32);`. If `total_read` is larger than `total_discovery` (negative savings), `savings` is negative. Casting a negative `i32` or negative `f64` to an unsigned integer `u32` (via `.round() as u32`) causes a silent underflow and wraps around to a massive positive integer, messing up token economic metrics.

**Suggested Fix:**
Clamp the savings or use a signed type for percentage representation.
```rust
let savings_percent = if total_discovery > 0 {
    let p = (savings as f64 / total_discovery as f64) * 100.0;
    p.round().max(0.0) as u32
} else {
    0
};
```
