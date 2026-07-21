---
labels: ["architecture-review", "adversarial"]
---

# Issue: 18-Month Scaling Liability - Static Model Routing and Eviction Thrashing

## Finding
The Model Broker uses a static, rule-based approach to route models (in-process vs. external port 8080) and employs sequential VRAM eviction before loading a new model, as documented in `ARCHITECTURE.md`.

## Current Assumption
The architecture assumes a low-concurrency environment where models can be swapped sequentially without significantly impacting the user experience. It assumes static routing rules are sufficient and that the host machine has enough unified memory to handle the working set, provided one model is evicted before another loads.

## Attack Scenario
1. **Accidental Denial of Service (Scale):** At 10x scale, multiple agents will concurrently request different models (e.g., Agent A needs Nomic for search, Agent B needs Qwen3.6 for synthesis, Agent C needs Qwen0.5B for routing). The static sequential eviction loop will cause catastrophic "eviction thrashing." The system will spend 95% of its time loading and unloading multi-gigabyte weights from disk to VRAM, reducing throughput to near zero.
2. **Adversarial Exploitation:** An attacker intentionally alternates requests that force model swaps (e.g., search -> synthesis -> search). By exploiting the sequential eviction lock, a single malicious agent can block all other agents on the server indefinitely.

## Blast Radius
**System Paralysis via I/O Wait.** The Model Broker's sequential swapping acts as a global lock. Eviction thrashing will cause the daemon's response times to spike from milliseconds to minutes, triggering client timeouts and effectively paralyzing all cognitive capabilities across the entire system.

## Recommended Structural Change
1. **Dynamic Model Multiplexing & KV Cache Sharing:** Implement dynamic batching (e.g., via vLLM) and paged KV caches to serve multiple requests concurrently without swapping weights.
2. **Dedicated Model Instances / Sharding:** At 10x scale, the architecture must support horizontal scaling, routing requests to dedicated inference nodes that keep specific models perpetually loaded in memory, eliminating on-the-fly eviction swapping entirely.

*Note: Do not close this issue without a documented Architectural Decision Record (ADR) response.*