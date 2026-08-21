# Red Team Architecture Brief: Eval Framework Lacks Adversarial Inputs

**Labels:** `architecture-review`, `adversarial`
**Requirement to Close:** A documented architectural decision record (ADR) response must be provided.

**Finding:** The evaluation framework in `evals/swebench/` exclusively measures "% Resolved" on the SWE-bench Verified dataset, which only contains "happy path" coding tasks, as documented in `evals/swebench/README.md`.
**Current Assumption:** Standard dataset coding performance correlates directly with safety and structural robustness in autonomous operations.
**Attack Scenario:** The agent is deployed after passing the SWE-bench eval but immediately falls victim to deceptive issues, recursive loop traps, or backdoored dependency updates because the evaluation never tested failure modes or adversarial inputs. LLM-based systems that only test happy paths are architecturally dishonest.
**Blast Radius:** False confidence in agent safety, leading to unchecked catastrophic failures in real-world adversarial environments.
**Recommended Structural Change:** Integrate a Red Team evaluation suite that explicitly injects prompt injections, impossible recursive loops, and malicious PRs to measure "% Refused" and safe degradation.
