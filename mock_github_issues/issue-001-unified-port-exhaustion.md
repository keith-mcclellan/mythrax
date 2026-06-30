---
title: "Unified Port Exhaustion and Shared State"
labels: ["architecture-review", "adversarial"]
---

**This issue requires a documented Architectural Decision Record (ADR) response to close.**

### Finding
Unified Port Exhaustion and Shared State

### Current Assumption
Consolidating all administrative, memory, MCP, and proxy endpoints onto a unified, single-port gateway (Port 8090) simplifies the architecture and deployment.

### Attack Scenario
A malicious or runaway agent floods the port 8090 endpoint with massive embeddings or completions requests.

### Blast Radius
Complete denial of service. The single unified router handles administration, memory operations, and model routing. If the thread pool or socket backlog is exhausted, the entire system becomes unresponsive, preventing administrative intervention or emergency shutdown.

### Recommended Structural Change
Decouple the administrative/control plane from the data/inference plane. Run administration and critical operations on a separate, rate-limited port.