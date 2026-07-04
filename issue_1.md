---
labels: ["architecture-review", "adversarial"]
---

# Red Team Architecture Brief: API Gateway Single Point of Failure

**Finding:** The "Single-Port API Gateway & Routing" (Port 8090) consolidates all administrative, memory, MCP, and completions proxy endpoints onto a single REST router without isolation.

**Current Assumption:** The gateway can handle all routing reliably, and fallback to "Server Mode" (direct DB access) is sufficient if the daemon is inactive. The assumption is that single-port consolidation simplifies the client experience without degrading resilience.

**Attack Scenario:** An adversarial or runaway agent sends a malformed MCP payload or an overwhelming burst of completions requests (e.g., via unbounded recursive calls). This triggers a panic in the Axum REST router or exhausts its connection pool, crashing the entire daemon process.

**Blast Radius:** Complete loss of cognitive functions, memory retrieval, agent handoffs, and inference proxying for all running agents. Because all capabilities are consolidated behind port 8090, a gateway crash takes down the entire intelligence layer.

**Recommended Structural Change:** Decouple the administrative/control plane (MCP, config) from the data/inference plane (chat completions). Run them on separate Axum instances or isolate the proxy completions to a dedicated lightweight router, ensuring a crash in complex MCP payload parsing does not take down the entire inference pipeline. Require a mandatory ADR response to close this issue.