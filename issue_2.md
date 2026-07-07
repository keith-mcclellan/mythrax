# In-Process Fallback and Model Broker Tight Coupling

**Tags:** `architecture-review`, `adversarial`

**Finding:** The Model Broker silently falls back to dummy or mocked LLM engines (e.g., `fallback-cpu-model`) in production code when acquisition fails.
**Current Assumption:** Creating dummy/mock LLM engines silently maintains system uptime when VRAM is exhausted or acquisition fails.
**Attack Scenario:** An adversarial prompt intentionally exploits VRAM limits to trigger acquisition failure. The system silently falls back to a mocked/degraded engine lacking guardrails, which the attacker then hijacks to inject hallucinated data.
**Blast Radius:** Silent data corruption in memory clustering and permanent knowledge storage. Garbage rules are permanently written into the database.
**Recommended Structural Change:** Remove all silent mocked fallbacks in production. Propagate model acquisition errors explicitly, implement circuit breakers, and enforce hard failures over degraded garbage outputs.
