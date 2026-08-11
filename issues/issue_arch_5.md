---
title: "Red Team Architecture Brief: Eval Framework Lacks Adversarial Robustness Testing"
labels: [architecture-review, adversarial]
---

### Finding
The eval framework (if present) tests functionality and correctness, but there is no explicit mention in the architecture of evaluating against adversarial inputs or jailbreaks.

### Current Assumption
The LLM-based system will perform safely if it passes functional 'happy path' tests.

### Attack Scenario
The system is deployed to a production environment. Attackers use context-crowding, role-play jailbreaks, or payload-splitting to bypass agent scope boundaries. The eval framework never tested these, so the system is breached immediately.

### Blast Radius
System compromise, unauthorized tool access, or leakage of sensitive vault contents.

### Recommended Structural Change
Integrate an adversarial testing suite (e.g., PromptFoo, Giskard) into the `evals/` framework. Mandate that every agent role and tool boundary is tested against known prompt injection taxonomies before merge.
