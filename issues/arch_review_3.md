---
title: "Finding: Streaming-to-Disk Cognitive Pipeline and Bounded Pagination"
labels: ["architecture-review", "adversarial"]
---

**Finding** -> Streaming-to-Disk Cognitive Pipeline and Bounded Pagination

**Current Assumption** -> Writing directly to disk and using `LIMIT 50` bounded pagination prevents unbounded memory growth.

**Attack Scenario** -> An attacker exploits the temporal expansion graph traversals (depth 1/2/3) or the DBSCAN ephemeral clustering. While bounded, generating massive amounts of interconnected temporal nodes forces worst-case disk I/O on the Obsidian Vault, bypassing memory limits but destroying disk IOPS.

**Blast Radius** -> Disk IOPS exhaustion and database lock contention, stalling other services.

**Recommended Structural Change** -> Enforce a global token/node creation budget per day/session, not just memory boundaries. Apply rate limiting on graph edge creation.
