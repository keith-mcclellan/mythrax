# Single-Port API Gateway & Shared Static Auth Token single point of failure

**Labels:** architecture-review, adversarial

**Finding:** The Single-Port API Gateway (port 8090) validates REST and MCP requests against a shared static auth token via `X-Mythrax-Token` and `Authorization` headers. We also observed `reqwest::Client` contention in memory, as it is reused across endpoints and tool invocations.

**Current Assumption:** A shared HTTP client and a single unified port guarded by a static token is efficient and secure enough for local or single-tenant deployments.

**Attack Scenario:** An attacker or compromised dependency extracts the single static `X-Mythrax-Token` (or leaks it via memory). They now have full unpartitioned access to all endpoints, including MCP tools, memory ingestion, and config overrides. Concurrently, a high-volume request flood on port 8090 can exhaust the shared `reqwest::Client` connection pool, locking out all administrative and routing capabilities simultaneously.

**Blast Radius:** Total daemon compromise and Denial of Service. No graceful degradation path exists because all traffic flows through the single port 8090 and a single shared token.

**Recommended Structural Change:** 1) Implement fine-grained token partitioning (e.g., separate tokens for MCP tools vs. chat routing). 2) Decouple the administrative/management interface (port 8090) from the proxy/completions interface (port 8080) to allow independent QoS and failure domains.
