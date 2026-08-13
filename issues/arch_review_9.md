---
title: "Finding: 18-Month Re-architecture Liability - Token-Based Sliding Windows"
labels: ["architecture-review", "adversarial"]
---

**Finding** -> 18-Month Re-architecture Liability - Token-Based Sliding Windows

**Current Assumption** -> Bounding tool sequences to 1,000 elements and prompt history by token limits prevents OOM.

**Attack Scenario** -> At 10x scale, long-running agents (running for days/weeks) will exceed these hardcoded sliding windows, leading to catastrophic forgetting of critical early constraints. The current architecture has no hierarchical summarization for the sliding window itself.

**Blast Radius** -> Agents lose critical context, leading to repetitive loops and failure to complete long-horizon tasks.

**Recommended Structural Change** -> Implement a hierarchical, continuously updating summary of evicted context (e.g., MemGPT-style tiered memory) rather than simple truncation/sliding windows.
