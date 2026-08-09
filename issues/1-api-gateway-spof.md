# [architecture-review] Single Point of Failure: Unified API Gateway & Shared reqwest::Client

**Finding:**
The Mythrax 3.0 Core Daemon routes all requests through a Single-Port API Gateway (127.0.0.1:8090) and uses a shared `reqwest::Client` instance for external API communication, relying on a single static `X-Mythrax-Token`.

**Current Assumption:**
The assumption is that the unified API gateway simplifies client interactions and that token-based auth is sufficient for security, while a single `reqwest::Client` prevents socket leaks and resource exhaustion.

**Attack Scenario / Failure Mode:**
1.  **DDoS / Resource Exhaustion:** A malfunctioning agent or adversarial input can spam the gateway. Since the `reqwest::Client` is shared and there's no apparent rate limiting or resource quota per connection/agent, a single rogue process can consume all available connection pool slots, starving all other agents and bringing the daemon to a halt.
2.  **Authentication Bypass / Compromise:** The static `X-Mythrax-Token` is a single point of failure. If leaked (e.g., via prompt injection reading `.mythrax/token`), an attacker gains full control over the daemon's read/write capabilities across all memory scopes.

**Blast Radius:**
System-wide denial of service and complete data compromise. All agents relying on the daemon will fail.

**Recommended Structural Change:**
1.  **Partitioned Resource Pools:** Implement connection pooling and rate limiting *per agent/session* at the gateway level.
2.  **Dynamic/Scoped Auth:** Move away from a single static token. Implement scoped, short-lived tokens per agent session with restricted capabilities (e.g., read-only for certain vaults, write-only for specific STM keys).
