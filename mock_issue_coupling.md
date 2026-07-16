---
title: "Architectural Liability: Coupling of Daemon and In-Process Model Engine"
labels: ["architecture-review", "adversarial"]
status: "open"
---

## 🛑 Finding: Coupling of Daemon and In-Process Model Engine

**Finding:** The daemon is tightly coupled to the MLX (Local Apple Silicon) execution pipeline for dense models. `ARCHITECTURE.md` states: "In-Process Engine: Lightweight dense models... are loaded natively into the Rust process memory and run in-process using the Metal GPU backend."

**Current Assumption:** Running lightweight models in-process using Apple's Metal GPU backend provides the lowest latency and optimal performance without compromising system stability.

**Attack Scenario:** A malformed payload or extremely long context window is passed to an in-process embedding or inference model. Due to an edge-case bug in the Metal FFI bindings or the `mlx` integration, the model execution triggers a segfault, OOM kill, or Rust panic.

**Blast Radius:** Complete loss of memory persistence and routing. Because the model engine runs natively within the same process memory as the core daemon (port 8090 API gateway, SurrealDB locks, etc.), a failure in the model execution takes down the entire daemon. The two modules cannot be independently deployed, tested, or scaled.

**Recommended Structural Change:** Decouple the local inference engine from the core daemon. Move the "In-Process Engine" into a separate sidecar process or worker pool that communicates via IPC or gRPC. This allows the core daemon to gracefully handle inference crashes (by restarting the worker) without dropping API connections or losing database state.
