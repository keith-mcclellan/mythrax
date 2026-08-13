---
title: "Finding: Unified Single-Port API Gateway without Rate Limiting"
labels: ["architecture-review", "adversarial"]
---

**Finding** -> Unified Single-Port API Gateway without Rate Limiting

**Current Assumption** -> The Gateway (port 8090) validates 'X-Mythrax-Token' and routes traffic efficiently without being overwhelmed.

**Attack Scenario** -> An attacker or rogue agent spams the gateway with millions of requests. Even if unauthorized, handling the requests blocks the async reactor or exhausts file descriptors.

**Blast Radius** -> Denial of Service (DoS) across the entire system. MCP, GUI, and background daemon operations halt.

**Recommended Structural Change** -> Implement strict per-IP and global token bucket rate limiting at the Axum gateway level. Separate administrative routes (like MCP and hooks) to a different internal-only binding (e.g., UDS or separate port).
