---
labels: ["architecture-review", "adversarial"]
---

# Adversarial Architecture Review: SWE-bench Eval Framework Lacks Adversarial Testing

## Finding
The evaluation framework (`evals/swebench/eval.sh`) acts as a wrapper for the official SWE-bench Verified harness. It tests the model's ability to apply predicted patches and pass existing test suites. It completely lacks any adversarial input, prompt injection, or boundary-testing evaluations.

## Current Assumption
The architecture assumes that passing the SWE-bench "happy paths" (functional correctness of patches) is sufficient validation for the system's reliability and safety. It assumes that if the model can resolve the SWE-bench issues, the underlying architecture and orchestration are sound.

## Attack Scenario
The current evaluation framework is "architecturally dishonest" for an LLM-based autonomous agent system. Because it only tests functional patch resolution, it blinds the developers to critical vulnerabilities. An attacker can easily exploit prompt injection in the pre-compaction hooks (as noted in another issue), bypass the API Gateway's static token, or trigger unbounded recursive tool loops, and none of these failure modes would ever be caught by the `eval.sh` framework. The system could achieve a high SWE-bench score while remaining fundamentally insecure and brittle to adversarial manipulation.

## Blast Radius
**Systemic.** The lack of adversarial evals creates a false sense of security. Critical vulnerabilities will be deployed to production because the CI/CD pipeline does not measure resilience against malicious inputs or orchestration failures.

## Recommended Structural Change
1. **Integrate Adversarial Datasets:** Expand the evaluation framework to include datasets specifically designed to test prompt injection resilience, jailbreak attempts, and out-of-bounds tool execution.
2. **Red-Team Harness:** Develop an automated red-team harness that actively attempts to poison the `precompact.rs` memory stream, brute-force the API Gateway, and trigger unbounded recursion loops during the CI evaluation phase.
3. **Fail-Safe Evals:** Mandate that passing the adversarial evaluation suite is a blocking requirement for any release, weighted equally with the SWE-bench functional scores.
