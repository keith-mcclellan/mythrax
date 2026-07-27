---
title: "Red Team Architecture Brief: Tight Coupling in GPU Broker and MLX Evaluation"
labels: ["architecture-review", "adversarial"]
---

### Finding
The Model Broker manages in-process Metal GPU inference alongside HTTP external delegation, and enforces mandatory `.eval()` calls on MLX graphs prior to buffer access or storage.

### Current Assumption
The assumption is that coupling GPU memory management (VRAM Eviction, Split GPU Semaphores) with the application logic (Cognitive Pipeline) provides maximal performance and simplifies deployment on macOS/Apple Silicon.

### Attack Scenario
If the Metal GPU backend hangs, panics during an `.eval()` due to an unexpected tensor shape (e.g., from malformed data bypassing the prompt budget), or exhausts VRAM despite the eviction logic, the entire Mythrax 3.0 Core Daemon crashes. This brings down the storage engine (SurrealKV), the API Gateway, and the file watcher, since they are all tightly coupled within the same process boundary.

### Blast Radius
High. Complete system crash. The inability to independently scale, restart, or failover the inference engine without impacting the persistent storage and gateway pipelines is a critical architectural liability.

### Recommended Structural Change
1. **Out-of-Process Inference Engine**: Decouple the MLX/Metal inference engine into a separate, isolated process (or sidecar) communicating via gRPC or shared memory. If the inference process crashes, the core daemon can restart it gracefully without losing connection pools, HTTP state, or disrupting database writes.