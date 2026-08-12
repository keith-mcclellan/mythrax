---
title: Off-by-one error risk in manage_handlers start/end lines
labels: bug, agent-found
---
# Bug Report

**Severity:** MEDIUM
**File:** `mythrax-core/src/mcp_routes/manage_handlers.rs`
**Line:** 1249, 1361

## Minimal Reproducible Scenario
The calculation `let end_idx = end_line.map(|e| e.min(lines.len())).unwrap_or(lines.len());` (and similarly for `start_line` `saturating_sub(1)`) has a subtle edge case. If `start_idx` is calculated to be strictly greater than `end_idx` (e.g., if the user provided contradictory bounds like start=10, end=5, or due to a 0-based vs 1-based index calculation mapping edge case), taking a slice `lines[start_idx..end_idx]` will panic the thread.

## Suggested Fix
Explicitly bound `start_idx` to not exceed `end_idx`: `let start_idx = start_idx.min(end_idx);`.
