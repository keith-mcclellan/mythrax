---
labels: [architecture-review, adversarial]
---
# Single Point of Failure: Gateway Hardcoded Auth Token (HARD-001)

**Finding:**
The Single-Port API Gateway relies on a hardcoded fallback authentication token (`"secret-token"`). This is documented in `mock_audit_report.md` under HARD-001.

**Current Assumption:**
The underlying assumption is that users will manually override this token using the configuration file, or that port 8090 is completely isolated and inaccessible from malicious actors.

**Attack Scenario:**
An attacker (e.g., via Server-Side Request Forgery or malicious local code execution) connects to the API on port 8090. If the user hasn't explicitly set a token, the attacker provides `"secret-token"` in the `X-Mythrax-Token` header. This successfully bypasses authentication, allowing the attacker to issue privileged MCP commands, manipulate the agent, and read sensitive episodic memories.

**Blast Radius:**
Full host compromise. Because the API Gateway routes all agent capabilities, the attacker gains the same level of access as the Mythrax Daemon, including file system access and agent orchestration control.

**Recommended Structural Change:**
Eliminate the fallback entirely. If no token file exists upon startup, generate a cryptographically secure random token, write it to `~/.mythrax/token` with 0600 permissions, and use that. Refuse to start if the token generation or save fails. Never hardcode authentication fallbacks in production endpoints.
