---
title: "Bug: Panic Path during downcast in inspect_failed_query.rs"
labels: bug, agent-found
---
## Issue
File: `mythrax-core/src/bin/inspect_failed_query.rs`
Line: 236

## Minimal Reproducible Scenario
The command `let surreal_backend = backend.as_any().downcast_ref::<SurrealBackend>().unwrap();` directly unwraps the result of downcasting the generic `StorageBackend` trait object. If the application configuration switches to a different backend implementation, this code path will deterministically panic upon execution.

## Severity
High

## Suggested Fix
Replace the `.unwrap()` with safe error handling, e.g., `.ok_or_else(|| anyhow::anyhow!("Expected SurrealBackend instance"))?` to surface a graceful error.
