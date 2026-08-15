---
labels: architecture-review, adversarial
---

# Red Team Architecture Brief: Single-Port API Gateway (Port 8090) is a Single Point of Failure

**Note: This issue MUST NOT be closed without a documented Architectural Decision Record (ADR) response.**

**Finding** -> The unified Single-Port API Gateway consolidates all REST, MCP, and proxy endpoints onto port 8090 without a documented graceful degradation path if the gateway panics or is DoS'd.

**Current Assumption** -> The Axum REST router is robust enough to handle all ingress traffic (admin, memory, MCP, completions) without crashing, and token auth is sufficient protection.

**Attack Scenario** -> An adversarial client floods the completions proxy endpoint with malformed requests or large payloads. If the payload size limits (mentioned in API Batch Size Limits) fail or are bypassed, the gateway thread panics, taking down ALL services (MCP, admin, memory) simultaneously.

**Blast Radius** -> Complete system outage. Agents lose all memory access, RAG capabilities, and model routing simultaneously.

**Recommended Structural Change** -> Decouple the control plane (admin/MCP) from the data plane (completions proxy). Run them on separate ports/processes, or at least isolate them in separate tokio task pools with independent rate limiting and failover capabilities.
