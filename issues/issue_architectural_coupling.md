---
tags: [architecture-review, adversarial]
---
# Finding: Search Logic and Test-Detection Logic

**Current Assumption:** Embedding test flags within production search logic is an acceptable way to mock or speed up tests.

**Attack Scenario:** `mythrax-core/src/` contains test-detection logic incorrectly embedded within production search code. An attacker triggers this mock path in production, bypassing access controls or returning poisoned search results.

**Blast Radius:** Bypassed authorization checks and data corruption.

**Recommended Structural Change:** Decouple test harnesses from production code entirely. Use dependency injection or trait-based mocking for tests instead of runtime flags.
