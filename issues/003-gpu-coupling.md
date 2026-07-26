---
labels: ["architecture-review", "adversarial"]
---

# Adversarial Architecture Review: Tight Coupling Between Cognitive Pipeline and MLX Inference

## Finding
The cognitive pipeline (specifically the daily dreaming compactor, DBSCAN clustering, and vector embedding calculations) is tightly coupled to the in-process MLX GPU inference engine. The architecture dictates that lightweight dense models (like Nomic embeddings) run natively in-process using the Metal GPU backend.

## Current Assumption
The architecture assumes that running inference in-process provides ultra-fast, zero-latency execution necessary for the heavy cognitive load of clustering and synthesis, and that VRAM safeguards (`.eval()` invariants, eviction) are sufficient to prevent instability.

## Attack Scenario
While not a traditional "hack," this is an architectural fragility. If the MLX library encounters an unhandled exception, a malformed tensor allocation, or a sudden spike in context size that bypasses the manual `.eval()` checks, it will trigger a fatal panic or segmentation fault. Because the inference engine is coupled directly to the core daemon process (handling API routes, database transactions, and file watching), a failure in the model execution instantly crashes the entire system.

## Blast Radius
**High.** Any GPU or tensor error takes down the entire daemon. The system cannot independently deploy, test, or replace the embedding/inference engine without modifying and restarting the core daemon. The API Gateway drops all requests, and in-flight cognitive compactions are aborted.

## Recommended Structural Change
1. **Out-of-Process Inference:** Decouple the MLX inference engine from the core daemon. Move all in-process model execution (including embeddings) to a dedicated local microservice or separate process managed by the daemon.
2. **gRPC/IPC Communication:** Use a fast IPC mechanism or gRPC for communication between the core daemon and the inference worker. This ensures that an OOM killer or segfault in the inference worker only requires restarting the worker, while the core daemon remains alive to gracefully handle the error and queue pending tasks.
