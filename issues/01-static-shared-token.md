---
title: "Single-Port Gateway with Static Shared Token Authentication"
labels: [architecture-review, adversarial]
---

**Finding:** Single-Port Gateway with Static Shared Token Authentication.

**Current Assumption:** A static X-Mythrax-Token shared via headers is sufficient for internal orchestration authentication without multi-tenant or rotating credentials.

**Attack Scenario:** Token leakage via logs, compromised plugins, or side-channel. An attacker uses the token to access port 8090, issuing arbitrary commands or manipulating the cognitive graph.

**Blast Radius:** Complete system compromise. Since the token is statically shared and the gateway orchestrates all components, an attacker gains RCE and full data exfiltration.

**Recommended Structural Change:** Implement ephemeral JWTs or mTLS for service-to-service authentication. Remove the single static token reliance and introduce scoped RBAC.
