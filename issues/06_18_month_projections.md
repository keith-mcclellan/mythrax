# Red Team Architecture Brief: 18-Month Scaling Projections

**Labels:** `architecture-review`, `adversarial`
**Requirement to Close:** A documented architectural decision record (ADR) response must be provided.

**Finding:** The current architecture makes three critical decisions that will become catastrophic liabilities if the system scales 10x over the next 18 months.
**Current Assumption:**
1. Local key-value storage (SurrealDB engine) is sufficient.
2. A single local node can handle all agent memory concurrency.
3. A strict token budget (e.g., `token_budget` passed in `mythrax-core/src/main.rs` and `mythrax-core/src/db/backend/mod.rs`) is viable for cognitive synthesis loops.
**Attack Scenario:**
1. The database reaches massive sizes, causing disk IO bottlenecks.
2. 1,000+ concurrent agents cause DB lock contention.
3. Complex multi-repository projects exceed the token budget synthesis cap, causing the agent to silently drop critical context and make destructive codebase changes.
**Blast Radius:** Severe performance degradation and irrecoverable cognitive amnesia at scale.
**Recommended Structural Change:**
1. Migrate embeddings to a scalable vector database (e.g., Qdrant/Milvus).
2. Transition to a horizontally distributed database cluster.
3. Replace bounded token synthesis with iterative multi-agent consensus (GraphRAG) that summarizes hierarchically without strict linear limits.
