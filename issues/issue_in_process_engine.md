---
tags: [architecture-review, adversarial]
---
# Finding: In-Process Engine (Metal GPU backend)

**Current Assumption:** Small models can safely run in-process without destabilizing the host process, reducing latency.

**Attack Scenario:** A malformed prompt or adversarial context window triggers a panic, memory corruption, or OOM in the MLX/C++ bindings (e.g., missing `.eval()` calls on lazy arrays).

**Blast Radius:** The entire `mythrax-core` daemon crashes instantly, dropping all in-flight asynchronous tasks, ephemeral states, and taking down the API gateway.

**Recommended Structural Change:** Isolate the in-process MLX engine into a separate sidecar process communicating via IPC, ensuring model crashes do not take down the control plane.
