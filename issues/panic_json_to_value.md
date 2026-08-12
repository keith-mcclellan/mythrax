---
title: Panic on JSON serialization in read_handlers
labels: bug, agent-found
---
# Bug Report

**Severity:** HIGH
**File:** `mythrax-core/src/mcp_routes/read_handlers.rs`
**Line:** 612

## Minimal Reproducible Scenario
The line `let mut v = serde_json::to_value(&r).unwrap();` inside the `map` closure panics if `&r` contains types that cannot be serialized to JSON. Given that `r` originates from a database query (`search_res.results`), if the database record contains invalid UTF-8 sequences (e.g. from malicious or corrupted user uploads), `serde_json::to_value` could fail, panicking the thread and crashing the read handler for all search queries.

## Suggested Fix
Handle the serialization error gracefully, potentially by skipping the invalid record or returning an HTTP error.
