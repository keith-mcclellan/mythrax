---
title: "Architecture Liability: Unbounded LLM Proxy Interception Risk"
labels: ["architecture-review", "adversarial"]
---

**Finding** -> The Proxy Mode actively intercepts OpenAI-compliant requests and injects status checks and formatting dynamically.

**Current Assumption** -> Intercepted payloads are well-formed and bounded in size, and the proxy layer can process them faster than they arrive.

**Attack Scenario** -> An adversary (or malfunctioning agent) sends massively nested or infinitely recursive payloads to the completions proxy. The proxy attempts to parse and format them, leading to CPU exhaustion or OOM.

**Blast Radius** -> Denial of Service (DoS) of the core daemon, preventing all agents from functioning.

**Recommended Structural Change** -> Implement strict payload size limits, request timeouts, and streaming passthrough with minimal buffering. Use a circuit breaker for upstream LLM timeouts.

*Note: Do not close this issue without a documented architectural decision record (ADR) response.*
