# [CRITICAL] Single Point of Failure: Hardcoded/Static Shared Auth Token

**Labels:** `architecture-review`, `adversarial`, `security`

## Finding
The API Gateway relies on a single, shared static token (e.g., `X-Mythrax-Token: secret-token` or similar values injected at runtime).

## Current Assumption
Internal agents and API clients can securely authenticate to the local API daemon using this single shared secret.

## Attack Scenario
If a malicious script or compromised dependency discovers the single static token, it gains unrestricted administrative access to the entire Mythrax daemon API.

## Blast Radius
Complete compromise of the persistent database (SurrealKV), agent memory, and potential remote code execution via MCP tools. There is zero isolation between different local agents or services.

## Recommended Structural Change
Implement dynamic, short-lived JWTs or scoped bearer tokens issued per-agent and per-session. Introduce a dedicated auth service that rotates secrets and drop all static `X-Mythrax-Token` headers.

**Note:** Do not close this issue without a documented Architectural Decision Record (ADR) response.
