---
title: "Adversarial CTO: Prompt Injection Vulnerability in Episodic Memory via precompact.rs"
labels: ['architecture-review', 'adversarial']
---

## Finding
Prompt Injection Vulnerability in Episodic Memory via `precompact.rs`

## Current Assumption
Agent orchestration and tools only process safe internal data; extracting tool results and user inputs verbatim into episodic memory is a benign operation that preserves context.

## Attack Scenario
A malicious external input or manipulated tool output contains a prompt injection payload. The pre-compaction hook in `mythrax-core/src/hooks/precompact.rs` extracts these verbatim without sanitization, poisoning the cross-session episodic memory. When future agent loops or the DBSCAN compactor process this memory, the injected payload hijacks the cognitive pipeline.

## Blast Radius
Complete compromise of downstream cognitive models and agents. An attacker could exfiltrate data, bypass scope boundaries, or manipulate the agent's actions across multiple sessions.

## Recommended Structural Change
Introduce a strict sanitization, encoding, and validation layer before any raw input or tool output is written to episodic memory. Segregate raw inputs from synthesized context, and enforce strict system-prompt boundaries during re-reading. Never close this issue without a documented ADR response.
