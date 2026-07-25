---
title: "Reliance on Local File DB Locks (RocksDB/SurrealKV)"
labels: [architecture-review, adversarial]
---

**Finding:** Reliance on Local File DB Locks (RocksDB/SurrealKV) for Persisted Memory.

**Current Assumption:** Single-node file-locking and a 10-attempt retry loop are sufficient to handle concurrent database writes during compaction and rapid restarts.

**Attack Scenario:** High concurrent load (e.g., rapid episodic memory bursts or clustered dreaming events) exhausts the retry loop or causes lock contentions. Adversarial requests can artificially spam logs, locking the DB.

**Blast Radius:** System freeze and data loss. As the daemon crashes or hangs waiting for locks, no memory can be ingested, breaking all cognitive functions.

**Recommended Structural Change:** Decouple storage to an independent scalable service (e.g. Postgres or distributed SurrealDB) without single-file locks.
