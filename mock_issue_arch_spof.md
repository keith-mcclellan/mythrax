# Finding: Persistent Lock Retry Loop Causes Single Point of Failure (SPOF)

**Current Assumption:**
"RocksDB and SurrealKV require exclusive file locks... wrapping the database connection in a retry loop with backoff (up to 10 attempts, 500ms sleep)" (ARCHITECTURE.md). It is assumed that 5 seconds is an acceptable maximum startup delay for the daemon.

**Attack Scenario:**
Rapid concurrent daemon restarts or high parallel test execution trigger lock contention. A 5-second backoff delay blocks the single port (8090) initialization on the main thread, timing out all thin client MCP auto-spawn polling loops (which wait 5-15s per `mock_audit_report.md` DOC-003). This causes a cascading failure where agents cannot spawn the daemon or connect to it.

**Blast Radius:**
100% gateway downtime. Agents are completely locked out of accessing memory, routing models, or executing tools, rendering the entire autonomous system inoperable.

**Recommended Structural Change:**
Decouple the database connection lifecycle from the Gateway HTTP router binding. Implement an async connection pool that allows the Gateway to bind port 8090 immediately and serve `503 Service Unavailable` with `Retry-After` headers during initialization/contention, instead of hanging the main thread and failing client readiness checks.