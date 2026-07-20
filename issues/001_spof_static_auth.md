---
tags: [architecture-review, adversarial]
status: open
---
# Red Team Architecture Brief: Single Point of Failure in Authentication

**Finding**: The Single-Port API Gateway relies on a shared static authentication token (`X-Mythrax-Token`), which is hardcoded as a fallback (`"secret-token"`).

**Current Assumption**: A single, static token shared across all clients and endpoints is sufficient to secure REST, MCP, and model routing endpoints without granular scoping.

**Attack Scenario**: An attacker compromises the token via a path traversal, memory dump, or logging mistake. With this single token, they bypass the API Gateway's authentication boundary. Because there is no graceful degradation or partitioned access (RBAC), the attacker gains full control over all MCP operations (including arbitrary file reads/writes) and memory injection endpoints.

**Blast Radius**: Complete system compromise (Confidentiality, Integrity, Availability). A single leaked string results in total loss of the host system and data.

**Recommended Structural Change**: Deprecate the static token. Implement scoped, short-lived JWTs, role-based access control (RBAC) per MCP tool, and mutual TLS (mTLS) for daemon-client communication. Do not close this issue without a documented ADR response.
