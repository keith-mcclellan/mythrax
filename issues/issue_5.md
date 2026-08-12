---
title: "Architecture Liability: Eval Framework Bias Towards Happy Paths"
labels: ["architecture-review", "adversarial"]
---

**Finding** -> The `evals/` framework is designed to test expected functionality but lacks explicit adversarial or chaotic failure modes.

**Current Assumption** -> Testing happy paths and expected error codes is sufficient to validate LLM-driven agent behavior.

**Attack Scenario** -> Agents deployed in production encounter unexpected, contradictory, or maliciously crafted context (prompt injection). Since evals don`t test these bounds, agents confidently execute unsafe actions or enter infinite loops.

**Blast Radius** -> Unpredictable agent behavior in production, leading to unauthorized data access, destructive actions, or severe hallucinations.

**Recommended Structural Change** -> Integrate adversarial prompt injection, malformed JSON, and chaos engineering (e.g., simulated database latency/errors) into the standard eval suite.

*Note: Do not close this issue without a documented architectural decision record (ADR) response.*
