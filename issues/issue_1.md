# [Red Team] Single-Port API Gateway: Token Auth Spoofing & DoS

**Labels:** architecture-review, adversarial

**Finding:** The entire API relies on a single shared static token passed via `X-Mythrax-Token` (as seen in `api.rs` and `main.rs`). The authentication boundary assumes that local port binding prevents external exploitation.

**Current Assumption:** The local port 8090 is completely isolated, and a single static token is sufficient for all REST/MCP requests because only trusted local agents will invoke it.

**Attack Scenario:**
1. A malicious script or dependency running on the same machine queries the same `~/.mythrax/config.json` or inspects daemon process arguments to extract the static `X-Mythrax-Token`.
2. Alternatively, a Cross-Site Request Forgery (CSRF) or DNS rebinding attack against `127.0.0.1:8090` from a local browser could bypass the network isolation if CORS is misconfigured.
3. Once the token is obtained or bypassed, the attacker can submit massive arbitrary text payloads to `/v1/chat/completions` or `/v1/mcp/call`.
4. The system will start ingesting and embedding this data, triggering OOM spikes since the SQLite cache eviction relies on `created_at` maximum capacity constraints rather than strict per-request rate limiting.

**Blast Radius:**
- Complete corruption of the cognitive memory state (`surrealkv://`).
- Daemon crash via OOM or disk exhaustion.
- Potential injection of malicious 'wisdom' rules affecting all future agent sessions.

**Recommended Structural Change:**
- Deprecate the single static token architecture. Implement short-lived, dynamically generated session tokens per client process (tied to PID and verified via OS-level socket credentials or Unix Domain Sockets).
- Introduce strict, configurable rate limiting per endpoint to gracefully degrade instead of processing unbounded local requests.
- Add an explicit ADR (Architectural Decision Record) detailing the migration to Unix Domain Sockets for local IPC instead of TCP port 8090 to eliminate the CSRF/DNS rebinding attack surface entirely.