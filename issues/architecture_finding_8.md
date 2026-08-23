---
title: "Agent Orchestration Vulnerability: Unbounded Recursion and Prompt Injection in Memory Extraction"
labels: ["architecture-review", "adversarial"]
---

## Finding
The agent orchestration (via `cognitive/arbor.rs` and streaming DBSCAN clustering) does not enforce explicit bounds on recursively synthesized memories beyond a 32K token budget.

## Current Assumption
A token budget limit (32K) and limited pagination (LIMIT 50) provide sufficient bounds on cognitive synthesis and memory expansion.

## Attack Scenario
An attacker injects a prompt into an episode or memory that instructs the agent to recursively generate and store specific payloads, or bypass scope constraints. Because there is no semantic boundary enforcement or explicit recursion limit (only a token cap), the agent may get trapped in an infinite loop of synthesizing its own injected thoughts, eventually exhausting the context window and polluting the cognitive pipeline.

## Blast Radius
Memory corruption, unbounded recursion leading to denial of service, and failure of cognitive synthesis.

## Recommended Structural Change
Implement explicit recursion depth limits for cognitive synthesis, validate all extracted rules and memories against a strict semantic schema, and sanitize inputs to prevent prompt injection from affecting agent orchestration logic.

> Note: This issue requires a documented architectural decision record (ADR) response before it can be closed.
