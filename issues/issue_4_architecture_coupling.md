---
title: "🛡️ Red Team Architecture Brief: Unbounded Recursion Risk via Vault Watcher and HTR Loop Coupling"
labels: ["architecture-review", "adversarial"]
status: "open"
---

# Red Team Architecture Brief

**Finding**
Arbor HTR Loop & Vault Watcher Cyclic Dependency causes unbounded recursion risk and represents a severe architectural coupling liability.

**Current Assumption**
The 500ms Obsidian Vault watcher and the Arbor HTR parallel verification loop operate independently and safely coalesce events.

**Attack Scenario**
The Arbor HTR loop synthesizes code and updates a `wiki_node` or file in the Obsidian Vault. The Vault Watcher detects this change, triggers the 500ms coalescing, and kicks off an ingestion cycle. This ingestion triggers the Sigmoid Gated Search, which updates the STM, potentially triggering another background sweep or another HTR loop. Under edge cases or specific file edits, this creates an unbounded recursion loop.

**Blast Radius**
Infinite resource exhaustion loop (OOM or thermal throttling), effectively a self-inflicted Denial of Service (DoS). The Vault Watcher and HTR Loop cannot be independently deployed, tested, or replaced without modifying both, making them highly coupled.

**Recommended Structural Change**
Decouple the Vault Watcher from the HTR Loop using an explicit Event Bus with cyclic-detection headers (e.g., `X-Generated-By: Arbor-HTR`). The Watcher must categorically ignore file changes tagged with internal generator IDs to prevent recursive feedback loops.
