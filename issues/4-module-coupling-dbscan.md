# [architecture-review] Module Coupling: DBSCAN Pipeline State and SurrealDB

**Finding:**
The DBSCAN clustering algorithm for the daily dreaming compactor stores its ephemeral machine state temporarily in SurrealDB `pipeline_cluster` tables.

**Current Assumption:**
The assumption is that using the persistent database for ephemeral clustering state simplifies the architecture and ensures state can be cleaned up via RAII scope guards.

**Attack Scenario / Failure Mode:**
If the database connection drops, or if the RAII scope guard fails to execute (e.g., due to a `SIGKILL` or an uncatchable panic in the MLX C++ bindings), the ephemeral state is orphaned in the persistent database. Over time, this leads to database bloat and performance degradation. Furthermore, this design tightly couples the compute-heavy clustering logic directly to the I/O-bound persistent storage, meaning the clustering algorithm cannot be scaled independently or swapped out without modifying the SurrealDB schema.

**Blast Radius:**
Database performance degradation (bloat) and inability to scale the cognitive pipeline independently of the storage layer.

**Recommended Structural Change:**
1.  **Decouple Ephemeral State:** Move ephemeral clustering state to a dedicated, purely in-memory data store (e.g., Redis or a dedicated fast in-memory structure managed by the broker) that naturally disappears on process termination. Do not use the persistent SurrealDB for scratchpad data.
