---
title: "Finding: Model Broker Unloading Models before Loading Rerankers"
labels: ["architecture-review", "adversarial"]
---

**Finding** -> Model Broker Unloading Models before Loading Rerankers

**Current Assumption** -> Unloading embedding models to free VRAM for reranking models prevents OOM crashes on constrained hardware.

**Attack Scenario** -> An agent rapidly interleaves small embedding requests with reranking requests. The system spends 99% of its time swapping multi-gigabyte models in and out of VRAM rather than doing useful work.

**Blast Radius** -> Complete pipeline stall. Throughput drops to near zero due to model thrashing.

**Recommended Structural Change** -> Implement batch-window buffering for embeddings and reranking tasks, or require agents to specify their operational phase to prevent thrashing. Introduce a VRAM usage quota per pipeline.
