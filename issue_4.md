---
title: "🛡️ Red Team Architecture Brief: Inadequate Eval Framework (Failure to Test Adversarial Inputs)"
labels: ["architecture-review", "adversarial"]
---

## Red Team Architecture Brief

**Finding:**
The current evaluation framework located in `evals/swebench/` is designed solely around the official `princeton-nlp/SWE-bench_Verified` dataset, which measures performance on human-verified, standard coding tasks. It acts strictly as an A/B test for standard operational capabilities (with vs. without memory) and structurally omits any testing of adversarial inputs, prompt injection vectors, or boundary constraints.

**Current Assumption:**
The evaluation architecture assumes that measuring standard task resolution ("happy paths") is sufficient to validate the production readiness of an autonomous, memory-augmented AI sidecar. It assumes that if the agent can solve benign SWE-bench issues, its memory storage, retrieval routing, and compaction models are structurally sound.

**Attack Scenario:**
Because the evals do not test malicious inputs, a structurally flawed memory ingestion hook (which we know ingests data verbatim) passes all CI/CD gating and is deployed to production. An attacker submits a pull request or an issue containing a masked prompt injection designed to poison the agent's project-level memory (`wiki_node`). The agent processes the input, the memory is compacted and preserved, and the agent's future behavior is silently hijacked. The evals remain green because they only test on the pristine SWE-bench dataset.

**Blast Radius:**
Systemic false confidence. The system is architecturally dishonest, declaring itself production-ready while entirely blind to adversarial contexts. In a production environment, this leads to complete compromise of the agent's cognitive graphs, data exfiltration through tool use, and unauthorized code commits, all while passing the primary evaluation metrics.

**Recommended Structural Change:**
1. **Mandatory Red Team Evals:** Integrate an adversarial dataset alongside SWE-bench. This dataset must contain prompt injections, unbounded recursion traps, and malformed JSON payloads designed specifically to break the parser, the pre-compaction hook, and the SurrealDB ingestion flow.
2. **Security-Specific Scoring:** Expand the `eval.sh` and `summarize.py` harness to track not just resolution rates, but "Mitigation Rates" (e.g., how successfully the agent identifies and neutralizes an injection without polluting its database).
3. **Fuzzing the Memory Router:** Implement an automated fuzzing suite targeting the Sigmoid Gating Formula and cross-scope RAG injection to ensure malicious payloads cannot arbitrarily boost their own relevance scores.

*Note: Do not close this issue without a documented Architectural Decision Record (ADR) response.*