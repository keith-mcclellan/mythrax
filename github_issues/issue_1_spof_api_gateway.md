---
title: "Architecture Review: Single-Port API Gateway represents a hard Single Point of Failure (SPOF)"
labels: ["architecture-review", "adversarial"]
---

### Finding
Single-Port API Gateway represents a hard Single Point of Failure (SPOF)

### Current Assumption
Consolidating all administrative, memory, MCP, and LLM proxy endpoints onto a unified, single-port gateway (Port 8090) simplifies client connectivity and enforces unified auth boundaries.

### Attack Scenario
An adversarial agent or malicious client sends a slowloris attack, unbounded payload, or malformed MCP request that exhausts Axum worker threads. Because the API gateway is shared across all daemon functions, the entire control and data plane crash simultaneously.

### Blast Radius
Total system unresponsiveness. Agents cannot access memory, clients cannot route to models, and administrative commands fail. No graceful degradation path exists (e.g., falling back to a separate admin port).

### Recommended Structural Change
Decouple the control plane (administrative API and MCP config) from the data plane (model proxy and high-throughput memory retrieval). Deploy separate ports or an explicit sidecar reverse proxy that implements rate limiting, load shedding, and connection timeouts per route.

> **Note:** Do not close this issue without a documented Architectural Decision Record (ADR) response.