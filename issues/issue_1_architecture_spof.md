---
title: "🛡️ Red Team Architecture Brief: Single Point of Failure in Gateway & SDK Direct DB Fallback"
labels: ["architecture-review", "adversarial"]
status: "open"
---

# Red Team Architecture Brief

**Finding**
Client SDK Direct Database Fallback & Process ID Spoofing (SPOF & Coupling). The architecture allows the SDK to bypass the Single-Port API Gateway and directly access the database if the daemon is inactive.

**Current Assumption**
The client SDK can safely fall back to "Server Mode" by opening the local database (`~/.mythrax/db`) directly if the daemon is inactive. It is assumed the auto-spawn sequence trusting `~/.mythrax/daemon.pid` is secure.

**Attack Scenario**
A malicious local actor or a race condition creates a fake `daemon.pid` pointing to a malicious server, hijacking all agent traffic and auth tokens. Furthermore, if two client SDKs fall back to direct database access simultaneously, they bypass the gateway's concurrency controls entirely, leading to SurrealKV lock corruption. The API Gateway is a SPOF; bypassing it breaks all systemic guarantees.

**Blast Radius**
Total data corruption of the local Vault and complete hijacking of agent REST/MCP traffic. Failure of the API Gateway has no graceful degradation path because the fallback mechanism inherently violates the data concurrency and security models.

**Recommended Structural Change**
Remove direct DB access from the Client SDK. Enforce a strict client-server boundary. The SDK must ONLY communicate via HTTP/MCP. If the daemon is down, the request must fail gracefully. Use proper IPC socket binding or domain sockets instead of PID files to guarantee daemon authenticity.
