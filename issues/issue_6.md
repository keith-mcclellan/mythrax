---
labels: architecture-review, adversarial
---
# Adversarial Review: Prompt Injection in Episodic Memory Hooks (Orchestration Risk)

## Finding
Failure to properly sanitize external inputs (user inputs, API tool results) before storing them in prompt-visible episodic memory (e.g., in `mythrax-core/src/hooks/precompact.rs`).

## Current Assumption
The system assumes that episodic memory and tool outputs are safe and can be directly included in prompts without sanitization.

## Attack Scenario
An attacker provides input containing control tokens like `<|`, `|>`, or code block backticks. The system stores this in episodic memory. Later, when the memory is retrieved and injected into a prompt, the control tokens hijack the LLM's parsing logic, causing it to execute attacker-controlled commands or leak information.

## Blast Radius
Complete agent compromise via cross-session prompt injection. The attacker can control the agent's actions in future sessions.

## Recommended Structural Change
Implement a strict sanitization layer for all external inputs before storing them in memory or injecting them into prompts. Replace or escape control tokens and ensure strong boundary markers are used in prompt templates.