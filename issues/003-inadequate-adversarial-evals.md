---
labels: architecture-review, adversarial
state: open
---
# Red Team Architecture Brief: Inadequate Adversarial Evaluation Framework

**Finding** -> The evaluation framework (`evals/swebench/eval.sh`) strictly relies on `SWE-bench_Verified` which only tests code resolution for known bugs (happy paths).
**Current Assumption** -> High scores on standard coding benchmarks correlate with production readiness and systemic stability against hostile inputs.
**Attack Scenario** -> An attacker feeds Mythrax a highly obfuscated or recursively nested code structure specifically designed to trigger infinite loops in the `cognitive/ast.rs` parser or exhaust the 32K token budget prematurely. Evals never tested this.
**Blast Radius** -> Silent memory truncation, partial insights, or complete failure of the Cognitive Scheduling pipeline.
**Recommended Structural Change** -> Implement a dedicated suite of adversarial evals, including prompt injections, OOM-triggering payloads, cyclic dependency graphs, and multi-turn deception tests. Must require an ADR response to close.
