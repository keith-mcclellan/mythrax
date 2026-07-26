---
labels: ["architecture-review", "adversarial"]
---

# Adversarial Architecture Review: Single-Port API Gateway & Static Auth Token SPOF

## Finding
The Single-Port API Gateway consolidates all administrative (REST), memory, Model Context Protocol (MCP), and transparent completions proxy endpoints onto a single port (8090) protected by a single shared static authentication token (`X-Mythrax-Token`). This is a critical Single Point of Failure (SPOF).

## Current Assumption
The architecture assumes that because Mythrax is a "local sidecar daemon," a shared static token on a localhost-bound port is sufficient security against external threats, and that consolidating all traffic simplifies the client auto-spawn sequence and port management.

## Attack Scenario
An attacker with minimal local access (e.g., a low-privileged background script) or an external attacker exploiting a Server-Side Request Forgery (SSRF) vulnerability in an unrelated local application can easily discover or brute-force the static token. Since all capabilities—including MCP tool execution which can run arbitrary code—are exposed on the same port and guarded by the same token, bypassing this single check grants immediate, unrestricted access. Furthermore, if an LLM is compromised via prompt injection, it could interact with the gateway loopback, effectively escalating its own privileges.

## Blast Radius
**Catastrophic.** Failure of this single authentication boundary leads to total system compromise. The attacker can execute arbitrary code via MCP tools, poison the cognitive database (SurrealKV/SQLite), exfiltrate the user's Obsidian Vault, and commandeer the LLM broker. There is no graceful degradation or compartmentalization.

## Recommended Structural Change
1. **Decouple Interfaces:** Separate the model proxy/inference endpoints from the administrative/MCP endpoints. Run sensitive MCP/Admin capabilities over Unix domain sockets rather than TCP, or bind them to a distinct, dynamically assigned port.
2. **Dynamic Authentication:** Eliminate the static `X-Mythrax-Token`. Implement dynamic, short-lived tokens (e.g., rotating JWTs negotiated on spawn) or mTLS for daemon-client communication.
3. **Capability Gating:** Implement fine-grained, scoped access controls for MCP tools so that a compromised token only grants least-privilege access, not global daemon control.
