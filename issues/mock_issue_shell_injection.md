# [CRITICAL] Shell Injection Vulnerability in Arbor HTR Execution

**Labels:** `architecture-review`, `adversarial`, `security`

## Finding
The `ArborExecutor` (`mythrax-core/src/cognitive/executor.rs`) dynamically evaluates test commands. If the command string contains shell operators (`|`, `>`, `&`, `;`), it delegates execution to raw `sh -c`.

## Current Assumption
The `ArborExecutor` safely evaluates code changes and test commands by running them in isolated git worktrees.

## Attack Scenario
A malicious agent output (or an injected prompt) can construct a test command like `cargo test; rm -rf /` or exfiltrate sensitive environment variables. The `sh -c` delegation will execute these arbitrary commands on the host system.

## Blast Radius
Full remote code execution (RCE) on the host machine running the daemon, completely bypassing the intended "isolated git worktree" boundary.

## Recommended Structural Change
Prohibit `sh -c` entirely. Parse all commands into strict arguments using a robust parser and execute them directly via `std::process::Command` with explicit binary paths. Reject any command containing shell operators.

**Note:** Do not close this issue without a documented Architectural Decision Record (ADR) response.
