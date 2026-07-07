# Single-Port API Gateway (Port 8090) Single Point of Failure

**Tags:** `architecture-review`, `adversarial`

**Finding:** The system uses a single-port API Gateway (8090) and a single shared static auth token (`X-Mythrax-Token`) for all REST, MCP, and proxy endpoints.
**Current Assumption:** Consolidating all endpoints onto a single port with a shared static token simplifies the API and auto-spawn detection.
**Attack Scenario:** An attacker floods the single port with malformed MCP commands or large payloads. Alternatively, if the single static token is leaked, the attacker gains full administrative and cognitive manipulation rights.
**Blast Radius:** Total system compromise and complete denial of service. The gateway cannot gracefully degrade, taking down the entire sidecar daemon.
**Recommended Structural Change:** Decouple administrative endpoints from memory/inference endpoints onto separate ports. Implement distinct RBAC/token mechanisms and rate-limiting isolated by worker pools.
