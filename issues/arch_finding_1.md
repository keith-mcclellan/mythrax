# 1. Local File Database Locks under Multi-Process Contention

**Tags**: `architecture-review`, `adversarial`

**Finding**: 1. Local File Database Locks under Multi-Process Contention

**Current Assumption**: RocksDB and SurrealKV file-based exclusive locks are sufficient for a single background daemon handling sequentially queued requests. The daemon retry loops (up to 10 attempts, 500ms sleep) will adequately resolve short-lived contention.

**Attack Scenario**: In a 10x scaled environment or during concurrent execution of heavy batch processes (e.g. DBSCAN clustering + rapid agent queries), lock contention will exceed retry limits. Furthermore, adversarial agents or high-volume CI/CD tests could intentionally spam API endpoints, causing permanent database starvation, locking out the core daemon and resulting in silent failure of the agent orchestration layer.

**Blast Radius**: System-wide persistent layer failure. Agents lose memory read/write capabilities, handoffs fail, and the orchestration mechanism collapses into a non-responsive state. No graceful degradation path exists if the DB layer cannot be written to.

**Recommended Structural Change**: Migrate from exclusive local file locks to a standalone network-attached database service (e.g., PostgreSQL or dedicated SurrealDB server) with connection pooling and robust concurrency controls, decoupling the persistence tier from the daemon process.
