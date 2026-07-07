# 18-Month Scaling Flaw: Sigmoid Gated Search in local KV/SQL

**Tags:** `architecture-review`, `adversarial`

**Finding:** The architecture currently executes continuous semantic vector searches (with Sigmoid gating) locally within SQLite/SurrealKV.
**Current Assumption:** Local KV/SQL layers can efficiently handle vector math and gating logic for episodic memory retrieval.
**Attack Scenario:** As the system scales 10x, the local database layers become crippled by millions of episodic vectors. An attacker can trivially trigger a denial-of-service by running complex queries that force full table vector scans.
**Blast Radius:** Complete memory retrieval lock-up, causing agents to timeout and fail to resolve context.
**Recommended Structural Change:** Migrate to a dedicated vector database (e.g., Qdrant, Milvus) optimized for high-scale, high-dimensional similarity search.
