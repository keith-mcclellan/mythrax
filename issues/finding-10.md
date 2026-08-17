---
title: "Finding 10: Missing Scope Boundary Enforcement in Agent Directives"
labels: ["architecture-review", "adversarial"]
---

**Finding**: The `.agents/AGENTS.md` file defines robust directives for coding standards and memory safety (e.g., "Top-Level Scoping & Safe RAII Guards"), but lacks explicit enforcement of agent scope boundaries or authorization levels when agents interact with the broader system or file system.

**Current Assumption**: The implicit assumption is that agents will voluntarily adhere to scope limitations and that restricting them via prompt directives (like `/docs:write-concisely`) is sufficient to prevent scope creep or unauthorized access.

**Attack Scenario**: A malicious prompt injection could exploit an agent's broad file system access. Without rigid, system-level scope boundaries, an attacker could instruct the agent to read sensitive configuration files (e.g., `~/.mythrax/daemon.pid`, authentication tokens) or modify code outside its designated track, leading to unauthorized data exfiltration or system modification. The lack of strict scope enforcement means prompt directives alone are the only line of defense, which is easily bypassed.

**Blast Radius**: Privilege escalation, unauthorized access to sensitive files, and cross-agent or cross-project data corruption.

**Recommended Structural Change**: Implement a capability-based security model and chroot-like isolation for each agent session at the system level. Agents must only be able to read/write to specific subdirectories (e.g., a designated workspace or scratchpad) that they are explicitly authorized for. This enforcement must happen at the file system and API gateway levels, not just within prompt directives.
