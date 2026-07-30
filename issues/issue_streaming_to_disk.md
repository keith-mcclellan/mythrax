---
tags: [architecture-review, adversarial]
---
# Finding: Streaming-to-Disk Cognitive Pipeline (Obsidian Vault)

**Current Assumption:** Persisting to Markdown files directly is safe and human-readable, with ephemeral state kept temporarily in SurrealDB.

**Attack Scenario:** Concurrent disk writes during high-throughput tool usage or adversarial path-traversal inputs via tool output can overwrite critical files (`MOC.md` or even `.ssh/config` if paths aren't strictly jailed).

**Blast Radius:** Vault corruption, loss of episodic memory, or arbitrary file overwrite.

**Recommended Structural Change:** Enforce a strict chroot/jail on Vault I/O and implement an intermediate durable WAL (Write-Ahead Log) instead of writing directly to the Markdown tree.
