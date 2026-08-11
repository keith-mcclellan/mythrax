---
title: "Red Team Architecture Brief: Model Broker VRAM Eviction Deadlock"
labels: [architecture-review, adversarial]
---

### Finding
The VRAM eviction process (`broker.evict_unused_models()`) relies on sequential flushing and waiting for memory release before loading new models.

### Current Assumption
The system assumes that memory release will always happen predictably and that eviction operations will never hang or panic.

### Attack Scenario
Under high load, an adversarial barrage of requests forcing rapid context switching between models could cause eviction to hang due to OS-level unified memory constraints or lock contention on `METAL_INFERENCE_SEMAPHORE` / `METAL_EMBEDDING_SEMAPHORE`.

### Blast Radius
Complete node deadlock. If eviction hangs, no new models can load. The entire pipeline stalls, halting processing.

### Recommended Structural Change
Introduce a timeout and hard-reset fallback for VRAM eviction. Decouple inference tracking from memory allocation limits so the broker can preemptively load smaller mock models or shed load when VRAM constraints are violated.
