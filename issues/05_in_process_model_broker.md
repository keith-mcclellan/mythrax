---
title: "🛡️ Sentinel: [HIGH] In-Process Model Broker Panics Cause System-Wide Crashes"
labels: ["architecture-review", "adversarial", "bug", "agent-found"]
---

### Finding:
In Mythrax 3.0, the In-Process Engine loads models natively into the daemon's process memory using the MLX Metal GPU backend. If the MLX C++ bindings panic or OOM, it will abruptly crash the entire daemon. Additionally, missing `.eval()` calls in MLX lazy evaluation can cause system-wide OOM crashes.

### Current Assumption:
The assumption is that the MLX bindings are stable and that developers will consistently manually call `.eval()` to prevent memory leaks.

### Attack Scenario:
An attacker sends a specifically crafted prompt designed to maximize context window usage or trigger an edge case in the MLX model, causing an OOM or a panic in the C++ bindings. This crashes the entire Mythrax daemon, resulting in a Denial of Service for all connected agents.

### Blast Radius:
HIGH. Complete daemon crash (Denial of Service). This violates the requirement for graceful degradation.

### Recommended Structural Change:
- Abstract the MLX lazy evaluation behind a Rust wrapper with type-level enforcement to guarantee `.eval()` is called.
- Move the MLX inference engine out of the main daemon process into a separate, isolated worker process that can crash and be restarted independently without bringing down the API gateway and storage engines.
