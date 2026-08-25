---
title: Panic path in llm/mod.rs from .expect()
labels: bug, agent-found
---

**File:** `mythrax-core/src/llm/mod.rs`
**Line Number:** 1961

**Description:**
The code uses `broker.get_weak_llm_reference().expect("Weak ref should exist")`. If the LLM reference is not initialized or has been dropped, this `.expect()` will panic.

**Minimal Reproducible Scenario:**
Attempt to retrieve the weak LLM reference from the broker before it has been properly initialized or after it has been explicitly dropped during teardown/reconfiguration.

**Severity:** High

**Suggested Fix:**
Return a `Result` or `Option` from `get_weak_llm_reference()` or handle the `None` case gracefully (e.g., returning an error to the caller) instead of unwrapping with `.expect()`.
