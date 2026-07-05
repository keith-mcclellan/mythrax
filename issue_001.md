---
title: "Architecture Review: Single-Port API Gateway Static Auth Token Vulnerability"
labels: ["architecture-review", "adversarial"]
---

# Red Team Architecture Brief

**Finding:** The unified API Gateway relies on a shared static authentication token (`X-Mythrax-Token`) for both REST and MCP requests, with a hardcoded fallback (`"secret-token"`) implemented in production paths (as identified in `HARD-001`).

**Current Assumption:** The architecture assumes that internal local environments are inherently trustworthy, that the daemon and clients always share the same isolated host environment securely, and that a single static token is sufficient for access control.

**Attack Scenario:** An attacker exploiting a Server-Side Request Forgery (SSRF) vulnerability in an adjacent local service, or a malicious script executed on the user's host (e.g., via a compromised npm/cargo package), can ping `http://127.0.0.1:8090` and supply the hardcoded `"secret-token"`. The attacker gains full control over the daemon, bypassing intended security barriers.

**Blast Radius:** Complete host compromise. The attacker can use MCP endpoints to execute arbitrary file operations (`manage_file`), steal secrets from the vault (`manage_vault`), or poison the agent's memory (`manage_memory`), affecting all active AI agent sessions and compromising local host data.

**Recommended Structural Change:**
- Immediately remove the hardcoded fallback token.
- Implement short-lived, dynamically generated session tokens (e.g., via OAuth2 or JWT with rotating keys) rather than a single static token.
- Enforce strict local binding and origin validation to mitigate SSRF vectors.

*ADR required to close this issue.*