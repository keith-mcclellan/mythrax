---
tags: [architecture-review, adversarial]
---
# Finding: API Gateway Authentication

**Current Assumption:** A static shared token via `X-Mythrax-Token` is sufficient for internal orchestration authentication.

**Attack Scenario:** The token is leaked via hardcoded values in `mythrax-core/src/` (as identified in `mock_audit_report.md`) or via a supply-chain attack.

**Blast Radius:** Systemic compromise. An attacker gains full control over the API gateway, model router, and cognitive memory data. No graceful degradation path exists if this token is compromised.

**Recommended Structural Change:** Implement rotating, short-lived JWTs scoped to specific agents or roles, backed by a proper secrets manager.
