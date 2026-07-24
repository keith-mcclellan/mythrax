## 2024-07-24 - Shell Injection in Arbor HTR Evaluator
**Vulnerability:** The Arbor HTR Parallel Verification Loop (`TestCommandEvaluator`) executed test commands using raw POSIX shell invocation (`sh -c`) if it detected shell operators, allowing severe shell command injection.
**Learning:** Checking for shell operators (`&`, `|`, etc.) and deciding to use `sh -c` bypasses standard safe argument passing, creating an injection risk. User or LLM-provided commands (e.g., `test_command`) must never be passed unescaped to a shell.
**Prevention:** Avoid `sh -c` for dynamic commands. If shell features are absolutely necessary, validate/sanitize heavily. Otherwise, parse arguments strictly into the executable and its arguments via `Command::new()`.
