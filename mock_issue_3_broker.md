---
tags: [architecture-review, adversarial]
---
# Finding: Model Broker Single Point of Failure

## Current Assumption
The Three-Tiered Model Broker dynamically routes requests to MLX, ORT, or External. It implicitly assumes that models will load and execute successfully without causing unrecoverable errors.

## Attack Scenario
An attacker submits heavily nested or adversarial prompts designed to maximize VRAM utilization or exploit parsing logic. The primary model fails to load, crashes, or leads to an out-of-memory error.

## Blast Radius
Denial of Service (DoS) for all agent capabilities. Without a working model, memory routing, cognitive summarization, and agent actions halt completely.

## Recommended Structural Change
Implement a strict and robust fallback hierarchy (e.g., Primary Local Model -> Secondary smaller Local Model -> External Cloud Provider). Failure to acquire the primary model must propagate errors cleanly and fall back to alternative degraded options instead of leaving the system without cognitive capabilities.
