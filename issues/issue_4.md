# [Red Team] Tight Coupling: Shared SQLite Embedding Cache and In-Memory Limits

**Labels:** architecture-review, adversarial

**Finding:** Mythrax 3.0 uses a shared SQLite embedding cache (`embeddings.db`) bounded by FIFO eviction based on maximum capacity constraints (using `created_at`) to prevent OOM spikes.

**Current Assumption:** FIFO eviction based on capacity limits is sufficient to maintain memory safety invariants and prevent OOM regardless of the embedding models used.

**Attack Scenario:**
1. The system scales to support multiple concurrent projects or multiple concurrent subagents via the agent orchestration design.
2. The SQLite embedding cache is tightly coupled to a single shared daemon instance. Under load, multiple agents flood the cache with new embeddings.
3. Because the eviction relies purely on `created_at` across the global instance rather than per-project or per-agent quotas, a noisy neighbor agent can force the eviction of all active working memory embeddings of another agent.
4. When the starved agent needs its embeddings, it experiences massive latency spikes as it recomputes them, triggering the GPU semaphore lock and halting the other agents.

**Blast Radius:**
- System-wide deadlock or severe latency degradation under multi-agent load.
- Thrashing of the Metal GPU semaphore.
- Failure of the "graceful degradation path" since there is no isolated failure domain per agent.

**Recommended Structural Change:**
- Decouple the embedding cache architecture from a single global SQLite database. Implement partitioned caching (e.g., per-workspace or per-session isolated caches).
- Introduce multi-tenant fair-share scheduling for the Metal GPU semaphores to prevent starvation.
- Document the migration from a monolithic cache to a partitioned architecture via an ADR.