---
title: "In-Process MLX Model Broker Coupling and Daemon Stability"
labels: ["architecture-review", "adversarial"]
---

# Red Team Architecture Brief

**Finding:**
Lightweight dense models (e.g., Nomic embeddings and Qwen2.5) are loaded natively into the Mythrax daemon's process memory and run in-process using the MLX Metal GPU backend.

**Current Assumption:**
In-process loading reduces latency and IPC overhead, leading to faster inference times, and that MLX graphs are perfectly managed via explicit `.eval()` calls.

**Attack Scenario:**
An adversarial input or edge-case context size triggers an Out-Of-Memory (OOM) error, a memory leak, or a segmentation fault within the MLX C++ bindings or Metal driver. Because the execution is in-process, this crash immediately brings down the entire Mythrax daemon, dropping in-flight HTTP connections, corrupting pending database transactions, and halting cognitive loops.

**Blast Radius:**
Full daemon unavailability, potential memory corruption, and loss of volatile state. There is no graceful degradation path; a failure in model inference causes a system-wide failure.

**Recommended Structural Change:**
Decouple model execution into isolated, standalone worker processes communicated via gRPC, IPC, or local HTTP. This ensures that an inference engine crash only restarts the worker process without taking down the core daemon and database gateway.
