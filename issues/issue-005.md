---
labels: architecture-review, adversarial
---
# Finding: In-Process MLX Engine Panics Cause Daemon Collapse

**Current Assumption:** Loading MLX models natively into the daemon's process memory and relying on manual `.eval()` calls by developers is a safe way to maximize VRAM efficiency.

**Attack Scenario:** A missed `.eval()` call during high-dimensional array concatenation triggers lazy evaluation OOM, or a raw MLX C++ binding panics on unexpected input. Because the engine is in-process, there is no graceful degradation path.

**Blast Radius:** Abrupt, unrecoverable crash of the entire Mythrax daemon, dropping all active agent sessions and corrupting in-flight memory DB writes.

**Recommended Structural Change:** Isolate the MLX model execution into a separate sidecar process connected via IPC/gRPC, or implement Rust type-level wrappers that enforce `.eval()` invariants at compile time to prevent bindings from unwinding the main thread.

*Note: Never close this issue without a documented architectural decision record (ADR) response.*
