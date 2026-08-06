# Single-Port API Gateway shared auth and client contention

**Labels**: `architecture-review`, `adversarial`

## Finding
The Single-Port API Gateway operates on port 8090 and uses a shared static auth token (`X-Mythrax-Token` and `Authorization`), creating a single point of failure and bottleneck.

## Current Assumption
A single shared port and token are sufficient for all cognitive tools, and concurrent requests won't exhaust thread pool or HTTP socket resources.

## Attack Scenario
A sudden burst of multi-agent concurrency or a single slow HTTP client exhausts the shared `reqwest::Client` connection pool, stalling all API Gateway traffic, dropping MCP events, and failing authentication for other subsystems.

## Blast Radius
System-wide deadlock. Total loss of API accessibility. Agent orchestration fails completely, as all components rely on the gateway for sync and auth.

## Recommended Structural Change
1. Implement dynamic, short-lived tokens per agent/session rather than a static shared secret.
2. Partition the API Gateway: dedicated port/pool for internal DB sync vs. external LLM calls vs. user-facing MCP traffic.
3. Replace shared `reqwest::Client` with bounded connection pools per subsystem.
