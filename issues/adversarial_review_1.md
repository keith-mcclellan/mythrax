---
title: Adversarial Review: Single-Port API Gateway Authentication Bypass Risk
labels: architecture-review, adversarial
---

**Finding:** The Single-Port API Gateway relies on a shared static auth token (`X-Mythrax-Token`) for all administrative, memory, MCP, and completions proxy endpoints.
**Current Assumption:** A single static token is sufficient for authenticating local sidecar requests across different agent models and internal administrative tasks.
**Attack Scenario:** If an external agent or script manages to exfiltrate the static token (e.g., via prompt injection leaking `X-Mythrax-Token` from an environment variable or STM, or via an SSRF reading the `.config` file), the attacker gains unrestricted admin access. They could wipe the database (`prune`), hijack models, modify prompt pipelines, or execute arbitrary code via the `audit_compliance` or `htr` workspace verification loops.
**Blast Radius:** Complete system compromise. An attacker could extract all vault memories, inject malicious `WisdomRule` nodes to poison future agent behavior globally, or execute arbitrary commands in isolated git worktrees.
**Recommended Structural Change:** Implement Scope-Based JWTs or MTLS for internal daemon communication. Decouple admin/system endpoints from agent-facing MCP endpoints, requiring distinct, rotation-capable tokens. Bind tokens to specific scopes (e.g., `role: agent`, `role: admin`).

*Note: Do not close this issue without a documented architectural decision record (ADR) response.*