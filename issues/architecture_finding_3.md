---
title: "Model Broker Evaluation Vulnerability: MLX Lazy Graph Evaluation Invariants"
labels: ["architecture-review", "adversarial"]
---

## Finding
MLX array operations require explicit evaluation (`.eval()`) to prevent Metal GPU unified memory leaks.

## Current Assumption
Developers will consistently and flawlessly execute `.eval()` immediately after array operations (like concatenation or casting) before holding references or extracting buffers.

## Attack Scenario
An adversarial input designed to trigger complex, nested model evaluations or edge cases in code paths where `.eval()` is missing or delayed can rapidly consume unified memory. This leads to catastrophic memory leaks, out-of-memory (OOM) errors, and denial of service for the entire daemon.

## Blast Radius
Daemon crash, GPU lockup, and system-wide unavailability due to memory exhaustion.

## Recommended Structural Change
Implement a mandatory, automated graph evaluation abstraction or smart pointer mechanism that guarantees evaluation upon access or out-of-scope, removing the reliance on manual `.eval()` calls. Alternatively, enforce strict static analysis rules to verify evaluation invariants at compile time.

> Note: This issue requires a documented architectural decision record (ADR) response before it can be closed.
