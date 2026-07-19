---
tags: [architecture-review, adversarial]
---
# Single Point of Failure: Static Token Auth

## Finding
The API Gateway uses a shared static auth token via `X-Mythrax-Token` headers. This is a single point of failure.

## Code Reference
`mythrax-core/src/api.rs`, specifically the `check_auth` function.

## Current Assumption
Network isolation and a single secure token are sufficient for protecting all REST and MCP endpoints.

## Attack Scenario
An attacker compromises a single client machine, an agent log, or accidentally exposed environment variable, obtaining the static token. Since all administrative and memory operations use this same token, they gain full administrative access.

## Blast Radius
Total system compromise. The attacker can modify configurations, manipulate episodic memories, poison knowledge bases, and use the LLM completions proxy for arbitrary workloads.

## Recommended Structural Change
Implement token scoping, dynamic rotation, and Role-Based Access Control (RBAC). Distinguish between administrative tokens and agent-specific memory tokens.

**Note: Do not close this issue without a documented architectural decision record (ADR) response.**
