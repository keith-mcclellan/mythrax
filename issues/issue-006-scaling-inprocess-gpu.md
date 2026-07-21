---
labels: ["architecture-review", "adversarial"]
---

# Issue: 18-Month Scaling Liability - Tightly Coupled In-Process GPU Inference

## Finding
The architecture leverages an "In-Process Engine" for lightweight models (like Qwen2.5-0.5B and Nomic embeddings) that runs directly within the Rust daemon process memory using the Metal GPU backend, as detailed in `ARCHITECTURE.md`.

## Current Assumption
The architecture assumes that hosting small models in-process yields performance benefits (lower latency) that outweigh the risks of shared memory spaces and coupled lifecycles. It assumes the daemon's VRAM usage will remain predictable and stable.

## Attack Scenario
1. **Accidental Denial of Service (Scale):** As the system scales 10x, processing concurrent requests across dozens of agents, the in-process Metal GPU allocations will spike. A memory leak in the Metal FFI bindings or an unusually large embedding batch will trigger an Out-Of-Memory (OOM) killer event or a kernel panic.
2. **Adversarial Exploitation:** An attacker submits crafted inputs designed to maximize context window usage or trigger pathological tensor allocations in the in-process engine. Because the engine shares the memory space with the core gateway and database management daemon, the resulting crash takes down the entire system, not just the model inference worker.

## Blast Radius
**Total System Crash.** Because the embedding and lightweight inference models run inside the core Rust daemon process, any fatal error (OOM, segfault in C/Metal bindings, panic in tensor processing) instantly terminates the entire API Gateway and persistent database connection layer. All active agent sessions are abruptly severed.

## Recommended Structural Change
1. **Out-of-Process Inference:** Decouple all model inference (including small embeddings) into isolated, dedicated worker processes or microservices (e.g., using a gRPC or ZeroMQ IPC boundary).
2. **Crash Resilience:** Ensure the core API gateway and cognitive router can survive the crash of an inference worker, gracefully queuing requests or failing over to secondary workers while the primary worker restarts.

*Note: Do not close this issue without a documented Architectural Decision Record (ADR) response.*