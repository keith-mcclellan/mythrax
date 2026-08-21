# Red Team Architecture Brief: Single-Port API Gateway Authentication

**Labels:** `architecture-review`, `adversarial`
**Requirement to Close:** A documented architectural decision record (ADR) response must be provided.

**Finding:** The daemon relies on a single shared static `X-Mythrax-Token` for authentication (as seen in `mythrax-core/src/main.rs` and `mythrax-core/src/mcp.rs`).
**Current Assumption:** A single static token is sufficient for both internal agents and external clients, and can be kept secure without rotation or scoping.
**Attack Scenario:** A compromised subagent or a successful prompt injection attack causes an agent to leak the token. The attacker gains full administrative access, allowing arbitrary code execution (via tools) and database exfiltration.
**Blast Radius:** Complete system compromise. Failure has no graceful degradation path—once the token is compromised, the entire daemon is breached.
**Recommended Structural Change:** Implement scoped JWTs or mTLS per client/agent with role-based access control (RBAC), and rotate tokens dynamically.
