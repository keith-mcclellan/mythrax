---
labels: bug, agent-found, architecture-review
title: "CTO Review: Integer Underflow Panic in Token Economics Calculation"
---

## Bug Description
In `mythrax-core/src/mcp_routes/manage_handlers.rs`, an arithmetic underflow panic can occur when calculating `savings_percent`. `savings` is calculated as `total_discovery - total_read` and stored as an `i32`. When `total_read > total_discovery`, `savings` is negative. The subsequent expression `((savings as f64 / total_discovery as f64) * 100.0).round() as u32` attempts to cast a negative floating-point number to `u32`, which results in a panic in Rust when compiled with debug assertions, or wraps to an unpredictable large value in release mode leading to silent failure or crashes down the line.

## File and Line Number
- File: `mythrax-core/src/mcp_routes/manage_handlers.rs`
- Line: 1545 (before fix)

## Reproducible Scenario
1. Trigger a management or discovery process where the `total_read` context token count exceeds the `total_discovery` tokens (e.g. during an intensive read-heavy agent sequence with little new discovery).
2. The logic enters the block:
```rust
let savings = (total_discovery as i32) - (total_read as i32); // Results in a negative number
let savings_percent = ((savings as f64 / total_discovery as f64) * 100.0).round() as u32; // Panics or wraps
```
3. The server thread panics handling the user's request.

## Severity
**High**. This bug leads to an easily reachable server panic in the MCP handler layer on perfectly valid requests, crashing the thread and dropping the API connection.

## Suggested Fix
Change the cast from `as u32` to `as i32`.
```rust
let savings_percent = if total_discovery > 0 {
    ((savings as f64 / total_discovery as f64) * 100.0).round() as i32
} else {
    0
};
```
