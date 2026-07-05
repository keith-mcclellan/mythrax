---
title: "Architecture Review: Model Broker Fallback SPOF and Silent Failures"
labels: ["architecture-review", "adversarial"]
---

# Red Team Architecture Brief

**Finding:** The Model Broker acts as a Single Point of Failure (SPOF) due to a lack of genuine graceful degradation. Instead of propagating hardware or VRAM exhaustion errors properly, the codebase silently fabricates a mock engine (`"fallback-cpu-model"` with `warmed_up: true`) on acquisition failure (as identified in `MOCK-006`).

**Current Assumption:** The architecture assumes that if the Metal GPU backend or external HTTP completions fail, pretending the model is loaded on the CPU is a safe degradation path, and that clients can handle whatever output this "dummy" model produces.

**Attack Scenario:** Under heavy concurrent load or adversarial memory pressure, an attacker (or simply a high volume of requests) exhausts VRAM or locks the `METAL_INFERENCE_SEMAPHORE`. The system fails to acquire the real model and switches to the silent dummy fallback. The AI agent, unaware of the failure, receives garbage or empty outputs from the mocked engine, leading to cascading logic failures or infinite loops as it retries to obtain a meaningful answer.

**Blast Radius:** Complete loss of cognitive function for all dependent agents. Because the error is swallowed and mocked, monitoring tools will report the daemon as healthy while it silently feeds broken inferences to agents, causing data corruption in the Short-Term Memory (STM) and vault.

**Recommended Structural Change:**
- Eliminate silent mock fallbacks in production (`MOCK-004`, `MOCK-005`, `MOCK-006`).
- Implement a true graceful degradation path: if the GPU model fails to load, explicitly route to a configured, real CPU-based ONNX fallback (ORT backend) or return a hard `503 Service Unavailable` so the client can apply backpressure and retry strategies.

*ADR required to close this issue.*