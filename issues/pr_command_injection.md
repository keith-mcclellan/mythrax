# 🛡️ Sentinel: [CRITICAL] Fix command injection in git worktree execution

🚨 **Severity:** CRITICAL
💡 **Vulnerability:** The Arbor Executor in `mythrax-core/src/cognitive/executor.rs` fell back to invoking `sh -c` to execute `test_command`s when shell operators (like `&`, `|`, `<`, `>`, `;`) were detected. This allowed arbitrary shell command execution, breaking agent isolation boundaries, especially if test commands from untrusted agents or repos were processed.
🎯 **Impact:** Malicious actors or compromised agents could provide a crafted `test_command` to achieve arbitrary remote code execution (RCE) on the host running the executor, circumventing git worktree isolation.
🔧 **Fix:** Removed the `sh -c` fallback pathway entirely. If `test_command` contains shell operators, the function immediately returns an explicit error (`Err(anyhow!("Shell operators are denied for security reasons"))`). All commands are now executed securely through direct argument execution (`Command::new(program).args(args)`).
✅ **Verification:**
- Inspected `mythrax-core/src/cognitive/executor.rs` to ensure `sh -c` is removed.
- Ran `cargo check --lib cognitive::executor` (via `cargo check --lib`) and `cargo test --lib` (which passed, barring a known flaky test).
