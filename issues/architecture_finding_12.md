---
title: "18-Month Re-Architecture Liability 3: Streaming-to-Disk Cognitive Pipeline to Event-Driven Message Bus"
labels: ["architecture-review", "adversarial"]
---

## Finding
The cognitive pipeline relies on a streaming-to-disk architecture where artifacts are written incrementally to the file system (Obsidian Vault).

## Current Assumption
File system writes are fast enough to handle cognitive synthesis outputs without blocking the main event loop.

## Attack Scenario
At 10x scale, the sheer volume of file system writes (and associated 500ms watcher coalescing) will overwhelm disk IO and cause race conditions, resulting in lost or corrupted memories.

## Blast Radius
Data loss and cognitive pipeline failure.

## Recommended Structural Change
Replace the streaming-to-disk architecture with an event-driven message bus (e.g., Kafka or RabbitMQ) that decouples memory synthesis from persistence, allowing for asynchronous, batched, and reliable artifact processing.

> Note: This issue requires a documented architectural decision record (ADR) response before it can be closed.
