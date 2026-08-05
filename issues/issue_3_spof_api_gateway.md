---
title: "Adversarial CTO: Single Point of Failure in Unified API Gateway & Shared Auth"
labels: ['architecture-review', 'adversarial']
---

## Finding
Single Point of Failure in Unified API Gateway & Shared Static Auth

## Current Assumption
Consolidating REST, Model Context Protocol (MCP), and transparent proxy routes onto a single port (8090) with a shared static auth token (`X-Mythrax-Token`) simplifies the architecture without compromising security.

## Attack Scenario
An attacker leaks, extracts (from hardcoded values), or brute-forces the single static `X-Mythrax-Token`. Since it is a unified credential guarding all administrative, routing, and memory endpoints, the attacker gains full control over the system. Furthermore, any crash or lockup in this single API Gateway process brings down all decoupled endpoints simultaneously.

## Blast Radius
Total system compromise and data exfiltration. In the event of a gateway crash, complete downtime across all cognitive and routing functions with no graceful degradation path.

## Recommended Structural Change
Decouple the administrative and MCP endpoints into separate services or ports. Replace the single static token with dynamic, cryptographically signed JWTs that are scoped strictly by role and subsystem. Never close this issue without a documented ADR response.
