---
title: "Finding 9: Lack of Agent Scope Boundaries in Vault Mirroring"
labels: ["architecture-review", "adversarial"]
---

**Finding**: The Workspace Vault Mirroring Isolation (`sync_workspace_docs_to_vault`) ignores specific paths (like `target/`, `.git/`) and uses atomic `.tmp` swaps, but there is no explicit mention of enforcing strict agent access boundaries based on identity or role during file ingestion or writing.

**Current Assumption**: The assumption is that if an agent has access to the daemon, it has uniform, global access to the entire Obsidian Vault and all workspaces synced to it.

**Attack Scenario**: An attacker compromises an agent or exploits prompt injection to instruct the agent to modify or read files outside of its intended operational scope. Because there is no granular authorization boundary within the filesystem watch/sync layer, the compromised agent can overwrite critical wisdom rules (`wisdom/*.md`), delete global reference data, or exfiltrate cross-tenant memory if multiple agents share the daemon.

**Blast Radius**: Privilege escalation, unauthorized data modification, and cross-agent data leakage. One compromised agent can poison the entire global knowledge base.

**Recommended Structural Change**: Implement a strict capability-based security model or chroot-like isolation per agent session. Agents must only be able to read/write to specific subdirectories within the vault that they have been explicitly granted access to, enforced at the filesystem and database layer, not just via prompt instructions.
