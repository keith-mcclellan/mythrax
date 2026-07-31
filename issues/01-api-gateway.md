---
title: "Single-Port API Gateway Auth Token Static Coupling & SPOF"
labels: ["architecture-review", "adversarial"]
---

# Red Team Architecture Brief

**Finding:**
The unified Single-Port API Gateway (Port 8090) validates all REST and MCP requests against a shared, static authentication token passed via `X-Mythrax-Token` and `Authorization` headers.

**Current Assumption:**
A single static auth token is sufficient for securing internal sidecar daemon tooling on a local machine, assuming the environment itself is trusted.

**Attack Scenario:**
If a single process or script leaks the static token (e.g., via logging, error messages, or compromised dependencies), an attacker gains full, unauthenticated access to the entire Mythrax daemon. They can manipulate memory, hijack the cognitive graph, or execute arbitrary MCP tools. Because the token is static, there is no way to isolate or revoke access for specific components.

**Blast Radius:**
Complete system compromise. All agent operations, memory stores, and model routing become accessible to the attacker.

**Recommended Structural Change:**
Implement dynamic, short-lived tokens (e.g., JWTs) issued per-agent or per-process. Transition away from a single static token to a system that supports token rotation and granular scopes.
