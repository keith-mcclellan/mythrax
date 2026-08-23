---
title: "Authentication Boundary Vulnerability: Shared Static Token Limits Revocation and Rotation"
labels: ["architecture-review", "adversarial"]
---

## Finding
Mythrax uses a single shared static token passed via the `X-Mythrax-Token` header for API gateway and MCP authentication.

## Current Assumption
A single shared static token provides sufficient security for local or managed environments where all requests are trusted or originate from the same user context.

## Attack Scenario
If the single token is compromised (e.g., leaked via a log, environment variable dump, or compromised agent), an attacker gains full access to all memory, cognitive functions, and model routing APIs. Since it's a shared static token, it cannot be easily rotated or revoked without breaking all existing, legitimate agent sessions simultaneously.

## Blast Radius
System-wide compromise. Unauthorized access to all APIs, memories, and administrative endpoints. Data exfiltration, corruption, or denial of service.

## Recommended Structural Change
Implement short-lived, dynamically generated, and agent-specific access tokens (e.g., JWTs) issued via a robust authentication mechanism (like mTLS or OIDC). Introduce an API gateway with role-based access control (RBAC) to restrict agent capabilities.

> Note: This issue requires a documented architectural decision record (ADR) response before it can be closed.
