---
title: "🛡️ Sentinel: [CRITICAL] Single-Port Gateway Static Token Authentication Vulnerability"
labels: ["architecture-review", "adversarial", "bug", "agent-found"]
---

# Vulnerability Report: Single-Port Gateway Authentication

## Finding
The single-port API Gateway (handling REST and MCP on port 8090) relies on a hardcoded, static shared authentication token (`X-Mythrax-Token`) for all incoming requests.

## Current Assumption
The local network or machine environment provides an impenetrable boundary, and internal communication between the client and daemon does not require robust, dynamic authentication mechanisms.

## Attack Scenario
An attacker who gains even low-level, unprivileged access to the machine or the local network can observe traffic or extract the static token from the client or configuration. Using this token, the attacker can bypass all authentication, gain full read/write access to persistent memories, execute arbitrary commands via the MCP server, and impersonate the user system-wide.

## Blast Radius
**Systemic compromise.** Complete loss of confidentiality and integrity of the cognitive graphs, episodic memories, and agent orchestration.

## Recommended Structural Change
Implement dynamic, short-lived, user-specific API keys or tokens (e.g., JWT) with rotating secrets. Introduce Role-Based Access Control (RBAC) to differentiate permissions between read-only memory queries and potentially destructive execution endpoints.

---
*Note: Do not close this issue without a documented Architectural Decision Record (ADR) response.*