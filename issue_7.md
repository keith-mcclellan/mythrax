# 18-Month Scaling Flaw: Vault Watcher and VRAM Sequential Eviction

**Tags:** `architecture-review`, `adversarial`

**Finding:** The Obsidian Vault watcher uses a static 500ms sliding window, and VRAM management uses sequential model eviction and swapping on a single node.
**Current Assumption:** Raw filesystem polling with a 500ms window and sequential VRAM swapping on consumer hardware will scale adequately.
**Attack Scenario:** At 10x scale (tens of thousands of files and concurrent multi-agent workloads), filesystem polling causes CPU thrashing and dropped events. Simultaneous large model requests create a VRAM swapping bottleneck, completely stalling inference.
**Blast Radius:** Silent loss of vault synchronization and massive multi-agent inference latency, effectively destroying the sidecar daemon's real-time capabilities.
**Recommended Structural Change:** Replace filesystem polling with an event-driven message queue (e.g., Redis PubSub or Kafka). Replace monolithic local broker VRAM swapping with a distributed model routing layer (e.g., vLLM or Ray Serve) across multiple nodes.
