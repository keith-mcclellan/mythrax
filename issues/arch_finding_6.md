# 6. Eval Framework Lacks Adversarial Input Testing

**Tags**: `architecture-review`, `adversarial`

**Finding**: 6. Eval Framework Lacks Adversarial Input Testing

**Current Assumption**: Evaluating the system solely using the SWE-bench Verified dataset (which focuses on happy path functional bug fixes) is sufficient to gauge system robustness and reliability.

**Attack Scenario**: The system may achieve high scores on functional tests but remains completely blind to prompt injection attacks, context overflow attacks, or resource starvation attacks. An attacker could trivially bypass or exploit the agent loop using adversarial inputs because the eval framework provides false confidence by only testing "honest" tasks.

**Blast Radius**: Systemic vulnerability blindness. The team operates under a false sense of security, believing the system is robust based on high SWE-bench scores, while adversarial edge cases (like malicious PR descriptions or injected tool responses) remain untested and fully exploitable.

**Recommended Structural Change**: Expand the evaluation framework (`evals/`) to include a comprehensive suite of adversarial inputs, prompt injection payloads, and resource exhaustion tests. LLM systems must be tested against malicious intent, not just functional capability.
