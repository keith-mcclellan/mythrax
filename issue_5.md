---
title: "Scaling Liability: File Lock Retry Loop vs Database Resiliency"
labels: ["architecture-review", "adversarial"]
---

# Finding: File Lock Retry Loop vs Database Resiliency

## Current Assumption
A 9-10 attempt, 500ms sleep retry loop is sufficient to handle multi-process RocksDB/SurrealKV lock contention.

## Attack Scenario
At 10x scale, multiple agents or high-frequency compaction sweeps hit the daemon simultaneously. System IO spikes cause delays exceeding 5 seconds. The naive retry loop exhausts, crashing the DB connection or dropping incoming memory writes.

## Blast Radius
Silent data loss. Memory writes from the Obsidian watcher or MCP hooks are dropped because the exclusive DB lock wasn't acquired in time.

## Recommended Structural Change
Replace the retry loop with a persistent, asynchronous MPSC (Multi-Producer, Single-Consumer) queue for write operations. Transactions must be queued in memory or WAL rather than dropped under lock contention.

**Status:** Requires Architectural Decision Record (ADR) response to close.