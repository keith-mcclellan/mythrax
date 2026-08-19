# Daemon Crash via In-Process MLX Engine

**Finding** -> Lightweight model tiers run in-process using the Metal GPU backend via `InProcessMlxEngine` (defined in `llm/mod.rs`).
**Current Assumption** -> In-process inference via MLX bindings is memory-safe and stable.
**Attack Scenario** -> A malformed tensor or out-of-bounds array access in the MLX bindings causes a segmentation fault.
**Blast Radius** -> The entire Mythrax daemon, including the SurrealDB memory backend, crashes instantly without graceful shutdown, violating safety safeguards.
**Recommended Structural Change** -> Isolate model execution into a separate sidecar process connected via IPC/gRPC.


**Labels**: `architecture-review`, `adversarial`

*Note: Do not close this issue without a documented architectural decision record (ADR) response.*
