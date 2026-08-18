---
title: "Bug: Panic path in cognitive executor due to aggressive unwrap on short-circuited error"
labels: ["bug", "agent-found"]
---

**File & Line:** `mythrax-core/src/cognitive/executor.rs`, line 105.

**Description:**
The code uses `if res.is_err() || !res.as_ref().unwrap().success() { ... }`. While technically safe due to boolean short-circuiting (the `unwrap()` won't be reached if `res` is an error), this violates the architectural constraint against `unwrap()` in production paths. It is fragile and can easily be broken during future refactoring.

**Minimal Reproducible Scenario:**
Code review inspection. The logic evaluates correctly right now, but relies on short-circuiting to prevent a panic. Any refactor that extracts the condition could trigger a panic.

**Severity:** Low (Currently safe, but violates strict architectural guidelines and introduces refactoring risk).

**Suggested Fix:**
Refactor the condition to use pattern matching or functional combinators. For example:
`if !res.as_ref().map(|r| r.success()).unwrap_or(false) { ... }`
