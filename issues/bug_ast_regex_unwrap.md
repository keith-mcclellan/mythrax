---
title: "Bug: Missing error handling for regex compilation in ast.rs"
labels: ["bug", "agent-found"]
---

### Description
In `mythrax-core/src/cognitive/ast.rs` lines 45, 101, 140, 179, `Regex::new(...).unwrap()` is called dynamically inside a function. While the regex string is a literal and valid, compiling regexes on every function call is very inefficient, and calling `.unwrap()` on production code violates the panic-free guidelines.

### Minimal Reproducible Scenario
Call `extract_symbols` or `extract_symbols_python` repeatedly on a large file. The regex is re-compiled on every call, causing severe performance degradation, and it uses `.unwrap()`.

### Severity
Low/Medium (Performance / Style guideline violation).

### Suggested Fix
Use `std::sync::LazyLock` or `OnceLock` to compile the regex statically once and handle the compilation result safely or unwrap statically (which is often acceptable for static regexes, though avoiding unwrap is preferred).
