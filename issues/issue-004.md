---
labels: architecture-review, adversarial
---
# Finding: Eval Framework Lacks Adversarial Robustness Testing

**Current Assumption:** Using SWE-bench functional coding benchmarks in `evals/swebench/` ensures the cognitive engine and agent orchestration are robust and accurate.

**Attack Scenario:** The agent orchestration design is vulnerable to prompt injection attacks via ingested external code or documents. The current `evals/swebench/` tests only evaluate "happy path" functionality and completely fail to detect adversarial payloads.

**Blast Radius:** Unauthorized code execution or privilege escalation via the agent, masquerading as a legitimate task. An LLM system that only tests happy paths is architecturally dishonest.

**Recommended Structural Change:** Architect an adversarial eval harness within `evals/` that explicitly injects malicious payloads, tests unbounded edge cases, and verifies agent scope boundary enforcement.

*Note: Never close this issue without a documented architectural decision record (ADR) response.*
