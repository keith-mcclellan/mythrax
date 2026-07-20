---
title: "Bug: Silent logic failure (division by zero) in agent_recall calculation"
labels: ["bug", "agent-found"]
---

### Description
In `mythrax-core/src/bench/agent_recall.rs`, the `overall_score` check correctly handles the case where `total_queries > 0`. However, inside the loop that processes `scores_by_type`, it unconditionally calculates `pct = (type_passed as f32 / type_total as f32) * 100.0;`. If `type_total` is 0 (an empty vector for a specific query type), `type_total as f32` evaluates to `0.0`. This results in `0.0 / 0.0`, producing `NaN`. The eval silently succeeds but records `NaN` for that query type's performance percentage, polluting downstream metrics and dashboards without alerting the developer.

### File and Line Number
* `mythrax-core/src/bench/agent_recall.rs`, line 79

### Minimal Reproducible Scenario
1. Run a benchmark configuration or load a dataset where a specific query type (e.g., `edge_cases`) is present in the `scores_by_type` HashMap but has an empty `passes` array (`type_total = 0`).
2. The loop iterates over this query type.
3. `pct` is computed as `(0 as f32 / 0 as f32) * 100.0`, resulting in `NaN`.
4. The `NaN` value is inserted into `report_scores` and serialized, silently corrupting metric tracking.

### Severity
**Medium** - Silent failure polluting evaluation metrics and potentially breaking SWE-bench gating.

### Suggested Fix
Add a condition to handle `type_total == 0` gracefully, just as is done for `total_queries`.

```rust
let pct = if type_total > 0 {
    (type_passed as f32 / type_total as f32) * 100.0
} else {
    0.0
};
```
