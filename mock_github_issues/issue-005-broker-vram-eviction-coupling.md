---
title: "VRAM Eviction and Broker Coupling"
labels: ["architecture-review", "adversarial"]
---

**This issue requires a documented Architectural Decision Record (ADR) response to close.**

### Finding
VRAM Eviction and Broker Coupling

### Current Assumption
The dynamic model broker can safely manage VRAM by executing a sequential eviction loop, flushing caches, and waiting for memory release before loading new models.

### Attack Scenario
An agent rapid-fires requests that alternate between the In-Process engine (Metal GPU) and the external Model Delegation port (8080).

### Blast Radius
The sequential eviction loop is tightly coupled with the model loading logic. Rapid context switching will cause severe VRAM thrashing, race conditions between the daemon's internal state and the actual Metal driver's memory release, and eventual Out-Of-Memory (OOM) crashes.

### Recommended Structural Change
Decouple the Model Broker's state management from the inference engines. Implement a dedicated, asynchronous VRAM hypervisor service that manages a pre-allocated memory pool and rejects/queues requests strictly based on available budget, rather than relying on reactive eviction and sleeping.