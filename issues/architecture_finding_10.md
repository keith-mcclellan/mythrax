---
title: "18-Month Re-Architecture Liability 1: SurrealKV/SQLite Hybrid to Distributed Database"
labels: ["architecture-review", "adversarial"]
---

## Finding
The current architecture relies on a local, file-based SurrealKV and SQLite hybrid storage engine.

## Current Assumption
Local file-based storage is sufficient for performance and concurrency.

## Attack Scenario
At 10x scale (e.g., massive concurrent agent sessions and exponential memory corpus growth), the file-based lock mechanisms and single-node constraint will become severe bottlenecks, leading to excessive latency and IO exhaustion.

## Blast Radius
System-wide performance degradation and inability to scale beyond a single node.

## Recommended Structural Change
Migrate to a distributed, horizontally scalable database architecture (e.g., distributed PostgreSQL or managed SurrealDB cluster) that natively supports high concurrency and eliminates file-lock dependencies.

> Note: This issue requires a documented architectural decision record (ADR) response before it can be closed.
