---
title: "Single-Port API Gateway Single Point of Failure and Shared Auth"
labels: ["architecture-review", "adversarial"]
---

## Finding
The Single-Port API Gateway consolidates all administrative, memory, MCP, and completions proxy endpoints onto a unified port (8090). This gateway relies on a shared `reqwest::Client` and a shared static auth token (`X-Mythrax-Token` and `Authorization` headers).

## Current Assumption
The assumption is that a single gateway simplifies client-daemon interaction (auto-spawn sequence) and that a shared static token provides sufficient security for local IPC, while shared connection pooling avoids socket exhaustion.

## Attack Scenario
1. **Denial of Service:** Contention on the shared `reqwest::Client` or a surge in requests to any one endpoint (e.g., intensive memory queries) blocks all other traffic (e.g., MCP tool invocations or completions).
2. **Privilege Escalation / Blast Radius:** If an attacker extracts the single shared static auth token (e.g., via prompt injection logging or reading `~/.mythrax/token` through a path traversal vulnerability), they gain full administrative control over all endpoints (memory, LLM configs, vault, execution).

## Blast Radius
**Critical.** Complete system compromise. A single compromised token grants full access to all cognitive and operational capabilities of the daemon. Complete denial of service if the single port or shared client pool is exhausted.

## Recommended Structural Change
1. **Network Segregation:** Split the unified gateway into at least two separate ports/interfaces: one for data/completions (lower privilege) and one for control/administration (higher privilege).
2. **Auth Segregation:** Replace the single static token with scoped JWTs or distinct capabilities/tokens per service (e.g., separate tokens for MCP tools vs. memory querying).
3. **Resource Isolation:** Use separate HTTP client pools or connection queues per critical path to prevent one noisy endpoint from starving the others.
