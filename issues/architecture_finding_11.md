---
title: "18-Month Re-Architecture Liability 2: In-Process MLX Engine to Dedicated Inference Cluster"
labels: ["architecture-review", "adversarial"]
---

## Finding
Mythrax heavily utilizes an in-process MLX engine for dense models to reduce latency.

## Current Assumption
Running models in-process on unified Metal GPU memory is the most efficient design.

## Attack Scenario
As agent workloads increase 10x, the in-process model execution will compete with the API gateway and cognitive pipeline for CPU/RAM resources, leading to OOM crashes and severe latency spikes. The manual VRAM tracking and `.eval()` invariants will become unmanageable.

## Blast Radius
Daemon crashes and severe inference latency.

## Recommended Structural Change
Extract the model broker into a dedicated, out-of-process inference microservice cluster (e.g., using vLLM or specialized inference servers) to isolate resource consumption and allow independent scaling.

> Note: This issue requires a documented architectural decision record (ADR) response before it can be closed.
