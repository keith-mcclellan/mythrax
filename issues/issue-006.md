---
labels: architecture-review, adversarial
---
# Finding: SQLite Embedding Cache I/O Lock Contention at 10x Scale

**Current Assumption:** SQLite (`embeddings.db`) is sufficient for caching vector embeddings locally for the 6-Signal Unified Retrieval system.

**Attack Scenario:** At 10x scale (18 months forward), high-throughput embedding calculations across concurrent agent sessions will constantly hit SQLite write locks ('database is locked'), completely stalling vector ingestion and search retrieval.

**Blast Radius:** Severe latency spikes and failure to retrieve relevant context in real-time, effectively degrading the agent's cognitive memory to zero. This is a primary bottleneck for scaling.

**Recommended Structural Change:** Migrate the embedding cache to an in-memory vector store with async background persistence, or shard the SQLite database to eliminate I/O lock contention.

*Note: Never close this issue without a documented architectural decision record (ADR) response.*
