---
tags: architecture-review, adversarial
---

# Prompt Injection via 32K Token Budget Constraints

**Finding**: Prompt concatenation for cluster insight synthesis uses a 32K token budget limit.

**Current Assumption**: Enforcing a strict 32K token budget limit safely bounds prompt size and guarantees stable cluster insight synthesis without memory exhaustion.

**Attack Scenario**: An adversarial agent or malicious user input creates episodes with dense, targeted prompt injections. When the DBSCAN compactor sweeps and clusters these memories, they are fetched and concatenated, easily consuming the 32K limit with malicious commands while pushing out legitimate context. The orchestrator then executes this synthesized payload.

**Blast Radius**: Complete agent hijack; the synthesized insight becomes an active attack vector against the agent's scope, allowing unauthorized commands, unbounded recursion loops, or data exfiltration.

**Recommended Structural Change**: Implement strict separation of control and data planes in the context window. Untrusted memory retrievals must be treated as untrusted strings, wrapped in XML/JSON tags, and explicitly sanitized, rather than evaluated as raw prompt instructions.
