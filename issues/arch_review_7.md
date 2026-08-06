# MLX lazy evaluation manual .eval() invariant relying on developer discipline

**Labels**: `architecture-review`, `adversarial`

## Finding
The system relies on developers manually calling `.eval()` for MLX lazy evaluation to prevent un-evaluated graph accumulation and system-wide OOM crashes.

## Current Assumption
Developers will consistently remember and correctly apply `.eval()` before accessing MLX buffers or returning from execution paths.

## Attack Scenario
A developer adds a new embedding model, cross-encoder step, or cognitive pipeline stage, but forgets to call `.eval()`. The MLX computation graph grows unbounded across multiple HTTP requests, eventually causing a fatal OOM crash that takes down the entire daemon.

## Blast Radius
System instability and frequent crashes as new models or complex pipelines are introduced. High cognitive load for developers.

## Recommended Structural Change
Abstract the MLX evaluation invariant behind a safe Rust wrapper type (e.g., `EvaluatedTensor`) that enforces `.eval()` at compile time. Functions that require evaluated data should only accept `EvaluatedTensor` rather than raw MLX arrays.
