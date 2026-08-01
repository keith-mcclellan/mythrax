---
title: "🛡️ Sentinel: [CRITICAL] Single-Port API Gateway shared static auth token is a Single Point of Failure"
labels: ["architecture-review", "adversarial", "bug", "agent-found"]
---

### Finding:
The Mythrax 3.0 Single-Port API Gateway operates on port 8090 and uses a shared static auth token via `X-Mythrax-Token` and `Authorization` headers for both administrative, memory, MCP, and completions proxy endpoints.

### Current Assumption:
The assumption is that a single static token is sufficient for securing local or sidecar traffic.

### Attack Scenario:
If an attacker or a compromised subagent leaks or discovers the static auth token, they gain unrestricted access to all endpoints, including administrative configuration, memory manipulation, and direct model execution proxies. This allows complete system compromise and arbitrary prompt injection across all agents using the gateway.

### Blast Radius:
CRITICAL. Total compromise of the intelligence daemon, including memory alteration and potential code execution depending on agent tools available. No graceful degradation path exists if the gateway is compromised.

### Recommended Structural Change:
- Implement robust, distinct authentication tokens for different scopes (e.g., admin vs. memory vs. proxy).
- Add mutual TLS (mTLS) for agent-to-gateway communication.
- Implement rate limiting and anomaly detection on the gateway.
