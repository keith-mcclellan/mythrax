---
title: "🛡️ Sentinel: [HIGH] Database Concurrency and Local Lock Contention"
labels: ["architecture-review", "adversarial", "bug", "agent-found"]
---

# Vulnerability Report: Database Lock Contention

## Finding
The dual-engine storage model (RocksDB and SurrealKV) relies heavily on exclusive local file locks. Under multi-process test runs or rapid daemon cycling, this triggers severe lock contention, which is currently mitigated by a fragile 500ms backoff and retry loop.

## Current Assumption
The 500ms backoff loop is sufficient to resolve contention, and the system will remain single-node with low concurrent access demands, ensuring locks are released in a timely manner.

## Attack Scenario
A sudden spike in parallel agent processes (such as those spun up during complex Arbor HTR parallel evaluations) or a deliberate resource exhaustion attack easily exhausts the retry loop. This causes database lockouts, transaction failures, and cascading failures across the gateway and daemon, preventing new memories from being written or read.

## Blast Radius
**Denial of Service (DoS)** for all memory operations, potential data corruption if writes are abruptly interrupted, and a hard ceiling on horizontal scaling. This represents a critical 18-month scaling liability.

## Recommended Structural Change
Decouple the storage layer from embedded local file databases. Migrate to a highly concurrent, networked database service (e.g., PostgreSQL or a dedicated SurrealDB cluster) that handles connection pooling and concurrent transactions natively without relying on brittle file-level locks.

---
*Note: Do not close this issue without a documented Architectural Decision Record (ADR) response.*