---
title: "Adversarial Architecture Review: Single-Port Gateway & Hardcoded Auth Token"
labels: architecture-review, adversarial
---

**Finding:** Single-Port API Gateway relies on a shared static auth token, and the codebase contains a hardcoded fallback token ("secret-token").
**Current Assumption:** The Gateway correctly validates all REST and MCP requests via `X-Mythrax-Token` and clients will securely provide a generated token.
**Attack Scenario:** An attacker discovers the hardcoded fallback token `secret-token` (as noted in MOCK-001/HARD-001) or leverages SSRF to bypass the authentication boundary, gaining full administrative, memory, and cognitive proxy access to the single port 8090.
**Blast Radius:** Complete compromise of the Mythrax daemon, including arbitrary command execution (via `mythrax exec` subcommand), arbitrary memory injection (prompt injection persistence), and unauthorized data exfiltration.
**Recommended Structural Change:** Remove the hardcoded fallback token entirely. Implement mutual TLS (mTLS) or short-lived JWTs instead of static shared tokens. Split the administrative/MCP control plane from the external API proxy plane onto separate isolated ports or Unix domain sockets.

Never close without a documented architectural decision record (ADR) response.
