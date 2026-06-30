---
title: "Dishonest Eval Framework (Happy Path Only)"
labels: ["architecture-review", "adversarial"]
---

**This issue requires a documented Architectural Decision Record (ADR) response to close.**

### Finding
Dishonest Eval Framework (Happy Path Only)

### Current Assumption
The evaluation framework in `evals/swebench/eval.sh` accurately measures the capabilities and resilience of the Mythrax system by running the official SWE-bench Verified dataset.

### Attack Scenario
The system passes the SWE-bench evaluation because it is only tested on well-formed, "happy path" software engineering tasks. When deployed in production, it is subjected to adversarial inputs, malformed repositories, or corrupted `_last_swept_at` timestamps.

### Blast Radius
The evaluation provides false confidence. The system will fail spectacularly in production under adversarial conditions, and developers will have no prior warning or regression tests to catch the failures.

### Recommended Structural Change
Introduce a dedicated adversarial test suite (`evals/adversarial/`) that specifically tests prompt injections, malformed JSONL transcripts, corrupted WAL logs, and simulated GPU OOM conditions. Refuse to merge code that lowers the adversarial resilience score.