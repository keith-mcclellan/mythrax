---
title: "Coupling Liability: Tightly Coupled API Gateway and Persistent Storage"
labels: ["architecture-review", "adversarial"]
---

## Finding
The single-port API gateway in `mythrax-core/src/main.rs` is tightly coupled to the persistent storage lifecycle (database initialization, lock retries, and fallback to Server Mode).

## Current Assumption
The daemon process can efficiently manage both routing requests and the persistence lifecycle without separation of concerns.

## Attack Scenario
A failure in the storage layer (e.g., a file lock deadlock during reconnection attempts) directly blocks the API gateway from processing incoming REST or MCP requests. They cannot be independently deployed, tested, or replaced.

## Blast Radius
Complete system unresponsiveness. Any issue in the storage engine cascades into an API outage.

## Recommended Structural Change
Decouple the API gateway from the storage engine. Introduce an intermediate abstraction layer (e.g., a message queue or gRPC service) that allows the gateway to operate independently and degrade gracefully if the storage layer is unavailable.

> Note: This issue requires a documented architectural decision record (ADR) response before it can be closed.
