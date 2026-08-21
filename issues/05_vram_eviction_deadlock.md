# Red Team Architecture Brief: VRAM Eviction Deadlock (Single Point of Failure)

**Labels:** `architecture-review`, `adversarial`
**Requirement to Close:** A documented architectural decision record (ADR) response must be provided.

**Finding:** The Model Broker manages GPU VRAM eviction in-process, relying on `metal_gpu_semaphore` and `metal_embedding_semaphore` (in `mythrax-core/src/llm/mod.rs` and `mythrax-core/src/llm/mxbai_mlx.rs`) for safe execution.
**Current Assumption:** Models will cleanly evict and release VRAM sequentially under heavy load without blocking the tokio runtime.
**Attack Scenario:** Two parallel MCP agents spam embedding and inference requests simultaneously. The model inference stalls, causing a memory leak or a semaphore deadlock on `metal_embedding_semaphore`. Since the broker is in-process, the entire API gateway freezes.
**Blast Radius:** Total daemon Denial of Service (DoS). No graceful degradation path exists; the entire system goes down and requires a hard kill.
**Recommended Structural Change:** Extract the model inference engine into a fully isolated sidecar process communicating via gRPC/IPC, ensuring that VRAM OOMs or MLX crashes do not take down the unified memory gateway.
