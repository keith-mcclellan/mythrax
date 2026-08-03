# 🛡️ Sentinel: [MEDIUM] Fix test coverage gaps in Eval Harness

**Labels:** `bug`, `agent-found`

🚨 Severity: MEDIUM
💡 Vulnerability: The evaluation framework in `evals/swebench/` solely focuses on verified, functional 'happy paths' (SWE-bench coding benchmarks) and fails to test adversarial input robustness or security boundaries.
🎯 Impact: The eval harness provides a false sense of security, allowing logic bugs and edge case vulnerabilities to slip into production unchecked.
🔧 Fix: Expand the eval suite to include adversarial scenarios, testing context window limits, prompt injections, and malformed inputs.
✅ Verification: The test suite fails when adversarial inputs cause panics or unauthorized state changes.

**Minimal Reproducible Scenario:**
Run the existing eval suite (`evals/swebench/eval.sh`). Observe that it only tests positive functionality (creating files, passing SWE-bench tasks). It contains no tests for prompt injection, buffer overflows, or invalid payload handling.

**File and Line Number:**
`evals/swebench/eval.sh` (entire file/suite)

**Estimated Effort:** High
