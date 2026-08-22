# Adversarial Architecture Review: Unauthenticated `X-Mythrax-Token` single point of failure

Labels: architecture-review, adversarial

**Finding**: Mythrax relies on a single shared static token passed via the `X-Mythrax-Token` header for API gateway and MCP authentication.

**Current Assumption**: The single token is sufficient because traffic is presumed local or protected by a firewall.

**Attack Scenario**: If an attacker gains read access to the process environment (e.g., via a directory traversal or compromised dependency) and steals the token, they have full unfettered administrative access to the entire memory and config store. There is no session tracking, IP pinning, or rotation mechanism.

**Blast Radius**: Total compromise of the agent's memory, cognitive pipeline, and secrets vault.

**Recommended Structural Change**: Introduce scoped, temporary bearer tokens with role-based access control (RBAC). The main static token should only be able to mint scoped tokens with minimal permissions.