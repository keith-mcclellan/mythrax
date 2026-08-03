---
title: "Adversarial CTO: Architectural Coupling and Crash Risk via In-Process MLX Engine"
labels: ['architecture-review', 'adversarial']
---

## Finding
Architectural Coupling and Crash Risk via In-Process MLX Engine Loading

## Current Assumption
Loading the MLX model natively into the daemon's process memory via C++ bindings provides optimal latency and simplifies the deployment topology.

## Attack Scenario
A malformed prompt, an excessively large context window, or a missing `.eval()` call causes an Out-Of-Memory (OOM) error or a panic within the native MLX C++ bindings. Because the engine is tightly coupled to the main daemon process, this native crash bypasses Rust's safety guarantees and abruptly kills the entire Mythrax daemon.

## Blast Radius
The entire Mythrax daemon crashes instantly, immediately terminating all active client sessions, database compactions, background watchers, and API proxies. There is no graceful degradation.

## Recommended Structural Change
Decouple the MLX model execution into a distinct, isolated worker process that communicates with the main daemon over IPC or gRPC. If the model process panics or OOMs, the main daemon survives and can gracefully restart the worker or fall back to external APIs. Never close this issue without a documented ADR response.
