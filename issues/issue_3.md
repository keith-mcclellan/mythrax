---
title: "Evident Adversarial Eval Gaps in SWE-bench"
labels: ["architecture-review", "adversarial"]
---

Finding -> Current Assumption -> Attack Scenario -> Blast Radius -> Recommended Structural Change

**Finding**: The evals/swebench/ framework tests only happy paths for coding tasks.
**Current Assumption**: Standard coding benchmarks adequately validate agent safety and behavioral constraints.
**Attack Scenario**: An adversary submits a PR containing hidden prompt injections. The eval framework passes, but the agent executes out-of-scope commands in production.
**Blast Radius**: Architecturally dishonest validation leading to unbounded recursion or host compromise.
**Recommended Structural Change**: Integrate adversarial eval datasets that actively test resilience against prompt injection and scope boundary enforcement.

**Note:** Never close this issue without a documented architectural decision record (ADR) response.
