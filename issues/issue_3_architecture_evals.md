---
title: "🛡️ Red Team Architecture Brief: Eval Framework is Architecturally Dishonest"
labels: ["architecture-review", "adversarial"]
status: "open"
---

# Red Team Architecture Brief

**Finding**
SWE-bench Framework in `evals/` is architecturally dishonest regarding adversarial inputs.

**Current Assumption**
Passing the "princeton-nlp/SWE-bench_Verified" harness (% Resolved) proves the architecture is robust and production-ready.

**Attack Scenario**
The system achieves high resolve rates on happy-path PR generation but completely fails when introduced to repositories with malicious `AGENTS.md` files or obfuscated prompt injections in source code. An adversary submits a PR containing a prompt injection; the Arbor HTR loop verifies it, gets hijacked, and modifies the local daemon config. The current evals only test the "happy path" and are blind to adversarial vectors.

**Blast Radius**
False confidence in system security leading to automated merging of malware. Agents will confidently execute malicious instructions because no baseline evaluation measures adversarial rejection rates.

**Recommended Structural Change**
Add a dedicated adversarial suite to `evals/`. Fork the SWE-bench harness to include repositories with poisoned instructions, infinite loop triggers, and context-window-exhausting files. The eval must assert that the system *gracefully rejects* or ignores malicious directives without crashing or adopting the injected goal.
