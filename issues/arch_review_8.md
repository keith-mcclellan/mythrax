---
title: "Finding: 18-Month Re-architecture Liability - Obsidian Vault as Canonical Cognitive Store"
labels: ["architecture-review", "adversarial"]
---

**Finding** -> 18-Month Re-architecture Liability - Obsidian Vault as Canonical Cognitive Store

**Current Assumption** -> Writing cognitive artifacts directly to local Markdown/JSON files (`wiki/<scope>/*.md`) provides a readable, canonical store.

**Attack Scenario** -> At 10x scale with multiple concurrent agents, file-system watching and writing to thousands of Markdown files creates a massive I/O bottleneck and synchronization nightmare. File watcher coalescing (500ms) will break under high concurrency.

**Blast Radius** -> Sync failures, lost memories, and massive CPU overhead from the file watcher.

**Recommended Structural Change** -> Treat the database as the single source of truth. Move Obsidian synchronization to a separate, asynchronous, low-priority export job, rather than making it the canonical store.
