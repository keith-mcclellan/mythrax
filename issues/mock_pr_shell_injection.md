# 🛡️ Sentinel: [CRITICAL] Fix shell injection in Arbor test evaluator

🚨 Severity: CRITICAL
💡 Vulnerability: The Arbor `TestCommandEvaluator` allowed raw shell invocation (`sh -c`) if it detected shell operators, creating a shell injection vulnerability when evaluating dynamic hypotheses.
🎯 Impact: A malicious payload in the test command could escape the execution boundary, leading to arbitrary code execution in the context of the agent.
🔧 Fix: Removed the `sh -c` branch and `has_shell_operators` check, forcing all test commands to be parsed safely via direct command arguments (`std::process::Command::new().args()`).
✅ Verification: Run `cd mythrax-core && cargo check --lib` and ensure it compiles successfully. Verify that `sh -c` is no longer present in `TestCommandEvaluator::evaluate`.
