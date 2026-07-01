---
title: "Unified Single-Port Router is a Single Point of Failure and Security Risk"
tags: [architecture-review, adversarial]
---

# Finding: Unified Single-Port Router is a Single Point of Failure and Security Risk

## Current Assumption
Consolidating all endpoints (Admin, Memory, MCP, and external completions proxy) onto a single port (8090) guarded by a single static `X-Mythrax-Token` is sufficient for a local daemon.

## Attack Scenario
A local privilege escalation or cross-site request forgery (CSRF) if the local agent exposes any web interface. If the single token is compromised (or hardcoded, as seen in HARD-001), the attacker gains full control over agent memory, MCP tool execution, and the ability to route arbitrary model completions. A resource exhaustion attack on the completions endpoint (which can take seconds) starves the control plane.

## Blast Radius
System Takeover & Resource Exhaustion. The entire daemon—both data plane and control plane—fails simultaneously if connection pools are exhausted, and a single token compromise yields root-level agent access.

## Recommended Structural Change
Split the monolithic port into a Control Plane (admin, config) and a Data Plane (MCP, memory, completions). Implement scoped, short-lived JWTs with principle-of-least-privilege (e.g., a token that can only query memory, not write it, and cannot change config).

*This issue requires an ADR response to close.*
