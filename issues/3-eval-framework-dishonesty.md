# [architecture-review] Eval Framework Dishonesty: Missing Adversarial SWE-bench Tests

**Finding:**
The `evals/swebench/` directory contains scaffolding for the SWE-bench Verified dataset to measure end-to-end coding performance. However, SWE-bench Verified only tests standard, "happy path" software engineering tasks (fixing known bugs).

**Current Assumption:**
The assumption is that high performance on SWE-bench Verified correlates with a robust, production-ready agent architecture.

**Attack Scenario / Failure Mode:**
The eval framework completely ignores adversarial inputs. It does not test the agent's ability to resist prompt injections embedded in bug reports, its handling of malicious tool outputs, or its resilience against malformed ASTs designed to crash the `code_symbol` indexer. Consequently, the system will achieve a high "resolve rate" in the lab but fail catastrophically when exposed to the internet or untrusted codebases.

**Blast Radius:**
False confidence in system security and robustness, leading to deployment in sensitive environments where the agent is trivially compromised.

**Recommended Structural Change:**
1.  **Adversarial Eval Suite:** Integrate an explicit adversarial eval suite alongside SWE-bench. This suite must test the `pre_invocation_hook`, AST extraction, and STM boundaries with malicious payloads (e.g., prompt injections, infinite recursion triggers, OOM-inducing inputs).
2.  **Security KPIs:** Track "Injection Resistance Rate" and "Graceful Degradation Rate" as top-level KPIs alongside the SWE-bench "% Resolved".
