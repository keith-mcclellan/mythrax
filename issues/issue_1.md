---
title: "Architecture Liability: Missing Graceful Degradation in Single-Port Gateway Auth"
labels: ["architecture-review", "adversarial"]
---

**Finding** -> The Single-Port API Gateway relies on a shared static auth token via `X-Mythrax-Token` headers for both REST and MCP requests.

**Current Assumption** -> The token remains secret and cannot be intercepted, leaked, or bypassed, and one token is sufficient for all roles.

**Attack Scenario** -> If the static token is leaked (e.g., via logs, prompt injection, or SSRF), an attacker gains full access to all endpoints, including memory manipulation and LLM context injection. There is no role-based access control or token rotation mechanism.

**Blast Radius** -> Complete system compromise. Adversary can read/write all agent memories, modify system prompts, and execute arbitrary MCP tool calls.

**Recommended Structural Change** -> Implement ephemeral, scope-limited JWTs or mutual TLS (mTLS) for agent-daemon communication. Enforce Role-Based Access Control (RBAC) at the gateway level.

*Note: Do not close this issue without a documented architectural decision record (ADR) response.*
