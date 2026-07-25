---
title: "Tightly-Coupled In-Process GPU Inference"
labels: [architecture-review, adversarial]
---

**Finding:** Tightly-Coupled In-Process GPU Inference via MLX.

**Current Assumption:** Loading lightweight models (0.5B/1.5B/7B) natively into the Rust process using the Metal GPU backend provides optimal ultra-fast local inference.

**Attack Scenario:** A malformed prompt or model crash causes an Out-Of-Memory (OOM) or segment fault within the in-process Metal backend.

**Blast Radius:** Daemon crash. Because inference is tightly coupled to the main Rust process, a GPU crash brings down the API Gateway, file watcher, and DB ingestion simultaneously.

**Recommended Structural Change:** Extract local MLX model execution to an independent sidecar process. Communicate over IPC/gRPC so crashes do not take down the entire daemon.
