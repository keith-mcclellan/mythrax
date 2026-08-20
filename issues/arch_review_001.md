# Red Team Architecture Brief

## Finding
Single Point of Failure: SurrealDB/RocksDB Persistence Engine

## Current Assumption
The system assumes that SurrealKV/RocksDB (or the `mem://` default) will remain available, uncorrupted, and performant enough to handle all streaming pipeline states, short-term memory (STM), AST symbol indexing, and ephemeral DBSCAN cluster states.

## Attack Scenario
Under a 10x scale or an adversarial load (e.g., massive concurrent code generation or log spamming), the single database instance becomes an I/O bottleneck. If the database crashes, corrupts, or locks up during a write-heavy compaction sweep, all cognitive artifact generation, memory retrieval, and API gateway routing (relying on `completions_proxy_handler`) halt simultaneously. There is no fallback or graceful degradation to read-only mode.

## Blast Radius
System-wide failure. Agents lose memory access, code indexing fails, and streaming pipelines crash, effectively rendering Mythrax inoperable.

## Recommended Structural Change
Decouple ephemeral/high-throughput state (like STM, sliding windows, and DBSCAN clusters) into a fast, distributed in-memory store (e.g., Redis). Implement a read replica or degraded read-only fallback for the canonical Obsidian Vault markdown files if the database is unreachable.
