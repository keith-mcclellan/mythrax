---
title: "Architecture Review: Inadequate Adversarial Evaluation in Evals Framework"
labels: ["architecture-review", "adversarial"]
---

**Finding:** The evaluation framework in `evals/` does not test adversarial inputs or prompt injections, presenting a critical blind spot in agent safety.

**Current Assumption:**
The current test harness assumes that optimizing for "% Resolved" on standard development datasets accurately reflects an agent's readiness for production. As seen in `evals/swebench/eval.sh` and `evals/swebench/README.md`, the entire E2E verification loop relies on `princeton-nlp/SWE-bench_Verified`, which exclusively measures functional coding problem resolution ("happy paths").

**Attack Scenario:**
Because the agent is never evaluated against adversarial conditions during the development lifecycle, security regressions slip into production unnoticed. In a live environment, an attacker supplies a malicious issue description or code snippet containing a prompt injection (e.g., instructing the agent to ignore previous instructions and print its auth token, or instructing it to execute arbitrary malicious code via the MCP `complete_code_task` tool without proper sandboxing). The agent complies because its boundaries were never stress-tested.

**Blast Radius:**
A compromised agent can leak sensitive static authentication tokens (`X-Mythrax-Token`), read/write arbitrary files via vault tools, or execute malicious payloads on the host environment, leading to full system compromise.

**Recommended Structural Change:**
Incorporate adversarial benchmarks (such as PromptBench, or a custom suite of prompt injection and jailbreak payloads) directly into the `evals/` test suite. The evaluation harness must assert that the agent correctly refuses out-of-scope actions and maintains its system prompt boundaries under active attack.
