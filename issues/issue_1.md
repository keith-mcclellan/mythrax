---
title: Panic vulnerability in AST extraction via regex compilation unwrap()
labels: bug, agent-found
---

**File:** `mythrax-core/src/cognitive/ast.rs`
**Line:** 45, 101, 140, 179

**Severity:** High (Crash in AST extraction)

**Issue:**
The `extract_rust_symbols`, `extract_python_symbols`, `extract_ts_symbols`, and `extract_go_symbols` functions compile regular expressions using `Regex::new(...).unwrap()`. While the regex strings are hardcoded, using `.unwrap()` violates the strict "no unwrap() in production paths" rule. Additionally, `extract_rust_symbols` compiles the regex on *every single invocation* rather than using a static `OnceLock` like the others, which is a major performance bottleneck in an $O(N)$ code path.

**Minimal Reproducible Scenario:**
Because the strings are hardcoded, they do not crash at runtime under normal circumstances. However, any future modification to these regex strings by another developer could introduce a syntax error, causing an uncatchable panic at runtime when the regex is evaluated.

**Suggested Fix:**
Initialize all regexes statically with `OnceLock` (already done for some, missing for Rust) and handle errors gracefully using safe fallback mechanisms or `expect()` if it is absolutely necessary (though `expect` is also discouraged). The most robust fix is to use `lazy_static!` with a safe fallback or a `OnceLock` that returns an `Option`.
