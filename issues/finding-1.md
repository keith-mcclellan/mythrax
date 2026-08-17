---
title: "Finding 1: Single-Port API Gateway (Port 8090) lacks isolation"
labels: ["architecture-review", "adversarial"]
---

**Finding**: The Single-Port API Gateway combines administrative, memory, Model Context Protocol (MCP), and transparent completions proxy endpoints onto a unified router on Port 8090.

**Current Assumption**: The assumption is that static auth token validation via `X-Mythrax-Token` headers is sufficient to isolate and protect sensitive administrative or memory endpoints from external unprivileged or agent traffic, while maintaining high performance.

**Attack Scenario**: An attacker or a compromised agent could leak the shared static auth token (e.g., through prompt injection reading environment variables or configuration files). Once leaked, the attacker has unrestricted access to all endpoints on port 8090, including the ability to manipulate memory, administrative configs, and MCP operations. There is no network-level segregation between control-plane and data-plane traffic.

**Blast Radius**: Total system compromise. Full access to memory, ability to manipulate agent context, modify configurations, and potentially achieve remote code execution if MCP operations allow shell access.

**Recommended Structural Change**: Separate the control plane (administrative, memory, configuration) from the data plane (MCP, completions) by binding them to different ports or network interfaces. Introduce role-based access control (RBAC) with distinct, short-lived, or scoped tokens for different operational boundaries rather than a single shared static token.
