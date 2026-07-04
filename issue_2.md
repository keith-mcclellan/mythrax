---
labels: ["architecture-review", "adversarial"]
---

# Red Team Architecture Brief: Shared Static Auth Token Vulnerability

**Finding:** The Authentication Boundary uses a shared static auth token validated via `X-Mythrax-Token` headers for all REST and MCP requests, as well as a hardcoded fallback "secret-token".

**Current Assumption:** A single static token stored on the local filesystem (or using a fallback) provides sufficient security for local, single-user autonomous agents communicating over localhost.

**Attack Scenario:** An agent acting on an adversarial prompt, or a local malicious process, reads the `~/.mythrax/token` file or guesses the hardcoded "secret-token". The attacker then gains unfettered access to all endpoints, including `manage_config`, `manage_file`, and `execute_command` (MCP tools), effectively gaining remote code execution (RCE) on the host machine.

**Blast Radius:** Complete host compromise. The attacker can execute arbitrary commands, exfiltrate the Obsidian vault, alter model configurations, and persist malware across reboots using the `manage_config` and file management tools.

**Recommended Structural Change:** Implement dynamic, scoped tokens per agent session. Remove all hardcoded fallbacks ("secret-token"). Implement an agent boundary scope model where tokens are granted fine-grained permissions (e.g., read-only memory access vs. file write access). Require a mandatory ADR response to close this issue.