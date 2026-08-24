# [Red Team] Single Point of Failure: The Single-Port API Gateway

**Labels:** architecture-review, adversarial

**Finding:** All administrative, memory, MCP, and proxy completions traffic is consolidated onto a single port (`8090`). `ARCHITECTURE.md` explicitly calls this out as a feature.

**Current Assumption:** A single unified router (`Axum`) handling all traffic via one port simplifies deployment and client discovery while maintaining stability.

**Attack Scenario:**
1. A malformed HTTP payload, an oversized chunked request, or an exploited panic path (e.g., the known mutex poisoning panic in `log_writer.rs` or unhandled `unwrap` in the HTTP router) crashes the Axum server.
2. Because all subsystems (REST, MCP, transparent proxy, and memory DB access) share this single process and single port, the entire sidecar daemon terminates.
3. The "Auto-Spawn Sequence" will attempt to restart the daemon, but if the malformed request is systematically retried by an automated client, it will result in an infinite crash-loop.
4. There is no graceful degradation path: if the gateway fails, the active models are evicted, the database closes, and the client agent is left completely blind.

**Blast Radius:**
- 100% loss of functionality (memory, inference, tool execution) across all connected clients simultaneously.
- Persistent denial of service (DoS) via crash-looping.

**Recommended Structural Change:**
- Separate the control plane (administrative and lifecycle endpoints) from the data plane (inference proxy and MCP calls).
- Run these planes in isolated threads or processes so that a panic in the proxy payload handler does not take down the memory database or administrative router.
- Issue an ADR defining the multi-process or strict panic-catch architecture for the unified router.