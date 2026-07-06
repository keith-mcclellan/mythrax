---
tags: [architecture-review, adversarial]
---
# Finding: Single-Port API Gateway relies on static token for authentication

## Current Assumption
The single-port gateway (port 8090) and the daemon assume the local environment is secure and validate REST and MCP requests using a shared static auth token (`X-Mythrax-Token`).

## Attack Scenario
If the local environment is compromised, or a malicious user space application/script runs with the user's privileges, it can easily discover the token (e.g. by reading `~/.mythrax/token` or inspecting process arguments). An attacker could then issue arbitrary MCP commands to proxy requests to an external LLM, manipulate agent memory, or extract sensitive context.

## Blast Radius
Full system compromise. The attacker gains complete control over the daemon's persistent store, memory, and cognitive models, essentially hijacking the AI agent's orchestration path and memory graph.

## Recommended Structural Change
Replace the static token authentication with dynamic, short-lived session-based authentication, or rely on robust OS-level IPC mechanisms (e.g., Unix Domain Sockets) with peer credential validation to enforce that only authorized IDEs/processes can communicate with the daemon.
