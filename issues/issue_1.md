---
title: "Single Shared Static Auth Token (API Gateway)"
labels: ["architecture-review", "adversarial"]
---

Finding -> Current Assumption -> Attack Scenario -> Blast Radius -> Recommended Structural Change

**Finding**: API Gateway and MCP rely on a single shared static token (X-Mythrax-Token).
**Current Assumption**: The local environment is trusted, and agents will not leak credentials.
**Attack Scenario**: Prompt injection in a processed document instructs the agent to echo the static token to an external attacker-controlled server.
**Blast Radius**: Complete compromise of the daemon and all agent orchestration, allowing arbitrary command execution on the host machine.
**Recommended Structural Change**: Implement scoped, short-lived JWTs or mutual TLS (mTLS), and strictly enforce agent network egress scope boundaries.

**Note:** Never close this issue without a documented architectural decision record (ADR) response.
