---
title: "🛡️ Sentinel: [CRITICAL] Tightly-Coupled In-Process GPU Inference Resource Exhaustion"
labels: ["architecture-review", "adversarial", "bug", "agent-found"]
---

# Vulnerability Report: In-Process Inference Coupling

## Finding
The Model Broker loads lightweight dense models natively into the Rust process memory and executes them using the Metal GPU backend. The inference engine shares the same memory space as the core Mythrax daemon.

## Current Assumption
Consumer hardware has sufficient resources, and the VRAM eviction loop will act fast enough to prevent Out-Of-Memory (OOM) crashes during inference.

## Attack Scenario
An adversary feeds large, complex, or malformed inputs designed to cause the in-process models to rapidly spike memory usage. This overwhelms the VRAM eviction loop before it can successfully evict unused models. Because the inference shares the memory space with the daemon, the entire Rust process panics and crashes.

## Blast Radius
**Complete daemon crash.** Disruption of all active agent sessions, memory operations, background compaction, and the API gateway. The system cannot gracefully degrade.

## Recommended Structural Change
Isolate all model inference into a separate child process or external microservice. Communication should occur via IPC or gRPC, ensuring that inference-related OOMs or panics are contained and do not bring down the core intelligence daemon.

---
*Note: Do not close this issue without a documented Architectural Decision Record (ADR) response.*