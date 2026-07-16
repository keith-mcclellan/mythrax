---
title: "Single Point of Failure: Consolidated Port 8090 API Gateway"
labels: ["architecture-review", "adversarial"]
status: "open"
---

## 🛑 Finding: Single Point of Failure at the API Gateway

**Finding:** The entire Mythrax architecture consolidates administrative, memory, MCP, and proxy endpoints onto a single port (8090) and a single daemon process.

**Current Assumption:** The `ARCHITECTURE.md` assumes that a "Unified Router & Request Processing Flow" on a single port is robust enough for all client and local routing interactions, relying on a lightweight Axum REST router.

**Attack Scenario:** An adversarial or simply malfunctioning agent (or external script) spams port 8090 with heavy `/v1/mcp/call` requests or incomplete connections (e.g., Slowloris attack). Alternatively, a panic within one of the native Rust in-process model endpoints could crash the entire unified daemon.

**Blast Radius:** Complete system failure. Because all routing (memory persistence, tool execution, model brokerage) is funneled through this single daemon on 8090, a crash or exhaustion of connections here means agents lose access to memory, cannot complete actions, and cannot dynamically switch LLMs. No graceful degradation path exists (the system completely halts).

**Recommended Structural Change:** Decouple the administrative/control plane from the memory/data plane. Separate the REST API/MCP routing from the core daemon persistence layer using Unix domain sockets or separate ports (e.g., 8090 for control, 8091 for data). Introduce circuit breakers and connection rate limiting on the gateway layer.
