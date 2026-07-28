## 2026-06-27 - Remove Shell Injection Vector in TestCommandEvaluator
**Vulnerability:** The TestCommandEvaluator in Arbor allowed raw POSIX shell invocation (`sh -c`) for dynamically constructed test commands if shell operators were detected.
**Learning:** Using raw shell invocations with dynamically constructed strings in a Git worktree context opens up a severe shell injection attack vector.
**Prevention:** Always use direct argument parsing and execution (`std::process::Command::new().args()`) to maintain strong agent execution boundaries, avoiding shell operators entirely.
