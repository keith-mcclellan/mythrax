---
title: "Red Team Architecture Brief: Non-Adversarial Evaluation Framework (SWE-bench)"
labels: ["architecture-review", "adversarial"]
---

### Finding
The evaluation framework in `evals/swebench/eval.sh` relies solely on the SWE-bench Verified dataset for performance scoring.

### Current Assumption
The assumption is that high performance on standard SWE-bench tasks equates to a robust, production-ready coding agent.

### Attack Scenario
The agent is deployed to production. An adversary submits a pull request or an issue containing obfuscated prompt injections, logic bombs, or malicious dependencies. Because the eval framework only tests "happy paths" and standard SWE-bench resolutions, it fails to evaluate whether the agent will correctly identify, reject, or safely handle adversarial inputs. The agent processes the malicious input, compromising the host or the repository.

### Blast Radius
Critical. Widespread vulnerability to prompt injection, supply chain attacks, and malicious code execution. An LLM-based system that cannot defend against adversarial inputs is architecturally dishonest about its readiness for autonomous operation.

### Recommended Structural Change
1. **Adversarial Red-Teaming Evals**: Integrate adversarial datasets (e.g., prompt injection benchmarks, obfuscated malware analysis, jailbreak attempts) directly into the `evals/` framework.
2. **Mandatory Security Scoring**: Require agents to pass both functional (SWE-bench) and security/adversarial evaluations before deployment or release.