---
title: "Arbor HTR Parallel Verification Loop uses raw POSIX shell invocation"
labels: [architecture-review, adversarial]
---

**Finding:** Arbor HTR Parallel Verification Loop uses raw POSIX shell invocation.

**Current Assumption:** Executing code refinements and tests in isolated git worktrees prevents database/test environment pollution.

**Attack Scenario:** Adversarial input (prompt injection) generates malicious code with shell commands. The HTR loop runs `sh -c` on these candidate changes.

**Blast Radius:** Host RCE. Worktrees do not provide sandbox isolation, allowing the attacker to escape the branch, read `.env`, exfiltrate keys, and pivot to the internal network.

**Recommended Structural Change:** Mandate execution of HTR loops inside strict Docker containers or WASM sandboxes with zero host file system access.
