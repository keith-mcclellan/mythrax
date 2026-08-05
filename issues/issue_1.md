---
labels: architecture-review, adversarial
---
# Adversarial Review: SQLite Embedding Cache I/O locks (Scaling Bottleneck)

## Finding
SQLite Embedding Cache (`embeddings.db`) I/O locks create a severe scaling bottleneck at 10x scale.

## Current Assumption
The current design assumes that local, synchronous database queries will easily scale to support parallel concurrent queries and scaling load.

## Attack Scenario
An attacker sends a high volume of queries (or simply a high volume of normal concurrent requests occur), leading to database lock contention. The system tries to serialize concurrent writes or blocks reads, causing the system to lock up or reject requests.

## Blast Radius
The entire daemon halts or significantly degrades in performance, unable to process memory retrieval or embeddings for any agent, leading to a system-wide denial-of-service.

## Recommended Structural Change
Decouple the embedding cache from a simple SQLite database or implement proper write-ahead logging (WAL) / connection pooling with async yields. Alternatively, move high-throughput embedding caches to an in-memory key-value store (like Redis or a distributed memcached equivalent) before persisting to disk.