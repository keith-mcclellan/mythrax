---
labels: ["architecture-review", "adversarial"]
---

# Issue: SPOF in Single-Port API Gateway Authentication

## Finding
The Single-Port API Gateway (`Port 8090`) uses a single, static shared authentication token (`X-Mythrax-Token`) to validate all REST and MCP requests, as documented in `ARCHITECTURE.md` and implemented in `daemon.rs` / `main.rs`.

## Current Assumption
The architecture assumes that a single static token is sufficient for security because the daemon is acting as a local sidecar. It assumes that if the token file (`~/.mythrax/token` or the hardcoded "secret-token" fallback) is kept local and uncompromised, the entire unified port boundary is secure.

## Attack Scenario
An attacker gains read access to the local file system (e.g., via a path traversal vulnerability in another application, or a malicious script executed by the user) and reads `~/.mythrax/token`, or exploits the hardcoded "secret-token" fallback. With this single token, the attacker can authenticate to the API Gateway on port 8090.

## Blast Radius
**Total System Compromise.** Because the gateway consolidates all administrative, memory, MCP, and proxy endpoints, possessing the single token grants the attacker full read/write access to the entire persistent cognitive graph (SurrealKV/RocksDB), allows them to execute arbitrary tools via MCP (which may include local file management or git operations), and lets them proxy arbitrary model completions, potentially exfiltrating sensitive context. There is no graceful degradation path; once the single token is breached, the entire perimeter falls.

## Recommended Structural Change
1. **Remove the single shared static token.** Implement a multi-tenant or scoped token system where different agents/clients receive distinct tokens with restricted scopes (e.g., read-only memory access vs. write access vs. admin capabilities).
2. **Remove the hardcoded fallback token** (`secret-token`), ensuring a cryptographic token is always required and dynamically generated if missing.
3. **Implement token rotation** and dynamic generation for agent sessions to limit the lifespan of any single credential.

*Note: Do not close this issue without a documented Architectural Decision Record (ADR) response.*