# 🛡️ Sentinel: [CRITICAL] Fix shell injection in Arbor HTR dynamically constructed test commands

🚨 **Severity:** CRITICAL
💡 **Vulnerability:** Unsanitized dynamically constructed test commands were passed to a raw POSIX shell invocation (`sh -c`) in `arbor.rs` and `executor.rs` whenever shell operators were detected. This created a shell injection vulnerability, allowing for potential adversarial manipulation of shell operators and compromising agent isolation boundaries.
🎯 **Impact:** If adversarial input manipulates the shell operators in the dynamically generated test commands, an attacker could achieve arbitrary command execution on the host machine executing the HTR evaluation pipeline, leading to complete system compromise.
🔧 **Fix:** Removed the `sh -c` fallback execution path that triggers when shell operators are present. Instead, arguments are carefully parsed (maintaining quote integrity) and commands are strictly invoked using direct argument execution (`std::process::Command::new(program).args(args)`), explicitly prohibiting shell interpretation.
✅ **Verification:**
1. Ran `cargo check` and verified compilation succeeds.
2. Ran `cargo test` for `cognitive::arbor` and `cognitive::executor` and verified tests continue to pass.
3. Verified the codebase modifications using `git diff`.
