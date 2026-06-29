---
title: "Architecture Review: Hardcoded Fallback Authentication Token"
labels: ["architecture-review", "adversarial"]
---

### Finding
Hardcoded Fallback Authentication Token ("secret-token")

### Current Assumption
Providing `"secret-token"` as a fallback string ensures the daemon can authenticate if the `~/.mythrax/token` file is missing.

### Attack Scenario
An attacker probes the single-port gateway on 8090 using the header `X-Mythrax-Token: secret-token`. Because this fallback is compiled statically into the binary across multiple modules, the attacker immediately gains full administrative access to the daemon.

### Blast Radius
Total system compromise. The attacker can read all private memories, manipulate the config, and execute arbitrary commands via the MCP gateway.

### Recommended Structural Change
Remove the hardcoded fallback token immediately. If no token file exists, securely generate a cryptographically random token on startup, write it to `~/.mythrax/token` with strict 0600 permissions, and enforce its use.

> **Note:** Do not close this issue without a documented Architectural Decision Record (ADR) response.