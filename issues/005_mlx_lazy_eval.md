---
title: "Red Team Architecture Brief: Implicit MLX Lazy Evaluation and OOM Risk"
labels: ["architecture-review", "adversarial"]
---

**Finding:** The Mythrax architecture relies on developers manually calling `.eval()` for MLX graph evaluation, array concatenations, and weight dtype casts to prevent massive delayed computation graphs.

**Current Assumption:** The assumption is that developers will consistently remember to append `.eval()` to MLX operations as dictated by the "Mandatory MLX Graph Evaluation" invariant in the `ARCHITECTURE.md`.

**Attack Scenario:** A developer introduces a new feature or optimization in `mythrax-core/src/` (e.g., in the embedding or routing logic) but forgets to manually call `.eval()`. The system processes a high volume of requests or a large batch of documents. The MLX lazy evaluation engine accumulates an enormous computation graph in memory instead of executing it. When the graph is eventually forced to evaluate, or when the memory buffer exceeds system capacity, it triggers a catastrophic Out-Of-Memory (OOM) crash.

**Blast Radius:** System-wide OOM crashes and Denial-of-Service. The tight coupling of the inference engine to manual developer discipline creates a fragile execution environment where a single missed method call can crash the entire daemon and all dependent agents.

**Recommended Structural Change:**
1. Abstract all MLX tensor and array operations behind a safe Rust wrapper API that enforces type-level guarantees.
2. The wrapper must automatically and implicitly trigger `.eval()` on all relevant operations (concatenation, casting, extraction) before returning the result or storing it in a buffer, removing the burden of manual compliance from the developer.
3. Introduce a static analysis lint (e.g., a custom Clippy rule) to detect and reject direct usage of raw MLX APIs that bypass the safe wrapper.
