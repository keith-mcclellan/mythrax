---
title: "Architecture Review: OOM Risk in In-Process Metal GPU Engine"
labels: ["architecture-review", "adversarial"]
---

**Finding:** Lack of backpressure for In-Process model execution leading to Out-Of-Memory (OOM) crashes.

**Current Assumption:**
As documented in `ARCHITECTURE.md`: "In-Process Engine: Lightweight dense models (e.g., Nomics embeddings and the Qwen2.5-0.5B/1.5B/7B family) are loaded natively into the Rust process memory and run in-process using the Metal GPU backend." The assumption is that because these models are "lightweight," they can be loaded and executed natively without threatening the host process's memory stability.

**Attack Scenario:**
An adversary (or a sudden spike in legitimate heavy cognitive load) submits a high volume of concurrent requests that require small dense model execution (e.g., massive parallel embedding extraction or rapid lightweight inference). Because these models run in-process and Metal utilizes shared system memory, the rapid allocation of tensors and context windows exhausts available RAM/VRAM before the sequential eviction loop or semaphores can adequately throttle the allocations.

**Blast Radius:**
Because the execution is in-process, an OOM event triggered by the Metal backend does not just crash an isolated worker; it causes the entire Mythrax 3.0 Core Daemon to crash. This brings down the Single-Port Gateway, halting all routing, memory access, and persistence operations.

**Recommended Structural Change:**
Implement memory pressure backpressure specifically for in-process models. Enforce strict, bounded concurrency queues for Metal GPU engine inference and embeddings, rejecting or queuing requests before memory allocation begins if available VRAM/RAM drops below a safe threshold.
