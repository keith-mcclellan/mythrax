---
tags: architecture-review, adversarial
---

# Shared Static Token Authentication Coupling

**Finding**: Authentication across REST, MCP, and Model Proxy is tightly coupled via a single shared `X-Mythrax-Token`.

**Current Assumption**: A single shared static auth token is sufficient for securing all internal routing boundaries and subsystem access.

**Attack Scenario**: If the static token is leaked (e.g., via prompt injection that tricks an agent into dumping its headers, or an LFI vulnerability), the attacker gains full administrative access to all subsystems across the monolith.

**Blast Radius**: Complete system compromise. The attacker can bypass agent scope boundaries, read/modify all persistent memories, alter the embedding cache, or pivot to the external OpenAI API using the proxy, completely violating agent boundaries.

**Recommended Structural Change**: Decouple the authentication domains. Use distinct scopes and tokens for MCP vs. REST Admin vs. Model Proxy. Issue short-lived, least-privilege tokens dynamically per agent session rather than a static global token.
