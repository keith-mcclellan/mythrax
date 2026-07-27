---
title: "Red Team Architecture Brief: Single-Port Gateway SPOF and DoS Vulnerability"
labels: ["architecture-review", "adversarial"]
---

### Finding
The Single-Port API Gateway (Port 8090) consolidates all administrative, memory, MCP, and external completions proxy endpoints behind a single port using a single shared static authentication token (`X-Mythrax-Token`).

### Current Assumption
The assumption is that a single port and static token simplify deployment and client routing (especially for the "Auto-Spawn Sequence") and that network isolation provides adequate defense-in-depth.

### Attack Scenario
An attacker who discovers or leaks the single static token (or brute-forces a timing attack on the token comparison) gains complete systemic control. Furthermore, because both high-throughput completions (proxying) and critical administrative/memory functions share the same `reqwest::Client` connection pool and tokio runtime boundaries, an adversary can flood the completions proxy or MCP endpoints, saturating the connection pool or triggering OOM/VRAM exhaustion.

### Blast Radius
Critical System-Wide Failure. Total compromise of memory (SurrealKV/Obsidian), administrative control, and denial of service (DoS) for all external agent orchestration. The failure of this gateway leaves no graceful degradation path.

### Recommended Structural Change
1. **Decouple Ports and Token Scopes**: Separate the data/completions plane from the control/administrative plane onto different ports (or distinct socket types) with scoped, short-lived JWTs rather than a single static token.
2. **Implement Rate Limiting and QoS**: Introduce strict per-endpoint rate limiting and resource quotas at the gateway level to ensure administrative endpoints remain responsive during high-load proxy traffic.