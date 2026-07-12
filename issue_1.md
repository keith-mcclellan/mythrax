---
title: "🛡️ Red Team Architecture Brief: Single-Port API Gateway & Hardcoded Auth Tokens (SPOF)"
labels: ["architecture-review", "adversarial"]
---

## Red Team Architecture Brief

**Finding:**
The Mythrax 2.0 Core Daemon relies on a Single-Port API Gateway (Port 8090) for all REST, MCP, and proxy traffic, with authentication secured by hardcoded static tokens (e.g., `X-Mythrax-Token` and other tokens identified in the audit report). This design creates a critical single point of failure (SPOF) with no graceful degradation path.

**Current Assumption:**
The current architecture assumes that consolidating all endpoints onto a single port simplifies client orchestration and that the local environment is completely trusted, negating the need for dynamic secret rotation or robust API gateway redundancy. It assumes the gateway process will never crash under load or be subjected to sophisticated local privilege escalation attacks.

**Attack Scenario:**
An adversarial actor (or a malicious process running in the same user space) extracts the hardcoded auth tokens from the binary or memory. By flooding port 8090 with malformed MCP requests or large payload completions, the actor crashes the Axum REST router. Because all services (memory management, configuration, chat completions proxy) share this single process and port, the entire cognitive sidecar goes down simultaneously. There is no independent fallback mechanism for memory retrieval if the primary gateway fails.

**Blast Radius:**
Total system failure. If port 8090 becomes unavailable or compromised, autonomous agents lose all access to short-term working memory, persistent project insights, and external model routing. The entire execution pipeline grinds to a halt without any graceful degradation (e.g., falling back to a read-only memory mode or direct in-process database queries). Furthermore, compromised hardcoded tokens allow unrestricted read/write access to all agent memories and API keys.

**Recommended Structural Change:**
1. **Decouple the Control Plane from the Data Plane:** Separate the management/configuration API from the high-throughput memory retrieval/completions proxy to ensure a crash in one does not bring down the other.
2. **Dynamic Secret Management:** Eliminate hardcoded static auth tokens immediately. Implement dynamic token generation on startup, distributed securely via inter-process communication (IPC) or a protected local socket.
3. **Implement Graceful Degradation:** The SDK client should be able to fallback to direct, read-only SurrealKV/RocksDB queries (if locks permit) when the primary gateway is unresponsive, ensuring agents retain context even if the daemon crashes.

*Note: Do not close this issue without a documented Architectural Decision Record (ADR) response.*