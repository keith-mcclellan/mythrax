---
title: Adversarial Review: Lack of Adversarial Testing in Eval Framework
labels: architecture-review, adversarial
---

**Finding:** The evaluation framework in `evals/swebench/` wraps SWE-bench and tests patches against predefined GitHub issues. It does not appear to test adversarial inputs, prompt injections, or malicious context scenarios.
**Current Assumption:** Evaluating against standard bug resolution datasets (SWE-bench) is sufficient to validate the system's architecture and agent reasoning capabilities.
**Attack Scenario:** The system deploys with high confidence based on SWE-bench scores. However, because it was never tested against adversarial inputs (e.g., source code containing hidden prompt injections in comments, or forged PDF assets designed to hijack the Forge ingestion pipeline), it fails catastrophically when exposed to real-world poisoned repositories or malicious external files.
**Blast Radius:** False sense of security leading to deployment in untrusted environments where the system is rapidly compromised.
**Recommended Structural Change:** Introduce a dedicated `evals/adversarial/` suite. This suite must inject known prompt injections into codebase comments, simulate poisoned Forge assets, and attempt to induce unbounded recursion in the Arbor HTR loop. A system routing autonomous agents must treat adversarial robustness as a core evaluation metric, not an afterthought.

*Note: Do not close this issue without a documented architectural decision record (ADR) response.*