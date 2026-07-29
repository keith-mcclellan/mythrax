---
title: "Red Team Architecture Brief: Single Point of Failure via Static API Gateway Authentication"
labels: ["architecture-review", "adversarial"]
---

**Finding:** The API Gateway in `mythrax-core` uses a static shared authentication token via `X-Mythrax-Token` headers.

**Current Assumption:** The assumption is that a single, statically configured token is sufficient for securing internal communication between agents, clients, and the Mythrax daemon within a presumed safe local environment.

**Attack Scenario:** An attacker gains read access to the file system, environment variables, or intercepts unencrypted local traffic (e.g., via a compromised dependency or adjacent process). They extract the static `X-Mythrax-Token`. With this token, they have full administrative access to the API Gateway on port 8090, allowing them to manipulate episodic memory, inject malicious wiki nodes, trigger unauthorized model inferences, or extract the entire Obsidian Vault contents.

**Blast Radius:** Complete system compromise. A single leaked token grants unrestricted access to all data and capabilities managed by the Mythrax daemon, bypassing all logical agent boundaries and exposing all historical and project-specific memory.

**Recommended Structural Change:**
1. Deprecate the static `X-Mythrax-Token` in favor of dynamic, short-lived, and scoped authentication tokens (e.g., JWTs with narrow claims and expirations).
2. Implement a secure token issuance and rotation mechanism.
3. Enforce principle-of-least-privilege by issuing distinct tokens to different agents or services, restricting access based on roles or specific API endpoints (e.g., read-only memory access vs. write access).
