---
labels: [architecture-review, adversarial]
---
# Unbounded Recursion: Silent Mock Fallbacks During Memory Pressure (MOCK-004 & MOCK-006)

**Finding:**
The Model Broker silently fabricates "dummy" or "fallback-cpu-model" instances when model acquisition fails or during memory pressure (documented in `mock_audit_report.md` as MOCK-004 and MOCK-006).

**Current Assumption:**
The architecture assumes that returning *any* model reference (even a fake one) is better than crashing or propagating an error to the caller, prioritizing uninterrupted daemon operation over correctness.

**Attack Scenario:**
Under heavy concurrent load or memory pressure (VRAM exhaustion), a real model fails to load. The daemon silently switches to a dummy model. When an agent queries the LLM for a complex task (e.g., analyzing a memory or generating code), the dummy model returns garbage or empty output. The agent, attempting to fulfill its prompt instructions, detects the failure and recursively retries the task indefinitely.

**Blast Radius:**
Infinite retry loops. This leads to complete system deadlock, unbounded CPU and disk I/O consumption, massive log spam, and ultimately denial of service. The agent makes no progress, and other tasks are starved of resources.

**Recommended Structural Change:**
Remove all silent fallback and dummy mock logic in production code. Implement explicit, robust error propagation. When model acquisition fails, the daemon should return an explicit failure (e.g., HTTP 503 Service Unavailable or a specific internal error type). The agent orchestration layer must handle these errors, implementing exponential backoff, circuit breaking, or graceful degradation instead of blind retries.
