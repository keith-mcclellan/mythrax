# 3. Single-Port API Gateway with Static Shared Authentication Token

**Tags**: `architecture-review`, `adversarial`

**Finding**: 3. Single-Port API Gateway with Static Shared Authentication Token

**Current Assumption**: Consolidating all endpoints (REST, MCP, and external completions proxy) onto a single Port 8090, secured by a shared static auth token (`X-Mythrax-Token`), simplifies the architecture and deployment.

**Attack Scenario**: If the shared static token is leaked or cracked (often easier since it's used uniformly across all services and scripts), an attacker gains full administrative control, database read/write access, and the ability to execute arbitrary MCP tools (including arbitrary code execution via `manage_htr` / test scripts). A single endpoint DDoS also starves both administrative commands and critical memory operations simultaneously.

**Blast Radius**: Total system compromise. An attacker has root-equivalent access to the entire Mythrax unified system, including modifying global wisdom and executing malicious code in agent test loops.

**Recommended Structural Change**: Implement Role-Based Access Control (RBAC) with granular, short-lived, scope-limited JWTs or mutual TLS (mTLS). Segregate administrative/management endpoints from runtime agent MCP endpoints onto different ports or strict network paths to reduce the blast radius of a single compromised token.
