# 4. Arbor HTR Loop Executing Shell Commands in Git Worktrees

**Tags**: `architecture-review`, `adversarial`

**Finding**: 4. Arbor HTR Loop Executing Shell Commands in Git Worktrees

**Current Assumption**: Isolating code verification and tests by running POSIX shell commands (`sh -c`) inside isolated git worktrees prevents pollution of the main database and repository.

**Attack Scenario**: An attacker or compromised subagent can submit adversarial code changes or test commands via the `manage_htr` tool that include shell injection payloads. Because the system invokes `sh -c` directly without sanitization, the payload executes arbitrary code with the permissions of the Mythrax daemon process.

**Blast Radius**: Arbitrary Code Execution (ACE) on the host machine. The isolation boundary (the git worktree) only protects the filesystem structure, not the execution environment, allowing attackers to escape the worktree, read sensitive files, or pivot to the broader network.

**Recommended Structural Change**: Execute all code verification, tests, and compilation steps inside strongly isolated, ephemeral sandboxes (e.g., Docker containers or firecracker microVMs) with strict resource quotas and no network access. Avoid `sh -c` and use direct binary execution for commands.
