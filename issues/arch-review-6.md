# Adversarial Architecture Review: 18-Month Re-architecture Projection

Labels: architecture-review, adversarial

**Finding**: Several architectural decisions will become significant liabilities if the system scales 10x over the next 18 months.

**Current Assumption**: Current design choices (e.g., `LIMIT 50` pagination, local VRAM swapping, synchronous background loops) will scale linearly.

**Attack Scenario**: As memory density increases 10x, the `LIMIT 50` hard cap will lead to severe context starvation (Issue #3). Local VRAM swapping will become a bottleneck as concurrent requests spike, leading to Denial of Service (DoS). The shared token auth (Issue #1) will become unmanageable and insecure with multiple users/agents.

**Blast Radius**: Severe performance degradation, systemic context loss, and unmanageable security posture.

**Recommended Structural Change**: Re-architecture projects required: 1) Replace `LIMIT 50` with semantic/priority-based eviction. 2) Move from local VRAM swapping to a distributed model router or dedicated inference servers. 3) Implement comprehensive RBAC and session management to replace the static token.