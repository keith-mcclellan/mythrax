---
title: "Bug: Data Loss via Integer Underflow in Token Savings Calculation"
labels: ["bug", "agent-found"]
---

**File:** `mythrax-core/src/mcp_routes/manage_handlers.rs`
**Line:** ~1545

**Description:**
The token economics calculation computes `savings = total_discovery - total_read` as an `i32`. It then calculates the `savings_percent` by casting a negative percentage to `u32` (e.g. `((savings as f64 / total_discovery as f64) * 100.0).round() as u32`). Casting a negative `f64` to `u32` silently truncates/wraps the value to `0`, discarding information about negative savings (token bloat).

**Minimal Reproducible Scenario:**
If `total_read` exceeds `total_discovery` (causing a negative savings value), the resulting percentage evaluates to 0 instead of reflecting the actual negative value.

**Severity:**
Medium (Silent logic failure and loss of critical telemetry)

**Suggested Fix:**
Change the type of `savings_percent` by casting the result to `i32` instead of `u32`.
