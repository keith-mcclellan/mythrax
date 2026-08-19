# Ephemeral State Persistence Data Corruption

**Finding** -> DBSCAN clustering ephemeral state is stored in SurrealDB's `pipeline_cluster` table (defined in `schema.rs` and accessed in `crud_operations.rs`).
**Current Assumption** -> The process will safely execute cleanup operations during normal operation or shutdown.
**Attack Scenario** -> A hard OOM kill (SIGKILL) bypassing tokio's graceful shutdown leaves orphaned clustering data in the persistent `pipeline_cluster` SurrealDB table. Subsequent runs hallucinate connections to stale cluster states, leading to memory contamination.
**Blast Radius** -> Corruption of cognitive synthesis and cross-contamination of memory clusters.
**Recommended Structural Change** -> Store ephemeral clustering state entirely in memory or an explicitly ephemeral store (like Redis) rather than the persistent SurrealKV.


**Labels**: `architecture-review`, `adversarial`

*Note: Do not close this issue without a documented architectural decision record (ADR) response.*
