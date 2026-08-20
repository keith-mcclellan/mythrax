# Bug: Panic in LLM Router Weak Reference Expect

**File:** `mythrax-core/src/llm/mod.rs`
**Line:** 1961

## Minimal Reproducible Scenario
In `mythrax-core/src/llm/mod.rs`, the code unwraps a weak reference via `let weak_ref = broker.get_weak_llm_reference().expect("Weak ref should exist");`. If the weak reference does not exist or has been dropped, this expect call will panic.

## Severity
High - Crashes the application when the LLM broker state doesn't have an active reference.

## Suggested Fix
Properly pattern match on the result of `get_weak_llm_reference()` rather than using `.expect()`.