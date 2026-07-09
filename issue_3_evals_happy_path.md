# 💥 Evals Dishonesty: SWE-bench Only Tests Happy Paths

**Tags:** `architecture-review`, `adversarial`

**Requires ADR response to close.**

**Finding:**
The evaluation framework in `evals/swebench/` tests purely against benchmark "happy paths" (standard open-source issues) and completely ignores adversarial inputs, edge cases, and context-poisoning attacks.

**Current Assumption:**
The `README.md` and evaluation harness rely on the `SWE-bench_Verified` dataset to measure "% Resolved" as the headline KPI for the advanced-memory program. It assumes that if the agent can solve standard PR issues, the architecture is sound for production deployment.

*What assumption does this break if it's wrong?* It assumes LLM-based systems fail gracefully when presented with deceptive, malformed, or hostile codebase states. It equates "coding capability on curated issues" with "system robustness."

**Attack Scenario:**
A user integrates Mythrax into a production enterprise codebase containing thousands of auto-generated mock files, heavily obfuscated legacy code, or explicitly conflicting `AGENTS.md` rules scattered across deep directories. Because the `evals/` framework never tested hostile or noisy codebases, the Sigmoid-gated search indexer saturates with garbage data, and the context window overflows with contradictory rules, rendering the agent completely paralyzed.

**Blast Radius:**
High failure rates in real-world, messy enterprise environments. The product appears highly capable in benchmarks but degrades into an infinite loop of confusion when faced with complex, non-standard architectures or intentional prompt obfuscation.

**Recommended Structural Change:**
Architectural honesty requires adversarial evaluations.
1. Expand `evals/` to include a "Red Team / Adversarial" dataset containing:
   - Malicious prompt injections hidden in `README.md` or comments.
   - Contradictory rules split across nested `.agents/` configurations.
   - Extremely large, minified files designed to test token budget truncation logic.
2. Require a minimum passing score on the Adversarial Eval suite before merging code to `main`.
