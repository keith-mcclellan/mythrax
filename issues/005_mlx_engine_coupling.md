---
title: "Coupling Liability: In-Process MLX Engine Crashes Daemon"
labels: ["architecture-review", "adversarial"]
---

## Finding
The In-Process Engine loads small dense models natively into the daemon's process memory using the MLX Metal GPU backend. If the MLX C++ bindings panic, OOM, or if an `.eval()` call is missed leading to a unified memory leak, it abruptly crashes the entire Mythrax daemon.

## Current Assumption
The assumption is that running small models (e.g., 0.5B-7B) in-process provides ultra-low latency and avoids IPC overhead, and that the MLX bindings are stable enough to share the same process space as the database and API gateway.

## Attack Scenario
An attacker sends a specifically crafted sequence of requests designed to cause an MLX assertion failure (e.g., edge-case tensor shapes, NaN propagation) or intentionally overwhelms the unified memory by skipping `eval()` boundaries. The resulting panic in the native MLX C++ layer instantly terminates the host daemon process, bypassing all Rust panic catchers.

## Blast Radius
**Critical.** Complete daemon termination. An inference-level crash brings down the persistent database connections, drops all active REST/MCP client connections, and halts all background compaction/watcher loops.

## Recommended Structural Change
1. **Process Isolation:** Decouple the MLX inference engine into a dedicated worker process (e.g., `mythrax-inference-worker`) that communicates with the core daemon via gRPC or local IPC.
2. **Supervisor Pattern:** If the inference worker crashes, the core daemon remains unaffected and can simply spawn a replacement worker and retry the request.
3. **Type-Level Enforcement for `.eval()`:** Abstract all MLX array operations behind a Rust wrapper that uses the type system to enforce `.eval()` before allowing buffer extraction or prolonged retention.