# 🛡️ Sentinel: [CRITICAL] Fix shell injection vulnerabilities

🚨 Severity: CRITICAL
💡 Vulnerability: The code used a fallback to `sh -c` to execute dynamically constructed test commands (in `arbor.rs` and `executor.rs`) when it detected shell operators like `&`, `|`, `<`, `>`. This created a severe shell injection vulnerability as the command strings contained unsanitized external input.
🎯 Impact: Attackers or compromised LLM inputs could execute arbitrary commands on the host machine by injecting shell metacharacters into the test command string, leading to total system compromise.
🔧 Fix: Removed the `sh -c` fallback and `has_shell_operators` check entirely. The code now always uses direct argument parsing and execution with `std::process::Command::new(program).args(args)`.
✅ Verification: Ran `cargo test cognitive::arbor` and `cargo test cognitive::executor` to ensure tests pass and the logic correctly parses commands directly.
