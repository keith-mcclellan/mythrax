---
title: "SPOF & Auth Bypass: Single-Port API Gateway & Static Auth Token"
labels: ["architecture-review", "adversarial"]
---

# Finding: Single-Port API Gateway & Static Auth Token

## Current Assumption
A static `X-Mythrax-Token` is sufficient to protect port 8090 on localhost, assuming the local environment is sterile and inaccessible.

## Attack Scenario
Local malware or a Cross-Site Request Forgery (CSRF) attack from a local browser executes a POST to `http://127.0.0.1:8090/v1/mcp/call`. Because the token is stored in plaintext (`~/.mythrax/token`) and defaults to a hardcoded `"secret-token"` fallback (identified in the mock audit), the attacker trivially bypasses authentication.

## Blast Radius
Full compromise of agent memory, database deletion, and malicious model instruction injection via the proxy port.

## Recommended Structural Change
Deprecate the static token and TCP port 8090. Migrate to OS-level Unix Domain Sockets with strict user/group file permissions to eliminate network-layer vectors.

**Status:** Requires Architectural Decision Record (ADR) response to close.