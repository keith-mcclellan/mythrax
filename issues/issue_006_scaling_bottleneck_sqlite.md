# 18-Month Scaling: SQLite Embedding Cache & Pipeline Ephemeral State Bloat

**Labels:** architecture-review, adversarial

**Finding:** The architecture relies on a local SQLite Embedding Cache (`embeddings.db`) and ephemeral DBSCAN states stored in the SurrealDB `pipeline_cluster` table (with RAII cleanup).

**Current Assumption:** Local SQLite and SurrealDB can handle embedding caching and ephemeral cluster state for single-user workloads efficiently.

**Attack Scenario:** As the system scales 10x over 18 months (more agents, massive context ingestion), the SQLite embedding cache will suffer from severe I/O lock contention on transaction-bounded batch writes. Concurrently, aborted pipeline runs (e.g., due to panics or OOMs) may leak `pipeline_cluster` records if the RAII `scopeguard::defer!` fails to execute (e.g., SIGKILL), bloating SurrealDB.

**Blast Radius:** Catastrophic I/O degradation and database corruption/bloat under heavy concurrent workload, requiring a full re-architecture of the storage layer.

**Recommended Structural Change:** 1) Replace the SQLite embedding cache with a dedicated, highly concurrent vector store (e.g., Qdrant or Milvus). 2) Move ephemeral pipeline state out of the persistent database entirely and into a volatile, fast in-memory store (e.g., Redis or a dedicated tokio async state manager).
