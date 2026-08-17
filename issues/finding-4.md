---
title: "Finding 4: Tight Coupling Between Model Broker and VRAM Eviction State"
labels: ["architecture-review", "adversarial"]
---

**Finding**: The Model Broker manages VRAM Eviction and Sequential Swapping by directly evicting unused models and flushing caches before loading a new model. The 5-Second Graceful Shutdown Sequence also directly relies on `broker.evict_unused_models()`.

**Current Assumption**: The assumption is that the Model Broker is the sole authority over VRAM lifecycle and that eviction can always complete synchronously or quickly enough to fit within load sequences and a 5-second shutdown window.

**Attack Scenario**: If the Model Broker becomes tightly coupled to specific GPU frameworks (MLX/ORT), testing or replacing the model execution layer becomes impossible without rewriting the eviction logic. Furthermore, if the VRAM flush hangs or takes longer than 5 seconds under heavy load (e.g., adversarial rapid model switching requests), the daemon will forcefully terminate, potentially corrupting active in-flight database transactions or leaving the GPU in a locked state (semaphores not released).

**Blast Radius**: Unpredictable daemon termination, corrupted database state due to aborted transactions, and deadlocked GPU semaphores requiring manual host machine restarts. Inability to independently deploy or test the model execution layer without the VRAM management layer.

**Recommended Structural Change**: Decouple VRAM management from the Model Broker into an independent Resource Manager module with a defined interface. Implement asynchronous, timeout-bounded eviction with guaranteed fallback (e.g., process-level isolation for model runners so the OS reclaims memory upon worker termination).
