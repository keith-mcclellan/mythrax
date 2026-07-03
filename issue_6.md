---
title: "Coupling Liability: Destructive Coupling of Compactor and Vault Watcher"
labels: ["architecture-review", "adversarial"]
---

# Finding: Destructive Coupling of Compactor and Vault Watcher

## Current Assumption
The Obsidian 500ms Vault Watcher and the DBSCAN/RAPTOR compactor can coexist inside the same Tokio runtime and share the DB connection.

## Attack Scenario
A mass file modification in the Obsidian vault (e.g., `git checkout` or bulk find/replace) triggers tens of thousands of watcher events. The coalescing window is overwhelmed, monopolizing the Tokio executor and starving the compactor and Gateway of threads.

## Blast Radius
The API gateway becomes unresponsive, causing agents to timeout. Background compaction halts entirely.

## Recommended Structural Change
Decouple the Vault Watcher into an independent edge service or strict background thread pool. Do not share the primary Gateway's Tokio scheduler with the high-throughput file watcher.

**Status:** Requires Architectural Decision Record (ADR) response to close.