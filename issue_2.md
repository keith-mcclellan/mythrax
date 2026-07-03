---
title: "SPOF & Process Crash: In-Process MLX Model Engine"
labels: ["architecture-review", "adversarial"]
---

# Finding: In-Process MLX Model Engine

## Current Assumption
Lightweight embedding and generation models can safely execute in-process via Metal FFI without compromising the primary daemon.

## Attack Scenario
An adversarial agent or a malformed document triggers an extreme token length edge case, causing a Metal GPU segmentation fault or Out-Of-Memory (OOM) panic in the in-process MLX engine.

## Blast Radius
Because the model executes within the daemon process, a GPU fault crashes the entire Mythrax daemon. This simultaneously terminates the Gateway, the WAL journaling loop, and the background compactor. No graceful degradation exists.

## Recommended Structural Change
Strictly decouple the MLX inference engine into a separate, isolated OS process communicating via IPC. If the model engine panics, the daemon survives, restarts the engine, and degrades gracefully to external models.

**Status:** Requires Architectural Decision Record (ADR) response to close.