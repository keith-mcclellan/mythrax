---
title: Adversarial Review: Tight Coupling Between Single-Port Gateway and Daemon Core
labels: architecture-review, adversarial
---

**Finding:** The Single-Port API Gateway directly routes REST, MCP, and external completions proxy requests into the core daemon, sharing the same async tokio runtime and process space.
**Current Assumption:** A monolithic daemon with a unified tokio runtime is efficient and sufficient for local sidecar deployments.
**Attack Scenario:** If the Completions Proxy or MCP handler experiences a severe bug (e.g., a panic that circumvents `catch_unwind`, or a resource exhaustion attack via malformed JSON payloads), it crashes the entire `mythrax` process. This takes down the database connection, the filesystem watchers, and the memory compaction scheduler simultaneously.
**Blast Radius:** Complete Denial of Service (DoS) and potential data corruption if the panic occurs during an unsafe database transaction or VRAM model swap. Single point of failure with no graceful degradation.
**Recommended Structural Change:** Decouple the Gateway and Proxy layers from the Core Daemon. Run the Gateway as a lightweight reverse proxy process (or distinct isolated actor) that communicates with the Core Daemon via IPC/UDS. If the Proxy crashes, the Database and Watcher loops should remain unaffected.

*Note: Do not close this issue without a documented architectural decision record (ADR) response.*