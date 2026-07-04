---
labels: ["architecture-review", "adversarial"]
---

# Red Team Architecture Brief: VRAM Thrashing via Model Broker Eviction

**Finding:** The Three-Tiered Model Broker uses a sequential eviction loop to prevent OOM crashes on consumer hardware, explicitly evicting unused models before loading a new one into VRAM.

**Current Assumption:** Sequential eviction is fast enough and requests are interleaved in a way that avoids pathological swapping, making large multi-model flows viable on constrained hardware.

**Attack Scenario:** An attacker (or complex multi-agent flow) rapidly alternates requests between an in-process dense model (e.g., Qwen2.5-1.5B for structural routing) and an external large hybrid model (e.g., Qwen3.6-35B for complex reasoning) on every turn. The broker constantly evicts and reloads multi-gigabyte models into VRAM to satisfy alternating requests.

**Blast Radius:** Severe performance degradation (VRAM thrashing). The daemon becomes essentially unresponsive as >95% of processing time is spent loading/unloading weights from disk to GPU memory rather than performing inference. The system grinds to a halt without officially crashing.

**Recommended Structural Change:** Implement a VRAM caching heuristic with a minimum Time-To-Live (TTL) for loaded models and a request queuing system. If a model is scheduled for eviction but was recently used, hold it and queue incoming conflicting requests, or reject them with a "VRAM busy" error to force client-side backoff instead of thrashing. Require a mandatory ADR response to close this issue.