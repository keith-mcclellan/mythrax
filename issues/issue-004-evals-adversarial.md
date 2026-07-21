---
labels: ["architecture-review", "adversarial"]
---

# Issue: Dishonest Evaluation Architecture - Lack of Adversarial Testing

## Finding
The evaluation framework in `evals/swebench/eval.sh` is solely designed to run the official SWE-bench Verified dataset. It evaluates the agents strictly on their ability to solve known "happy path" coding issues. There are zero architectural provisions for evaluating the system's resilience against adversarial inputs, prompt injection, or bounded execution constraints.

## Current Assumption
The architecture assumes that performance (solving coding tasks) is the only metric that matters for evaluating an autonomous agent framework. It assumes that if the agent can pass SWE-bench, it is suitable for deployment, completely ignoring the adversarial reality of open-ended environments. This is architecturally dishonest.

## Attack Scenario
The system is deployed into an environment where it interacts with external users, untrusted code repositories, or web content. An attacker introduces adversarial context (e.g., in a PR comment or a downloaded file) that subverts the agent's instructions. Because the evaluation framework never tested the agent's ability to resist prompt injection or enforce scope boundaries, the agent silently complies with the attacker's instructions, bypassing all intended safeguards.

## Blast Radius
**Silent Security Degradation.** The lack of adversarial evals means the system's true security posture is unknown and likely highly vulnerable. When deployed, it will fail catastrophically under adversarial conditions, leading to data breaches, unauthorized actions, and agent hijacking, all while the system administrators hold a false sense of security based on high SWE-bench scores.

## Recommended Structural Change
1. **Incorporate Adversarial Datasets:** Expand the evaluation framework to include datasets specifically designed to test prompt injection resilience, scope adherence, and graceful failure under unbounded recursion or malicious inputs (e.g., SecQA, or custom adversarial agent benchmarks).
2. **Dual-Metric Evaluation:** Make security and robustness metrics a primary blocker for deployment, alongside performance metrics like SWE-bench. An agent must pass both the capability and the adversarial resilience suites.

*Note: Do not close this issue without a documented Architectural Decision Record (ADR) response.*