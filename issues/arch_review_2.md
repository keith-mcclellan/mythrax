# In-Process Engine panic and OOM risks crashing the daemon

**Labels**: `architecture-review`, `adversarial`

## Finding
The In-Process Engine loads models natively into the daemon's process memory via MLX Metal GPU backend. If MLX panics or triggers an Out-Of-Memory (OOM) error, it halts the entire process.

## Current Assumption
The Metal GPU backend can gracefully manage VRAM evictions before allocations, and the MLX C++ bindings will return errors rather than triggering fatal process aborts.

## Attack Scenario
An adversary (or just a large codebase) triggers an extremely long context completion or high-dimensional batch embedding request, exceeding the `Eviction & Sequential Swapping` guardrails. The MLX engine attempts a massive VRAM allocation, fails, and the C++ bindings throw a fatal exception or segfault.

## Blast Radius
Immediate daemon crash. Active database transactions are interrupted, temporal graph expansions are lost, and all connected MCP agents disconnect.

## Recommended Structural Change
Decouple the model execution engine from the main daemon process. Run MLX inferences in an isolated child process or sidecar container connected via IPC/gRPC, so that an OOM crash only restarts the isolated worker without killing the API gateway or database connection.
